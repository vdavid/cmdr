package checks

import "sync"

// Per-test instrumentation, shared by every test lane.
//
// A lane's verdict travels as a `CheckResult` on the green path and as an `error`
// on the red one, so neither can carry "which test went red". This is the ONE
// side-channel that can: a lane parses its runner's own machine-readable report,
// hands back one [TestRecord] per test, and the runner drains them into
// ~/cmdr-test-log.csv (`scripts/check/DETAILS.md` § "The per-test log"). Recording
// happens before the pass/fail branch, so a red run says exactly as much as a
// green one.
//
// Guardrail: instrumentation must NEVER change a verdict. A report that's missing
// or unparsable yields zero records and nothing else — no warn, no failure. A lane
// that grows its own idea of what a test result is defeats the point; extend the
// vocabulary here instead.

// TestOutcome is how one individual test ended.
type TestOutcome string

const (
	TestPassed TestOutcome = "pass"
	TestFailed TestOutcome = "fail"
	// TestFlaky is red on its first attempt and green on a retry. The runners exit 0
	// on it, which is exactly why it needs its own status here.
	TestFlaky TestOutcome = "flaky"
	// TestTimedOut was killed at the runner's cap, so it never finished.
	TestTimedOut TestOutcome = "timeout"
	// TestLeaked passed its assertions but left a handle or process behind.
	TestLeaked  TestOutcome = "leak"
	TestSkipped TestOutcome = "skip"
)

// outcomeSeverity ranks outcomes so a deduping parser keeps the worst one it saw
// for a test. Runners repeat a test across lines (a `FAIL` in the run and again in
// the summary, an attempt line plus a final one), and "the worst thing that
// happened to this test" is the honest single answer.
var outcomeSeverity = map[TestOutcome]int{
	TestSkipped:  0,
	TestPassed:   1,
	TestFlaky:    2,
	TestLeaked:   3,
	TestTimedOut: 4,
	TestFailed:   5,
}

// TestRecord is one individual test's outcome inside one check run.
type TestRecord struct {
	// ID identifies the test within its lane, and must stay stable across runs or
	// the log can't be grouped by test. Nextest lanes use `<binary>::<test path>`;
	// the Playwright and Vitest lanes use `<spec file>::<describe chain>::<title>`.
	ID string
	// Outcome is the worst thing that happened to this test in the run.
	Outcome TestOutcome
	// Seconds is the wall clock of the attempt that produced Outcome, or -1 when the
	// reporter gave none.
	Seconds float64
	// Attempt is the 1-based attempt that produced Outcome, so a retry-rescued test
	// says which try saved it.
	Attempt int
}

// TestRecorder collects a single check's [TestRecord]s. The runner hands each
// check its own recorder, so concurrent lanes never mix; the mutex guards the
// lanes that parse several reports in parallel (the Playwright shards).
type TestRecorder struct {
	mu      sync.Mutex
	records []TestRecord
}

// Record files test outcomes. Safe from several goroutines.
func (r *TestRecorder) Record(records ...TestRecord) {
	if len(records) == 0 {
		return
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	r.records = append(r.records, records...)
}

// Records returns a copy of what's been filed so far.
func (r *TestRecorder) Records() []TestRecord {
	r.mu.Lock()
	defer r.mu.Unlock()
	return append([]TestRecord(nil), r.records...)
}

// MergeTestRecords collapses records for the same test down to the worst outcome
// seen, keeping that outcome's duration and attempt. Lanes that read several
// reports (sharded Playwright) or a report that repeats a test (nextest's summary
// block) route through this so one test yields one row.
func MergeTestRecords(records []TestRecord) []TestRecord {
	best := make(map[string]TestRecord, len(records))
	order := make([]string, 0, len(records))
	for _, rec := range records {
		prev, seen := best[rec.ID]
		if !seen {
			order = append(order, rec.ID)
			best[rec.ID] = rec
			continue
		}
		if outcomeSeverity[rec.Outcome] > outcomeSeverity[prev.Outcome] {
			best[rec.ID] = rec
		}
	}
	merged := make([]TestRecord, 0, len(order))
	for _, id := range order {
		merged = append(merged, best[id])
	}
	return merged
}
