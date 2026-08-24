package checks

import (
	"strings"
	"testing"
)

func snapshot(pairs map[string]any) map[string]any { return pairs }

func TestUnrecordedReleaseAcceptsAnIntactHistory(t *testing.T) {
	// Nothing has moved since the newest entry, so every later release is faithfully described by
	// it and there is nothing to record.
	manifest := settingsDefaultsFile{
		Versions: map[string]map[string]any{"0.39.0": snapshot(map[string]any{"indexing.enabled": true})},
		Next:     snapshot(map[string]any{"indexing.enabled": true}),
	}
	if problem := unrecordedRelease(manifest, "0.40.0"); problem != "" {
		t.Fatalf("expected an intact history to pass, got: %s", problem)
	}
}

func TestUnrecordedReleaseAcceptsUnreleasedWork(t *testing.T) {
	// A default changed after v0.40.0 shipped. `next` has moved, but the newest entry still covers
	// the last release, so the history is complete: the change hasn't shipped to anyone yet.
	manifest := settingsDefaultsFile{
		Versions: map[string]map[string]any{"0.40.0": snapshot(map[string]any{"indexing.enabled": true})},
		Next:     snapshot(map[string]any{"indexing.enabled": false}),
	}
	if problem := unrecordedRelease(manifest, "0.40.0"); problem != "" {
		t.Fatalf("expected unreleased work to pass, got: %s", problem)
	}
}

func TestUnrecordedReleaseCatchesASkippedPromotion(t *testing.T) {
	// v0.41.0 shipped without `--promote`, so its installs would silently resolve against v0.40.0's
	// defaults. This is the case the whole guard exists for.
	manifest := settingsDefaultsFile{
		Versions: map[string]map[string]any{"0.40.0": snapshot(map[string]any{"indexing.enabled": true})},
		Next:     snapshot(map[string]any{"indexing.enabled": false}),
	}
	problem := unrecordedRelease(manifest, "0.41.0")
	if !strings.Contains(problem, "--promote 0.41.0") {
		t.Fatalf("expected the fix command in the failure, got: %s", problem)
	}
}

func TestUnrecordedReleaseRejectsAnUnreleasedEntry(t *testing.T) {
	manifest := settingsDefaultsFile{
		Versions: map[string]map[string]any{"0.41.0": snapshot(map[string]any{"indexing.enabled": true})},
		Next:     snapshot(map[string]any{"indexing.enabled": true}),
	}
	if problem := unrecordedRelease(manifest, "0.40.0"); !strings.Contains(problem, "v0.41.0") {
		t.Fatalf("expected the unreleased entry to be named, got: %s", problem)
	}
}

func TestUnrecordedReleaseRejectsAnEmptyManifest(t *testing.T) {
	manifest := settingsDefaultsFile{Versions: map[string]map[string]any{}, Next: map[string]any{}}
	if problem := unrecordedRelease(manifest, "0.40.0"); !strings.Contains(problem, "--backfill") {
		t.Fatalf("expected the rebuild command in the failure, got: %s", problem)
	}
}

func TestCompareVersionsOrdersNumerically(t *testing.T) {
	if compareVersions("0.9.0", "0.10.0") >= 0 {
		t.Fatal("0.9.0 must sort below 0.10.0")
	}
	if compareVersions("0.40.0", "0.40.0") != 0 {
		t.Fatal("equal versions must compare equal")
	}
	if compareVersions("0.40.1", "0.40.0") <= 0 {
		t.Fatal("0.40.1 must sort above 0.40.0")
	}
}
