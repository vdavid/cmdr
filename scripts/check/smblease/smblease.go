// Package smblease is the machine-wide lease + lock that makes the shared
// `smb-consumer` Docker stack safe to share across concurrent agent sessions
// in different git worktrees.
//
// # Why this exists
//
// cmdr's SMB fixture stack runs under a single Docker Compose project name
// (`smb-consumer`) on a fixed host-port range, so every worktree's `check.sh`,
// `start.sh`, and `e2e-linux.sh` resolve to the *same* containers. Before this
// package, any one session's teardown (`stop.sh`'s `down`, the orchestrator's
// deferred `Stop`, `e2e-linux.sh`'s conditional `down`) nuked the shared stack
// out from under a live suite in another worktree, producing
// "Cannot reach smb-consumer-X" cascades. And a second session bringing the
// stack up with slightly different config could `--force-recreate` the running
// containers mid-run.
//
// The fix: a machine-wide flock guards an *adopt-or-start* bring-up and a
// *refcounted, lock-held* teardown so the stack only goes down when the last
// user leaves.
//
// # The asymmetry (read before touching Release)
//
// The whole design hinges on degrading to "leave it UP" on any doubt, never to
// "tear it down". A leaked stack costs a human one `stop.sh`; a premature
// teardown re-breaks a live run. So:
//
//   - Teardown re-verifies the lease count under the lock and only downs at
//     ZERO. Any inconsistency → log + leave UP.
//   - Dead-PID leases are swept ONLY on acquire, never on a timer. A background
//     reaper would race a just-started suite whose lease file exists but whose
//     process hasn't been observed alive yet.
//   - The lock is HELD ACROSS the `compose down`. Releasing before the down
//     reopens the teardown race: an arriving acquirer would see zero leases,
//     start a fresh `up` while the old `down` is mid-flight, and get
//     half-torn-down containers.
//
// # Holder model
//
// Acquire takes an explicit holder-id, NOT always self-pid, because the
// standalone callers don't outlive their bring-up:
//
//   - `start.sh` (manual / default) uses the sentinel "manual" lease that the
//     dead-PID sweep NEVER reaps; only `stop.sh` (or `--force`) removes it. A
//     forgotten manual lease lingers — the benign direction.
//   - `e2e-linux.sh` uses its own long-lived shell PID ($$), acquired at
//     bring-up and released on EXIT.
//   - The orchestrator uses its `check.sh` PID (long-lived for the whole run),
//     calling into this package in-process.
//
// Acquire is idempotent per holder-id: re-acquiring an id that already holds a
// lease is a no-op rewrite, not a second refcount. This lets e2e-linux.sh's own
// $$ lease and the child start.sh's "manual" lease coexist as two distinct
// holders without double-counting.
package smblease

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const (
	// defaultLockPath is the flock target: a stable inode held for the full
	// acquire-or-release critical section. Separate from the lease dir so flock
	// targets a fixed file. `/tmp` (not `$TMPDIR`) deliberately: we want one
	// shared namespace across all of a user's worktrees, not a per-shell one.
	defaultLockPath = "/tmp/cmdr-smb.lock"
	// defaultLeaseDir holds one file per live holder. World-traversable,
	// predictable, survives across worktrees.
	defaultLeaseDir = "/tmp/cmdr-smb-leases"
	// ManualHolder is the sentinel lease the dead-PID sweep never reaps. It's
	// non-numeric, so the numeric-PID sweep skips it by construction.
	ManualHolder = "manual"
	// ProjectName is the shared Docker Compose project all callers share.
	ProjectName = "smb-consumer"
)

// LockPath and LeaseDir resolve the lock file and lease directory. They read
// CMDR_SMB_LEASE_ROOT first (tests point it at a tmpdir to isolate from the real
// machine-wide paths); production callers leave it unset and get the fixed
// /tmp paths that all worktrees share.
func LockPath() string {
	if root := os.Getenv("CMDR_SMB_LEASE_ROOT"); root != "" {
		return filepath.Join(root, "cmdr-smb.lock")
	}
	return defaultLockPath
}

func LeaseDir() string {
	if root := os.Getenv("CMDR_SMB_LEASE_ROOT"); root != "" {
		return filepath.Join(root, "cmdr-smb-leases")
	}
	return defaultLeaseDir
}

// Action is what the caller should do after Acquire returns. The lib decides;
// the bash callers act on the matching exit code, the orchestrator branches on
// the value directly.
type Action int

const (
	// ActionAdopt: the stack is already serving the requested services with a
	// matching config — do NOT issue any compose call. (Still probe for serving
	// readiness afterward; that happens outside the held lock.)
	ActionAdopt Action = iota
	// ActionReconcile: bring the stack up with `up -d` (idempotent — starts
	// missing/sick services without disturbing healthy ones).
	ActionReconcile
)

func (a Action) String() string {
	switch a {
	case ActionAdopt:
		return "adopt"
	case ActionReconcile:
		return "reconcile"
	default:
		return "unknown"
	}
}

// Composer abstracts the docker-compose interactions so the lease/lock logic is
// testable without a real Docker daemon. The real implementation lives in
// compose.go; tests inject a fake.
type Composer interface {
	// Status returns, for the project, the set of services that are running and
	// the set that are healthy (a subset of running). Services with no
	// healthcheck count as "running but not healthy" — the caller treats
	// running-without-healthcheck as acceptable for adoption (see note below).
	Status() (running map[string]bool, healthy map[string]bool, err error)
	// Up brings the named services up (idempotent reconcile). Empty slice means
	// "all defined services" per compose semantics. Holds no lock itself; the
	// caller owns the flock.
	Up(services []string) error
	// Down tears the whole project down.
	Down() error
	// RunningServices returns the list of services currently in the project
	// (running), used for the all-services adoption decision when the requested
	// set is "all".
	RunningServices() ([]string, error)
}

// modeServices maps an SMB mode to the exact service set start.sh brings up for
// it. Kept in lock-step with apps/desktop/test/smb-servers/start.sh — if the
// modes there change, change them here. "all" returns nil, meaning "every
// service the project defines"; the caller resolves the concrete set at runtime.
func modeServices(mode string) []string {
	switch mode {
	case "minimal":
		return []string{"smb-consumer-guest", "smb-consumer-auth"}
	case "e2e":
		return []string{
			"smb-consumer-guest", "smb-consumer-auth",
			"smb-consumer-50shares", "smb-consumer-unicode",
		}
	case "core":
		return []string{
			"smb-consumer-guest", "smb-consumer-auth", "smb-consumer-both",
			"smb-consumer-readonly", "smb-consumer-flaky", "smb-consumer-slow",
			"smb-consumer-maxreadsize", "smb-consumer-50shares",
		}
	case "all":
		return nil
	default:
		// Unknown mode: treat like core (the start.sh default), but the bash
		// caller validates mode before reaching us, so this is defensive only.
		return []string{
			"smb-consumer-guest", "smb-consumer-auth", "smb-consumer-both",
			"smb-consumer-readonly", "smb-consumer-flaky", "smb-consumer-slow",
			"smb-consumer-maxreadsize", "smb-consumer-50shares",
		}
	}
}

// servicesWithoutHealthcheck is the set of services that intentionally ship no
// HEALTHCHECK, so adoption must NOT require them healthy — only running.
// `smb-consumer-flaky` cycles up/down by design (no healthcheck); every other
// service bakes `HEALTHCHECK nc -z localhost 445`.
var servicesWithoutHealthcheck = map[string]bool{
	"smb-consumer-flaky": true,
}

// Logf is the package's diagnostic sink. Defaults to stderr; the CLI main and
// tests can redirect it. We log loudly on every non-trivial decision so a human
// reading a leaked-stack situation can reconstruct what happened.
var Logf = func(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "[smblease] "+format+"\n", args...)
}

// newComposer is overridable in tests to inject a fake Composer.
var newComposer = func() Composer { return &dockerComposer{} }

// AcquireResult is what Acquire reports back to the caller.
type AcquireResult struct {
	Action Action
	// Services is the concrete service set the caller should probe for serving
	// readiness (resolved from the mode). For ActionReconcile the caller may
	// skip its own `up` because Acquire already ran it; the field is for the
	// post-lock probe loop in either case.
	Services []string
}

// Acquire registers holderID as a live user of the stack and decides whether
// the caller should adopt the already-serving stack or reconcile it via
// `up -d`. The entire critical section runs under the held flock; the lock is
// released before returning so the caller's TCP/health probe runs lock-free.
//
// holderID is "manual" for bare start.sh, the e2e-linux.sh shell PID, or the
// orchestrator's check.sh PID. mode is one of minimal|e2e|core|all.
func Acquire(holderID, mode string) (AcquireResult, error) {
	if err := validateHolderID(holderID); err != nil {
		return AcquireResult{}, err
	}
	lock, err := acquireLock()
	if err != nil {
		return AcquireResult{}, err
	}
	defer lock.release()

	if err := os.MkdirAll(LeaseDir(), 0o755); err != nil {
		return AcquireResult{}, fmt.Errorf("create lease dir: %w", err)
	}

	// 1. Sweep dead numeric-PID leases. ONLY here, under the lock. The "manual"
	//    sentinel is non-numeric → never swept.
	sweepDeadLeases()

	// 2. Write own lease (idempotent rewrite per holder-id).
	if err := writeLease(holderID, mode); err != nil {
		return AcquireResult{}, fmt.Errorf("write lease %q: %w", holderID, err)
	}

	// "Other leases" excludes self because we just wrote our own.
	otherLeases := otherLeaseCount(holderID)

	// 3. Inspect the running project and apply the adopt-vs-reconcile policy.
	composer := newComposer()
	services := resolveServices(composer, mode)
	action := decideAction(composer, services, mode, otherLeases)

	if action == ActionReconcile {
		if err := composer.Up(modeServices(mode)); err != nil {
			// Reconcile failed: we still hold a lease and the stack is in
			// whatever state it was. Surface the error; the caller decides
			// whether to abort. We do NOT remove our lease here — a half-up
			// stack with our lease present is the safe direction (next acquire
			// reconciles again; release only downs at zero).
			return AcquireResult{}, fmt.Errorf("compose up (reconcile, mode %s): %w", mode, err)
		}
		// Stamp the config we just brought up so a later adopter compares
		// against it rather than re-reconciling.
		writeConfigHash(mode)
	}

	return AcquireResult{Action: action, Services: services}, nil
}

// decideAction implements the adopt-vs-reconcile policy table under the held
// lock. otherLeases is the count of leases NOT belonging to the caller.
func decideAction(c Composer, services []string, mode string, otherLeases int) Action {
	running, healthy, err := c.Status()
	if err != nil {
		// Can't read the running project. Reconcile is the safe-but-active
		// choice ONLY if nobody else holds a lease; under a foreign lease we
		// must never recreate, so adopt-and-warn. With no other lease, `up -d`
		// is harmless.
		if otherLeases > 0 {
			Logf("WARN: cannot inspect running stack (%v) but a foreign lease is live; adopting without a compose call to avoid disturbing it", err)
			return ActionAdopt
		}
		Logf("WARN: cannot inspect running stack (%v); reconciling via up -d (no other leases)", err)
		return ActionReconcile
	}

	allServing := allServicesServing(services, running, healthy)
	hashMatches := configHashMatches(mode)

	switch {
	case allServing && hashMatches:
		// All requested services healthy + config matches → adopt, no compose call.
		Logf("adopt: all %d requested service(s) serving, config hash matches", len(services))
		return ActionAdopt
	case allServing && !hashMatches && otherLeases > 0:
		// Hash mismatch under a foreign live lease → adopt ANYWAY + WARN. The
		// running config is the first-comer's. NEVER force-recreate here.
		Logf("WARN: config hash differs from the running stack but a foreign lease is live (%d other holder(s)); adopting the running config rather than recreating under a live run", otherLeases)
		return ActionAdopt
	case allServing && !hashMatches && otherLeases == 0:
		// Hash mismatch, only self → reconcile is safe.
		Logf("config hash differs and no other leases; reconciling via up -d to apply this session's config")
		return ActionReconcile
	default:
		// Partially up / unhealthy → reconcile (brings missing/sick up without
		// disturbing healthy ones). Safe regardless of other leases: `up -d` is
		// additive, never a recreate.
		missing := missingServices(services, running, healthy)
		Logf("reconcile: stack partially up/unhealthy (missing-or-sick: %s); up -d", strings.Join(missing, ", "))
		return ActionReconcile
	}
}

// Reconcile is the verb for e2e-linux.sh's "running but not serving" path. It
// must NOT blanket-`down` the shared stack: under the held lock it brings the
// requested services up (`up -d`, additive). If other leases are live, the
// stale-but-shared stack is the first-comer's to manage; we still run the
// idempotent `up -d` (which never recreates healthy containers) and let the
// standard probe retry.
func Reconcile(mode string) error {
	lock, err := acquireLock()
	if err != nil {
		return err
	}
	defer lock.release()

	composer := newComposer()
	if err := composer.Up(modeServices(mode)); err != nil {
		return fmt.Errorf("compose up (reconcile, mode %s): %w", mode, err)
	}
	// Refresh the config-hash stamp so a later adopter compares against the
	// config we just reconciled toward.
	writeConfigHash(mode)
	Logf("reconcile (mode %s): up -d issued (additive; no down, no force-recreate)", mode)
	return nil
}

// Release removes holderID's lease and, ONLY if zero leases remain, downs the
// stack — with the lock still held. Any inconsistency leaves the stack UP.
func Release(holderID string) error {
	if err := validateHolderID(holderID); err != nil {
		return err
	}
	lock, err := acquireLock()
	if err != nil {
		return err
	}
	defer lock.release()

	// 1. Remove own lease.
	if err := removeLease(holderID); err != nil {
		// Couldn't remove our own lease → the count can't be trusted →
		// leave UP. This is the never-down-on-uncertainty rule.
		Logf("WARN: could not remove lease %q (%v); leaving the stack UP", holderID, err)
		return nil
	}

	// 2. Re-verify the lease count under the lock.
	remaining, err := leaseCount()
	if err != nil {
		Logf("WARN: lease dir unreadable during release (%v); leaving the stack UP", err)
		return nil
	}
	if remaining > 0 {
		Logf("release %q: %d lease(s) still held; leaving the stack UP", holderID, remaining)
		return nil
	}

	// 3. Zero leases → down, with the lock STILL HELD (an arriving acquirer
	//    blocks on the lock until the down finishes, then starts fresh).
	Logf("release %q: last lease gone; tearing the stack down (compose down)", holderID)
	composer := newComposer()
	if err := composer.Down(); err != nil {
		// Down errored → inconsistency → leave UP, don't pretend it's gone.
		Logf("WARN: compose down failed (%v); the stack may still be up — clean up manually with `docker compose -p %s down`", err, ProjectName)
		return nil
	}
	// Down succeeded → the config-hash stamp is stale; drop it.
	_ = os.Remove(configHashPath())
	return nil
}

// Status prints the current lease state and the running project state. Used by
// the CLI `status` verb and the contention script for assertions.
func Status() error {
	lock, err := acquireLock()
	if err != nil {
		return err
	}
	defer lock.release()

	holders, err := listLeaseHolders()
	if err != nil {
		return fmt.Errorf("read lease dir: %w", err)
	}
	fmt.Printf("leases (%d):\n", len(holders))
	for _, h := range holders {
		content, _ := os.ReadFile(filepath.Join(LeaseDir(), h))
		fmt.Printf("  %s\t%s\n", h, strings.TrimSpace(string(content)))
	}
	composer := newComposer()
	running, healthy, err := composer.Status()
	if err != nil {
		fmt.Printf("stack: unreadable (%v)\n", err)
		return nil
	}
	names := make([]string, 0, len(running))
	for s := range running {
		names = append(names, s)
	}
	sort.Strings(names)
	fmt.Printf("running services (%d): %s\n", len(names), strings.Join(names, ", "))
	healthyNames := make([]string, 0, len(healthy))
	for s := range healthy {
		healthyNames = append(healthyNames, s)
	}
	sort.Strings(healthyNames)
	fmt.Printf("healthy services (%d): %s\n", len(healthyNames), strings.Join(healthyNames, ", "))
	return nil
}

// ---- lease-file helpers (all callers hold the flock) ----

func validateHolderID(holderID string) error {
	if holderID == "" {
		return fmt.Errorf("holder-id must not be empty")
	}
	// A holder-id becomes a filename in LeaseDir; reject path separators so a
	// caller can't escape the dir.
	if strings.ContainsAny(holderID, "/\\") || holderID == "." || holderID == ".." {
		return fmt.Errorf("invalid holder-id %q", holderID)
	}
	return nil
}

func writeLease(holderID, mode string) error {
	body := fmt.Sprintf("mode=%s\nwhen=%s\nwd=%s\n", mode, time.Now().Format(time.RFC3339), workingDir())
	return os.WriteFile(filepath.Join(LeaseDir(), holderID), []byte(body), 0o644)
}

func removeLease(holderID string) error {
	err := os.Remove(filepath.Join(LeaseDir(), holderID))
	if os.IsNotExist(err) {
		return nil // already gone; idempotent
	}
	return err
}

// sweepDeadLeases removes numeric-PID lease files whose process is gone. The
// "manual" sentinel and any non-numeric holder-id are skipped by construction.
// Called ONLY under the acquire lock — never on a timer.
func sweepDeadLeases() {
	holders, err := listLeaseHolders()
	if err != nil {
		Logf("WARN: lease dir unreadable during sweep (%v); skipping sweep", err)
		return
	}
	for _, h := range holders {
		pid, err := strconv.Atoi(h)
		if err != nil {
			continue // non-numeric (e.g. "manual") → never swept
		}
		if !processAlive(pid) {
			if rmErr := os.Remove(filepath.Join(LeaseDir(), h)); rmErr == nil {
				Logf("swept dead lease %d (process gone)", pid)
			}
		}
	}
}

// processAlive reports whether pid names a live process via kill(pid, 0).
// Accepts the PID-reuse caveat by design: a recycled PID reads as alive and
// won't be swept, lingering the stack a bit longer — the benign direction.
func processAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	// On Unix, FindProcess always succeeds; Signal(0) is the liveness probe.
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	err = proc.Signal(syscall.Signal(0))
	if err == nil {
		return true
	}
	// EPERM means the process exists but we can't signal it → still alive.
	return err == syscall.EPERM
}

func listLeaseHolders() ([]string, error) {
	entries, err := os.ReadDir(LeaseDir())
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []string
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		out = append(out, e.Name())
	}
	sort.Strings(out)
	return out, nil
}

func leaseCount() (int, error) {
	holders, err := listLeaseHolders()
	if err != nil {
		return 0, err
	}
	return len(holders), nil
}

func otherLeaseCount(self string) int {
	holders, err := listLeaseHolders()
	if err != nil {
		return 0
	}
	n := 0
	for _, h := range holders {
		if h != self {
			n++
		}
	}
	return n
}

// ---- service-state helpers ----

// resolveServices returns the concrete service set for a mode. For "all" it
// asks the composer for the running set (falling back to the empty slice if it
// can't, which the probe loop treats as "all defined").
func resolveServices(c Composer, mode string) []string {
	svcs := modeServices(mode)
	if svcs != nil {
		return svcs
	}
	// "all": resolve from the running project.
	running, err := c.RunningServices()
	if err != nil || len(running) == 0 {
		return nil
	}
	sort.Strings(running)
	return running
}

// allServicesServing reports whether every requested service is "serving":
// running, and healthy unless it's a no-healthcheck service (where running is
// the strongest signal available).
func allServicesServing(services []string, running, healthy map[string]bool) bool {
	if len(services) == 0 {
		// "all" with no resolvable set → can't claim all-serving.
		return false
	}
	for _, s := range services {
		if !running[s] {
			return false
		}
		if servicesWithoutHealthcheck[s] {
			continue // running is the best we can assert
		}
		if !healthy[s] {
			return false
		}
	}
	return true
}

func missingServices(services []string, running, healthy map[string]bool) []string {
	var out []string
	for _, s := range services {
		switch {
		case !running[s]:
			out = append(out, s)
		case !servicesWithoutHealthcheck[s] && !healthy[s]:
			out = append(out, s+"(unhealthy)")
		}
	}
	return out
}

// ---- config-hash ----
//
// The hash captures the merged compose inputs so a later adopter can tell
// whether the running stack matches this session's config. We hash the vendored
// compose file, the override, the resolved service set, and the
// SMB_CONSUMER_*_PORT env. We stamp it to a file next to the lock on `up`
// (writeConfigHash) and compare at adopt time (configHashMatches). This is the
// "hash file" fallback the plan allows — simpler and more reliable than
// round-tripping a compose label.

func configHashPath() string {
	return LockPath() + ".confighash"
}

// composeDir resolves the smb-servers/.compose directory from this package's
// known location relative to the repo. Callers run from scripts/check (bash
// `cd`s there; the orchestrator's cwd is the repo root). We resolve via the
// CMDR_SMB_COMPOSE_DIR override first, then a best-effort walk; if we can't find
// it, the hash falls back to env-only, which still distinguishes port configs.
func composeDir() string {
	if d := os.Getenv("CMDR_SMB_COMPOSE_DIR"); d != "" {
		return d
	}
	// Walk up from cwd looking for apps/desktop/test/smb-servers/.compose.
	wd, err := os.Getwd()
	if err != nil {
		return ""
	}
	for dir := wd; ; {
		candidate := filepath.Join(dir, "apps", "desktop", "test", "smb-servers", ".compose")
		if st, err := os.Stat(candidate); err == nil && st.IsDir() {
			return candidate
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}
		dir = parent
	}
}

func computeConfigHash(mode string) string {
	h := sha256.New()
	cd := composeDir()
	for _, f := range []string{"docker-compose.yml", "docker-compose.override.yml"} {
		if cd != "" {
			if b, err := os.ReadFile(filepath.Join(cd, f)); err == nil {
				h.Write(b)
			}
		}
	}
	fmt.Fprintf(h, "mode=%s\n", mode)
	for _, s := range modeServices(mode) {
		fmt.Fprintf(h, "svc=%s\n", s)
	}
	// Port env: the one config dimension that genuinely changes container
	// bindings across worktrees/sessions.
	var ports []string
	for _, kv := range os.Environ() {
		if strings.HasPrefix(kv, "SMB_CONSUMER_") && strings.Contains(kv, "_PORT=") {
			ports = append(ports, kv)
		}
	}
	sort.Strings(ports)
	for _, kv := range ports {
		fmt.Fprintf(h, "%s\n", kv)
	}
	return hex.EncodeToString(h.Sum(nil))
}

func writeConfigHash(mode string) {
	if err := os.WriteFile(configHashPath(), []byte(computeConfigHash(mode)), 0o644); err != nil {
		Logf("WARN: could not stamp config hash (%v); future adopters will treat config as mismatched", err)
	}
}

// configHashMatches reports whether the stamped hash equals this session's
// computed hash. A missing stamp means "unknown" → treat as mismatch so the
// caller errs toward reconcile-when-safe / adopt-and-warn-under-foreign-lease.
func configHashMatches(mode string) bool {
	stamped, err := os.ReadFile(configHashPath())
	if err != nil {
		return false
	}
	return strings.TrimSpace(string(stamped)) == computeConfigHash(mode)
}

func workingDir() string {
	wd, err := os.Getwd()
	if err != nil {
		return "?"
	}
	return wd
}
