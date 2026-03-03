package main

import (
	"encoding/csv"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

const csvFileName = "cmdr-check-log.csv"

var (
	csvHeader = []string{"timestamp", "app", "check", "duration_s", "result", "total", "issues", "changes", "message"}
	csvMu     sync.Mutex
)

// logCheckStats appends one CSV row to ~/cmdr-check-log.csv with the check result.
func logCheckStats(state *CheckState) {
	csvMu.Lock()
	defer csvMu.Unlock()
	home, err := os.UserHomeDir()
	if err != nil {
		return
	}

	csvPath := filepath.Join(home, csvFileName)

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
		_ = w.Write(csvHeader)
	}

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
	}

	// First line only — error messages include verbose output after a newline
	if i := strings.IndexByte(message, '\n'); i >= 0 {
		message = message[:i]
	}

	total := formatCount(state.Result.Total)
	issues := formatCount(state.Result.Issues)
	changes := formatCount(state.Result.Changes)

	_ = w.Write([]string{timestamp, app, check, durationS, result, total, issues, changes, message})
}

func formatCount(n int) string {
	if n < 0 {
		return "N/A"
	}
	return fmt.Sprintf("%d", n)
}
