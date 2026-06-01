package main

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"sort"
	"sync"

	"cmdr/scripts/check/checks"
)

// SmbOrchestrator manages the shared `smb-consumer` Docker Compose project's
// lifecycle for all selected checks that have a non-empty `NeedsSmb` field.
//
// Before this existed, each SMB-using check owned the lifecycle in its own
// function: start in the entry, `defer ./stop.sh` at the end. When two such
// checks ran in parallel under `--include-slow`, whichever finished first
// would tear down containers the other was still using, producing the
// "Cannot reach smb-consumer-X" cascade documented in
// `apps/desktop/test/e2e-linux/CLAUDE.md`. Lifting lifecycle one level up,
// to the runner, keeps the start/stop count at exactly one regardless of how
// many SMB-using checks are scheduled. The smaller scripts (start.sh,
// e2e-linux.sh::start_smb_containers) still work standalone for manual /
// non-runner invocations — the orchestrator just makes their work an
// idempotent no-op when run from check.sh.
type SmbOrchestrator struct {
	smbServersDir string
	mu            sync.Mutex
	startedModes  map[checks.SmbMode]bool
}

// NewSmbOrchestrator returns an orchestrator scoped to the given repo root.
func NewSmbOrchestrator(rootDir string) *SmbOrchestrator {
	return &SmbOrchestrator{
		smbServersDir: filepath.Join(rootDir, "apps", "desktop", "test", "smb-servers"),
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
// given modes. Idempotent: docker compose up -d on already-running services
// is a no-op. Returns nil if no SMB-using mode was passed.
func (o *SmbOrchestrator) EnsureStarted(modes []checks.SmbMode) error {
	o.mu.Lock()
	defer o.mu.Unlock()
	// Pin cmdr's SMB stack to its dedicated 11480+ host ports before bringing it
	// up, so it never collides with smb2's own harness (10480+). Set in this
	// process so start.sh/compose and every SMB-using check inherit it. See
	// checks/smb_ports.go.
	checks.ApplySmbPortEnv()
	for _, mode := range modes {
		if mode == checks.SmbModeNone || o.startedModes[mode] {
			continue
		}
		fmt.Printf("📦 Bringing up SMB Docker containers (%s)...\n", mode)
		cmd := exec.Command("./start.sh", string(mode))
		cmd.Dir = o.smbServersDir
		if out, err := checks.RunCommand(cmd, true); err != nil {
			return fmt.Errorf("SMB orchestrator: start.sh %s failed:\n%s", mode, out)
		}
		o.startedModes[mode] = true
	}
	return nil
}

// Stop tears down all SMB containers in the smb-consumer compose project.
// Idempotent: safe to call when nothing was started (no-op). Prints a friendly
// banner so the user knows the dev environment is being cleaned up — relevant
// when stop runs from a Ctrl+C signal handler vs normal exit.
func (o *SmbOrchestrator) Stop() {
	o.mu.Lock()
	defer o.mu.Unlock()
	if len(o.startedModes) == 0 {
		return
	}
	fmt.Println("\nShutting down SMB Docker containers before quitting...")
	cmd := exec.Command("./stop.sh")
	cmd.Dir = o.smbServersDir
	// Best-effort: if stop fails the user can clean up manually with
	// `docker compose -p smb-consumer down`. We do not want to mask the
	// underlying check exit code with a stop failure.
	_, _ = checks.RunCommand(cmd, true)
	o.startedModes = map[checks.SmbMode]bool{}
}
