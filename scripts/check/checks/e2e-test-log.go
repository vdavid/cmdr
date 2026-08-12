package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

// Per-test records for the two Playwright lanes (macOS shards and the Linux
// Docker suite), feeding the log described in `scripts/check/DETAILS.md` §
// "The per-test log".
//
// Source is the same structured JSON report `e2e-durations.go` and `e2e-flaky.go`
// already read (`CMDR_E2E_JSON_REPORT`), which Playwright writes on a red run as
// well as a green one. That's what makes the flakiest lane in the suite
// diagnosable: the check's own error message is a shard-level summary, while the
// report names every spec.

// playwrightOutcome maps Playwright's per-test status. The four values are stable
// reporter fields, unlike anything in the `list` reporter's presentation text.
var playwrightOutcome = map[string]TestOutcome{
	"expected":   TestPassed,
	"unexpected": TestFailed,
	"flaky":      TestFlaky,
	"skipped":    TestSkipped,
}

// parsePlaywrightRecords reads one Playwright JSON report into per-test records.
// Keys match the duration allowlist's (`<file>::<describe chain>::<title>`), so a
// log row and an allowlist entry name the same thing.
func parsePlaywrightRecords(path string) ([]TestRecord, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var report e2eJSONReport
	if err := json.Unmarshal(data, &report); err != nil {
		return nil, err
	}

	var records []TestRecord
	for _, s := range report.Suites {
		collectSuiteRecords(s, s.File, nil, &records)
	}
	return records, nil
}

// collectSuiteRecords mirrors collectSuiteDurations' walk of Playwright's nested
// suite tree, accumulating the describe chain. Duration is the worst single
// attempt, matching how the duration flagger reads a retried test: the slowest
// real execution, without summing the retries into a number no run ever took.
func collectSuiteRecords(s e2eJSONSuite, file string, describe []string, out *[]TestRecord) {
	scope := describe
	if s.Title != "" && s.Title != filepath.Base(file) && s.Title != file {
		scope = append(append([]string{}, describe...), s.Title)
	}
	for _, sp := range s.Specs {
		key := file + "::" + strings.Join(scope, " › ") + "::" + sp.Title
		for _, test := range sp.Tests {
			outcome, known := playwrightOutcome[test.Status]
			if !known {
				continue // a status Playwright grew since; inventing a meaning is worse than skipping
			}
			maxMs := -1
			for _, r := range test.Results {
				maxMs = max(maxMs, r.DurationMs)
			}
			seconds := -1.0
			if maxMs >= 0 {
				seconds = float64(maxMs) / 1000
			}
			*out = append(*out, TestRecord{
				ID:      key,
				Outcome: outcome,
				Seconds: seconds,
				Attempt: max(len(test.Results), 1),
			})
		}
	}
	for _, child := range s.Suites {
		collectSuiteRecords(child, file, scope, out)
	}
}

// recordPlaywrightTests files every spec in the run's reports. Sharded lanes pass
// one path per shard; the merge collapses a spec that a retry moved between
// shards' reports down to its worst outcome. An unreadable report contributes
// nothing and says nothing: the lane's own verdict already stands on the run's
// exit status, and instrumentation must never colour it.
func recordPlaywrightTests(ctx *CheckContext, reportPaths []string) {
	var records []TestRecord
	for _, path := range reportPaths {
		parsed, err := parsePlaywrightRecords(path)
		if err != nil {
			continue
		}
		records = append(records, parsed...)
	}
	ctx.RecordTests(MergeTestRecords(records)...)
}
