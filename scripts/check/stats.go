package main

import (
	"encoding/csv"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"cmdr/scripts/check/checks"
)

const csvFileName = "cmdr-check-log.csv"

// testCSVFileName is the SECOND log, one row per individual test rather than per
// check run. It's a separate file on purpose: ~/cmdr-check-log.csv holds nearly a
// hundred thousand rows against a nine-column header, and every reader of it
// (Go's csv.Reader, Python's csv/pandas) hard-errors on a field-count mismatch, so
// widening that schema would destroy the history in place. Schema, retention, and
// example queries: `scripts/check/DETAILS.md` § "The per-test log".
const testCSVFileName = "cmdr-test-log.csv"

var (
	csvHeader = []string{"timestamp", "app", "check", "duration_s", "result", "total", "issues", "changes", "message"}
	csvMu     sync.Mutex

	testCSVHeader = []string{"timestamp", "check", "test_id", "status", "duration_s", "attempt"}
	testCSVMu     sync.Mutex
)

// testLogSlowSeconds is the wall clock at which a PASSING test earns a row.
// Everything that isn't a clean pass or a skip is always logged, so "which tests
// fail most often" stays exact; passes are thresholded so "which tests are slow"
// stays answerable without writing ~5 000 rows per Rust run (roughly half a
// gigabyte a month on this laptop, for rows that can only ever say "fast test was
// fast again"). A test that never crosses this line is by definition not one of
// the slow ones.
const testLogSlowSeconds = 1.0

// appendCSVRows appends rows to ~/<fileName>, writing `header` first when the
// file is new. Every failure is silent: these logs are instrumentation, and a
// full disk or a read-only home must not colour a run's verdict.
func appendCSVRows(mu *sync.Mutex, fileName string, header []string, rows [][]string) {
	if len(rows) == 0 {
		return
	}
	mu.Lock()
	defer mu.Unlock()
	home, err := os.UserHomeDir()
	if err != nil {
		return
	}

	csvPath := filepath.Join(home, fileName)

	_, statErr := os.Stat(csvPath)
	isNew := os.IsNotExist(statErr)

	f, err := os.OpenFile(csvPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return
	}
	defer f.Close()

	w := csv.NewWriter(f)
	defer w.Flush()

	if isNew {
		_ = w.Write(header)
	}
	for _, row := range rows {
		_ = w.Write(row)
	}
}

// logCheckStats appends one CSV row to ~/cmdr-check-log.csv with the check result.
func logCheckStats(state *CheckState) {
	timestamp := time.Now().Format("2006-01-02 15:04:05")
	app := string(state.Definition.App)
	check := state.Definition.CLIName()
	durationS := fmt.Sprintf("%.3f", state.Duration.Seconds())

	result := "pass"
	message := state.Result.Message
	switch state.Status {
	case StatusFailed:
		result = "fail"
		if state.Error != nil {
			message = state.Error.Error()
		}
	case StatusSkipped:
		result = "skip"
	case StatusBlocked:
		result = "blocked"
		message = "dependency failed"
	case StatusCached:
		// Distinct from "pass" so --graph's median (which counts only "pass"
		// rows) stays honest: a ~0s cache hit must not drag the median down.
		result = "cached"
	}

	// First line only; error messages include verbose output after a newline
	if i := strings.IndexByte(message, '\n'); i >= 0 {
		message = message[:i]
	}

	total := formatCount(state.Result.Total)
	issues := formatCount(state.Result.Issues)
	changes := formatCount(state.Result.Changes)

	appendCSVRows(&csvMu, csvFileName, csvHeader,
		[][]string{{timestamp, app, check, durationS, result, total, issues, changes, message}})
}

// logTestStats appends one row per individual test to ~/cmdr-test-log.csv, for
// the test lanes that recorded any. Fast clean passes are dropped
// (`testLogSlowSeconds`); everything that failed, flaked, timed out, or leaked is
// always kept, so a red run leaves behind WHICH test went red rather than only
// that something did.
func logTestStats(state *CheckState) {
	timestamp := time.Now().Format("2006-01-02 15:04:05")
	check := state.Definition.CLIName()

	rows := make([][]string, 0, len(state.Tests))
	for _, rec := range state.Tests {
		if !worthLogging(rec) {
			continue
		}
		rows = append(rows, []string{
			timestamp, check, rec.ID, string(rec.Outcome),
			formatTestDuration(rec.Seconds), fmt.Sprintf("%d", rec.Attempt),
		})
	}
	appendCSVRows(&testCSVMu, testCSVFileName, testCSVHeader, rows)
}

// worthLogging decides whether one test's outcome earns a row. Anything other
// than a clean pass or a skip always does; a pass earns one only by being slow.
func worthLogging(rec checks.TestRecord) bool {
	switch rec.Outcome {
	case checks.TestPassed:
		return rec.Seconds >= testLogSlowSeconds
	case checks.TestSkipped:
		return false
	default:
		return true
	}
}

// formatTestDuration renders a test's wall clock, keeping "the reporter gave no
// timing" distinct from "it took no time".
func formatTestDuration(seconds float64) string {
	if seconds < 0 {
		return "N/A"
	}
	return fmt.Sprintf("%.3f", seconds)
}

func formatCount(n int) string {
	if n < 0 {
		return "N/A"
	}
	return fmt.Sprintf("%d", n)
}
