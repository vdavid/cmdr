package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"sync"

	"cmdr/scripts/check/checks"
	"cmdr/scripts/check/smblease"
)

// SmbOrchestrator manages the shared `smb-consumer` Docker Compose project's
// lifecycle for all selected checks that have a non-empty `NeedsSmb` field.
//
// It coordinates at two levels:
//
//   - In-process (this orchestrator): keeps the start/stop count at exactly one
//     per check.sh run regardless of how many SMB-using checks are scheduled,
//     via the `startedModes` map. That solved intra-process contention.
//   - Machine-wide (the smblease library): two check.sh processes in different
//     worktrees have independent orchestrators, so the in-process map can't stop
//     them from racing the same containers. The orchestrator therefore takes a
//     machine-wide PID-keyed lease (its own check.sh PID) via smblease, which
//     refcounts every concurrent session and only downs the stack when the last
//     one leaves. EnsureStarted acquires (adopt-or-reconcile); Stop releases
//     (down-at-zero). See scripts/check/smblease for the lock/lease model.
//
// The standalone scripts (start.sh, e2e-linux.sh::start_smb_containers) still
// work for manual / non-runner invocations: they take their OWN leases ("manual"
// for start.sh, $$ for e2e-linux.sh), so a manual run alongside a check.sh run
// just registers as a second holder and neither tears the other's stack down.
type SmbOrchestrator struct {
	smbServersDir string
	holderID      string
	mu            sync.Mutex
	startedModes  map[checks.SmbMode]bool
	leaseHeld     bool
}

// NewSmbOrchestrator returns an orchestrator scoped to the given repo root. Its
// lease holder-id is this check.sh process's PID — long-lived for the whole run,
// so the dead-PID sweep keeps the lease only while the run is alive.
func NewSmbOrchestrator(rootDir string) *SmbOrchestrator {
	return &SmbOrchestrator{
		smbServersDir: filepath.Join(rootDir, "apps", "desktop", "test", "smb-servers"),
		holderID:      strconv.Itoa(os.Getpid()),
		startedModes:  map[checks.SmbMode]bool{},
	}
}

// collectModes returns the deduplicated, deterministically-ordered set of SMB
// modes the given checks need. Empty modes are skipped.
func collectModes(defs []checks.CheckDefinition) []checks.SmbMode {
	seen := map[checks.SmbMode]bool{}
	var out []checks.SmbMode
	for _, d := range defs {
		if d.NeedsSmb == checks.SmbModeNone || seen[d.NeedsSmb] {
			continue
		}
		seen[d.NeedsSmb] = true
		out = append(out, d.NeedsSmb)
	}
	// Stable order so logs are reproducible. Alphabetic is fine (core, e2e).
	sort.Slice(out, func(i, j int) bool { return string(out[i]) < string(out[j]) })
	return out
}

// EnsureStarted brings up the union of SMB consumer containers needed by the
// given modes by acquiring this run's machine-wide lease (which adopts an
// already-serving stack or reconciles it via `up -d` under the lock). Idempotent
// per mode via startedModes. Returns nil if no SMB-using mode was passed.
func (o *SmbOrchestrator) EnsureStarted(modes []checks.SmbMode) error {
	o.mu.Lock()
	defer o.mu.Unlock()
	// Pin cmdr's SMB stack to its dedicated 11480+ host ports before bringing it
	// up, so it never collides with smb2's own harness (10480+). Set in this
	// process so compose and every SMB-using check inherit it, and so the lease's
	// config hash reflects this run's ports. See checks/smb_ports.go.
	checks.ApplySmbPortEnv()
	// Point smblease's config-hash + compose-file resolution at this repo's
	// .compose dir, independent of the orchestrator's cwd.
	_ = os.Setenv("CMDR_SMB_COMPOSE_DIR", filepath.Join(o.smbServersDir, ".compose"))
	for _, mode := range modes {
		if mode == checks.SmbModeNone || o.startedModes[mode] {
			continue
		}
		fmt.Printf("📦 Ensuring SMB Docker containers (%s) via lease %s...\n", mode, o.holderID)
		res, err := smblease.Acquire(o.holderID, string(mode))
		if err != nil {
			return fmt.Errorf("SMB orchestrator: lease acquire (%s) failed: %w", mode, err)
		}
		o.leaseHeld = true
		fmt.Printf("   → %s (%d service(s))\n", res.Action, len(res.Services))
		o.startedModes[mode] = true
	}
	return nil
}

// Stop releases this run's lease. The shared stack is torn down only if no other
// session still holds a lease (down-at-zero, under the lock — see smblease). Safe
// to call when nothing was started (no-op). Prints a friendly banner so the user
// knows cleanup is happening — relevant when Stop runs from a Ctrl+C handler.
func (o *SmbOrchestrator) Stop() {
	o.mu.Lock()
	defer o.mu.Unlock()
	if !o.leaseHeld {
		return
	}
	fmt.Println("\nReleasing SMB lease (the stack downs only if no other session needs it)...")
	// Best-effort: a release error leaves the stack UP by design (never down on
	// uncertainty); it must not mask the underlying check exit code.
	if err := smblease.Release(o.holderID); err != nil {
		fmt.Printf("   SMB lease release reported: %v\n", err)
	}
	o.leaseHeld = false
	o.startedModes = map[checks.SmbMode]bool{}
}
