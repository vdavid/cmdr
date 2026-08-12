package checks

import (
	"os"
	"path/filepath"
	"testing"
)

// Verbatim shape of Vitest 4.1.10's json reporter, captured 2026-08-12 by running
// `vitest run src/lib/app-mode.test.ts` with VITEST_JSON_REPORT set and reading
// the file. Don't hand-tidy it: the absolute `name` and the millisecond float
// `duration` are exactly what the parser has to cope with.
const vitestReport = `{
  "numTotalTests": 3,
  "success": false,
  "testResults": [{
    "name": "/repo/apps/desktop/src/lib/app-mode.test.ts",
    "status": "failed",
    "assertionResults": [
      {"ancestorTitles": ["app-mode"], "fullName": "app-mode resolves to e2e", "status": "passed", "title": "resolves to e2e", "duration": 1.2036659999999983, "failureMessages": []},
      {"ancestorTitles": ["app-mode", "when the backend is quiet"], "fullName": "app-mode when the backend is quiet falls back", "status": "failed", "title": "falls back", "duration": 2500.5, "failureMessages": ["nope"]},
      {"ancestorTitles": ["app-mode"], "fullName": "app-mode skips this one", "status": "pending", "title": "skips this one", "duration": 0, "failureMessages": []}
    ]
  }]
}`

func writeVitestReport(t *testing.T, body string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "test-results.json")
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
	return path
}

func TestParseVitestRecordsBuildsLaneWideIDsAndSeconds(t *testing.T) {
	records, err := parseVitestRecords(writeVitestReport(t, vitestReport), "/repo/apps/desktop")
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(records) != 3 {
		t.Fatalf("expected 3 records, got %d: %+v", len(records), records)
	}

	// The spec path is relative to the app dir, or every worktree writes a
	// different ID for the same test and the log can't be grouped.
	passed := recordByID(t, records, "src/lib/app-mode.test.ts::app-mode::resolves to e2e")
	if passed.Outcome != TestPassed {
		t.Errorf("expected pass, got %q", passed.Outcome)
	}
	if passed.Seconds > 0.002 || passed.Seconds <= 0 {
		t.Errorf("expected ~1.2ms in seconds, got %v", passed.Seconds)
	}

	// The full describe chain is part of the ID, so a title reused under two
	// describes stays two tests.
	failed := recordByID(t, records, "src/lib/app-mode.test.ts::app-mode › when the backend is quiet::falls back")
	if failed.Outcome != TestFailed {
		t.Errorf("expected fail, got %q", failed.Outcome)
	}
	if failed.Seconds != 2.5005 {
		t.Errorf("expected 2.5005s, got %v", failed.Seconds)
	}
}

func TestParseVitestRecordsReadsPendingAsSkipped(t *testing.T) {
	records, err := parseVitestRecords(writeVitestReport(t, vitestReport), "/repo/apps/desktop")
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if got := recordByID(t, records, "src/lib/app-mode.test.ts::app-mode::skips this one").Outcome; got != TestSkipped {
		t.Errorf("expected skip, got %q", got)
	}
}

func TestRecordVitestTestsStaysSilentWhenTheReportIsMissing(t *testing.T) {
	// A worker that died before Vitest wrote the report must record nothing and
	// leave the lane's own verdict alone.
	recorder := &TestRecorder{}
	ctx := &CheckContext{RootDir: t.TempDir(), Tests: recorder}

	recordVitestTests(ctx, filepath.Join(t.TempDir(), "gone.json"), "/repo/apps/desktop")

	if records := recorder.Records(); len(records) != 0 {
		t.Fatalf("expected no records, got %+v", records)
	}
}
