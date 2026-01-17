package main

import (
	"fmt"
	"os"
	"runtime"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"vmail/scripts/check/checks"

	"golang.org/x/term"
)

// CheckStatus represents the status of a check during execution.
type CheckStatus int

const (
	StatusPending CheckStatus = iota
	StatusRunning
	StatusCompleted
	StatusFailed
	StatusSkipped
	StatusBlocked // Blocked due to dependency failure
)

// CheckState holds the runtime state of a check.
type CheckState struct {
	Definition *checks.CheckDefinition
	Status     CheckStatus
	Result     checks.CheckResult
	Error      error
	Duration   time.Duration
	mu         sync.Mutex
}

// Runner manages parallel check execution.
type Runner struct {
	ctx         *checks.CheckContext
	checks      []*CheckState
	checkMap    map[string]*CheckState
	failFast    bool
	hasFailed   bool
	mu          sync.Mutex
	outputMu    sync.Mutex
	statusLine  string
	maxWorkers  int
	completedCh chan *CheckState
	isTTY       bool // true if stdout is a terminal (supports status line)
	prefixWidth int  // max width of "App: Tech / Name" prefix for alignment
}

// NewRunner creates a new check runner.
func NewRunner(ctx *checks.CheckContext, defs []checks.CheckDefinition, failFast bool) *Runner {
	r := &Runner{
		ctx:         ctx,
		checks:      make([]*CheckState, 0, len(defs)),
		checkMap:    make(map[string]*CheckState),
		failFast:    failFast,
		maxWorkers:  runtime.NumCPU(),
		completedCh: make(chan *CheckState, len(defs)),
		isTTY:       term.IsTerminal(int(os.Stdout.Fd())),
	}

	for i := range defs {
		state := &CheckState{
			Definition: &defs[i],
			Status:     StatusPending,
		}
		r.checks = append(r.checks, state)
		r.checkMap[defs[i].ID] = state
	}

	// Calculate max prefix width for alignment
	for _, state := range r.checks {
		def := state.Definition
		prefix := fmt.Sprintf("%s: %s / %s", checks.AppDisplayName(def.App), def.Tech, def.DisplayName)
		width := utf8.RuneCountInString(prefix)
		if width > r.prefixWidth {
			r.prefixWidth = width
		}
	}

	return r
}

// Run executes all checks in parallel respecting dependencies.
func (r *Runner) Run() (failed bool, failedChecks []string) {
	if len(r.checks) == 0 {
		return false, nil
	}

	var wg sync.WaitGroup
	semaphore := make(chan struct{}, r.maxWorkers)

	// Start status line updater
	stopStatus := make(chan struct{})
	go r.updateStatusLine(stopStatus)

	// Keep trying to start checks until all are done
	for {
		r.mu.Lock()
		allDone := true
		startedAny := false

		for _, state := range r.checks {
			state.mu.Lock()
			if state.Status == StatusPending {
				allDone = false
				if r.canStart(state) {
					state.Status = StatusRunning
					startedAny = true
					wg.Add(1)
					go func(s *CheckState) {
						defer wg.Done()
						semaphore <- struct{}{}
						r.runCheck(s)
						<-semaphore
						r.completedCh <- s
					}(state)
				}
			} else if state.Status == StatusRunning {
				allDone = false
			}
			state.mu.Unlock()
		}
		r.mu.Unlock()

		if allDone {
			break
		}

		// If we didn't start anything new and not all done, wait for completions
		if !startedAny {
			select {
			case <-r.completedCh:
				// A check completed, try to start more
			case <-time.After(100 * time.Millisecond):
				// Timeout, check again
			}
		}
	}

	wg.Wait()
	close(stopStatus)
	r.clearStatusLine()

	// Collect failed checks
	for _, state := range r.checks {
		if state.Status == StatusFailed {
			failed = true
			failedChecks = append(failedChecks, state.Definition.ID)
		} else if state.Status == StatusBlocked {
			failed = true
		}
	}

	return failed, failedChecks
}

// canStart checks if a check can start based on its dependencies.
func (r *Runner) canStart(state *CheckState) bool {
	if r.failFast && r.hasFailed {
		return false
	}

	for _, depID := range state.Definition.DependsOn {
		dep, ok := r.checkMap[depID]
		if !ok {
			// Dependency not in run list, consider it satisfied
			continue
		}
		dep.mu.Lock()
		depStatus := dep.Status
		dep.mu.Unlock()

		switch depStatus {
		case StatusPending, StatusRunning:
			return false // Still waiting
		case StatusFailed, StatusBlocked:
			// Mark as blocked
			state.Status = StatusBlocked
			r.printBlocked(state, depID)
			return false
		}
	}
	return true
}

// runCheck executes a single check.
func (r *Runner) runCheck(state *CheckState) {
	start := time.Now()
	result, err := state.Definition.Run(r.ctx)
	state.Duration = time.Since(start)

	state.mu.Lock()
	if err != nil {
		state.Status = StatusFailed
		state.Error = err
		r.mu.Lock()
		r.hasFailed = true
		r.mu.Unlock()
	} else if result.Code == checks.ResultSkipped {
		state.Status = StatusSkipped
		state.Result = result
	} else {
		state.Status = StatusCompleted
		state.Result = result
	}
	state.mu.Unlock()

	r.printResult(state)
}

// printResult outputs the result of a check.
func (r *Runner) printResult(state *CheckState) {
	r.outputMu.Lock()
	defer r.outputMu.Unlock()

	// Clear status line before printing
	r.clearStatusLineUnsafe()

	def := state.Definition
	prefix := fmt.Sprintf("%s: %s / %s", checks.AppDisplayName(def.App), def.Tech, def.DisplayName)
	paddedPrefix := r.padPrefix(prefix)

	switch state.Status {
	case StatusCompleted:
		msg := state.Result.Message
		color := colorGreen
		statusText := "OK"
		if state.Result.Code == checks.ResultWarning {
			color = colorYellow
			statusText = "warn"
		}
		if strings.Contains(msg, "\n") {
			fmt.Printf("• %s... %s%s%s (%s)\n", paddedPrefix, color, statusText, colorReset, formatDuration(state.Duration))
			fmt.Printf("  %s%s%s\n", color, indentMultiline(msg, "  "), colorReset)
		} else {
			fmt.Printf("• %s... %s%s%s (%s) - %s%s%s\n", paddedPrefix, color, statusText, colorReset, formatDuration(state.Duration), color, msg, colorReset)
		}

	case StatusSkipped:
		fmt.Printf("• %s... %sSKIPPED%s (%s) - %s\n", paddedPrefix, colorYellow, colorReset, formatDuration(state.Duration), state.Result.Message)

	case StatusFailed:
		fmt.Printf("• %s... %sFAILED%s (%s)\n", paddedPrefix, colorRed, colorReset, formatDuration(state.Duration))
		errMsg := state.Error.Error()
		fmt.Print(indentOutput(errMsg, "      "))
	}
}

// printBlocked outputs that a check was blocked.
func (r *Runner) printBlocked(state *CheckState, depID string) {
	r.outputMu.Lock()
	defer r.outputMu.Unlock()

	r.clearStatusLineUnsafe()

	def := state.Definition
	prefix := fmt.Sprintf("%s: %s / %s", checks.AppDisplayName(def.App), def.Tech, def.DisplayName)
	paddedPrefix := r.padPrefix(prefix)
	fmt.Printf("• %s... %sBLOCKED%s (dependency %s failed)\n", paddedPrefix, colorYellow, colorReset, depID)
}

// padPrefix pads a prefix string to the calculated max width for alignment.
func (r *Runner) padPrefix(prefix string) string {
	currentWidth := utf8.RuneCountInString(prefix)
	if currentWidth >= r.prefixWidth {
		return prefix
	}
	return prefix + strings.Repeat(" ", r.prefixWidth-currentWidth)
}

// updateStatusLine continuously updates the status line showing running checks.
func (r *Runner) updateStatusLine(stop chan struct{}) {
	ticker := time.NewTicker(200 * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-stop:
			return
		case <-ticker.C:
			r.outputMu.Lock()
			r.printStatusLine()
			r.outputMu.Unlock()
		}
	}
}

// printStatusLine prints the current running checks (only in TTY mode).
func (r *Runner) printStatusLine() {
	if !r.isTTY {
		return
	}

	var running []string
	for _, state := range r.checks {
		state.mu.Lock()
		if state.Status == StatusRunning {
			running = append(running, state.Definition.ID)
		}
		state.mu.Unlock()
	}

	if len(running) == 0 {
		return
	}

	line := "Waiting for: " + strings.Join(running, ", ")
	maxLen := 80
	if len(line) > maxLen {
		line = line[:maxLen-3] + "..."
	}

	// Clear previous line and print new one
	fmt.Printf("\r\033[K%s%s%s", colorDim, line, colorReset)
	r.statusLine = line
}

// clearStatusLine clears the status line.
func (r *Runner) clearStatusLine() {
	r.outputMu.Lock()
	defer r.outputMu.Unlock()
	r.clearStatusLineUnsafe()
}

// clearStatusLineUnsafe clears without locking (caller must hold lock).
func (r *Runner) clearStatusLineUnsafe() {
	if r.isTTY && r.statusLine != "" {
		fmt.Print("\r\033[K")
		r.statusLine = ""
	}
}

// formatDuration formats a duration in a human-readable way with color coding.
// Under 5s: dark green, 5-15s: yellow, over 15s: orange.
func formatDuration(d time.Duration) string {
	var text string
	if d < time.Second {
		text = fmt.Sprintf("%dms", d.Milliseconds())
	} else if d < time.Minute {
		text = fmt.Sprintf("%.2fs", d.Seconds())
	} else {
		minutes := int(d.Minutes())
		seconds := int(d.Seconds()) % 60
		text = fmt.Sprintf("%dm%ds", minutes, seconds)
	}

	// Color based on duration
	var color string
	switch {
	case d < 5*time.Second:
		color = colorDarkGreen
	case d < 15*time.Second:
		color = colorYellow
	default:
		color = colorOrange
	}

	return fmt.Sprintf("%s%s%s", color, text, colorReset)
}

// indentMultiline indents a multiline string.
func indentMultiline(s, indent string) string {
	lines := strings.Split(s, "\n")
	for i, line := range lines {
		if line != "" {
			lines[i] = indent + line
		}
	}
	return strings.Join(lines, "\n")
}
