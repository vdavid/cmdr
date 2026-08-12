package checks

import "testing"

// The nextest fixtures live in `rust-test-diagnostics_test.go` and are verbatim
// cargo-nextest 0.9.136 output, so these tests are pinned to the real format
// rather than to a hand-written idea of it.

// recordByID finds one test's record, failing the test when it's missing.
func recordByID(t *testing.T, records []TestRecord, id string) TestRecord {
	t.Helper()
	for _, rec := range records {
		if rec.ID == id {
			return rec
		}
	}
	t.Fatalf("no record for %q; got %d records: %+v", id, len(records), records)
	return TestRecord{}
}

func TestParseNextestResultsRecordsEveryPassWithItsDuration(t *testing.T) {
	records := ParseNextestResults(cleanGreenRun)

	if len(records) != 2 {
		t.Fatalf("expected 2 records, got %d: %+v", len(records), records)
	}
	rec := recordByID(t, records, "cmdr_lib::downloads::latest_ring::tests::ring_wraps")
	if rec.Outcome != TestPassed {
		t.Errorf("expected a pass, got %q", rec.Outcome)
	}
	if rec.Seconds != 0.012 {
		t.Errorf("expected 0.012s, got %v", rec.Seconds)
	}
	if rec.Attempt != 1 {
		t.Errorf("expected attempt 1, got %d", rec.Attempt)
	}
}

func TestParseNextestResultsMarksARetryRescuedTestFlaky(t *testing.T) {
	records := ParseNextestResults(greenRunWithFlaky)

	// The flake is reported three times (TRY 1 FAIL, TRY 2 PASS, FLAKY 2/3) and
	// must still collapse to one record, or the log double-counts it.
	if len(records) != 2 {
		t.Fatalf("expected 2 records, got %d: %+v", len(records), records)
	}
	rec := recordByID(t, records, "cmdr_lib::file_viewer::watcher_test::sees_append")
	if rec.Outcome != TestFlaky {
		t.Errorf("expected flaky, got %q", rec.Outcome)
	}
	if rec.Attempt != 2 {
		t.Errorf("expected the pass on attempt 2, got %d", rec.Attempt)
	}
}

func TestParseNextestResultsSeparatesFailuresFromTimeouts(t *testing.T) {
	records := ParseNextestResults(mixedFailureRun)

	// Four tests ran; the summary block repeats three of them, and the
	// `TERMINATING [> 8.000s]` progress line must not become a fifth.
	if len(records) != 4 {
		t.Fatalf("expected 4 records, got %d: %+v", len(records), records)
	}
	failing := recordByID(t, records, "cmdr_lib::file_viewer::session_test::test_session_close_stops_watcher")
	if failing.Outcome != TestFailed {
		t.Errorf("expected fail, got %q", failing.Outcome)
	}
	timedOut := recordByID(t, records, "cmdr_lib::downloads::watcher::tests::dropping_a_file_emits_one_event")
	if timedOut.Outcome != TestTimedOut {
		t.Errorf("expected timeout, got %q", timedOut.Outcome)
	}
	if timedOut.Seconds != 8.002 {
		t.Errorf("expected 8.002s, got %v", timedOut.Seconds)
	}
}

func TestParseNextestResultsReadsSkipsLeaksAndLongDurations(t *testing.T) {
	// Hand-assembled from the same line grammar: a SKIP, a LEAK, and a duration
	// past a minute, none of which the captured fixtures happen to contain.
	const run = `        SKIP [   0.000s] (1/3) cmdr_lib mtp::backends::mtp_test::reads_a_file
        LEAK [   0.104s] (2/3) cmdr_lib downloads::watcher::tests::keeps_a_handle
        PASS [  1m 03.500s] (3/3) cmdr_lib index::scan_test::walks_a_big_tree
     Summary [  1m 03.6s] 3 tests run: 2 passed (1 leaky), 1 skipped
`
	records := ParseNextestResults(run)

	if len(records) != 3 {
		t.Fatalf("expected 3 records, got %d: %+v", len(records), records)
	}
	if got := recordByID(t, records, "cmdr_lib::mtp::backends::mtp_test::reads_a_file").Outcome; got != TestSkipped {
		t.Errorf("expected skip, got %q", got)
	}
	if got := recordByID(t, records, "cmdr_lib::downloads::watcher::tests::keeps_a_handle").Outcome; got != TestLeaked {
		t.Errorf("expected leak, got %q", got)
	}
	if got := recordByID(t, records, "cmdr_lib::index::scan_test::walks_a_big_tree").Seconds; got != 63.5 {
		t.Errorf("expected 63.5s, got %v", got)
	}
}

func TestParseNextestResultsIgnoresOutputThatOnlyLooksLikeAStatusLine(t *testing.T) {
	// A panic body quoting a status line must not become a record: nextest indents
	// captured output, and the anchored grammar is the only thing keeping the two
	// apart.
	const run = `        FAIL [   0.009s] (1/1) cmdr_lib parser::tests::rejects_a_bad_line
  stderr ───

    thread 'rejects_a_bad_line' panicked at src/parser.rs:12:5:
    unexpected line: PASS [   0.001s] fake_bin some::other::test
`
	records := ParseNextestResults(run)

	if len(records) != 1 {
		t.Fatalf("expected 1 record, got %d: %+v", len(records), records)
	}
	if records[0].ID != "cmdr_lib::parser::tests::rejects_a_bad_line" {
		t.Errorf("unexpected record: %+v", records[0])
	}
}

func TestParseNextestResultsFindsNothingInNonTestOutput(t *testing.T) {
	// A build break never reaches the test phase. Zero records is the honest
	// answer, and the caller must still fail the check on its own evidence.
	const run = `error[E0425]: cannot find value ` + "`nope`" + ` in this scope
  --> src/lib.rs:3:5
error: could not compile ` + "`cmdr`" + ` (lib test) due to 1 previous error
`
	if records := ParseNextestResults(run); len(records) != 0 {
		t.Fatalf("expected no records, got %+v", records)
	}
}

func TestMergeTestRecordsKeepsTheWorstOutcomePerTest(t *testing.T) {
	merged := MergeTestRecords([]TestRecord{
		{ID: "a", Outcome: TestPassed, Seconds: 0.1, Attempt: 1},
		{ID: "b", Outcome: TestPassed, Seconds: 0.2, Attempt: 1},
		{ID: "a", Outcome: TestFailed, Seconds: 0.3, Attempt: 1},
		{ID: "b", Outcome: TestSkipped, Seconds: 0, Attempt: 1},
	})

	if len(merged) != 2 {
		t.Fatalf("expected 2 records, got %d: %+v", len(merged), merged)
	}
	a := recordByID(t, merged, "a")
	if a.Outcome != TestFailed || a.Seconds != 0.3 {
		t.Errorf("expected the failure to win with its own duration, got %+v", a)
	}
	if b := recordByID(t, merged, "b"); b.Outcome != TestPassed {
		t.Errorf("expected the pass to beat the skip, got %+v", b)
	}
}

func TestRecordTestsIsANoOpWithoutARecorder(t *testing.T) {
	// Every check-level unit test builds a bare CheckContext, so a lane calling
	// RecordTests must not care whether the runner attached a recorder.
	ctx := &CheckContext{RootDir: t.TempDir()}
	ctx.RecordTests(TestRecord{ID: "a", Outcome: TestPassed})
}

func TestRecorderKeepsWhatEachLaneFiles(t *testing.T) {
	recorder := &TestRecorder{}
	ctx := &CheckContext{RootDir: t.TempDir(), Tests: recorder}

	ctx.RecordTests(TestRecord{ID: "a", Outcome: TestPassed, Seconds: 0.1, Attempt: 1})
	ctx.RecordTests(TestRecord{ID: "b", Outcome: TestFailed, Seconds: 0.2, Attempt: 1})

	records := recorder.Records()
	if len(records) != 2 {
		t.Fatalf("expected 2 records, got %d: %+v", len(records), records)
	}
	if records[0].ID != "a" || records[1].ID != "b" {
		t.Errorf("expected the filing order preserved, got %+v", records)
	}
}
