package checks

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// AllowDroppingTimeoutComment opts a single line out of the dropping-timeout
// check. Put it on the line above the flagged line, or as a trailing comment,
// with a reason that says what the dropped future is holding (and why that's
// nothing).
//
//	// allowed-dropping-timeout: a Mutex wait holds nothing on the wire
//	tokio::time::timeout(Duration::from_secs(DEVICE_LOCK_WAIT_SECS), device_arc.lock())
const AllowDroppingTimeoutComment = "// allowed-dropping-timeout:"

// droppingTimeoutPatterns are the two ways Cmdr can drop an in-flight future.
// `tokio::time::timeout(d, fut)` drops `fut` when the deadline fires;
// `JoinHandle::abort()` drops the task's future at its current await point.
// Both are fine over a plain value; over an mtp-rs call they abandon a PTP
// transaction mid-data-phase, which wedges real phones until they're replugged.
var droppingTimeoutPatterns = []string{
	"tokio::time::timeout(",
	"time::timeout(",
	".abort()",
}

type droppingTimeoutSite struct {
	relPath string
	line    int
	text    string
}

// RunMtpDroppingTimeout fails the build if any non-test file under `src/mtp/`
// wraps something in a wall-clock timeout or aborts a task without recording why
// dropping that future is safe.
//
// The MTP session layer is the one place in Cmdr where dropping a future has a
// physical consequence: the device is left mid-transaction, expecting bytes
// nobody will send or holding bytes nobody will read. mtp-rs bounds every USB
// transfer on its own and fails CLEANLY, so an outer wall-clock timeout can only
// ever preempt a clean failure with a wedge. See
// `apps/desktop/src-tauri/src/mtp/connection/CLAUDE.md`.
func RunMtpDroppingTimeout(ctx *CheckContext) (CheckResult, error) {
	mtpDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src", "mtp")

	violations, orphans, scanned, err := scanForDroppingTimeouts(ctx.RootDir, mtpDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to scan MTP Rust files: %w", err)
	}

	var parts []string
	if len(violations) > 0 {
		sort.Slice(violations, func(i, j int) bool {
			if violations[i].relPath == violations[j].relPath {
				return violations[i].line < violations[j].line
			}
			return violations[i].relPath < violations[j].relPath
		})
		var sb strings.Builder
		for _, v := range violations {
			sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", v.relPath, v.line, v.text))
		}
		parts = append(parts, fmt.Sprintf(
			"found %d %s that can drop an in-flight future under `src/mtp/` "+
				"(dropping an mtp-rs call abandons a PTP transaction mid-data-phase and wedges the user's phone; "+
				"mtp-rs bounds each USB transfer itself and fails cleanly, and `CancelToken` bails at a safe boundary — "+
				"use those instead; add `%s <reason>` on the line above or as a trailing comment when the dropped "+
				"future genuinely holds nothing on the wire):\n%s",
			len(violations), Pluralize(len(violations), "site", "sites"), AllowDroppingTimeoutComment,
			strings.TrimRight(sb.String(), "\n"),
		))
	}
	if len(orphans) > 0 {
		parts = append(parts, formatOrphanDirectives(AllowDroppingTimeoutComment, orphans))
	}
	if len(parts) > 0 {
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	return Success(fmt.Sprintf(
		"%d MTP Rust %s scanned, no unjustified future-dropping timeout or abort",
		scanned, Pluralize(scanned, "file", "files"),
	)), nil
}

func scanForDroppingTimeouts(rootDir, srcDir string) ([]droppingTimeoutSite, []orphanDirective, int, error) {
	var violations []droppingTimeoutSite
	var orphans []orphanDirective
	scanned := 0

	err := filepath.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") {
			return nil
		}
		if isRustTestFile(d.Name()) {
			return nil
		}
		scanned++

		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}

		fileViolations, fileOrphans, scanErr := scanRustFileForDroppingTimeouts(path, relPath)
		if scanErr != nil {
			return scanErr
		}
		violations = append(violations, fileViolations...)
		orphans = append(orphans, fileOrphans...)
		return nil
	})

	return violations, orphans, scanned, err
}

// scanRustFileForDroppingTimeouts scans one file, skipping `#[cfg(test)]` mods
// (a test asserting with a tight timeout drives no real device).
func scanRustFileForDroppingTimeouts(path, relPath string) ([]droppingTimeoutSite, []orphanDirective, error) {
	f, openErr := os.Open(path)
	if openErr != nil {
		return nil, nil, openErr
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)

	var violations []droppingTimeoutSite
	// Reuses the lock-poison check's test-mod skip state machine (same package).
	var state lockPoisonScanState
	tracker := newDirectiveTracker(AllowDroppingTimeoutComment, "//")
	var prev string
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		if site := classifyDroppingTimeoutLine(line, prev, relPath, lineNum, &state, tracker); site != nil {
			violations = append(violations, *site)
		}
		prev = line
	}
	return violations, tracker.orphans(relPath), scanner.Err()
}

func classifyDroppingTimeoutLine(
	line, prev, relPath string,
	lineNum int,
	state *lockPoisonScanState,
	tracker *directiveTracker,
) *droppingTimeoutSite {
	if state.inTestMod {
		state.testModDepth += strings.Count(line, "{") - strings.Count(line, "}")
		if state.testModDepth <= 0 {
			state.inTestMod = false
		}
		return nil
	}

	tracker.observe(lineNum, line)

	trimmed := strings.TrimLeft(line, " \t")
	if strings.HasPrefix(trimmed, "//") {
		return nil
	}

	if strings.Contains(line, "#[cfg(test)]") {
		state.pendingCfgTest = true
	}
	if state.pendingCfgTest && strings.Contains(line, "mod ") && strings.Contains(line, "{") {
		state.pendingCfgTest = false
		state.testModDepth = strings.Count(line, "{") - strings.Count(line, "}")
		state.inTestMod = state.testModDepth > 0
		return nil
	}

	if !lineHasDroppingTimeout(line) {
		return nil
	}

	if strings.Contains(prev, AllowDroppingTimeoutComment) || strings.Contains(line, AllowDroppingTimeoutComment) {
		tracker.markUsed(lineNum, line, prev)
		return nil
	}

	return &droppingTimeoutSite{relPath: relPath, line: lineNum, text: strings.TrimSpace(line)}
}

func lineHasDroppingTimeout(line string) bool {
	for _, pattern := range droppingTimeoutPatterns {
		if strings.Contains(line, pattern) {
			return true
		}
	}
	return false
}
