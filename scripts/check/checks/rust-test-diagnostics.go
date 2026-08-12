package checks

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"
)

// Diagnostics over cargo-nextest output, shared by the macOS (`desktop-rust-tests`)
// and Linux (`desktop-rust-tests-linux`) lanes. Two jobs:
//
//  1. Surface retry-passes. `.config/nextest.toml` grants `retries` to a named set of
//     real-FSEvents tests. A retry that passes is a FLAKE, and nextest exits 0, so
//     without this the suite reports "all green" while quietly hiding the exact signal
//     the retries exist to tolerate. Reporting them turns the retry budget into a
//     flake-rate meter instead of a hiding place.
//  2. Say WHICH deadline blew. A red run has two very different causes that look
//     identical in the raw output: nextest killing the process at `slow-timeout`, and
//     the test's own `wait_until` deadline panicking well under that cap. The fixes are
//     opposite (raise the cap vs. raise/scale the in-test wait), so a reader who can't
//     tell them apart guesses, and guesses wrong.
//
// Parsing tool stdout is textual by nature, so the ban on message-string
// classification doesn't apply here. The one string we own rather than observe is
// `wait_until`'s panic format; `TestWaitUntilPanicFormatStillMatchesTheClassifier`
// pins it so a reword can't silently degrade classification.

// FlakyTest is a test that failed at least once, then passed on a retry.
type FlakyTest struct {
	Binary      string // nextest binary, e.g. "cmdr_lib"
	Name        string // test path, e.g. "file_viewer::watcher_test::sees_append"
	PassedOnTry int    // the attempt that finally passed
	MaxTries    int    // total attempts allowed (0 when only a `TRY n PASS` line was seen)
}

// FailureClass says which deadline (if any) a failing test blew.
type FailureClass string

const (
	// ClassNextestCap: nextest terminated the process at its `slow-timeout`. The test
	// never got to finish, so there is no panic message to read.
	ClassNextestCap FailureClass = "nextest-cap"
	// ClassInTestDeadline: the test's own `wait_until` deadline expired, below the cap.
	ClassInTestDeadline FailureClass = "in-test-deadline"
	// ClassLeak: the test passed its assertions but left a handle or process behind.
	ClassLeak FailureClass = "leak"
	// ClassOther: an ordinary assertion failure or panic.
	ClassOther FailureClass = "other"
)

// RustFailure is one failing test plus the reason class we could infer for it.
type RustFailure struct {
	Binary string
	Name   string
	Class  FailureClass
	Detail string // for ClassInTestDeadline, the `wait_until` description
}

// nextest status lines share a shape: `<STATUS> [ 0.009s] (2/4) <binary> <test::path>`.
// The counter is optional (it's `(───)` before a slot is assigned) and its INSIDES may
// contain spaces: nextest right-aligns the index to the total's width, so a 4 802-test
// run prints `(  42/4802)`. Matching the parens as `\S+` therefore silently missed every
// failure numbered under 1000, which meant no diagnosis and no contention re-run for it.
// Anchored at line start so a panic body quoting these words can't be misread as a status
// line.
var (
	flakyLineRE   = regexp.MustCompile(`^\s*FLAKY (\d+)/(\d+) \[[^\]]*\]\s+(?:\([^)]*\)\s+)?(\S+)\s+(\S+)\s*$`)
	tryPassLineRE = regexp.MustCompile(`^\s*TRY (\d+) PASS \[[^\]]*\]\s+(?:\([^)]*\)\s+)?(\S+)\s+(\S+)\s*$`)
	failLineRE    = regexp.MustCompile(`^\s*FAIL \[[^\]]*\]\s+(?:\([^)]*\)\s+)?(\S+)\s+(\S+)\s*$`)
	timeoutLineRE = regexp.MustCompile(`^\s*TIMEOUT \[[^\]]*\]\s+(?:\([^)]*\)\s+)?(\S+)\s+(\S+)\s*$`)
	leakLineRE    = regexp.MustCompile(`^\s*LEAK \[[^\]]*\]\s+(?:\([^)]*\)\s+)?(\S+)\s+(\S+)\s*$`)

	// Ends the search for a panic body: the next test's status line, a retry marker, or
	// the summary separator.
	statusBoundaryRE = regexp.MustCompile(
		`^\s*(?:PASS|FAIL|SKIP|TIMEOUT|LEAK|FLAKY|TERMINATING|Summary|TRY \d+ (?:PASS|FAIL)|RETRY \d+/\d+|────)`,
	)

	// `wait_until`'s panic, from `timed_out()` in `apps/desktop/src-tauri/src/test_support.rs`:
	//   format!("timed out after {timeout:.1?} waiting for {description}")
	// The async twin appends " (at <caller>)", stripped from the captured description.
	inTestDeadlineRE = regexp.MustCompile(`timed out after \S+ waiting for (.+?)\s*$`)
	asyncCallerRE    = regexp.MustCompile(`\s*\(at [^)]*\)\s*$`)
)

// ParseFlakyTests returns every test that needed a retry to pass, deduped: nextest
// reports the same flake twice, as `TRY n PASS` during the run and `FLAKY n/m` in the
// summary block.
func ParseFlakyTests(output string) []FlakyTest {
	type key struct{ binary, name string }
	seen := map[key]FlakyTest{}
	var order []key

	record := func(f FlakyTest) {
		k := key{f.Binary, f.Name}
		prev, exists := seen[k]
		if !exists {
			order = append(order, k)
			seen[k] = f
			return
		}
		// Prefer whichever record knows the retry budget (the `FLAKY n/m` line).
		if prev.MaxTries == 0 && f.MaxTries > 0 {
			seen[k] = f
		}
	}

	for line := range strings.SplitSeq(output, "\n") {
		if m := flakyLineRE.FindStringSubmatch(line); m != nil {
			record(FlakyTest{
				Binary:      m[3],
				Name:        m[4],
				PassedOnTry: atoiOrZero(m[1]),
				MaxTries:    atoiOrZero(m[2]),
			})
			continue
		}
		if m := tryPassLineRE.FindStringSubmatch(line); m != nil {
			try := atoiOrZero(m[1])
			if try <= 1 {
				continue // a first-try pass isn't a flake
			}
			record(FlakyTest{Binary: m[2], Name: m[3], PassedOnTry: try})
		}
	}

	flaky := make([]FlakyTest, 0, len(order))
	for _, k := range order {
		flaky = append(flaky, seen[k])
	}
	return flaky
}

// ClassifyRustFailures returns each failing test with the deadline class we could infer.
// `TRY n FAIL` lines are ignored: those are retried attempts, reported separately by
// ParseFlakyTests when they eventually pass, and by their own final status when they don't.
func ClassifyRustFailures(output string) []RustFailure {
	lines := strings.Split(output, "\n")
	type key struct{ binary, name string }
	seen := map[key]bool{}
	var failures []RustFailure

	add := func(binary, name string, class FailureClass, detail string) {
		k := key{binary, name}
		if seen[k] {
			return // the summary block repeats every FAIL/TIMEOUT line
		}
		seen[k] = true
		failures = append(failures, RustFailure{Binary: binary, Name: name, Class: class, Detail: detail})
	}

	for i, line := range lines {
		if m := timeoutLineRE.FindStringSubmatch(line); m != nil {
			add(m[1], m[2], ClassNextestCap, "")
			continue
		}
		if m := leakLineRE.FindStringSubmatch(line); m != nil {
			add(m[1], m[2], ClassLeak, "")
			continue
		}
		if m := failLineRE.FindStringSubmatch(line); m != nil {
			class, detail := ClassOther, ""
			if desc, ok := findInTestDeadline(lines, i+1); ok {
				class, detail = ClassInTestDeadline, desc
			}
			add(m[1], m[2], class, detail)
		}
	}
	return failures
}

// findInTestDeadline scans a failing test's captured output (which runs until the next
// status line) for a `wait_until` timeout panic.
func findInTestDeadline(lines []string, start int) (string, bool) {
	for i := start; i < len(lines); i++ {
		if statusBoundaryRE.MatchString(lines[i]) {
			return "", false
		}
		if m := inTestDeadlineRE.FindStringSubmatch(lines[i]); m != nil {
			return strings.TrimSpace(asyncCallerRE.ReplaceAllString(m[1], "")), true
		}
	}
	return "", false
}

// DiagnoseRustFailures renders a short header explaining which deadline blew, so the
// reader knows whether the nextest cap is even the relevant knob. Empty when there's
// nothing to classify.
func DiagnoseRustFailures(failures []RustFailure) string {
	if len(failures) == 0 {
		return ""
	}

	groups := []struct {
		class   FailureClass
		heading string
	}{
		{ClassNextestCap, "Killed at the nextest cap, so the test never finished. Look for a hang, or for starvation under load:"},
		{ClassInTestDeadline, "Blew its own in-test `wait_until` deadline, well under the cap, so raising the nextest cap won't help. Raise or load-scale the wait instead:"},
		{ClassLeak, "Passed, but leaked a handle or process that outlived the test:"},
		{ClassOther, "Ordinary assertion or panic:"},
	}

	// Leaks are a nextest PASS status ("N passed (M leaky)"), so they're reported but
	// never counted as failures: doing so overstates how red a run actually was.
	realCount := len(RealFailures(failures))

	var b strings.Builder
	b.WriteString(fmt.Sprintf("Diagnosis of %d failing %s:\n", realCount, Pluralize(realCount, "test", "tests")))
	for _, g := range groups {
		var members []RustFailure
		for _, f := range failures {
			if f.Class == g.class {
				members = append(members, f)
			}
		}
		if len(members) == 0 {
			continue
		}
		b.WriteString("  • " + g.heading + "\n")
		for _, f := range members {
			b.WriteString("      - " + f.Name)
			if f.Detail != "" {
				b.WriteString(" (waiting for " + f.Detail + ")")
			}
			b.WriteString("\n")
		}
	}
	return b.String()
}

// FlakySummary renders the one-line result message for a run that went green only
// because a retry rescued it. Empty when nothing was flaky.
func FlakySummary(flaky []FlakyTest) string {
	if len(flaky) == 0 {
		return ""
	}
	parts := make([]string, 0, len(flaky))
	for _, f := range flaky {
		attempt := strconv.Itoa(f.PassedOnTry)
		if f.MaxTries > 0 {
			attempt = fmt.Sprintf("%d/%d", f.PassedOnTry, f.MaxTries)
		}
		parts = append(parts, fmt.Sprintf("%s (passed on try %s)", f.Name, attempt))
	}
	return fmt.Sprintf("%d flaky %s passed only on retry: %s",
		len(flaky), Pluralize(len(flaky), "test", "tests"), strings.Join(parts, ", "))
}

// nextestResultRE matches a FINAL per-test status line, sharing the grammar the
// classifier regexes above are anchored to. Progress-only markers (`SLOW`,
// `TERMINATING`, `RETRY n/m`) are deliberately absent from the alternation: they
// describe a test still in flight, and reading them as outcomes would invent
// results. Anchored at line start, so a panic body quoting a status line stays
// captured output.
var nextestResultRE = regexp.MustCompile(
	`^\s*(PASS|FAIL|SKIP|TIMEOUT|LEAK|FLAKY \d+/\d+|TRY \d+ PASS) \[([^\]]*)\]\s+(?:\([^)]*\)\s+)?(\S+)\s+(\S+)\s*$`,
)

// nextestDurationRE reads the bracketed wall clock nextest prints per test, in
// its `[  1m 03.500s]` / `[   0.012s]` forms.
var nextestDurationRE = regexp.MustCompile(`^(?:(\d+)h\s*)?(?:(\d+)m\s*)?([\d.]+)s$`)

// nextestAttemptRE pulls the winning attempt out of `FLAKY 2/3` and `TRY 2 PASS`.
var nextestAttemptRE = regexp.MustCompile(`\d+`)

// ParseNextestResults returns one [TestRecord] per test in a nextest run, for the
// per-test log (`test-log.go`). It reads the same captured output the classifier
// above does, so it costs one extra scan and no extra reporter: nextest's JSON
// output is still experimental, and a lane's verdict must never hang on a flag
// that can change under it.
//
// Test IDs are `<binary>::<test path>`, because a test path alone repeats across
// binaries. Output with no test phase in it (a build break) yields no records,
// which is the honest answer; the caller still fails on its own evidence.
func ParseNextestResults(output string) []TestRecord {
	var records []TestRecord
	for line := range strings.SplitSeq(output, "\n") {
		m := nextestResultRE.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		outcome, attempt := nextestOutcome(m[1])
		records = append(records, TestRecord{
			ID:      m[3] + "::" + m[4],
			Outcome: outcome,
			Seconds: parseNextestDuration(m[2]),
			Attempt: attempt,
		})
	}
	// nextest reports a test more than once (every failure repeats in the summary
	// block, a flake shows as both `TRY n PASS` and `FLAKY n/m`), so the merge is
	// what keeps one test to one row.
	return MergeTestRecords(records)
}

// nextestOutcome maps a status token to an outcome plus the attempt that produced
// it. Only the retry forms carry an attempt; everything else is a first try.
func nextestOutcome(status string) (TestOutcome, int) {
	switch {
	case strings.HasPrefix(status, "FLAKY "), strings.HasPrefix(status, "TRY "):
		return TestFlaky, atoiOrZero(nextestAttemptRE.FindString(status))
	case status == "FAIL":
		return TestFailed, 1
	case status == "TIMEOUT":
		return TestTimedOut, 1
	case status == "LEAK":
		return TestLeaked, 1
	case status == "SKIP":
		return TestSkipped, 1
	default:
		return TestPassed, 1
	}
}

// parseNextestDuration turns the bracketed wall clock into seconds, returning -1
// when there's nothing to read (a `[         ]` placeholder, or a format nextest
// changed under us). A missing duration must read as "unknown", never as zero.
func parseNextestDuration(bracket string) float64 {
	trimmed := strings.TrimSpace(strings.TrimPrefix(strings.TrimSpace(bracket), ">"))
	m := nextestDurationRE.FindStringSubmatch(strings.ReplaceAll(trimmed, " ", ""))
	if m == nil {
		return -1
	}
	seconds, err := strconv.ParseFloat(m[3], 64)
	if err != nil {
		return -1
	}
	return float64(atoiOrZero(m[1]))*3600 + float64(atoiOrZero(m[2]))*60 + seconds
}

func atoiOrZero(s string) int {
	n, err := strconv.Atoi(s)
	if err != nil {
		return 0
	}
	return n
}
