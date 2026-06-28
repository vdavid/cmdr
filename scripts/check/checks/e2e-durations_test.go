package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// syntheticPlaywrightReport mirrors the shape of Playwright's JSON reporter
// output: nested suites (the top-level suite is the spec file; nested suites
// are describe blocks), specs with one test each, and one result per attempt.
const syntheticPlaywrightReport = `{
  "suites": [
    {
      "title": "fast.spec.ts",
      "file": "fast.spec.ts",
      "specs": [
        {
          "title": "renders instantly",
          "tests": [{ "results": [{ "duration": 150, "status": "passed" }] }]
        },
        {
          "title": "skipped on this platform",
          "tests": [{ "results": [] }]
        }
      ],
      "suites": [
        {
          "title": "nested group",
          "file": "fast.spec.ts",
          "specs": [
            {
              "title": "retried test",
              "tests": [
                {
                  "results": [
                    { "duration": 2500, "status": "failed" },
                    { "duration": 900, "status": "passed" }
                  ]
                }
              ]
            }
          ]
        }
      ]
    },
    {
      "title": "slow.spec.ts",
      "file": "slow.spec.ts",
      "specs": [
        {
          "title": "takes ages",
          "tests": [{ "results": [{ "duration": 3100, "status": "passed" }] }]
        },
        {
          "title": "borderline",
          "tests": [{ "results": [{ "duration": 2000, "status": "passed" }] }]
        }
      ]
    }
  ]
}`

func writeSyntheticReport(t *testing.T, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "report.json")
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestParsePlaywrightDurations(t *testing.T) {
	path := writeSyntheticReport(t, syntheticPlaywrightReport)
	durations, err := parsePlaywrightDurations(path)
	if err != nil {
		t.Fatalf("parse failed: %v", err)
	}

	got := map[string]int{}
	for _, d := range durations {
		got[d.key] = d.durMs
	}

	want := map[string]int{
		"fast.spec.ts::::renders instantly":        150,
		"fast.spec.ts::::skipped on this platform": 0,
		"fast.spec.ts::nested group::retried test": 2500, // max attempt, not sum
		"slow.spec.ts::::takes ages":               3100,
		"slow.spec.ts::::borderline":               2000,
	}
	if len(got) != len(want) {
		t.Fatalf("got %d tests, want %d: %v", len(got), len(want), got)
	}
	for key, wantMs := range want {
		if got[key] != wantMs {
			t.Errorf("key %q: got %d ms, want %d ms", key, got[key], wantMs)
		}
	}
}

func TestParsePlaywrightDurationsDuplicateKeysKeepMax(t *testing.T) {
	report := `{
	  "suites": [
	    {
	      "title": "dup.spec.ts",
	      "file": "dup.spec.ts",
	      "specs": [
	        { "title": "same name", "tests": [{ "results": [{ "duration": 100, "status": "passed" }] }] },
	        { "title": "same name", "tests": [{ "results": [{ "duration": 2400, "status": "passed" }] }] }
	      ]
	    }
	  ]
	}`
	path := writeSyntheticReport(t, report)
	durations, err := parsePlaywrightDurations(path)
	if err != nil {
		t.Fatalf("parse failed: %v", err)
	}
	if len(durations) != 1 {
		t.Fatalf("got %d entries, want 1 (duplicates collapse)", len(durations))
	}
	if durations[0].durMs != 2400 {
		t.Errorf("got %d ms, want 2400 (max of duplicates)", durations[0].durMs)
	}
}

func TestAnalyzeE2EDurations(t *testing.T) {
	durations := []e2eTestDuration{
		{key: "a.spec.ts::::fast", durMs: 150},
		{key: "a.spec.ts::::slow", durMs: 2500},
		{key: "a.spec.ts::::allowlisted slow", durMs: 2600},
		{key: "a.spec.ts::::borderline", durMs: 2000},          // exactly at threshold: not flagged
		{key: "a.spec.ts::::allowlisted in band", durMs: 1900}, // under threshold but within the stale margin
		{key: "a.spec.ts::::allowlisted now fast", durMs: 300}, // way under threshold: stale candidate
		{key: "a.spec.ts::::capped within", durMs: 2800},       // over 2s but under its raised 3s cap: suppressed
		{key: "a.spec.ts::::capped over", durMs: 3200},         // past its raised 3s cap: flagged separately
	}
	entries := map[string]e2eDurationEntry{
		"a.spec.ts::::allowlisted slow":     {Reason: "MTP protocol overhead"},
		"a.spec.ts::::allowlisted in band":  {Reason: "hovers around 2s"},
		"a.spec.ts::::allowlisted now fast": {Reason: "was slow once"},
		"a.spec.ts::::gone":                 {Reason: "test was deleted"},
		"a.spec.ts::::capped within":        {MaxMs: 3000, Reason: "contention headroom"},
		"a.spec.ts::::capped over":          {MaxMs: 3000, Reason: "contention headroom"},
	}

	analysis := analyzeE2EDurations(durations, entries)

	if analysis.totalTests != 8 {
		t.Errorf("totalTests: got %d, want 8", analysis.totalTests)
	}
	if len(analysis.slow) != 1 || analysis.slow[0].key != "a.spec.ts::::slow" {
		t.Errorf("slow: got %v, want exactly [a.spec.ts::::slow]", analysis.slow)
	}
	if analysis.allowlisted != 2 {
		// Over-threshold and suppressed: the legacy "allowlisted slow" plus the
		// "capped within" test that stays under its raised 3s cap.
		t.Errorf("allowlisted: got %d, want 2", analysis.allowlisted)
	}
	if len(analysis.exceededCap) != 1 || analysis.exceededCap[0].key != "a.spec.ts::::capped over" {
		t.Errorf("exceededCap: got %v, want [a.spec.ts::::capped over]", analysis.exceededCap)
	}
	if len(analysis.exceededCap) == 1 && analysis.exceededCap[0].capMs != 3000 {
		t.Errorf("exceededCap cap: got %d, want 3000", analysis.exceededCap[0].capMs)
	}
	if len(analysis.staleCandidates) != 1 || analysis.staleCandidates[0] != "a.spec.ts::::allowlisted now fast" {
		t.Errorf("staleCandidates: got %v, want [a.spec.ts::::allowlisted now fast]", analysis.staleCandidates)
	}
	if len(analysis.deadEntries) != 1 || analysis.deadEntries[0] != "a.spec.ts::::gone" {
		t.Errorf("deadEntries: got %v, want [a.spec.ts::::gone]", analysis.deadEntries)
	}
}

func TestE2EDurationEntryRoundTrip(t *testing.T) {
	// Legacy bare-string entries stay bare strings; capped entries serialize as
	// objects. This keeps untouched entries' diffs clean.
	list := e2eDurationAllowlist{
		Macos: map[string]e2eDurationEntry{
			"legacy.spec.ts::::heavy": {Reason: "inherently slow"},
			"capped.spec.ts::::roomy": {MaxMs: 3000, Reason: "contention headroom"},
		},
	}
	data, err := json.Marshal(list)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	got := string(data)
	if !strings.Contains(got, `"legacy.spec.ts::::heavy":"inherently slow"`) {
		t.Errorf("legacy entry should marshal as a bare string, got: %s", got)
	}
	if !strings.Contains(got, `"maxMs":3000`) {
		t.Errorf("capped entry should marshal as an object with maxMs, got: %s", got)
	}

	var back e2eDurationAllowlist
	if err := json.Unmarshal(data, &back); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if e := back.Macos["legacy.spec.ts::::heavy"]; e.MaxMs != 0 || e.Reason != "inherently slow" {
		t.Errorf("legacy round-trip: got %+v", e)
	}
	if e := back.Macos["capped.spec.ts::::roomy"]; e.MaxMs != 3000 || e.Reason != "contention headroom" {
		t.Errorf("capped round-trip: got %+v", e)
	}
}

func TestApplyE2EDurationWarningsConvertsSuccessToWarn(t *testing.T) {
	rootDir := t.TempDir()
	mustWriteE2EAllowlist(t, rootDir, e2eDurationAllowlist{
		Macos: map[string]e2eDurationEntry{"slow.spec.ts::::takes ages": {Reason: "known heavy axe audit"}},
	})
	reportPath := writeSyntheticReport(t, syntheticPlaywrightReport)

	ctx := &CheckContext{RootDir: rootDir}
	result := applyE2EDurationWarnings(ctx, Success("5 tests passed"), []string{reportPath}, "macos")

	if result.Code != ResultWarning {
		t.Fatalf("got code %v, want ResultWarning", result.Code)
	}
	// "takes ages" (3100 ms) is allowlisted; "retried test" (2500 ms) is not.
	if !strings.Contains(result.Message, "retried test") {
		t.Errorf("message should flag the non-allowlisted slow test, got:\n%s", result.Message)
	}
	if strings.Contains(result.Message, "takes ages") {
		t.Errorf("message should not flag the allowlisted test, got:\n%s", result.Message)
	}
	if !strings.Contains(result.Message, "5 tests passed") {
		t.Errorf("original message should be preserved, got:\n%s", result.Message)
	}
}

func TestApplyE2EDurationWarningsAllFastStaysSuccess(t *testing.T) {
	rootDir := t.TempDir()
	report := `{
	  "suites": [
	    {
	      "title": "fast.spec.ts",
	      "file": "fast.spec.ts",
	      "specs": [
	        { "title": "quick", "tests": [{ "results": [{ "duration": 500, "status": "passed" }] }] }
	      ]
	    }
	  ]
	}`
	reportPath := writeSyntheticReport(t, report)

	ctx := &CheckContext{RootDir: rootDir}
	result := applyE2EDurationWarnings(ctx, Success("1 test passed"), []string{reportPath}, "linux")

	if result.Code != ResultSuccess {
		t.Fatalf("got code %v, want ResultSuccess; message:\n%s", result.Code, result.Message)
	}
}

func TestApplyE2EDurationWarningsRemovesDeadEntriesLocally(t *testing.T) {
	rootDir := t.TempDir()
	mustWriteE2EAllowlist(t, rootDir, e2eDurationAllowlist{
		Linux: map[string]e2eDurationEntry{"deleted.spec.ts::::gone test": {Reason: "was slow"}},
		Macos: map[string]e2eDurationEntry{"deleted.spec.ts::::gone test": {Reason: "untouched: other platform's section"}},
	})
	report := `{
	  "suites": [
	    {
	      "title": "fast.spec.ts",
	      "file": "fast.spec.ts",
	      "specs": [
	        { "title": "quick", "tests": [{ "results": [{ "duration": 500, "status": "passed" }] }] }
	      ]
	    }
	  ]
	}`
	reportPath := writeSyntheticReport(t, report)

	ctx := &CheckContext{RootDir: rootDir}
	result := applyE2EDurationWarnings(ctx, Success("1 test passed"), []string{reportPath}, "linux")

	if !result.MadeChanges {
		t.Errorf("expected MadeChanges after dead-entry removal")
	}
	written := mustReadE2EAllowlist(t, rootDir)
	if len(written.Linux) != 0 {
		t.Errorf("dead linux entry should be removed, got %v", written.Linux)
	}
	if len(written.Macos) != 1 {
		t.Errorf("macos section must stay untouched on a linux run, got %v", written.Macos)
	}
}

func TestApplyE2EDurationWarningsCIDoesNotWrite(t *testing.T) {
	rootDir := t.TempDir()
	list := e2eDurationAllowlist{Linux: map[string]e2eDurationEntry{"deleted.spec.ts::::gone test": {Reason: "was slow"}}}
	mustWriteE2EAllowlist(t, rootDir, list)
	report := `{
	  "suites": [
	    {
	      "title": "fast.spec.ts",
	      "file": "fast.spec.ts",
	      "specs": [
	        { "title": "quick", "tests": [{ "results": [{ "duration": 500, "status": "passed" }] }] }
	      ]
	    }
	  ]
	}`
	reportPath := writeSyntheticReport(t, report)

	ctx := &CheckContext{RootDir: rootDir, CI: true}
	result := applyE2EDurationWarnings(ctx, Success("1 test passed"), []string{reportPath}, "linux")

	if result.MadeChanges {
		t.Errorf("CI run must not rewrite the allowlist")
	}
	written := mustReadE2EAllowlist(t, rootDir)
	if len(written.Linux) != 1 {
		t.Errorf("CI run must leave the allowlist file untouched, got %v", written.Linux)
	}
	if result.Code != ResultWarning {
		t.Errorf("CI run should still report the dead entry as a warning")
	}
}

func TestApplyE2EDurationWarningsMissingReportKeepsResult(t *testing.T) {
	rootDir := t.TempDir()
	ctx := &CheckContext{RootDir: rootDir}
	original := Success("42 tests passed")
	result := applyE2EDurationWarnings(ctx, original, []string{filepath.Join(rootDir, "nope.json")}, "macos")

	if result.Code != ResultSuccess {
		t.Fatalf("missing report must not fail or warn, got code %v", result.Code)
	}
	if !strings.Contains(result.Message, "42 tests passed") {
		t.Errorf("original message should be preserved, got:\n%s", result.Message)
	}
}

func mustWriteE2EAllowlist(t *testing.T, rootDir string, list e2eDurationAllowlist) {
	t.Helper()
	path := e2eDurationAllowlistPath(rootDir)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	data, err := json.Marshal(list)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatal(err)
	}
}

func mustReadE2EAllowlist(t *testing.T, rootDir string) e2eDurationAllowlist {
	t.Helper()
	data, err := os.ReadFile(e2eDurationAllowlistPath(rootDir))
	if err != nil {
		t.Fatal(err)
	}
	var list e2eDurationAllowlist
	if err := json.Unmarshal(data, &list); err != nil {
		t.Fatal(err)
	}
	return list
}
