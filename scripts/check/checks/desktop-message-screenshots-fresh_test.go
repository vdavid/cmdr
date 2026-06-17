package checks

import "testing"

func TestCountDriftLines_CountsBulletedCouplings(t *testing.T) {
	output := "Missing/stale screenshot couplings (2):\n" +
		"  - common.ok → dialog.png (currently undefined)\n" +
		"  - common.cancel → dialog.png (currently \"old.png\")\n"
	if got := countDriftLines(output); got != 2 {
		t.Fatalf("got %d drift lines, want 2", got)
	}
}

func TestCountDriftLines_ZeroWhenNoBullets(t *testing.T) {
	output := "All captured keys are already coupled to their screenshots.\n"
	if got := countDriftLines(output); got != 0 {
		t.Fatalf("got %d drift lines, want 0", got)
	}
}

func TestCountDriftLines_IgnoresHeaderLine(t *testing.T) {
	// The "(N):" header must NOT be counted as a drift line; only the "  - …"
	// bullets are. A regression here would double-count or miscount.
	output := "Missing/stale screenshot couplings (1):\n  - settings.x → s.png (currently undefined)\n"
	if got := countDriftLines(output); got != 1 {
		t.Fatalf("got %d drift lines, want 1", got)
	}
}
