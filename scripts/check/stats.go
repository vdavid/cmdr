package main

import (
	"bytes"
	"encoding/csv"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"cmdr/scripts/check/checks"
	"cmdr/scripts/check/filelock"
)

// projectLogDirName is this repo's subdirectory under the shared check-runner
// data dir, and the ONE token a copy of this runner in another repo has to
// change: the filenames and the resolver below are identical everywhere.
const projectLogDirName = "cmdr"

const csvFileName = "check-log.csv"

// testCSVFileName is the SECOND log, one row per individual test rather than per
// check run. It's a separate file on purpose: `check-log.csv` holds nearly three
// hundred thousand rows against a nine-column header, and every reader of it
// (Go's csv.Reader, Python's csv/pandas) hard-errors on a field-count mismatch, so
// widening that schema would destroy the history in place. Schema, retention, and
// example queries: `scripts/check/DETAILS.md` § "The per-test log".
const testCSVFileName = "test-log.csv"

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

// logPath resolves one log to ~/.local/share/check-runner/<project>/<fileName>,
// honoring $XDG_DATA_HOME the way `scripts/mcp-call.sh` does. The logs live
// outside the repo on purpose: they're a measurement history spanning years and
// worktrees, and a worktree teardown must never take them with it.
func logPath(fileName string) (string, error) {
	dataHome := os.Getenv("XDG_DATA_HOME")
	if dataHome == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		dataHome = filepath.Join(home, ".local", "share")
	}
	return filepath.Join(dataHome, "check-runner", projectLogDirName, fileName), nil
}

// appendCSVRows appends rows to one of the logs, creating its directory and
// writing `header` first when the file is new. Every failure is silent: these
// logs are instrumentation, and a full disk or a read-only home must not colour
// a run's verdict.
//
// ❗ Several runners append to the same file at once (one per worktree), and `mu`
// only orders this process's writers. So the batch is rendered whole first and
// leaves in ONE write under an exclusive `flock`, and the header is decided under
// that lock by the file's size. Through csv.Writer's 4 KiB buffer, one write per
// buffer-full, two processes' chunks interleaved mid-row
// (`TestAppendCSVRowsKeepsConcurrentProcessesRowsWhole`).
func appendCSVRows(mu *sync.Mutex, fileName string, header []string, rows [][]string) {
	if len(rows) == 0 {
		return
	}
	mu.Lock()
	defer mu.Unlock()
	csvPath, err := logPath(fileName)
	if err != nil {
		return
	}
	if err := os.MkdirAll(filepath.Dir(csvPath), 0o755); err != nil {
		return
	}

	f, err := os.OpenFile(csvPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return
	}
	defer f.Close()
	if err := filelock.Lock(f); err != nil {
		return
	}
	defer func() { _ = filelock.Unlock(f) }()

	info, err := f.Stat()
	if err != nil {
		return
	}
	var batch bytes.Buffer
	w := csv.NewWriter(&batch)
	if info.Size() == 0 {
		_ = w.Write(header)
	}
	for _, row := range rows {
		_ = w.Write(row)
	}
	w.Flush()
	_, _ = f.Write(batch.Bytes())
}

// logCheckStats appends one CSV row to the per-run log with the check result.
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

// logTestStats appends one row per individual test to the per-test log, for
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
