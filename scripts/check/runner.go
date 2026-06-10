package main

import (
	"fmt"
	"os"
	"runtime"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"cmdr/scripts/check/checks"

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
	StatusCached  // Skipped: inputs unchanged since the last passing run
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
	cached      []*CheckState // cache hits: pre-resolved, never run, reported up front
	checkMap    map[string]*CheckState
	failFast    bool
	noLog       bool
	hasFailed   bool
	mu          sync.Mutex
	outputMu    sync.Mutex
	statusLine  string
	capacity    int // CPU-core budget for concurrent checks (= NumCPU)
	usedWeight  int // sum of EffectiveCpuWeight of currently-running checks (guarded by mu)
	completedCh chan *CheckState
	isTTY       bool // true if stdout is a terminal (supports status line)
	prefixWidth int  // max width of "App: Tech / Name" prefix for alignment
}

// NewRunner creates a new check runner. cached are checks resolved from the
// input-fingerprint cache (inputs unchanged since their last pass); they're
// reported up front and never run, but are logged and counted like real passes.
func NewRunner(ctx *checks.CheckContext, defs []checks.CheckDefinition, cached []cachedHit, failFast, noLog bool) *Runner {
	r := &Runner{
		ctx:         ctx,
		checks:      make([]*CheckState, 0, len(defs)),
		checkMap:    make(map[string]*CheckState),
		failFast:    failFast,
		noLog:       noLog,
		capacity:    runtime.NumCPU(),
		completedCh: make(chan *CheckState, len(defs)),
		isTTY:       term.IsTerminal(int(os.Stdout.Fd())),
	}

	for i := range defs {
		state := &CheckState{
			Definition: &defs[i],
			Status:     StatusPending,
			Result:     checks.CheckResult{Total: -1, Issues: -1, Changes: -1},
		}
		r.checks = append(r.checks, state)
		r.checkMap[defs[i].ID] = state
	}

	for i := range cached {
		state := &CheckState{
			Definition: &cached[i].def,
			Status:     StatusCached,
			Result:     checks.Success(cached[i].message),
		}
		r.cached = append(r.cached, state)
	}

	// Calculate max prefix width for alignment (across run + cached checks).
	for _, state := range append(append([]*CheckState{}, r.checks...), r.cached...) {
		def := state.Definition
		prefix := fmt.Sprintf("%s: %s / %s", checks.AppDisplayName(def.App), def.Tech, def.CLIName())
		width := utf8.RuneCountInString(prefix)
		if width > r.prefixWidth {
			r.prefixWidth = width
		}
	}

	return r
}

// Run executes all checks in parallel respecting dependencies. Cache hits are
// reported and logged first (so their "(cached)" lines lead the output), then
// the real checks run.
func (r *Runner) Run() (failed bool, failedChecks []string) {
	r.reportCached()

	if len(r.checks) == 0 {
		return false, nil
	}

	var wg sync.WaitGroup

	// Start status line updater
	stopStatus := make(chan struct{})
	go r.updateStatusLine(stopStatus)

	// Keep trying to start checks until all are done
	for {
		r.mu.Lock()
		allDone := true
		startedAny := false

		for _, state := range r.checks {
			active, started := r.tryStartPending(state, &wg)
			if active {
				allDone = false
			}
			if started {
				startedAny = true
			}
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
			failedChecks = append(failedChecks, state.Definition.CLIName())
		} else if state.Status == StatusBlocked {
			failed = true
		}
	}

	return failed, failedChecks
}

// reportCached prints and logs the cache-hit checks before the real run starts.
func (r *Runner) reportCached() {
	for _, state := range r.cached {
		r.printResult(state)
		if !r.noLog {
			logCheckStats(state)
		}
	}
}

// CachedCount returns how many checks were served from the cache this run.
func (r *Runner) CachedCount() int {
	return len(r.cached)
}

// RanCount returns how many checks actually executed this run (cache misses).
func (r *Runner) RanCount() int {
	return len(r.checks)
}

// RunStates returns the runtime state of every check that actually ran, for the
// cache writer to record this run's passing fingerprints.
func (r *Runner) RunStates() []*CheckState {
	return r.checks
}

// tryStartPending evaluates one check and starts it if it's ready. The caller
// must hold r.mu (the weight budget and the start decision share it). Returns:
//   - active: the check is not yet done (pending or running), so the run loop
//     must keep iterating.
//   - started: a goroutine was launched this call (so the loop made progress).
//
// Admission is two-stage: dependencies first (canStart also marks the check
// Blocked if a dep failed), then CPU weight — a check starts only when the sum
// of running weights stays within the core budget, so two CPU-heavy checks
// don't oversubscribe the machine. The usedWeight==0 clause guarantees an
// over-budget check still runs (alone) rather than deadlocking the gate. When
// deps are ready but the budget is full, the check stays Pending and is retried
// once a running check frees its weight.
func (r *Runner) tryStartPending(state *CheckState, wg *sync.WaitGroup) (active, started bool) {
	state.mu.Lock()
	defer state.mu.Unlock()

	switch state.Status {
	case StatusRunning:
		return true, false
	case StatusPending:
		// fall through to admission below
	default:
		return false, false
	}

	if !r.canStart(state) {
		return true, false
	}
	w := state.Definition.EffectiveCpuWeight(r.capacity)
	if r.usedWeight != 0 && r.usedWeight+w > r.capacity {
		return true, false // budget full; retry once a running check frees weight
	}

	state.Status = StatusRunning
	r.usedWeight += w
	wg.Add(1)
	go func() {
		defer wg.Done()
		r.runCheck(state)
		r.mu.Lock()
		r.usedWeight -= w
		r.mu.Unlock()
		r.completedCh <- state
	}()
	return true, true
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
		case StatusCompleted:
		case StatusSkipped:
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
	if !r.noLog {
		logCheckStats(state)
	}
}

// printResult outputs the result of a check.
func (r *Runner) printResult(state *CheckState) {
	r.outputMu.Lock()
	defer r.outputMu.Unlock()

	// Clear status line before printing
	r.clearStatusLineUnsafe()

	def := state.Definition
	prefix := fmt.Sprintf("%s: %s / %s", checks.AppDisplayName(def.App), def.Tech, def.CLIName())
	paddedPrefix := r.padPrefix(prefix)

	switch state.Status {
	case StatusCompleted:
		msg := state.Result.Message
		statusColor := colorGreen
		statusText := "OK"
		if state.Result.Code == checks.ResultWarning {
			statusColor = colorYellow
			statusText = "warn"
		}
		// Message color: green if changes were made, dim/gray otherwise
		msgColor := colorDim
		if state.Result.MadeChanges {
			msgColor = colorGreen
		}
		if strings.Contains(msg, "\n") {
			fmt.Printf("• %s... %s%s%s (%s)\n", paddedPrefix, statusColor, statusText, colorReset, formatDuration(state.Duration))
			fmt.Printf("  %s%s%s\n", msgColor, indentMultiline(msg, "  "), colorReset)
		} else {
			fmt.Printf("• %s... %s%s%s (%s) - %s%s%s\n", paddedPrefix, statusColor, statusText, colorReset, formatDuration(state.Duration), msgColor, msg, colorReset)
		}

	case StatusCached:
		// Inputs unchanged since the last passing run: ~0s, replay the pass's
		// own summary so the line keeps real context, with a clear (cached) tag.
		fmt.Printf("• %s... %sOK%s %s(cached)%s - %s%s%s\n",
			paddedPrefix, colorGreen, colorReset, colorDim, colorReset, colorDim, state.Result.Message, colorReset)

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
	prefix := fmt.Sprintf("%s: %s / %s", checks.AppDisplayName(def.App), def.Tech, def.CLIName())
	paddedPrefix := r.padPrefix(prefix)
	fmt.Printf("• %s... %sBLOCKED%s (dependency %s failed)\n", paddedPrefix, colorYellow, colorReset, depID)
	if !r.noLog {
		logCheckStats(state)
	}
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
			running = append(running, state.Definition.CLIName())
		}
		state.mu.Unlock()
	}

	if len(running) == 0 {
		return
	}

	const maxLen = 120
	const prefix = "Waiting for: "

	// Try to fit as many checks as possible with "... and N more" suffix
	line := prefix + strings.Join(running, ", ")
	if len(line) <= maxLen {
		// All checks fit
	} else {
		// Find how many checks fit with the suffix
		for i := len(running) - 1; i >= 1; i-- {
			remaining := len(running) - i
			suffix := fmt.Sprintf("... and %d more", remaining)
			partial := prefix + strings.Join(running[:i], ", ") + " " + suffix
			if len(partial) <= maxLen {
				line = partial
				break
			}
		}
		// If even one check doesn't fit, just show the count
		if len(line) > maxLen {
			line = fmt.Sprintf("%s%d checks running", prefix, len(running))
		}
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
