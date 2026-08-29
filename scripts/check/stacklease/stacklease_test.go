package stacklease

import (
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
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

	upCalls      [][]string
	downCalls    int
	restartCalls [][]string
	restartErr   error
	// restartWrites stands in for the entrypoint re-running: a real restart is
	// what republishes a container's key pair, and a fake that only records the
	// call would let a broken wait pass.
	restartWrites func(services []string)
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

func (f *fakeComposer) Restart(services []string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.restartCalls = append(f.restartCalls, services)
	if f.restartErr != nil {
		return f.restartErr
	}
	if f.restartWrites != nil {
		f.restartWrites(services)
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

// composerFakes hands each stack its own fake, so a test that drives two stacks
// can tell which one a compose call landed on.
type composerFakes struct {
	mu     sync.Mutex
	byName map[string]*fakeComposer
}

func (c *composerFakes) forStack(s *Stack) *fakeComposer {
	c.mu.Lock()
	defer c.mu.Unlock()
	if f, ok := c.byName[s.Name]; ok {
		return f
	}
	f := newFakeComposer()
	c.byName[s.Name] = f
	return f
}

// withFakes installs an isolated lease root + per-stack fake composers for one
// test, restoring globals afterward.
func withFakes(t *testing.T) *composerFakes {
	t.Helper()
	root := t.TempDir()
	t.Setenv(leaseRootEnv, root)
	// Pin a stable compose dir + port env per stack so the config hash is
	// deterministic and doesn't depend on the real repo layout or the ambient
	// environment.
	for _, s := range All() {
		t.Setenv(s.composeDirEnv, filepath.Join(root, "nonexistent-compose-"+s.Name))
		// Neutralize any keys-dir override so KeysDir tracks the sandboxed lease
		// root. ❗ Also what keeps a test honest: `EnsureKeysDir` pins the
		// resolved path with a bare `os.Setenv`, and without a `t.Setenv` to
		// restore, one test's temp path would ride into the next one's config
		// hash.
		if env := s.KeysDirEnv(); env != "" {
			t.Setenv(env, "")
		}
	}
	t.Setenv("CMDR_ALPHA_COMPOSE_DIR", filepath.Join(root, "nonexistent-compose-alpha"))
	t.Setenv("CMDR_BETA_COMPOSE_DIR", filepath.Join(root, "nonexistent-compose-beta"))
	t.Setenv("SMB_CONSUMER_GUEST_PORT", "11480")

	fakes := &composerFakes{byName: map[string]*fakeComposer{}}
	prev := newComposer
	newComposer = func(s *Stack) Composer { return fakes.forStack(s) }
	prevLog := Logf
	Logf = func(string, ...any) {} // silence
	t.Cleanup(func() {
		newComposer = prev
		Logf = prevLog
	})
	return fakes
}

// withFake is the single-stack shorthand: the SMB stack's fake, which is what
// every policy-table test drives.
func withFake(t *testing.T) *fakeComposer {
	t.Helper()
	return withFakes(t).forStack(SMB)
}

// leaseHolders lists the stack's live holders, failing the test if the dir is
// unreadable.
func (s *Stack) leaseHolders(t *testing.T) []string {
	t.Helper()
	holders, err := s.listLeaseHolders()
	if err != nil {
		t.Fatalf("listLeaseHolders(%s): %v", s.Name, err)
	}
	return holders
}

// serve marks the e2e service set as running+healthy in the fake.
func serveE2E(f *fakeComposer) {
	for _, s := range SMB.modeServicesFor(ModeE2E) {
		f.running[s] = true
		if s != "smb-consumer-flaky" {
			f.healthy[s] = true
		}
	}
}

func leaseFiles(t *testing.T) []string {
	t.Helper()
	return SMB.leaseHolders(t)
}

// --- acquire / reconcile path ---

func TestAcquireOnEmptyStackReconciles(t *testing.T) {
	fake := withFake(t)
	res, err := SMB.Acquire("manual", "e2e")
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
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatalf("seed Acquire: %v", err)
	}
	upBefore := len(fake.upCalls)

	// Second holder, same config + serving stack → adopt, NO compose call.
	res, err := SMB.Acquire("12345", "e2e")
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
	if err := os.MkdirAll(SMB.LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(SMB.LeaseDir(), "99999"), []byte("foreign"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Keep the foreign PID "alive" by using our own test process PID so the
	// sweep doesn't reap it.
	self := strconv.Itoa(os.Getpid())
	_ = os.Rename(filepath.Join(SMB.LeaseDir(), "99999"), filepath.Join(SMB.LeaseDir(), self))

	res, err := SMB.Acquire("manual", "e2e")
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
	res, err := SMB.Acquire("manual", "e2e")
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
	res, err := SMB.Acquire("manual", "e2e")
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
	for _, s := range SMB.modeServicesFor(ModeE2E) {
		fake.running[s] = true
		fake.healthy[s] = true
	}
	fake.healthy["smb-consumer-unicode"] = false
	res, err := SMB.Acquire("manual", "e2e")
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
	for _, s := range SMB.modeServicesFor(ModeCore) {
		fake.running[s] = true
		if s != "smb-consumer-flaky" {
			fake.healthy[s] = true
		}
	}
	// Stamp the matching hash so adopt isn't blocked on hash.
	SMB.writeConfigHash(ModeCore)
	res, err := SMB.Acquire("manual", "core")
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
	if err := os.MkdirAll(SMB.LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	self := strconv.Itoa(os.Getpid())
	if err := os.WriteFile(filepath.Join(SMB.LeaseDir(), self), []byte("foreign"), 0o644); err != nil {
		t.Fatal(err)
	}
	res, err := SMB.Acquire("manual", "e2e")
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
		if _, err := SMB.Acquire("manual", "e2e"); err != nil {
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
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	if _, err := SMB.Acquire("777", "e2e"); err != nil {
		t.Fatal(err)
	}
	if got := leaseFiles(t); len(got) != 2 {
		t.Fatalf("want 2 leases, got %v", got)
	}

	// First release: one lease remains → NO down.
	if err := SMB.Release("777"); err != nil {
		t.Fatal(err)
	}
	if fake.downCalls != 0 {
		t.Fatalf("release with a remaining lease must NOT down; downCalls=%d", fake.downCalls)
	}
	if got := leaseFiles(t); len(got) != 1 || got[0] != "manual" {
		t.Fatalf("want only manual lease left, got %v", got)
	}

	// Last release: zero leases → down.
	if err := SMB.Release("manual"); err != nil {
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
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	// Releasing a holder that never had a lease must not down (manual still held).
	if err := SMB.Release("does-not-exist"); err != nil {
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
	if err := os.MkdirAll(SMB.LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	// A dead PID lease: PID 1 is init, but we want a guaranteed-dead PID. Use a
	// very high unlikely-live PID. (Acquire's sweep uses kill(pid,0).)
	deadPID := "2147480000"
	if err := os.WriteFile(filepath.Join(SMB.LeaseDir(), deadPID), []byte("dead"), 0o644); err != nil {
		t.Fatal(err)
	}
	// A manual sentinel lease must NEVER be swept.
	if err := os.WriteFile(filepath.Join(SMB.LeaseDir(), ManualHolder), []byte("manual"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Acquiring (any holder) triggers the sweep.
	if _, err := SMB.Acquire("12321", "e2e"); err != nil {
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
	if err := os.MkdirAll(SMB.LeaseDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	// Our own PID is alive → must survive the sweep.
	self := strconv.Itoa(os.Getpid())
	if err := os.WriteFile(filepath.Join(SMB.LeaseDir(), self), []byte("alive"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
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
	if _, err := SMB.Acquire("manual", "e2e"); err != nil {
		t.Fatal(err)
	}
	// Make Down fail. Release must NOT pretend the stack is gone, but it has
	// already removed the lease (the count is the contract; down failure is a
	// docker problem to report, not a reason to re-add the lease).
	fake.downErr = os.ErrPermission
	if err := SMB.Release("manual"); err != nil {
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
		if _, err := SMB.Acquire(bad, "e2e"); err == nil {
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
			if _, err := SMB.Acquire(holder, "e2e"); err != nil {
				t.Errorf("Acquire(%s): %v", holder, err)
				return
			}
			if err := SMB.Release(holder); err != nil {
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

// --- two stacks on one machine ---

// testStack is a synthetic second stack, so the coexistence properties are
// asserted on the mechanism rather than on whichever fixtures happen to be
// registered today.
func testStack(name string) *Stack {
	return &Stack{
		Name:          name,
		ProjectName:   name + "-fixture",
		lockFile:      "cmdr-" + name + ".lock",
		leaseDirName:  "cmdr-" + name + "-leases",
		composeDirEnv: "CMDR_" + strings.ToUpper(name) + "_COMPOSE_DIR",
		composeDirRel: "apps/desktop/test/" + name + "-servers/.compose",
		composeFiles:  []string{"docker-compose.yml"},
		portEnvPrefix: strings.ToUpper(name) + "_FIXTURE_",
		modeServices: map[string][]string{
			"core": {name + "-fixture-a", name + "-fixture-b"},
		},
	}
}

func TestTwoStacksKeepSeparateLocksAndLeaseDirs(t *testing.T) {
	fakes := withFakes(t)
	alpha, beta := testStack("alpha"), testStack("beta")

	if alpha.LockPath() == beta.LockPath() {
		t.Fatalf("two stacks must not share a lock file: %s", alpha.LockPath())
	}
	if alpha.LeaseDir() == beta.LeaseDir() {
		t.Fatalf("two stacks must not share a lease dir: %s", alpha.LeaseDir())
	}

	if _, err := alpha.Acquire("manual", "core"); err != nil {
		t.Fatalf("alpha Acquire: %v", err)
	}
	if _, err := beta.Acquire("manual", "core"); err != nil {
		t.Fatalf("beta Acquire: %v", err)
	}

	// Releasing every holder of alpha must down alpha and leave beta alone: a
	// shared lease dir would read beta's holder as alpha's and never tear down.
	if err := alpha.Release("manual"); err != nil {
		t.Fatalf("alpha Release: %v", err)
	}
	if got := fakes.forStack(alpha).downCalls; got != 1 {
		t.Fatalf("alpha's last release must down alpha exactly once, got %d", got)
	}
	if got := fakes.forStack(beta).downCalls; got != 0 {
		t.Fatalf("alpha's release must not touch beta; beta down calls: %d", got)
	}
	if holders := beta.leaseHolders(t); len(holders) != 1 || holders[0] != "manual" {
		t.Fatalf("beta's lease must survive alpha's teardown, got %v", holders)
	}
}

func TestOneHolderIdLeasesEachStackIndependently(t *testing.T) {
	withFakes(t)
	alpha, beta := testStack("alpha"), testStack("beta")
	// The check runner uses its own PID as the holder-id for every stack it
	// needs, so the same id must count once per stack, never once overall.
	if _, err := alpha.Acquire("4242", "core"); err != nil {
		t.Fatal(err)
	}
	if _, err := beta.Acquire("4242", "core"); err != nil {
		t.Fatal(err)
	}
	if got := alpha.leaseHolders(t); len(got) != 1 {
		t.Fatalf("alpha should hold one lease, got %v", got)
	}
	if got := beta.leaseHolders(t); len(got) != 1 {
		t.Fatalf("beta should hold one lease, got %v", got)
	}
}

func TestRegisteredStacksAreDistinct(t *testing.T) {
	seen := map[string]string{}
	claim := func(kind, value, stack string) {
		t.Helper()
		key := kind + "=" + value
		if other, taken := seen[key]; taken {
			t.Fatalf("%s and %s share a %s (%q); they would tear each other's fixtures down", other, stack, kind, value)
		}
		seen[key] = stack
	}
	stacks := All()
	if len(stacks) < 2 {
		t.Fatalf("want at least the SMB and SFTP stacks registered, got %d", len(stacks))
	}
	for _, s := range stacks {
		claim("name", s.Name, s.Name)
		claim("compose project", s.ProjectName, s.Name)
		claim("lock path", s.LockPath(), s.Name)
		claim("lease dir", s.LeaseDir(), s.Name)
		claim("compose dir", s.composeDirRel, s.Name)
		if dir := s.KeysDir(); dir != "" {
			claim("keys dir", dir, s.Name)
		}
	}
}

func TestSftpKeysDirIsMachineWideAndSmbHasNone(t *testing.T) {
	// ❗ The keys dir is a bind SOURCE for a MACHINE-WIDE stack, so it belongs in
	// /tmp beside the lock and the lease dir. A path under a checkout bakes the
	// starting worktree into containers that sibling worktrees adopt; deleting
	// that worktree then breaks key auth in all of them at once.
	if got := SFTP.KeysDir(); got != "/tmp/cmdr-sftp-keys" {
		t.Fatalf("the SFTP keys dir is %q, want /tmp/cmdr-sftp-keys", got)
	}
	// SMB mounts nothing from the host, and must stay that way rather than
	// inheriting an empty-but-present directory.
	if got := SMB.KeysDir(); got != "" {
		t.Fatalf("the SMB stack grew a keys dir (%q); it bind-mounts nothing", got)
	}
	if got := SMB.KeysDirEnv(); got != "" {
		t.Fatalf("the SMB stack grew a keys-dir env var (%q)", got)
	}
}

func TestEnsureKeysDirCreatesEveryLeafAndPinsTheEnv(t *testing.T) {
	withFakes(t)
	want := SFTP.KeysDir()

	if err := SFTP.EnsureKeysDir(); err != nil {
		t.Fatalf("EnsureKeysDir: %v", err)
	}

	// Every leaf up front: Docker auto-creates a missing bind source root-owned
	// on Linux, and the container's own write into it then fails.
	for _, leaf := range SFTP.KeysLeaves() {
		if st, err := os.Stat(filepath.Join(want, leaf.Dir)); err != nil || !st.IsDir() {
			t.Fatalf("EnsureKeysDir left %s/%s uncreated (%v)", want, leaf.Dir, err)
		}
	}
	if len(SFTP.KeysLeaves()) == 0 {
		t.Fatal("the SFTP stack lists no keys leaves, so the loop above asserts nothing")
	}
	// Pinned in the env, so compose binds what this process resolved rather than
	// falling back to its own `${…:-default}`.
	if got := os.Getenv(SFTP.KeysDirEnv()); got != want {
		t.Fatalf("EnsureKeysDir left %s=%q, want %q", SFTP.KeysDirEnv(), got, want)
	}
}

func TestEnsureKeysDirIsANoOpForAStackThatMountsNothing(t *testing.T) {
	withFakes(t)
	if err := SMB.EnsureKeysDir(); err != nil {
		t.Fatalf("EnsureKeysDir on a stack with no keys dir: %v", err)
	}
}

func TestKeysDirFoldsIntoTheConfigHash(t *testing.T) {
	withFakes(t)
	before := SFTP.computeConfigHash(ModeCore)

	// A stack whose containers bind a DIFFERENT host directory is as stale as
	// one bound to different ports, and far quieter: every container stays
	// healthy while every key-auth cell reads a directory nobody writes.
	t.Setenv(SFTP.KeysDirEnv(), filepath.Join(t.TempDir(), "elsewhere"))
	if after := SFTP.computeConfigHash(ModeCore); after == before {
		t.Fatal("moving the keys dir left the config hash unchanged, so an adopter would keep containers bound to the old one")
	}
}

func TestSmbStackKeepsItsHistoricalLeasePaths(t *testing.T) {
	// A sibling worktree on older code holds its lease at these exact paths, so
	// moving them would hide a live holder and re-open the teardown race.
	if got := SMB.LockPath(); got != "/tmp/cmdr-smb.lock" {
		t.Fatalf("SMB lock path moved to %q", got)
	}
	if got := SMB.LeaseDir(); got != "/tmp/cmdr-smb-leases" {
		t.Fatalf("SMB lease dir moved to %q", got)
	}
}

func TestUnknownModeIsRejected(t *testing.T) {
	withFakes(t)
	if _, err := SMB.Acquire("manual", "not-a-mode"); err == nil {
		t.Fatal("an unknown mode must be rejected rather than silently served the default service set")
	}
}
