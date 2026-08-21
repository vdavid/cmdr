package checks

import (
	"fmt"
	"os"
	"regexp"
	"strconv"
	"strings"
)

// Turning a raw E2E transcript into something worth printing. Shared by all three
// consumers of Playwright output: the macOS shard lane, the Linux Docker lane, and
// the binary build. Every function here is pure text in, text out, which is why
// they carry the bulk of the lane's unit tests.

// parsePlaywrightTotals extracts "N passed", "N failed", "N skipped" counts
// from Playwright's tail summary. Missing counters are zero.
func parsePlaywrightTotals(output string) (passed, failed, skipped int) {
	rePassed := regexp.MustCompile(`(\d+) passed`)
	reFailed := regexp.MustCompile(`(\d+) failed`)
	reSkipped := regexp.MustCompile(`(\d+) skipped`)
	if m := rePassed.FindStringSubmatch(output); len(m) > 1 {
		passed, _ = strconv.Atoi(m[1])
	}
	if m := reFailed.FindStringSubmatch(output); len(m) > 1 {
		failed, _ = strconv.Atoi(m[1])
	}
	if m := reSkipped.FindStringSubmatch(output); len(m) > 1 {
		skipped, _ = strconv.Atoi(m[1])
	}
	return passed, failed, skipped
}

// extractE2ETestOutput returns a concise failure summary for E2E test runs.
// The captured output has four sections with stable delimiters:
//
//	§1 setup/build         → trimmed at the last "Starting Tauri app..."
//	§2 per-test progress   → ✓/- markers (and their preceding annotation
//	                          lines) are dropped; ✘ markers and their
//	                          preceding annotation lines are kept
//	§3 failure blocks      → kept verbatim (numbered `N) [tauri] …` blocks
//	                          plus the final `N failed / M flaky / X passed`
//	                          tally)
//	§4 post-ELIFECYCLE     → dropped (this is the Tauri stdout dump and
//	                          out-of-order build output Docker flushes after
//	                          the run exits, already saved in the full log
//	                          file the surrounding error message links to)
//
// If the run died before reaching the test phase (e.g. SMB container setup
// failed silently in desktop-e2e-linux), none of §1, §3, or the tally exist.
// We detect that by absence of all of: the Tauri start marker, a numbered
// failure block, and a `N passed`/`N failed` tally. In that case the full
// pre-ELIFECYCLE transcript is kept, the verbose `docker compose ps` table
// is dropped, and a one-line hint is prepended.
//
// The Tauri-marker check alone is insufficient because the macOS playwright
// shards start Tauri in the Go check (with its stdout going to a log file),
// so the marker never appears in Playwright's stdout, even on a successful
// run.
func extractE2ETestOutput(output string) string {
	// Extract any SMB-stack readiness lines from §1 BEFORE we trim the setup
	// phase away. These banners come from desktop-e2e-linux's pre-flight and
	// post-flight probes and are crucial signal for diagnosing SMB-related
	// test failures: they answer "were the SMB containers healthy when
	// tests started / ended?" Without preserving them, every SMB failure
	// looks like a pure Cmdr-side bug.
	smbBanners := extractSMBBanners(output)

	tauriStarted := strings.Contains(output, "Starting Tauri app...")
	if tauriStarted {
		idx := strings.LastIndex(output, "Starting Tauri app...")
		output = output[idx:]
	}
	if idx := strings.Index(output, "[ELIFECYCLE]"); idx >= 0 {
		if eol := strings.IndexByte(output[idx:], '\n'); eol >= 0 {
			output = output[:idx+eol]
		} else {
			output = output[:idx]
		}
	}
	lines := strings.Split(output, "\n")
	boundary := len(lines)
	for i, line := range lines {
		if failureBlockHeaderRE.MatchString(stripANSI(line)) {
			boundary = i
			break
		}
	}
	kept := filterTestProgress(lines[:boundary])
	kept = append(kept, lines[boundary:]...)

	if isPreTestFailure(output, lines, boundary) {
		kept = dropDockerComposePsTable(kept)
		kept = append(
			[]string{"note: tests did not reach the run phase; failure was in pre-test setup. See full log for details.", ""},
			kept...,
		)
	}

	if len(smbBanners) > 0 {
		kept = append(smbBanners, append([]string{""}, kept...)...)
	}
	return strings.Join(kept, "\n")
}

// smbBannerRE matches the pre-flight and post-flight SMB readiness banners
// emitted by `e2e-linux.sh` (via `log_info`/`log_warn`). ANSI colour codes
// are stripped before matching. Anchored on substring rather than start of
// line so the `[INFO]` / `[WARN]` prefix is tolerated.
var smbBannerRE = regexp.MustCompile(`SMB (?:e2e stack ready|post-flight): .+`)

// extractSMBBanners pulls out the SMB readiness banners (pre-flight and
// post-flight) from raw output. Returned strings have ANSI escapes removed
// and a `[SMB] ` prefix added so they're trivially greppable in the
// failing-test summary.
func extractSMBBanners(output string) []string {
	var out []string
	for line := range strings.SplitSeq(output, "\n") {
		stripped := stripANSI(line)
		if m := smbBannerRE.FindString(stripped); m != "" {
			out = append(out, "[SMB] "+m)
		}
	}
	return out
}

// playwrightTallyRE matches the Playwright run-summary lines like `1 failed`,
// `42 passed (1.2m)`, `3 flaky`. Presence of any of these, or of a
// `\d+) [tauri]` failure block, proves the run reached the test phase.
var playwrightTallyRE = regexp.MustCompile(`(?m)^\s*\d+\s+(?:passed|failed|flaky|skipped)\b`)

// isPreTestFailure reports whether the captured output looks like the run
// died before reaching the Playwright test phase. True only if NONE of these
// are present: the `Starting Tauri app...` marker, a `\d+) [tauri] …`
// failure-block header, or a `\d+ (passed|failed|flaky|skipped)` tally line.
func isPreTestFailure(rawOutput string, prefilterLines []string, failureBlockBoundary int) bool {
	if strings.Contains(rawOutput, "Starting Tauri app...") {
		return false
	}
	if failureBlockBoundary < len(prefilterLines) {
		// failureBlockHeaderRE already matched at this index.
		return false
	}
	return !playwrightTallyRE.MatchString(rawOutput)
}

// dockerPsHeaderRE matches the column header emitted by `docker compose ps`.
// The exact column set varies by Docker version but NAME and IMAGE are always
// the first two, separated by run-length whitespace.
var dockerPsHeaderRE = regexp.MustCompile(`^NAME\s+IMAGE\s+COMMAND\b`)

// dockerPsRowRE matches a `docker compose ps` data row, identified by the
// container-status token `Up <duration> [(state)]`. Only used once a header
// has been seen (see dropDockerComposePsTable), so similar phrases in prose
// can't trigger it.
var dockerPsRowRE = regexp.MustCompile(`\bUp \d+\s+\w+(\s+\((healthy|unhealthy|starting)\))?`)

// dropDockerComposePsTable removes the column header and data rows of any
// `docker compose ps` block embedded in the output. To avoid eating benign
// prose that happens to contain `Up <N> …`, rows are only dropped after a
// matching `NAME IMAGE COMMAND` header line; the next blank line or
// non-matching line ends the table.
func dropDockerComposePsTable(lines []string) []string {
	out := make([]string, 0, len(lines))
	inTable := false
	for _, line := range lines {
		stripped := stripANSI(line)
		if dockerPsHeaderRE.MatchString(stripped) {
			inTable = true
			continue
		}
		if inTable {
			if strings.TrimSpace(stripped) == "" || !dockerPsRowRE.MatchString(stripped) {
				inTable = false
				out = append(out, line)
				continue
			}
			continue
		}
		out = append(out, line)
	}
	return out
}

// failureBlockHeaderRE matches the first line of a Playwright failure entry,
// e.g. "  1) [tauri] › test/e2e-playwright/smb.spec.ts:206:3 › …". This is
// the §2 → §3 boundary.
var failureBlockHeaderRE = regexp.MustCompile(`^\s*\d+\)\s+\[tauri\]\s`)

// ansiEscapeRE matches ANSI CSI escape sequences (e.g. color codes).
var ansiEscapeRE = regexp.MustCompile(`\x1b\[[0-9;]*[A-Za-z]`)

func stripANSI(s string) string {
	return ansiEscapeRE.ReplaceAllString(s, "")
}

// filterTestProgress collapses Playwright per-test progress output: lines
// preceding a ✓ or - marker (and the marker itself) are dropped, while lines
// preceding a ✘ marker (and the marker) are kept. Lines that have no marker
// at all at the end of the section (typically blank padding before §3) are
// kept too.
func filterTestProgress(lines []string) []string {
	out := make([]string, 0, len(lines))
	var buf []string
	for _, line := range lines {
		trimmed := strings.TrimSpace(stripANSI(line))
		switch {
		case strings.HasPrefix(trimmed, "✘"):
			out = append(out, buf...)
			out = append(out, line)
			buf = buf[:0]
		case strings.HasPrefix(trimmed, "✓"), strings.HasPrefix(trimmed, "- "):
			buf = buf[:0]
		default:
			buf = append(buf, line)
		}
	}
	out = append(out, buf...)
	return out
}

// readLogTail reads the last N lines of a log file.
func readLogTail(path string, n int) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Sprintf("(could not read log: %v)", err)
	}
	lines := strings.Split(string(data), "\n")
	start := max(len(lines)-n, 0)
	return strings.Join(lines[start:], "\n")
}

// appendToLogFile appends text to a log file.
func appendToLogFile(path, text string) {
	f, err := os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return
	}
	defer f.Close()
	f.WriteString(text)
}
