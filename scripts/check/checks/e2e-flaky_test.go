package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Shape verified against a real Playwright JSON report on disk (2026-07-29):
// `stats.flaky` plus a per-test `status` of expected/unexpected/flaky/skipped,
// under a nested suite tree whose top-level suite is the spec file.
func writeE2EReport(t *testing.T, body string) string {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "report.json")
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
	return path
}

const flakyReport = `{
  "stats": {"expected": 2, "unexpected": 0, "flaky": 1, "skipped": 0},
  "suites": [{
    "title": "navigation.spec.ts",
    "file": "navigation.spec.ts",
    "specs": [
      {"title": "moves the cursor", "tests": [{"status": "expected", "results": [{"duration": 100}]}]}
    ],
    "suites": [{
      "title": "with two panes",
      "specs": [
        {"title": "swaps panes", "tests": [{"status": "flaky", "results": [{"duration": 900}, {"duration": 800}]}]}
      ]
    }]
  }]
}`

const cleanReport = `{
  "stats": {"expected": 2, "unexpected": 0, "flaky": 0, "skipped": 0},
  "suites": [{
    "title": "navigation.spec.ts",
    "file": "navigation.spec.ts",
    "specs": [
      {"title": "moves the cursor", "tests": [{"status": "expected", "results": [{"duration": 100}]}]}
    ]
  }]
}`

// A genuinely failing test is `unexpected`, not `flaky`. Reporting it here would
// double-count it against the failure the check already raises.
const failingReport = `{
  "stats": {"expected": 0, "unexpected": 1, "flaky": 0, "skipped": 0},
  "suites": [{
    "title": "navigation.spec.ts",
    "file": "navigation.spec.ts",
    "specs": [
      {"title": "moves the cursor", "tests": [{"status": "unexpected", "results": [{"duration": 100}]}]}
    ]
  }]
}`

func TestParsePlaywrightFlakyFindsRetryRescuedSpecs(t *testing.T) {
	flaky, err := parsePlaywrightFlaky(writeE2EReport(t, flakyReport))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(flaky) != 1 {
		t.Fatalf("expected 1 flaky spec, got %v", flaky)
	}
	// The describe chain must survive, or two same-titled specs are indistinguishable.
	for _, want := range []string{"navigation.spec.ts", "with two panes", "swaps panes"} {
		if !strings.Contains(flaky[0], want) {
			t.Errorf("key %q missing %q", flaky[0], want)
		}
	}
}

func TestParsePlaywrightFlakyIgnoresPassingAndFailingSpecs(t *testing.T) {
	for name, body := range map[string]string{"clean": cleanReport, "failing": failingReport} {
		flaky, err := parsePlaywrightFlaky(writeE2EReport(t, body))
		if err != nil {
			t.Fatalf("%s: parse: %v", name, err)
		}
		if len(flaky) != 0 {
			t.Errorf("%s: expected no flaky specs, got %v", name, flaky)
		}
	}
}

// The whole point: a retry-rescued run exits 0 and Playwright reports it as
// passed, so without this the check reports a clean green.
func TestApplyE2EFlakyWarningDowngradesAPassToAWarn(t *testing.T) {
	result := Success("276 tests passed across 3 shards")
	got := applyE2EFlakyWarning(result, []string{writeE2EReport(t, flakyReport)})

	if got.Code != ResultWarning {
		t.Errorf("code = %v, want ResultWarning", got.Code)
	}
	if !strings.Contains(got.Message, "swaps panes") {
		t.Errorf("message should name the flaky spec: %q", got.Message)
	}
	if !strings.Contains(got.Message, "276 tests passed") {
		t.Errorf("message should keep the original tally: %q", got.Message)
	}
}

func TestApplyE2EFlakyWarningLeavesACleanRunAlone(t *testing.T) {
	result := Success("276 tests passed across 3 shards")
	got := applyE2EFlakyWarning(result, []string{writeE2EReport(t, cleanReport)})

	if got.Code != ResultSuccess {
		t.Errorf("a clean run must stay a pass, got %v", got.Code)
	}
	if got.Message != result.Message {
		t.Errorf("message should be untouched, got %q", got.Message)
	}
}

// Shards are separate Playwright processes with separate reports; a flake in any
// one of them counts.
func TestApplyE2EFlakyWarningUnionsAcrossShards(t *testing.T) {
	result := Success("276 tests passed")
	got := applyE2EFlakyWarning(result, []string{
		writeE2EReport(t, cleanReport),
		writeE2EReport(t, flakyReport),
	})
	if got.Code != ResultWarning {
		t.Errorf("a flake in a later shard must still warn, got %v", got.Code)
	}
}

// An unreadable report must not crash the check or, worse, silently look clean.
func TestApplyE2EFlakyWarningSaysSoWhenAReportIsUnreadable(t *testing.T) {
	result := Success("276 tests passed")
	got := applyE2EFlakyWarning(result, []string{filepath.Join(t.TempDir(), "missing.json")})

	if got.Code != ResultSuccess {
		t.Errorf("an unreadable report isn't evidence of a flake, got %v", got.Code)
	}
	if !strings.Contains(got.Message, "flaky check skipped") {
		t.Errorf("the gap must be disclosed, not silent: %q", got.Message)
	}
}
