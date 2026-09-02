// Package stacklease is the machine-wide lease + lock that makes a shared
// Docker fixture stack safe to share across concurrent agent sessions in
// different git worktrees.
//
// # Why this exists
//
// Each fixture stack runs under a single Docker Compose project name on a fixed
// host-port range, so every worktree's `check.sh`, `start.sh`, and
// `e2e-linux.sh` resolve to the *same* containers. Before this package, any one
// session's teardown (`stop.sh`'s `down`, the orchestrator's deferred `Stop`,
// `e2e-linux.sh`'s conditional `down`) nuked the shared stack out from under a
// live suite in another worktree, producing "Cannot reach smb-consumer-X"
// cascades. And a second session bringing the stack up with slightly different
// config could `--force-recreate` the running containers mid-run.
//
// The fix: a machine-wide flock guards an *adopt-or-start* bring-up and a
// *refcounted, lock-held* teardown so a stack only goes down when its last user
// leaves.
//
// # One lease namespace per stack
//
// Every policy below is per Stack: its own lock file, its own lease dir, its own
// compose project. Two stacks therefore never see each other's holders, and
// downing one at zero can't touch the other. `registry.go` holds the registered
// stacks; a Stack is a value, so adding a protocol is data rather than a second
// copy of this file.
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
// Acquire is idempotent per holder-id per stack: re-acquiring an id that already
// holds a lease on that stack is a no-op rewrite, not a second refcount. This
// lets e2e-linux.sh's own $$ lease and the child start.sh's "manual" lease
// coexist as two distinct holders without double-counting, and lets the runner
// hold one lease per stack under a single PID.
package stacklease

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// ManualHolder is the sentinel lease the dead-PID sweep never reaps. It's
// non-numeric, so the numeric-PID sweep skips it by construction.
const ManualHolder = "manual"

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
	// running-without-healthcheck as acceptable for adoption.
	Status() (running map[string]bool, healthy map[string]bool, err error)
	// Up brings the named services up (idempotent reconcile). Empty slice means
	// "all defined services" per compose semantics. Holds no lock itself; the
	// caller owns the flock.
	Up(services []string) error
	// Down tears the whole project down.
	Down() error
	// Restart stops and starts the named services, so each container's
	// entrypoint runs again. The verb exists for one reason: re-running an
	// entrypoint is the only way to refill key material that vanished from the
	// host side of a bind mount.
	Restart(services []string) error
	// RunningServices returns the list of services currently in the project
	// (running), used for the all-services adoption decision when the requested
	// set is "all".
	RunningServices() ([]string, error)
}

// Logf is the package's diagnostic sink. Defaults to stderr; the CLI main and
// tests can redirect it. We log loudly on every non-trivial decision so a human
// reading a leaked-stack situation can reconstruct what happened.
var Logf = func(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "[stacklease] "+format+"\n", args...)
}

// newComposer is overridable in tests to inject a fake Composer.
var newComposer = func(s *Stack) Composer { return &dockerComposer{stack: s} }

// AcquireResult is what Acquire reports back to the caller.
type AcquireResult struct {
	Action Action
	// Services is the concrete service set the caller should probe for serving
	// readiness (resolved from the mode). For ActionReconcile the caller may
	// skip its own `up` because Acquire already ran it; the field is for the
	// post-lock probe loop in either case.
	Services []string
}

// Acquire registers holderID as a live user of this stack and decides whether
// the caller should adopt the already-serving stack or reconcile it via
// `up -d`. The entire critical section runs under the held flock; the lock is
// released before returning so the caller's TCP/health probe runs lock-free.
//
// holderID is "manual" for bare start.sh, the e2e-linux.sh shell PID, or the
// orchestrator's check.sh PID.
func (s *Stack) Acquire(holderID, mode string) (AcquireResult, error) {
	if err := validateHolderID(holderID); err != nil {
		return AcquireResult{}, err
	}
	if err := s.validateMode(mode); err != nil {
		return AcquireResult{}, err
	}
	// Before the hash and before any compose call: the bind source has to exist,
	// and the resolved path has to be in the env both the hash and compose read.
	if err := s.EnsureKeysDir(); err != nil {
		return AcquireResult{}, err
	}
	lock, err := acquireLock(s.LockPath())
	if err != nil {
		return AcquireResult{}, err
	}
	defer lock.release()

	if err := os.MkdirAll(s.LeaseDir(), 0o755); err != nil {
		return AcquireResult{}, fmt.Errorf("create lease dir: %w", err)
	}

	// 1. Sweep dead numeric-PID leases. ONLY here, under the lock. The "manual"
	//    sentinel is non-numeric → never swept.
	s.sweepDeadLeases()

	// 2. Write own lease (idempotent rewrite per holder-id).
	if err := s.writeLease(holderID, mode); err != nil {
		return AcquireResult{}, fmt.Errorf("write lease %q: %w", holderID, err)
	}

	// "Other leases" excludes self because we just wrote our own.
	otherLeases := s.otherLeaseCount(holderID)

	// 3. Inspect the running project and apply the adopt-vs-reconcile policy.
	composer := newComposer(s)
	services := s.resolveServices(composer, mode)
	action := s.decideAction(composer, services, mode, otherLeases)

	if action == ActionReconcile {
		if err := composer.Up(s.modeServicesFor(mode)); err != nil {
			// Reconcile failed: we still hold a lease and the stack is in
			// whatever state it was. Surface the error; the caller decides
			// whether to abort. We do NOT remove our lease here — a half-up
			// stack with our lease present is the safe direction (next acquire
			// reconciles again; release only downs at zero).
			return AcquireResult{}, fmt.Errorf("compose up (reconcile, %s mode %s): %w", s.Name, mode, err)
		}
		// Stamp the config we just brought up so a later adopter compares
		// against it rather than re-reconciling.
		s.writeConfigHash(mode)
	}

	// 4. Whatever the action was, the key material the suite reads has to be on
	//    disk. Adoption is the case that needs this: a stack can be running,
	//    healthy, and hash-matching with its published keys long gone.
	if err := s.healKeyMaterial(composer, services, action == ActionReconcile); err != nil {
		return AcquireResult{}, err
	}

	return AcquireResult{Action: action, Services: services}, nil
}

// keyMaterialDeadline bounds the wait for a container to publish its pair, and
// keyMaterialPoll is the probe interval inside that wait. Generous rather than
// tight: the deadline is never reached on the happy path, and the alternative to
// waiting is a suite that races the regeneration. A var so a test can shorten
// it (`withKeyMaterialDeadline`).
var keyMaterialDeadline = 90 * time.Second

const keyMaterialPoll = 100 * time.Millisecond

// healKeyMaterial republishes key material that went missing from the host while
// the containers kept running, and reports rather than returning to a caller
// whose key-auth cells would all fail.
//
// ❗ Restarting is the whole mechanism: the container's entrypoint regenerates
// the pair and rewrites its own `authorized_keys` from it, which is the only
// thing that can put the two halves back in agreement. `up -d` won't do it (it
// never touches a healthy container) and neither will anything on the host,
// which has no way to add a public key to a running sshd's account.
func (s *Stack) healKeyMaterial(c Composer, requested []string, broughtUp bool) error {
	if len(s.servicesMissingKeyMaterial(requested)) == 0 {
		return nil
	}
	// A stack this call just brought up is still writing. `up -d` returns when a
	// container reaches "running", which is well before its entrypoint has
	// generated anything, so reaching for a restart here would bounce a
	// perfectly healthy container that was seconds from publishing.
	if broughtUp {
		if err := s.waitForKeyMaterial(requested); err == nil {
			return nil
		}
	}
	gaps := s.servicesMissingKeyMaterial(requested)
	Logf("WARN: %s has no published key material for %s under %s; restarting so each entrypoint regenerates the pair its authorized_keys names",
		s.Name, strings.Join(gaps, ", "), s.KeysDir())
	if err := c.Restart(gaps); err != nil {
		return fmt.Errorf("restart %s service(s) with no key material (%s): %w", s.Name, strings.Join(gaps, ", "), err)
	}
	if err := s.waitForKeyMaterial(requested); err != nil {
		return err
	}
	Logf("%s republished key material for %s", s.Name, strings.Join(gaps, ", "))
	return nil
}

// waitForKeyMaterial polls until every requested leaf holds its private key, so
// a suite never races a container that is still generating one.
func (s *Stack) waitForKeyMaterial(requested []string) error {
	deadline := time.Now().Add(keyMaterialDeadline)
	for {
		still := s.servicesMissingKeyMaterial(requested)
		if len(still) == 0 {
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("%s publishes no private key for %s after %s; every key-auth cell would fail an auth rung against a server whose authorized_keys names a key nothing can read. The usual cause is a container running an image older than the entrypoint that knows how to republish: stop.sh then start.sh rebuilds it",
				s.Name, strings.Join(still, ", "), keyMaterialDeadline)
		}
		time.Sleep(keyMaterialPoll)
	}
}

// decideAction implements the adopt-vs-reconcile policy table under the held
// lock. otherLeases is the count of leases NOT belonging to the caller.
func (s *Stack) decideAction(c Composer, services []string, mode string, otherLeases int) Action {
	running, healthy, err := c.Status()
	if err != nil {
		// Can't read the running project. Reconcile is the safe-but-active
		// choice ONLY if nobody else holds a lease; under a foreign lease we
		// must never recreate, so adopt-and-warn. With no other lease, `up -d`
		// is harmless.
		if otherLeases > 0 {
			Logf("WARN: cannot inspect the running %s stack (%v) but a foreign lease is live; adopting without a compose call to avoid disturbing it", s.Name, err)
			return ActionAdopt
		}
		Logf("WARN: cannot inspect the running %s stack (%v); reconciling via up -d (no other leases)", s.Name, err)
		return ActionReconcile
	}

	allServing := s.allServicesServing(services, running, healthy)
	hashMatches := s.configHashMatches(mode)

	switch {
	case allServing && hashMatches:
		// All requested services healthy + config matches → adopt, no compose call.
		Logf("adopt %s: all %d requested service(s) serving, config hash matches", s.Name, len(services))
		return ActionAdopt
	case allServing && !hashMatches && otherLeases > 0:
		// Hash mismatch under a foreign live lease → adopt ANYWAY + WARN. The
		// running config is the first-comer's. NEVER force-recreate here.
		Logf("WARN: %s config hash differs from the running stack but a foreign lease is live (%d other holder(s)); adopting the running config rather than recreating under a live run", s.Name, otherLeases)
		return ActionAdopt
	case allServing && !hashMatches && otherLeases == 0:
		// Hash mismatch, only self → reconcile is safe.
		Logf("%s config hash differs and no other leases; reconciling via up -d to apply this session's config", s.Name)
		return ActionReconcile
	default:
		// Partially up / unhealthy → reconcile (brings missing/sick up without
		// disturbing healthy ones). Safe regardless of other leases: `up -d` is
		// additive, never a recreate.
		missing := s.missingServices(services, running, healthy)
		Logf("reconcile %s: stack partially up/unhealthy (missing-or-sick: %s); up -d", s.Name, strings.Join(missing, ", "))
		return ActionReconcile
	}
}

// Reconcile is the verb for e2e-linux.sh's "running but not serving" path. It
// must NOT blanket-`down` the shared stack: under the held lock it brings the
// requested services up (`up -d`, additive). If other leases are live, the
// stale-but-shared stack is the first-comer's to manage; we still run the
// idempotent `up -d` (which never recreates healthy containers) and let the
// standard probe retry.
func (s *Stack) Reconcile(mode string) error {
	if err := s.validateMode(mode); err != nil {
		return err
	}
	if err := s.EnsureKeysDir(); err != nil {
		return err
	}
	lock, err := acquireLock(s.LockPath())
	if err != nil {
		return err
	}
	defer lock.release()

	composer := newComposer(s)
	if err := composer.Up(s.modeServicesFor(mode)); err != nil {
		return fmt.Errorf("compose up (reconcile, %s mode %s): %w", s.Name, mode, err)
	}
	// Refresh the config-hash stamp so a later adopter compares against the
	// config we just reconciled toward.
	s.writeConfigHash(mode)
	if err := s.healKeyMaterial(composer, s.resolveServices(composer, mode), true); err != nil {
		return err
	}
	Logf("reconcile %s (mode %s): up -d issued (additive; no down, no force-recreate)", s.Name, mode)
	return nil
}

// Release removes holderID's lease on this stack and, ONLY if zero leases
// remain, downs it — with the lock still held. Any inconsistency leaves the
// stack UP.
func (s *Stack) Release(holderID string) error {
	if err := validateHolderID(holderID); err != nil {
		return err
	}
	lock, err := acquireLock(s.LockPath())
	if err != nil {
		return err
	}
	defer lock.release()

	// 1. Remove own lease.
	if err := s.removeLease(holderID); err != nil {
		// Couldn't remove our own lease → the count can't be trusted →
		// leave UP. This is the never-down-on-uncertainty rule.
		Logf("WARN: could not remove %s lease %q (%v); leaving the stack UP", s.Name, holderID, err)
		return nil
	}

	// 2. Re-verify the lease count under the lock.
	remaining, err := s.leaseCount()
	if err != nil {
		Logf("WARN: %s lease dir unreadable during release (%v); leaving the stack UP", s.Name, err)
		return nil
	}
	if remaining > 0 {
		Logf("release %s/%q: %d lease(s) still held; leaving the stack UP", s.Name, holderID, remaining)
		return nil
	}

	// 3. Zero leases → down, with the lock STILL HELD (an arriving acquirer
	//    blocks on the lock until the down finishes, then starts fresh).
	Logf("release %s/%q: last lease gone; tearing the stack down (compose down)", s.Name, holderID)
	composer := newComposer(s)
	if err := composer.Down(); err != nil {
		// Down errored → inconsistency → leave UP, don't pretend it's gone.
		Logf("WARN: %s compose down failed (%v); the stack may still be up — clean up manually with `docker compose -p %s down`", s.Name, err, s.ProjectName)
		return nil
	}
	// Down succeeded → the config-hash stamp is stale; drop it.
	_ = os.Remove(s.configHashPath())
	return nil
}

// PrintStatus prints this stack's lease state and running project state. Used by
// the CLI `status` verb and the contention script for assertions.
func (s *Stack) PrintStatus() error {
	lock, err := acquireLock(s.LockPath())
	if err != nil {
		return err
	}
	defer lock.release()

	holders, err := s.listLeaseHolders()
	if err != nil {
		return fmt.Errorf("read %s lease dir: %w", s.Name, err)
	}
	fmt.Printf("%s leases (%d):\n", s.Name, len(holders))
	for _, h := range holders {
		content, _ := os.ReadFile(filepath.Join(s.LeaseDir(), h))
		fmt.Printf("  %s\t%s\n", h, strings.TrimSpace(string(content)))
	}
	composer := newComposer(s)
	running, healthy, err := composer.Status()
	if err != nil {
		fmt.Printf("%s stack: unreadable (%v)\n", s.Name, err)
		return nil
	}
	fmt.Printf("%s running services (%d): %s\n", s.Name, len(running), strings.Join(sortedSet(running), ", "))
	fmt.Printf("%s healthy services (%d): %s\n", s.Name, len(healthy), strings.Join(sortedSet(healthy), ", "))
	return nil
}

func sortedSet(m map[string]bool) []string {
	names := make([]string, 0, len(m))
	for s := range m {
		names = append(names, s)
	}
	sort.Strings(names)
	return names
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

// validateMode refuses a mode the stack doesn't define. A silent fallback to the
// default service set would bring up the wrong containers and then wait for
// services the caller never asked for.
func (s *Stack) validateMode(mode string) error {
	if _, ok := s.modeServices[mode]; ok {
		return nil
	}
	return fmt.Errorf("stack %q has no mode %q; it serves %s", s.Name, mode, strings.Join(s.Modes(), ", "))
}

// modeServicesFor is the exact service set for a mode. nil means "every service
// the project defines".
func (s *Stack) modeServicesFor(mode string) []string {
	return s.modeServices[mode]
}

func (s *Stack) writeLease(holderID, mode string) error {
	body := fmt.Sprintf("stack=%s\nmode=%s\nwhen=%s\nwd=%s\n", s.Name, mode, time.Now().Format(time.RFC3339), workingDir())
	return os.WriteFile(filepath.Join(s.LeaseDir(), holderID), []byte(body), 0o644)
}

func (s *Stack) removeLease(holderID string) error {
	err := os.Remove(filepath.Join(s.LeaseDir(), holderID))
	if os.IsNotExist(err) {
		return nil // already gone; idempotent
	}
	return err
}

// sweepDeadLeases removes numeric-PID lease files whose process is gone. The
// "manual" sentinel and any non-numeric holder-id are skipped by construction.
// Called ONLY under the acquire lock — never on a timer.
func (s *Stack) sweepDeadLeases() {
	holders, err := s.listLeaseHolders()
	if err != nil {
		Logf("WARN: %s lease dir unreadable during sweep (%v); skipping sweep", s.Name, err)
		return
	}
	for _, h := range holders {
		pid, err := strconv.Atoi(h)
		if err != nil {
			continue // non-numeric (e.g. "manual") → never swept
		}
		if !processAlive(pid) {
			if rmErr := os.Remove(filepath.Join(s.LeaseDir(), h)); rmErr == nil {
				Logf("swept dead %s lease %d (process gone)", s.Name, pid)
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

func (s *Stack) listLeaseHolders() ([]string, error) {
	entries, err := os.ReadDir(s.LeaseDir())
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

func (s *Stack) leaseCount() (int, error) {
	holders, err := s.listLeaseHolders()
	if err != nil {
		return 0, err
	}
	return len(holders), nil
}

func (s *Stack) otherLeaseCount(self string) int {
	holders, err := s.listLeaseHolders()
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

// resolveServices returns the concrete service set for a mode. For an
// all-services mode it asks the composer for the running set (falling back to
// the empty slice if it can't, which the probe loop treats as "all defined").
func (s *Stack) resolveServices(c Composer, mode string) []string {
	if svcs := s.modeServicesFor(mode); svcs != nil {
		return svcs
	}
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
func (s *Stack) allServicesServing(services []string, running, healthy map[string]bool) bool {
	if len(services) == 0 {
		// An all-services mode with no resolvable set → can't claim all-serving.
		return false
	}
	for _, svc := range services {
		if !running[svc] {
			return false
		}
		if s.servicesWithoutHealthcheck[svc] {
			continue // running is the best we can assert
		}
		if !healthy[svc] {
			return false
		}
	}
	return true
}

func (s *Stack) missingServices(services []string, running, healthy map[string]bool) []string {
	var out []string
	for _, svc := range services {
		switch {
		case !running[svc]:
			out = append(out, svc)
		case !s.servicesWithoutHealthcheck[svc] && !healthy[svc]:
			out = append(out, svc+"(unhealthy)")
		}
	}
	return out
}

// ---- config-hash ----
//
// The hash captures the merged compose inputs so a later adopter can tell
// whether the running stack matches this session's config. We hash the stack's
// compose files, the resolved service set, and the stack's port env. We stamp it
// to a file next to the lock on `up` (writeConfigHash) and compare at adopt time
// (configHashMatches) — simpler and more reliable than round-tripping a compose
// label.

func (s *Stack) configHashPath() string {
	return s.LockPath() + ".confighash"
}

// repoRoot is the in-process override for compose-dir resolution. The check
// runner sets it once so every stack resolves against the repo it's checking,
// independent of the orchestrator's cwd.
var repoRoot string

// SetRepoRoot points compose-dir resolution at a known repo root.
func SetRepoRoot(dir string) { repoRoot = dir }

// composeDir resolves this stack's compose directory: the stack's own env
// override first, then the repo root the runner set, then a best-effort walk up
// from cwd. Returns "" when none of them find it, which `Up` reports rather than
// falling back to docker's default file lookup.
func (s *Stack) composeDir() string {
	if d := os.Getenv(s.composeDirEnv); d != "" {
		return d
	}
	rel := filepath.FromSlash(s.composeDirRel)
	if repoRoot != "" {
		if candidate := filepath.Join(repoRoot, rel); isDir(candidate) {
			return candidate
		}
	}
	wd, err := os.Getwd()
	if err != nil {
		return ""
	}
	for dir := wd; ; {
		if candidate := filepath.Join(dir, rel); isDir(candidate) {
			return candidate
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}
		dir = parent
	}
}

func isDir(path string) bool {
	st, err := os.Stat(path)
	if err != nil || st == nil {
		return false
	}
	return st.IsDir()
}

func (s *Stack) computeConfigHash(mode string) string {
	h := sha256.New()
	cd := s.composeDir()
	for _, f := range s.composeFiles {
		if cd != "" {
			if b, err := os.ReadFile(filepath.Join(cd, f)); err == nil {
				h.Write(b)
			}
		}
	}
	fmt.Fprintf(h, "stack=%s\nmode=%s\n", s.Name, mode)
	for _, svc := range s.modeServicesFor(mode) {
		fmt.Fprintf(h, "svc=%s\n", svc)
	}
	// Port env: the one config dimension that genuinely changes container
	// bindings across worktrees/sessions.
	var ports []string
	for _, kv := range os.Environ() {
		if strings.HasPrefix(kv, s.portEnvPrefix) && strings.Contains(kv, "_PORT=") {
			ports = append(ports, kv)
		}
	}
	sort.Strings(ports)
	for _, kv := range ports {
		fmt.Fprintf(h, "%s\n", kv)
	}
	// A first-party image: its build context decides what the containers RUN, so
	// an edited entrypoint has to read as staleness the same way an edited
	// compose file does. Nothing else would notice.
	//
	// ❗ The context's own name goes into the hash beside each file's, so two
	// contexts holding a same-named file (`Dockerfile`, say) can't cancel each
	// other out and leave an edit invisible.
	for _, ctx := range s.BuildContextDirs() {
		var files []string
		_ = filepath.WalkDir(ctx, func(path string, d fs.DirEntry, err error) error {
			if err != nil || d.IsDir() {
				return nil //nolint:nilerr // an unreadable entry just doesn't contribute
			}
			files = append(files, path)
			return nil
		})
		sort.Strings(files)
		for _, f := range files {
			rel, _ := filepath.Rel(ctx, f)
			fmt.Fprintf(h, "build=%s/%s\n", filepath.Base(ctx), filepath.ToSlash(rel))
			if b, err := os.ReadFile(f); err == nil {
				h.Write(b)
			}
		}
	}
	// The keys dir is a bind SOURCE, so a running stack that mounts a different
	// one is exactly as stale as one bound to different ports — and far quieter
	// about it, since the containers stay healthy while every key-auth cell
	// fails.
	if keys := s.KeysDir(); keys != "" {
		fmt.Fprintf(h, "keys=%s\n", keys)
	}
	return hex.EncodeToString(h.Sum(nil))
}

func (s *Stack) writeConfigHash(mode string) {
	if err := os.WriteFile(s.configHashPath(), []byte(s.computeConfigHash(mode)), 0o644); err != nil {
		Logf("WARN: could not stamp the %s config hash (%v); future adopters will treat config as mismatched", s.Name, err)
	}
}

// configHashMatches reports whether the stamped hash equals this session's
// computed hash. A missing stamp means "unknown" → treat as mismatch so the
// caller errs toward reconcile-when-safe / adopt-and-warn-under-foreign-lease.
func (s *Stack) configHashMatches(mode string) bool {
	stamped, err := os.ReadFile(s.configHashPath())
	if err != nil {
		return false
	}
	return strings.TrimSpace(string(stamped)) == s.computeConfigHash(mode)
}

func workingDir() string {
	wd, err := os.Getwd()
	if err != nil {
		return "?"
	}
	return wd
}
