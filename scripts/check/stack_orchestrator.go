package main

import (
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"
	"sync"

	"cmdr/scripts/check/checks"
	"cmdr/scripts/check/stacklease"
)

// StackOrchestrator manages the Docker Compose fixture stacks' lifecycle for all
// selected checks that declare a non-empty `NeedsContainers`.
//
// It coordinates at two levels:
//
//   - In-process (this orchestrator): keeps the start/stop count at exactly one
//     per stack + mode per check.sh run regardless of how many checks are
//     scheduled, via the `started` map. That solved intra-process contention.
//   - Machine-wide (the stacklease library): two check.sh processes in different
//     worktrees have independent orchestrators, so the in-process map can't stop
//     them from racing the same containers. The orchestrator therefore takes a
//     machine-wide PID-keyed lease (its own check.sh PID) per stack via
//     stacklease, which refcounts every concurrent session and only downs a
//     stack when the last one leaves. EnsureStarted acquires (adopt-or-reconcile);
//     Stop releases (down-at-zero). See scripts/check/stacklease for the model.
//
// Leases are per stack, so needing two stacks means two independent lease
// namespaces: downing one at zero can never touch the other.
//
// The standalone scripts (a fixture's start.sh, e2e-linux.sh::start_smb_containers)
// still work for manual / non-runner invocations: they take their OWN leases
// ("manual" for start.sh, $$ for e2e-linux.sh), so a manual run alongside a
// check.sh run just registers as a second holder and neither tears the other's
// stack down.
type StackOrchestrator struct {
	holderID string
	mu       sync.Mutex
	started  map[checks.StackMode]bool
	held     map[string]*stacklease.Stack
}

// portEnvAppliers pins a stack's host-port range in this process before bring-up,
// so compose and every check that talks to the fixture inherit it. A stack with
// no entry needs no pinning.
var portEnvAppliers = map[string]func(){
	"smb":    checks.ApplySmbPortEnv,
	"sftp":   checks.ApplySftpPortEnv,
	"webdav": checks.ApplyWebdavPortEnv,
}

// NewStackOrchestrator returns an orchestrator scoped to the given repo root. Its
// lease holder-id is this check.sh process's PID — long-lived for the whole run,
// so the dead-PID sweep keeps the lease only while the run is alive.
func NewStackOrchestrator(rootDir string) *StackOrchestrator {
	// Point stacklease's config-hash + compose-file resolution at this repo,
	// independent of the orchestrator's cwd.
	stacklease.SetRepoRoot(rootDir)
	return &StackOrchestrator{
		holderID: strconv.Itoa(os.Getpid()),
		started:  map[checks.StackMode]bool{},
		held:     map[string]*stacklease.Stack{},
	}
}

// collectStackModes returns the deduplicated, deterministically-ordered set of
// stack + mode pairs the given checks need.
func collectStackModes(defs []checks.CheckDefinition) []checks.StackMode {
	seen := map[checks.StackMode]bool{}
	var out []checks.StackMode
	for _, d := range defs {
		for _, want := range d.NeedsContainers {
			if seen[want] {
				continue
			}
			seen[want] = true
			out = append(out, want)
		}
	}
	// Stable order so logs are reproducible.
	sort.Slice(out, func(i, j int) bool { return out[i].String() < out[j].String() })
	return out
}

// EnsureStarted brings up the containers the given stack + mode pairs need by
// acquiring this run's machine-wide lease on each stack (which adopts an
// already-serving stack or reconciles it via `up -d` under the lock). Idempotent
// per pair. Returns nil if no pair was passed.
func (o *StackOrchestrator) EnsureStarted(wanted []checks.StackMode) error {
	o.mu.Lock()
	defer o.mu.Unlock()
	for _, want := range wanted {
		if o.started[want] {
			continue
		}
		stack, err := stacklease.Lookup(want.Stack)
		if err != nil {
			return fmt.Errorf("fixture orchestrator: %w", err)
		}
		// Pin the stack's host ports in this process before bringing it up, so
		// compose, every check, and the lease's config hash all see this run's
		// ports. See checks/smb_ports.go for why cmdr's SMB range is its own.
		if apply, ok := portEnvAppliers[stack.Name]; ok {
			apply()
		}
		fmt.Printf("📦 Ensuring %s Docker containers (%s) via lease %s...\n", stack.Name, want.Mode, o.holderID)
		res, err := stack.Acquire(o.holderID, want.Mode)
		if err != nil {
			return fmt.Errorf("fixture orchestrator: %s lease acquire (%s) failed: %w", stack.Name, want.Mode, err)
		}
		o.held[stack.Name] = stack
		fmt.Printf("   → %s (%d service(s))\n", res.Action, len(res.Services))
		o.started[want] = true
	}
	return nil
}

// Stop releases this run's lease on every stack it acquired. A shared stack is
// torn down only if no other session still holds a lease on it (down-at-zero,
// under the lock — see stacklease). Safe to call when nothing was started
// (no-op). Prints a friendly banner so the user knows cleanup is happening —
// relevant when Stop runs from a Ctrl+C handler.
func (o *StackOrchestrator) Stop() {
	o.mu.Lock()
	defer o.mu.Unlock()
	if len(o.held) == 0 {
		return
	}
	names := make([]string, 0, len(o.held))
	for name := range o.held {
		names = append(names, name)
	}
	sort.Strings(names)
	fmt.Printf("\nReleasing fixture leases (%s); a stack downs only if no other session needs it...\n", strings.Join(names, ", "))
	for _, name := range names {
		stack := o.held[name]
		if stack == nil {
			continue
		}
		// Best-effort: a release error leaves the stack UP by design (never down
		// on uncertainty); it must not mask the underlying check exit code.
		if err := stack.Release(o.holderID); err != nil {
			fmt.Printf("   %s lease release reported: %v\n", name, err)
		}
	}
	o.held = map[string]*stacklease.Stack{}
	o.started = map[checks.StackMode]bool{}
}

// fixtureStackNames is the graph annotation for a check's fixture needs: the
// distinct stack names it asks for, in order ("smb", "smb+sftp"), or "" when it
// needs none.
func fixtureStackNames(d *checks.CheckDefinition) string {
	seen := map[string]bool{}
	var names []string
	for _, want := range d.NeedsContainers {
		if seen[want.Stack] {
			continue
		}
		seen[want.Stack] = true
		names = append(names, want.Stack)
	}
	return strings.Join(names, "+")
}
