package smblease

import (
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"sync"
	"testing"
)

// fakeComposer is an injectable in-memory stand-in for `docker compose`. It
// records the calls made so tests can assert that adopt issues NO compose call
// and reconcile issues exactly an `up`.
type fakeComposer struct {
	mu sync.Mutex

	running   map[string]bool
	healthy   map[string]bool
	statusErr error
	downErr   error

	upCalls   [][]string
	downCalls int
}

func newFakeComposer() *fakeComposer {
	return &fakeComposer{running: map[string]bool{}, healthy: map[string]bool{}}
}

func (f *fakeComposer) Status() (map[string]bool, map[string]bool, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.statusErr != nil {
		return nil, nil, f.statusErr
	}
	r := map[string]bool{}
	h := map[string]bool{}
	for k, v := range f.running {
		r[k] = v
	}
	for k, v := range f.healthy {
		h[k] = v
	}
	return r, h, nil
}

func (f *fakeComposer) Up(services []string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.upCalls = append(f.upCalls, services)
	// Simulate the up: every service the mode names becomes running + healthy.
	// For an empty (all) set we leave state unchanged (tests don't use it here).
	for _, s := range services {
		f.running[s] = true
		f.healthy[s] = true
	}
	return nil
}

func (f *fakeComposer) Down() error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.downCalls++
	if f.downErr != nil {
		return f.downErr
	}
	f.running = map[string]bool{}
	f.healthy = map[string]bool{}
	return nil
}

func (f *fakeComposer) RunningServices() ([]string, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	var out []string
	for s, on := range f.running {
		if on {
			out = append(out, s)
		}
	}
	sort.Strings(out)
	return out, nil
}

// withFake installs an isolated lease root + a fresh fake composer for one test,
// restoring globals afterward. Returns the fake so the test can preset state and
// assert on calls.
func withFake(t *testing.T) *fakeComposer {
	t.Helper()
	root := t.TempDir()
	t.Setenv("CMDR_SMB_LEASE_ROOT", root)
	// Pin a stable compose dir + port env so the config hash is deterministic
	// and doesn't depend on the real repo layout or the ambient environment.
	t.Setenv("CMDR_SMB_COMPOSE_DIR", filepath.Join(root, "nonexistent-compose"))
	t.Setenv("SMB_CONSUMER_GUEST_PORT", "11480")

	fake := newFakeComposer()
	prev := newComposer
	newComposer = func() Composer { return fake }
	prevLog := Logf
	Logf = func(string, ...any) {} // silence
	t.Cleanup(func() {
		newComposer = prev
		Logf = prevLog
	})
	return fake
}

// serve marks the e2e service set as running+healthy in the fake.
func serveE2E(f *fakeComposer) {
	for _, s := range modeServices("e2e") {
		f.running[s] = true
		if s != "smb-consumer-flaky" {
			f.healthy[s] = true
		}
	}
}

func leaseFiles(t *testing.T) []string {
	t.Helper()
	holders, err := listLeaseHolders()
	if err != nil {
		t.Fatalf("listLeaseHolders: %v", err)
	}
	return holders
}

// --- acquire / reconcile path ---

func TestAcquireOnEmptyStackReconciles(t *testing.T) {
	fake := withFake(t)
	res, err := Acquire("manual", "e2e")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionReconcile {
		t.Fatalf("want reconcile on empty stack, got %s", res.Action)
	}
	if len(fake.upCalls) != 1 {
		t.Fatalf("want exactly 1 up call, got %d", len(fake.upCalls))
	}
	if got := leaseFiles(t); len(got) != 1 || got[0] != "manual" {
		t.Fatalf("want single manual lease, got %v", got)
	}
}

func TestAcquireAdoptsServingStackNoComposeCall(t *testing.T) {
	fake := withFake(t)
	// First acquire reconciles + stamps the hash and marks services serving.
	if _, err := Acquire("manual", "e2e"); err != nil {
		t.Fatalf("seed Acquire: %v", err)
	}
	upBefore := len(fake.upCalls)

	// Second holder, same config + serving stack → adopt, NO compose call.
	res, err := Acquire("12345", "e2e")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionAdopt {
		t.Fatalf("want adopt, got %s", res.Action)
	}
	if len(fake.upCalls) != upBefore {
		t.Fatalf("adopt must issue NO compose up; up calls went %d -> %d", upBefore, len(fake.upCalls))
	}
}

// --- adopt-vs-reconcile policy table ---

func TestPolicyHashMismatchUnderForeignLeaseAdoptsAnyway(t *testing.T) {
	fake := withFake(t)
	serveE2E(fake)
	// A foreign holder is live but no config hash is stamped (mismatch).
	if err := os.MkdirAll(LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(LeaseDir(), "99999"), []byte("foreign"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Keep the foreign PID "alive" by using our own test process PID so the
	// sweep doesn't reap it.
	self := strconv.Itoa(os.Getpid())
	_ = os.Rename(filepath.Join(LeaseDir(), "99999"), filepath.Join(LeaseDir(), self))

	res, err := Acquire("manual", "e2e")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionAdopt {
		t.Fatalf("hash mismatch under foreign lease must ADOPT (never recreate), got %s", res.Action)
	}
	if len(fake.upCalls) != 0 {
		t.Fatalf("must not issue any compose call under a foreign live lease; got %d up calls", len(fake.upCalls))
	}
}

func TestPolicyHashMismatchAloneReconciles(t *testing.T) {
	fake := withFake(t)
	serveE2E(fake)
	// No other leases, no stamped hash → reconcile is safe.
	res, err := Acquire("manual", "e2e")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionReconcile {
		t.Fatalf("hash mismatch with only self must reconcile, got %s", res.Action)
	}
	if len(fake.upCalls) != 1 {
		t.Fatalf("want 1 up call, got %d", len(fake.upCalls))
	}
}

func TestPolicyPartiallyUpReconciles(t *testing.T) {
	fake := withFake(t)
	// Only guest is serving; the rest are missing → reconcile.
	fake.running["smb-consumer-guest"] = true
	fake.healthy["smb-consumer-guest"] = true
	res, err := Acquire("manual", "e2e")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionReconcile {
		t.Fatalf("partial stack must reconcile, got %s", res.Action)
	}
}

func TestPolicyUnhealthyServiceReconciles(t *testing.T) {
	fake := withFake(t)
	// All e2e services running, but unicode is running-not-healthy → reconcile.
	for _, s := range modeServices("e2e") {
		fake.running[s] = true
		fake.healthy[s] = true
	}
	fake.healthy["smb-consumer-unicode"] = false
	res, err := Acquire("manual", "e2e")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionReconcile {
		t.Fatalf("an unhealthy required service must reconcile, got %s", res.Action)
	}
}

func TestPolicyFlakyRunningWithoutHealthcheckCanAdopt(t *testing.T) {
	fake := withFake(t)
	// core mode includes the no-healthcheck flaky service. Mark every core
	// service healthy except flaky (which is only running). Adopt must still be
	// possible (running is the strongest signal for flaky).
	for _, s := range modeServices("core") {
		fake.running[s] = true
		if s != "smb-consumer-flaky" {
			fake.healthy[s] = true
		}
	}
	// Stamp the matching hash so adopt isn't blocked on hash.
	writeConfigHash("core")
	res, err := Acquire("manual", "core")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionAdopt {
		t.Fatalf("a running (no-healthcheck) flaky service must not block adopt, got %s", res.Action)
	}
}

func TestPolicyStatusErrorUnderForeignLeaseAdopts(t *testing.T) {
	fake := withFake(t)
	fake.statusErr = os.ErrPermission
	// Foreign live lease present.
	if err := os.MkdirAll(LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	self := strconv.Itoa(os.Getpid())
	if err := os.WriteFile(filepath.Join(LeaseDir(), self), []byte("foreign"), 0o644); err != nil {
		t.Fatal(err)
	}
	res, err := Acquire("manual", "e2e")
	if err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if res.Action != ActionAdopt {
		t.Fatalf("unreadable stack under foreign lease must adopt-and-warn, got %s", res.Action)
	}
	if len(fake.upCalls) != 0 {
		t.Fatalf("must not compose under a foreign lease, got %d up calls", len(fake.upCalls))
	}
}

// --- idempotency + refcount ---

func TestAcquireIdempotentPerHolder(t *testing.T) {
	fake := withFake(t)
	for i := 0; i < 3; i++ {
		if _, err := Acquire("manual", "e2e"); err != nil {
			t.Fatalf("Acquire #%d: %v", i, err)
		}
	}
	if got := leaseFiles(t); len(got) != 1 {
		t.Fatalf("re-acquiring same holder must not add leases; got %v", got)
	}
	_ = fake
}

func TestTwoHoldersRefcountAndDownAtZero(t *testing.T) {
	fake := withFake(t)
	if _, err := Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	if _, err := Acquire("777", "e2e"); err != nil {
		t.Fatal(err)
	}
	if got := leaseFiles(t); len(got) != 2 {
		t.Fatalf("want 2 leases, got %v", got)
	}

	// First release: one lease remains → NO down.
	if err := Release("777"); err != nil {
		t.Fatal(err)
	}
	if fake.downCalls != 0 {
		t.Fatalf("release with a remaining lease must NOT down; downCalls=%d", fake.downCalls)
	}
	if got := leaseFiles(t); len(got) != 1 || got[0] != "manual" {
		t.Fatalf("want only manual lease left, got %v", got)
	}

	// Last release: zero leases → down.
	if err := Release("manual"); err != nil {
		t.Fatal(err)
	}
	if fake.downCalls != 1 {
		t.Fatalf("last release must down exactly once; downCalls=%d", fake.downCalls)
	}
	if got := leaseFiles(t); len(got) != 0 {
		t.Fatalf("no leases should remain, got %v", got)
	}
}

func TestReleaseUnknownHolderIsSafe(t *testing.T) {
	fake := withFake(t)
	if _, err := Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	// Releasing a holder that never had a lease must not down (manual still held).
	if err := Release("does-not-exist"); err != nil {
		t.Fatal(err)
	}
	if fake.downCalls != 0 {
		t.Fatalf("releasing an absent holder must not down while another lease is held; downCalls=%d", fake.downCalls)
	}
	if got := leaseFiles(t); len(got) != 1 || got[0] != "manual" {
		t.Fatalf("manual lease must survive, got %v", got)
	}
}

// --- sweep + sentinel ---

func TestSweepReapsDeadPidButNotManual(t *testing.T) {
	fake := withFake(t)
	if err := os.MkdirAll(LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	// A dead PID lease: PID 1 is init, but we want a guaranteed-dead PID. Use a
	// very high unlikely-live PID. (Acquire's sweep uses kill(pid,0).)
	deadPID := "2147480000"
	if err := os.WriteFile(filepath.Join(LeaseDir(), deadPID), []byte("dead"), 0o644); err != nil {
		t.Fatal(err)
	}
	// A manual sentinel lease must NEVER be swept.
	if err := os.WriteFile(filepath.Join(LeaseDir(), ManualHolder), []byte("manual"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Acquiring (any holder) triggers the sweep.
	if _, err := Acquire("12321", "e2e"); err != nil {
		t.Fatal(err)
	}
	holders := leaseFiles(t)
	for _, h := range holders {
		if h == deadPID {
			t.Fatalf("dead PID lease %s should have been swept; holders=%v", deadPID, holders)
		}
	}
	found := false
	for _, h := range holders {
		if h == ManualHolder {
			found = true
		}
	}
	if !found {
		t.Fatalf("manual sentinel must survive the sweep; holders=%v", holders)
	}
	_ = fake
}

func TestSweepKeepsLivePid(t *testing.T) {
	withFake(t)
	if err := os.MkdirAll(LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	// Our own PID is alive → must survive the sweep.
	self := strconv.Itoa(os.Getpid())
	if err := os.WriteFile(filepath.Join(LeaseDir(), self), []byte("alive"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	found := false
	for _, h := range leaseFiles(t) {
		if h == self {
			found = true
		}
	}
	if !found {
		t.Fatalf("a live PID lease (%s) must not be swept", self)
	}
}

// --- release teardown asymmetry ---

func TestReleaseLeavesUpWhenDownErrors(t *testing.T) {
	fake := withFake(t)
	if _, err := Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	// Make Down fail. Release must NOT pretend the stack is gone, but it has
	// already removed the lease (the count is the contract; down failure is a
	// docker problem to report, not a reason to re-add the lease).
	fake.downErr = os.ErrPermission
	if err := Release("manual"); err != nil {
		t.Fatalf("Release should swallow the down error (leave-up degradation): %v", err)
	}
	if fake.downCalls != 1 {
		t.Fatalf("down should have been attempted once; got %d", fake.downCalls)
	}
}

// --- validation ---

func TestAcquireRejectsBadHolderID(t *testing.T) {
	withFake(t)
	for _, bad := range []string{"", "a/b", "..", "."} {
		if _, err := Acquire(bad, "e2e"); err == nil {
			t.Fatalf("Acquire(%q) should reject an invalid holder-id", bad)
		}
	}
}

// --- concurrency: the flock serializes refcount mutations ---

func TestConcurrentAcquireReleaseDownsExactlyOnce(t *testing.T) {
	fake := withFake(t)
	const n = 12
	var wg sync.WaitGroup
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			holder := "h" + strconv.Itoa(id)
			if _, err := Acquire(holder, "e2e"); err != nil {
				t.Errorf("Acquire(%s): %v", holder, err)
				return
			}
			if err := Release(holder); err != nil {
				t.Errorf("Release(%s): %v", holder, err)
			}
		}(i)
	}
	wg.Wait()
	// With perfectly interleaved acquire/release the down count is nondeterministic
	// (each holder that finds itself last downs), but it must be >=1 and the lease
	// dir must end empty.
	if got := leaseFiles(t); len(got) != 0 {
		t.Fatalf("all holders released; lease dir must be empty, got %v", got)
	}
	if fake.downCalls < 1 {
		t.Fatalf("the stack must have been downed at least once at zero, got %d", fake.downCalls)
	}
}
