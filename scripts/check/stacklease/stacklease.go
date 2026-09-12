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
//
// # Module map
//
// This file holds the core: Acquire (adopt-or-reconcile), decideAction (the
// policy table), Reconcile, Release, PrintStatus, and service-set resolution.
// log.go holds the two log sinks (Logf/InfoLogf) and the OnReconcileStart/
// OnTeardown hooks; confighash.go the config-hash stamp/compare; leases.go the
// per-holder lease files and the dead-PID sweep; keymaterial.go the
// host-key-material heal/wait pair.
package stacklease

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
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
		OnReconcileStart(s.Name)
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
		InfoLogf("adopt %s: all %d requested service(s) serving, config hash matches", s.Name, len(services))
		return ActionAdopt
	case allServing && !hashMatches && otherLeases > 0:
		// Hash mismatch under a foreign live lease → adopt ANYWAY + WARN. The
		// running config is the first-comer's. NEVER force-recreate here.
		Logf("WARN: %s config hash differs from the running stack but a foreign lease is live (%d other holder(s)); adopting the running config rather than recreating under a live run", s.Name, otherLeases)
		return ActionAdopt
	case allServing && !hashMatches && otherLeases == 0:
		// Hash mismatch, only self → reconcile is safe.
		InfoLogf("%s config hash differs and no other leases; reconciling via up -d to apply this session's config", s.Name)
		return ActionReconcile
	default:
		// Partially up / unhealthy → reconcile (brings missing/sick up without
		// disturbing healthy ones). Safe regardless of other leases: `up -d` is
		// additive, never a recreate.
		missing := s.missingServices(services, running, healthy)
		InfoLogf("reconcile %s: stack partially up/unhealthy (missing-or-sick: %s); up -d", s.Name, strings.Join(missing, ", "))
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
	InfoLogf("reconcile %s (mode %s): up -d issued (additive; no down, no force-recreate)", s.Name, mode)
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
		InfoLogf("release %s/%q: %d lease(s) still held; leaving the stack UP", s.Name, holderID, remaining)
		return nil
	}

	// 3. Zero leases → down, with the lock STILL HELD (an arriving acquirer
	//    blocks on the lock until the down finishes, then starts fresh).
	OnTeardown(s.Name)
	InfoLogf("release %s/%q: last lease gone; tearing the stack down (compose down)", s.Name, holderID)
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
