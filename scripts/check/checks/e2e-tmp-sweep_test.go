package checks

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

// The sweep's whole job is telling a run-scoped leftover from something that isn't
// one. Getting that wrong in the generous direction deletes a live run's evidence;
// getting it wrong in the mean direction deletes the 170 MB fixture cache every run
// depends on, or a person's hand-made data dir, and both are silent.
func TestE2EArtifactIsSweepable(t *testing.T) {
	t.Parallel()

	sweepable := []string{
		"cmdr-e2e-report-mtp-4242.json",
		"cmdr-e2e-report-nonmtp1-4242.json",
		"cmdr-e2e-report-linux-4242.json",
		"cmdr-e2e-playwright-mtp-1700000000-4242.log",
		"cmdr-e2e-playwright-build-1700000000-4242.log",
		"cmdr-e2e-linux-1700000000.log",
		"cmdr-e2e-results-mtp-4242",
		"cmdr-e2e-data-mtp-4242",
		"cmdr-e2e-data-nonmtp2-4242",
		"cmdr-e2e-fixtures-e2e-mtp-4242-1700000000",
		"cmdr-mtp-e2e-fixtures-4242",
		"tauri-playwright-nonmtp1-4242.sock",
		// The Linux fixture root: a bare millisecond timestamp.
		"cmdr-e2e-1700000000000",
	}
	for _, name := range sweepable {
		if !e2eArtifactIsSweepable(name) {
			t.Errorf("e2eArtifactIsSweepable(%q) = false, want true", name)
		}
	}

	keep := []string{
		// Shared on purpose, rebuilt only when the fixture shape changes, and older
		// than any cutoff. Sweeping it costs every worktree a 170 MB rebuild.
		"cmdr-e2e-fixtures-cache",
		"cmdr-e2e-fixtures-cache-tmp-4242",
		// Hand-made throwaways from a manual run. `test_mode.rs` suggests the first
		// one by name, and none of them belong to a run we can date.
		"cmdr-e2e-data",
		"cmdr-e2e-data-eta",
		"cmdr-e2e-data-wt-copyfocus",
		"cmdr-mtp-e2e-fixtures",
		"cmdr-e2e-app.log",
		"cmdr-e2e-app.pid",
		// Machine-wide by design: the per-fixture lease refcounts that let concurrent
		// runs share one container stack each.
		"cmdr-smb-leases",
		"cmdr-sftp-leases",
		// Somebody else's.
		"com.apple.launchd.abc123",
		"cmdr-xattr-bench",
	}
	for _, name := range keep {
		if e2eArtifactIsSweepable(name) {
			t.Errorf("e2eArtifactIsSweepable(%q) = true, want false", name)
		}
	}
}

// Age is the second half of the rule: a run in flight owns paths that look exactly
// like a leftover, and the only thing separating them is the clock.
func TestSweepStaleE2EArtifactsKeepsTheYoungAndTheUnmatched(t *testing.T) {
	tmp := t.TempDir()
	now := time.Date(2026, 8, 14, 12, 0, 0, 0, time.UTC)

	write := func(name string, age time.Duration) string {
		p := filepath.Join(tmp, name)
		if err := os.WriteFile(p, []byte("x"), 0o644); err != nil {
			t.Fatalf("writing %s: %v", name, err)
		}
		stamp := now.Add(-age)
		if err := os.Chtimes(p, stamp, stamp); err != nil {
			t.Fatalf("aging %s: %v", name, err)
		}
		return p
	}

	old := write("cmdr-e2e-report-mtp-4242.json", 8*24*time.Hour)
	// A suite that has been running for two hours still owns this one.
	young := write("cmdr-e2e-report-mtp-4343.json", 2*time.Hour)
	cache := write("cmdr-e2e-fixtures-cache", 400*24*time.Hour)

	removed := sweepStaleE2EArtifactsIn(tmp, now)

	if removed != 1 {
		t.Errorf("sweep removed %d entries, want 1", removed)
	}
	if _, err := os.Stat(old); !os.IsNotExist(err) {
		t.Errorf("stale report survived the sweep")
	}
	for _, keep := range []string{young, cache} {
		if _, err := os.Stat(keep); err != nil {
			t.Errorf("sweep removed %s: %v", filepath.Base(keep), err)
		}
	}
}

// A /tmp we can't read is not a reason to fail a test lane. Instrumentation and
// housekeeping never change a verdict.
func TestSweepStaleE2EArtifactsToleratesAMissingDir(t *testing.T) {
	t.Parallel()

	if removed := sweepStaleE2EArtifactsIn(filepath.Join(t.TempDir(), "nope"), time.Now()); removed != 0 {
		t.Errorf("sweep of a missing dir removed %d entries, want 0", removed)
	}
}
