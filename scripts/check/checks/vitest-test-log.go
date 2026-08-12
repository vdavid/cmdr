package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

// Per-test records for the `svelte-tests` lane, feeding the log described in
// `scripts/check/DETAILS.md` § "The per-test log".
//
// Source is Vitest's `json` reporter, which `vitest.config.ts` adds ALONGSIDE the
// default one when VITEST_JSON_REPORT names an output path. Its per-test
// `status` / `duration` are stable reporter fields; the default reporter's
// `Tests N passed` tally, which the check itself parses, is presentation text and
// stays untouched.
//
// Shape verified on vitest 4.1.10 (ran `vitest run src/lib/app-mode.test.ts` with
// the reporter on and read the file, 2026-08-12): `testResults[].name` is the
// spec's ABSOLUTE path, and each `assertionResults[]` carries `ancestorTitles`,
// `title`, `status`, and a millisecond `duration`.

// Subset of Vitest's json reporter output; unknown fields are ignored.
type vitestJSONReport struct {
	TestResults []vitestJSONFile `json:"testResults"`
}

type vitestJSONFile struct {
	Name             string                `json:"name"`
	AssertionResults []vitestJSONAssertion `json:"assertionResults"`
}

type vitestJSONAssertion struct {
	AncestorTitles []string `json:"ancestorTitles"`
	Title          string   `json:"title"`
	// One of passed/failed/pending/todo/skipped.
	Status string `json:"status"`
	// Milliseconds. Absent (0) for a test that never ran.
	Duration float64 `json:"duration"`
}

// vitestOutcome maps Vitest's per-test status. Everything that didn't run reads
// as a skip: `pending`, `todo`, and `skipped` differ in why, not in what the log
// can say about them.
var vitestOutcome = map[string]TestOutcome{
	"passed":  TestPassed,
	"failed":  TestFailed,
	"pending": TestSkipped,
	"todo":    TestSkipped,
	"skipped": TestSkipped,
}

// parseVitestRecords reads one Vitest json report into per-test records. IDs are
// `<spec path relative to desktopDir>::<describe chain>::<title>`, the same shape
// the Playwright lanes use, so one query can rank tests across every lane.
func parseVitestRecords(path, desktopDir string) ([]TestRecord, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var report vitestJSONReport
	if err := json.Unmarshal(data, &report); err != nil {
		return nil, err
	}

	var records []TestRecord
	for _, file := range report.TestResults {
		spec := file.Name
		if rel, relErr := filepath.Rel(desktopDir, spec); relErr == nil {
			spec = rel
		}
		for _, assertion := range file.AssertionResults {
			outcome, known := vitestOutcome[assertion.Status]
			if !known {
				continue // a status Vitest grew since; inventing a meaning is worse than skipping
			}
			records = append(records, TestRecord{
				ID:      spec + "::" + strings.Join(assertion.AncestorTitles, " › ") + "::" + assertion.Title,
				Outcome: outcome,
				Seconds: assertion.Duration / 1000,
				Attempt: 1,
			})
		}
	}
	return records, nil
}

// recordVitestTests files every test in the run's report. An unreadable report
// (a worker died before Vitest could write one) contributes nothing and says
// nothing: the lane's verdict stands on the run's own exit status.
func recordVitestTests(ctx *CheckContext, path, desktopDir string) {
	records, err := parseVitestRecords(path, desktopDir)
	if err != nil {
		return
	}
	ctx.RecordTests(MergeTestRecords(records)...)
}
