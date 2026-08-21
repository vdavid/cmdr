package checks

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// Fixtures below are VERBATIM cargo-nextest 0.9.136 output (captured 2026-07-29 on
// macOS by running a probe crate with a marker-file flaky test, a `wait_until`-shaped
// panic, and a test that sleeps past the cap). Don't hand-tidy them: the whole point
// is that the parsers are pinned to the real format, including the `(2/2)` progress
// counter and the `(───)` placeholder nextest uses before a slot is assigned.

const greenRunWithFlaky = `────────────
 Nextest run ID 1d3872ce-d43f-47ac-887f-622938570e3b with nextest profile: default
    Starting 2 tests across 1 binary
        PASS [   0.012s] (1/2) cmdr_lib downloads::latest_ring::tests::ring_wraps
  TRY 1 FAIL [   0.009s] (───) cmdr_lib file_viewer::watcher_test::sees_append
   RETRY 2/3 [         ] (───) cmdr_lib file_viewer::watcher_test::sees_append
  TRY 2 PASS [   0.009s] (2/2) cmdr_lib file_viewer::watcher_test::sees_append
────────────
     Summary [   0.017s] 2 tests run: 2 passed (1 flaky), 0 skipped
   FLAKY 2/3 [   0.009s] (2/2) cmdr_lib file_viewer::watcher_test::sees_append
`

const cleanGreenRun = `────────────
 Nextest run ID 80cc5822-420a-44fb-99a0-9a819155c820 with nextest profile: default
    Starting 2 tests across 1 binary
        PASS [   0.012s] (1/2) cmdr_lib downloads::latest_ring::tests::ring_wraps
        PASS [   0.012s] (2/2) cmdr_lib downloads::filter::tests::ignores_partials
────────────
     Summary [   0.013s] 2 tests run: 2 passed, 0 skipped
`

const mixedFailureRun = `────────────
 Nextest run ID 80cc5822-420a-44fb-99a0-9a819155c820 with nextest profile: default
    Starting 4 tests across 1 binary
        PASS [   0.009s] (1/4) cmdr_lib downloads::filter::tests::ignores_partials
        FAIL [   0.009s] (2/4) cmdr_lib file_viewer::session_test::test_session_close_stops_watcher
  stdout ───

    running 1 test
    test test_session_close_stops_watcher ... FAILED

  stderr ───

    thread 'test_session_close_stops_watcher' (27302680) panicked at src/file_viewer/session_test.rs:1643:5:
    timed out after 2.0s waiting for close_session to drop the watcher subscription
    note: run with ` + "`RUST_BACKTRACE=1`" + ` environment variable to display a backtrace

        FAIL [   0.004s] (3/4) cmdr_lib downloads::ignore_set::tests::rejects_bad_glob
  stdout ───

  stderr ───

    thread 'rejects_bad_glob' (27302681) panicked at src/downloads/ignore_set.rs:212:9:
    assertion ` + "`left == right`" + ` failed
      left: 3
     right: 4

 TERMINATING [>  8.000s] (───) cmdr_lib downloads::watcher::tests::dropping_a_file_emits_one_event
     TIMEOUT [   8.002s] (4/4) cmdr_lib downloads::watcher::tests::dropping_a_file_emits_one_event
  stdout ───

    running 1 test

    (test timed out)

────────────
     Summary [   8.002s] 4 tests run: 1 passed, 2 failed, 1 timed out, 0 skipped
        FAIL [   0.009s] (2/4) cmdr_lib file_viewer::session_test::test_session_close_stops_watcher
        FAIL [   0.004s] (3/4) cmdr_lib downloads::ignore_set::tests::rejects_bad_glob
     TIMEOUT [   8.002s] (4/4) cmdr_lib downloads::watcher::tests::dropping_a_file_emits_one_event
error: test run failed
`

func TestParseFlakyTestsFindsRetryPassInGreenRun(t *testing.T) {
	flaky := ParseFlakyTests(greenRunWithFlaky)
	if len(flaky) != 1 {
		t.Fatalf("expected 1 flaky test, got %d: %+v", len(flaky), flaky)
	}
	got := flaky[0]
	if got.Name != "file_viewer::watcher_test::sees_append" {
		t.Errorf("name = %q", got.Name)
	}
	if got.Binary != "cmdr_lib" {
		t.Errorf("binary = %q", got.Binary)
	}
	if got.PassedOnTry != 2 || got.MaxTries != 3 {
		t.Errorf("attempts = %d/%d, want 2/3", got.PassedOnTry, got.MaxTries)
	}
}

func TestParseFlakyTestsIsEmptyOnACleanRun(t *testing.T) {
	if flaky := ParseFlakyTests(cleanGreenRun); len(flaky) != 0 {
		t.Fatalf("expected no flaky tests, got %+v", flaky)
	}
}

// A `TRY n PASS` line during the run must not be double-counted with the
// `FLAKY n/m` line in the summary block: they describe the same test.
func TestParseFlakyTestsDoesNotDoubleCount(t *testing.T) {
	doubled := greenRunWithFlaky + greenRunWithFlaky
	if flaky := ParseFlakyTests(doubled); len(flaky) != 1 {
		t.Fatalf("expected dedupe to 1 flaky test, got %d: %+v", len(flaky), flaky)
	}
}

func TestClassifyRustFailuresSeparatesCapKillsFromInTestDeadlines(t *testing.T) {
	failures := ClassifyRustFailures(mixedFailureRun)
	if len(failures) != 3 {
		t.Fatalf("expected 3 failures, got %d: %+v", len(failures), failures)
	}

	byName := map[string]RustFailure{}
	for _, f := range failures {
		byName[f.Name] = f
	}

	deadline, ok := byName["file_viewer::session_test::test_session_close_stops_watcher"]
	if !ok {
		t.Fatalf("session_test failure missing from %+v", failures)
	}
	if deadline.Class != ClassInTestDeadline {
		t.Errorf("session_test class = %q, want %q", deadline.Class, ClassInTestDeadline)
	}
	if !strings.Contains(deadline.Detail, "close_session to drop the watcher subscription") {
		t.Errorf("session_test detail should quote the wait_until description, got %q", deadline.Detail)
	}

	capKill, ok := byName["downloads::watcher::tests::dropping_a_file_emits_one_event"]
	if !ok {
		t.Fatalf("watcher timeout missing from %+v", failures)
	}
	if capKill.Class != ClassNextestCap {
		t.Errorf("watcher class = %q, want %q", capKill.Class, ClassNextestCap)
	}

	plain, ok := byName["downloads::ignore_set::tests::rejects_bad_glob"]
	if !ok {
		t.Fatalf("plain assertion failure missing from %+v", failures)
	}
	if plain.Class != ClassOther {
		t.Errorf("plain class = %q, want %q", plain.Class, ClassOther)
	}
}

func TestClassifyRustFailuresIsEmptyOnACleanRun(t *testing.T) {
	if f := ClassifyRustFailures(cleanGreenRun); len(f) != 0 {
		t.Fatalf("expected no failures, got %+v", f)
	}
}

// The diagnosis header is the whole point of the check: a reader must be able to
// tell, without opening a single file, whether raising the nextest cap is even
// relevant to what broke.
func TestDiagnoseRustFailuresSaysWhichDeadlineBlew(t *testing.T) {
	diagnosis := DiagnoseRustFailures(ClassifyRustFailures(mixedFailureRun))

	if !strings.Contains(diagnosis, "nextest cap") {
		t.Errorf("diagnosis should name the nextest cap class:\n%s", diagnosis)
	}
	if !strings.Contains(diagnosis, "raising the nextest cap won't help") {
		t.Errorf("diagnosis must warn that the cap is irrelevant for in-test deadlines:\n%s", diagnosis)
	}
	if !strings.Contains(diagnosis, "test_session_close_stops_watcher") {
		t.Errorf("diagnosis should name the offending tests:\n%s", diagnosis)
	}
}

func TestDiagnoseRustFailuresIsEmptyWhenNothingIsClassifiable(t *testing.T) {
	if d := DiagnoseRustFailures(nil); d != "" {
		t.Errorf("expected empty diagnosis, got %q", d)
	}
}

func TestFlakySummaryNamesTheTestAndTheAttempt(t *testing.T) {
	summary := FlakySummary(ParseFlakyTests(greenRunWithFlaky))
	if !strings.Contains(summary, "1 flaky") {
		t.Errorf("summary should count the flakes: %q", summary)
	}
	if !strings.Contains(summary, "file_viewer::watcher_test::sees_append") {
		t.Errorf("summary should name the test: %q", summary)
	}
	if !strings.Contains(summary, "2/3") {
		t.Errorf("summary should say which attempt passed: %q", summary)
	}
}

// Coupling guard. `ClassifyRustFailures` recognises an in-test deadline by the
// panic text `wait_until` produces. That string lives in Rust, the classifier
// lives in Go, and nothing but this test ties them together: if someone rewords
// the panic, classification silently degrades to `ClassOther` and every
// `wait_until` timeout starts looking like an ordinary assertion failure.
func TestWaitUntilPanicFormatStillMatchesTheClassifier(t *testing.T) {
	// Go runs a test with cwd = its package dir (`scripts/check/checks`). The
	// helper lives in `cmdr-fs` (every crate in the workspace waits the same way);
	// the app re-exports it as `crate::test_support::wait_until`.
	path := filepath.Join("..", "..", "..", "crates", "cmdr-fs", "src", "testing.rs")
	src, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	// The literal in `timed_out()`: format!("timed out after {timeout:.1?} waiting for {description}")
	if !strings.Contains(string(src), `"timed out after {timeout:.1?} waiting for {description}"`) {
		t.Fatalf("the wait_until panic format in %s changed; update inTestDeadlineRE in rust-test-diagnostics.go to match", path)
	}
}

// colorizedFailureRun is a VERBATIM excerpt of a real `pnpm check rust-tests` capture
// from an agent session (M1 Max, cargo-nextest 0.9.136, 2026-08-21). nextest colours
// its output whenever `FORCE_COLOR` / `CLICOLOR_FORCE` is set, which the Claude Code
// harness and several terminals do, so a captured buffer is NOT plain text just
// because it isn't a TTY.
const colorizedFailureRun = "\x1b[32;1m        PASS\x1b[0m [   0.046s] (   8/6333) \x1b[35;1mcmdr\x1b[0m \x1b[36magent::chat::budget::tests\x1b[0m\x1b[36m::\x1b[0m\x1b[34;1ma_size_above_the_known_window_warns_and_is_still_used\x1b[0m\n" +
	"\x1b[31;1m     TIMEOUT\x1b[0m [   8.005s] (5307/6333) \x1b[35;1mcmdr\x1b[0m \x1b[36mpriority::roots::tests\x1b[0m\x1b[36m::\x1b[0m\x1b[34;1ma_protected_folder_is_taken_on_trust_while_the_fda_gate_is_pending\x1b[0m\n" +
	"            ────────────\n" +
	"\x1b[31;1m     Summary\x1b[0m [  34.534s] \x1b[1m6333\x1b[0m tests run: \x1b[1m6332\x1b[0m \x1b[32;1mpassed\x1b[0m, \x1b[1m1\x1b[0m \x1b[31;1mtimed out\x1b[0m, \x1b[1m158\x1b[0m \x1b[33;1mskipped\x1b[0m\n"

// The whole contention/diagnosis apparatus reads captured nextest text with
// line-anchored regexes. Under a forced-colour environment every status token is
// preceded by an SGR escape, so an unnormalised parser matches NOTHING and the run
// degrades silently: no deadline diagnosis, no isolated re-run, no per-test log rows,
// and a retry-rescued run reported as a clean pass. Every one of those is the exact
// failure the tooling exists to prevent, so the parsers normalise before matching.
func TestParsersReadColourisedNextestOutput(t *testing.T) {
	failures := ClassifyRustFailures(colorizedFailureRun)
	if len(failures) != 1 {
		t.Fatalf("expected 1 classified failure, got %d: %+v", len(failures), failures)
	}
	if failures[0].Class != ClassNextestCap {
		t.Errorf("class = %v, want a cap kill", failures[0].Class)
	}
	if failures[0].Name != "priority::roots::tests::a_protected_folder_is_taken_on_trust_while_the_fda_gate_is_pending" {
		t.Errorf("name = %q", failures[0].Name)
	}

	records := ParseNextestResults(colorizedFailureRun)
	if len(records) != 2 {
		t.Fatalf("expected 2 per-test records, got %d: %+v", len(records), records)
	}
}

// The colour codes also hide the PASS/SKIP lines from the trimmer, so a red run dumps
// every one of thousands of progress lines and buries the diagnosis under them (one
// real capture came to 1.2 MB).
func TestTrimRustTestProgressDropsColourisedPassLines(t *testing.T) {
	trimmed := trimRustTestProgress(colorizedFailureRun)
	if strings.Contains(trimmed, "a_size_above_the_known_window_warns_and_is_still_used") {
		t.Errorf("colourised PASS line survived the trim:\n%s", trimmed)
	}
	if !strings.Contains(trimmed, "a_protected_folder_is_taken_on_trust_while_the_fda_gate_is_pending") {
		t.Errorf("the TIMEOUT line must survive the trim:\n%s", trimmed)
	}
}
