package checks

import (
	"path/filepath"
	"testing"
)

// Fixtures (`flakyReport`, `cleanReport`, `failingReport`) and the
// `writeE2EReport` helper live in `e2e-flaky_test.go`, so both readers of the
// Playwright report are pinned to one verified shape.

func TestParsePlaywrightRecordsCapturesStatusDurationAndAttempt(t *testing.T) {
	records, err := parsePlaywrightRecords(writeE2EReport(t, flakyReport))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(records) != 2 {
		t.Fatalf("expected 2 records, got %d: %+v", len(records), records)
	}

	passed := recordByID(t, records, "navigation.spec.ts::::moves the cursor")
	if passed.Outcome != TestPassed || passed.Seconds != 0.1 || passed.Attempt != 1 {
		t.Errorf("unexpected record for the passing spec: %+v", passed)
	}

	// The describe chain is part of the ID, or two same-titled specs collapse into one.
	flaky := recordByID(t, records, "navigation.spec.ts::with two panes::swaps panes")
	if flaky.Outcome != TestFlaky {
		t.Errorf("expected flaky, got %q", flaky.Outcome)
	}
	if flaky.Attempt != 2 {
		t.Errorf("expected the rescue on attempt 2, got %d", flaky.Attempt)
	}
	// The worst single attempt, never the sum: no run ever took 1.7s here.
	if flaky.Seconds != 0.9 {
		t.Errorf("expected 0.9s, got %v", flaky.Seconds)
	}
}

func TestParsePlaywrightRecordsMarksAnUnexpectedSpecFailed(t *testing.T) {
	records, err := parsePlaywrightRecords(writeE2EReport(t, failingReport))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(records) != 1 {
		t.Fatalf("expected 1 record, got %d: %+v", len(records), records)
	}
	if records[0].Outcome != TestFailed {
		t.Errorf("expected fail, got %q", records[0].Outcome)
	}
}

func TestRecordPlaywrightTestsMergesTheShardReports(t *testing.T) {
	recorder := &TestRecorder{}
	ctx := &CheckContext{RootDir: t.TempDir(), Tests: recorder}

	recordPlaywrightTests(ctx, []string{
		writeE2EReport(t, cleanReport),
		writeE2EReport(t, failingReport),
	})

	// Both shards report `moves the cursor`; the failure has to win, or a red spec
	// hides behind another shard's green copy of it.
	records := recorder.Records()
	if len(records) != 1 {
		t.Fatalf("expected 1 merged record, got %d: %+v", len(records), records)
	}
	if records[0].Outcome != TestFailed {
		t.Errorf("expected the failure to win, got %q", records[0].Outcome)
	}
}

func TestRecordPlaywrightTestsStaysSilentWhenAReportIsMissing(t *testing.T) {
	// A run that died before writing its report must record nothing and, above all,
	// not blow up: the lane's own verdict is the only thing allowed to fail it.
	recorder := &TestRecorder{}
	ctx := &CheckContext{RootDir: t.TempDir(), Tests: recorder}

	recordPlaywrightTests(ctx, []string{filepath.Join(t.TempDir(), "gone.json")})

	if records := recorder.Records(); len(records) != 0 {
		t.Fatalf("expected no records, got %+v", records)
	}
}
