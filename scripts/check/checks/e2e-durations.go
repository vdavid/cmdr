package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// E2E test duration flagger: after a successful E2E run (macOS Playwright
// shards or the Linux Docker suite), every individual test that took more
// than e2eSlowTestThresholdMs wall-clock gets flagged as a warning. The
// analysis is embedded in the two E2E checks (not a separate registry check)
// because the JSON reports are per-run /tmp artifacts: a standalone check
// with DependsOn would still run when the E2E check wasn't selected (deps
// outside the run set count as satisfied) and would then warn about a stale
// previous run's data.
//
// Warn-only by design: a slow test never fails the suite, it converts the
// check's Success into a yellow `warn` line listing the offenders.
const (
	// e2eSlowTestThresholdMs is the per-test wall-clock budget. The E2E
	// suites were hard-won down to under 2 s per test; this flags regressions.
	e2eSlowTestThresholdMs = 2000

	// e2eDurationStaleMarginPct: an allowlisted test must drop this far below
	// the threshold before its entry is reported as a stale candidate.
	// Deliberately wider than file-length's 10% ratchet buffer: wall-clock
	// durations oscillate run to run (machine load, cold caches), so a test
	// hovering at 1.9 s must not cause remove/re-add churn.
	e2eDurationStaleMarginPct = 25
)

// e2eDurationAllowlist is the on-disk shape of e2e-duration-allowlist.json.
// Entries map a test key (`<spec file>::<describe chain>::<title>`) to a
// reason. Sections are per platform because the same test can be slow on
// Linux Docker but fine on macOS (or vice versa); each E2E check judges only
// its own section, so a macOS run never reports a Linux-only entry as stale.
type e2eDurationAllowlist struct {
	Comment string            `json:"$comment,omitempty"`
	Macos   map[string]string `json:"macos,omitempty"`
	Linux   map[string]string `json:"linux,omitempty"`
}

// platformEntries returns the section for "macos" or "linux".
func (a *e2eDurationAllowlist) platformEntries(platform string) map[string]string {
	if platform == "linux" {
		return a.Linux
	}
	return a.Macos
}

// setPlatformEntries replaces the section for "macos" or "linux".
func (a *e2eDurationAllowlist) setPlatformEntries(platform string, entries map[string]string) {
	if platform == "linux" {
		a.Linux = entries
	} else {
		a.Macos = entries
	}
}

// e2eDurationAllowlistPath returns the allowlist location (next to the check
// source files, like file-length-allowlist.json).
func e2eDurationAllowlistPath(rootDir string) string {
	return filepath.Join(rootDir, "scripts", "check", "checks", "e2e-duration-allowlist.json")
}

// loadE2EDurationAllowlist reads the allowlist JSON. A missing or unparsable
// file yields an empty allowlist (every slow test gets reported).
func loadE2EDurationAllowlist(rootDir string) e2eDurationAllowlist {
	var list e2eDurationAllowlist
	data, err := os.ReadFile(e2eDurationAllowlistPath(rootDir))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil {
		return e2eDurationAllowlist{}
	}
	return list
}

// e2eTestDuration is one test's wall-clock cost in a run.
type e2eTestDuration struct {
	// key is `<spec file>::<describe chain joined with " › ">::<title>`,
	// matching the allowlist key format.
	key string
	// durMs is the maximum single-attempt duration across retries: the worst
	// real execution of the test, without double-counting flaky retries.
	durMs int
}

// Subset of Playwright's JSON reporter output (same shape as
// scripts/e2e-test-timings); unknown fields are ignored.
type e2eJSONReport struct {
	Suites []e2eJSONSuite `json:"suites"`
}

type e2eJSONSuite struct {
	Title  string         `json:"title"`
	File   string         `json:"file"`
	Specs  []e2eJSONSpec  `json:"specs"`
	Suites []e2eJSONSuite `json:"suites"`
}

type e2eJSONSpec struct {
	Title string        `json:"title"`
	Tests []e2eJSONTest `json:"tests"`
}

type e2eJSONTest struct {
	Results []e2eJSONResult `json:"results"`
}

type e2eJSONResult struct {
	DurationMs int `json:"duration"`
}

// parsePlaywrightDurations reads one Playwright JSON report and returns the
// per-test max-attempt durations. Duplicate keys (parameterized tests with
// identical titles) collapse to the slowest instance. Skipped tests appear
// with 0 ms (no results).
func parsePlaywrightDurations(path string) ([]e2eTestDuration, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read %s: %w", path, err)
	}
	var report e2eJSONReport
	if err := json.Unmarshal(data, &report); err != nil {
		return nil, fmt.Errorf("parse %s: %w", path, err)
	}

	byKey := map[string]int{}
	for _, s := range report.Suites {
		collectSuiteDurations(s, s.File, nil, byKey)
	}

	durations := make([]e2eTestDuration, 0, len(byKey))
	for _, key := range sortedKeys(byKey) {
		durations = append(durations, e2eTestDuration{key: key, durMs: byKey[key]})
	}
	return durations, nil
}

// collectSuiteDurations recurses Playwright's nested suite tree, accumulating
// the describe chain. The top-level suite is the spec file itself (its title
// equals the file name), so it contributes no chain segment.
func collectSuiteDurations(s e2eJSONSuite, file string, describe []string, byKey map[string]int) {
	scope := describe
	if s.Title != "" && s.Title != filepath.Base(file) && s.Title != file {
		scope = append(append([]string{}, describe...), s.Title)
	}
	for _, sp := range s.Specs {
		key := file + "::" + strings.Join(scope, " › ") + "::" + sp.Title
		maxMs := 0
		for _, t := range sp.Tests {
			for _, r := range t.Results {
				maxMs = max(maxMs, r.DurationMs)
			}
		}
		byKey[key] = max(byKey[key], maxMs)
	}
	for _, child := range s.Suites {
		collectSuiteDurations(child, file, scope, byKey)
	}
}

// e2eDurationAnalysis is the verdict over one run's durations.
type e2eDurationAnalysis struct {
	totalTests int
	// slow holds tests over the threshold and not allowlisted, slowest first.
	slow []e2eTestDuration
	// allowlisted counts tests over the threshold that an entry suppressed.
	allowlisted int
	// staleCandidates are entries whose test now runs comfortably under the
	// threshold (below threshold minus the stale margin). Reported for an
	// agent to judge, never auto-removed: the reason may encode intent and
	// durations oscillate.
	staleCandidates []string
	// deadEntries are entries whose test no longer exists in the run at all
	// (removed or renamed; skipped tests still appear in the report).
	deadEntries []string
}

// analyzeE2EDurations applies the threshold and one platform's allowlist
// section to a run's durations.
func analyzeE2EDurations(durations []e2eTestDuration, entries map[string]string) e2eDurationAnalysis {
	analysis := e2eDurationAnalysis{totalTests: len(durations)}
	staleCeilingMs := e2eSlowTestThresholdMs * (100 - e2eDurationStaleMarginPct) / 100

	seen := map[string]int{}
	for _, d := range durations {
		seen[d.key] = d.durMs
		_, allowlisted := entries[d.key]
		switch {
		case d.durMs > e2eSlowTestThresholdMs && allowlisted:
			analysis.allowlisted++
		case d.durMs > e2eSlowTestThresholdMs:
			analysis.slow = append(analysis.slow, d)
		case allowlisted && d.durMs < staleCeilingMs:
			analysis.staleCandidates = append(analysis.staleCandidates, d.key)
		}
	}
	for _, key := range sortedKeys(entries) {
		if _, ok := seen[key]; !ok {
			analysis.deadEntries = append(analysis.deadEntries, key)
		}
	}

	sort.SliceStable(analysis.slow, func(i, j int) bool { return analysis.slow[i].durMs > analysis.slow[j].durMs })
	sort.Strings(analysis.staleCandidates)
	return analysis
}

// applyE2EDurationWarnings is the post-run hook both E2E checks call on their
// success path. It parses the run's JSON reports, flags tests over the
// threshold, shrink-wraps dead allowlist entries (locally; report-only in
// CI), and converts the passed-in Success result into a warning when there's
// anything to surface. Failures of the analysis itself (missing/unparsable
// report) never fail or warn the check: the E2E result stands, with a note.
func applyE2EDurationWarnings(ctx *CheckContext, result CheckResult, reportPaths []string, platform string) CheckResult {
	byKey := map[string]int{}
	for _, path := range reportPaths {
		durations, err := parsePlaywrightDurations(path)
		if err != nil {
			// Without every report, dead-entry detection would mass-flag the
			// missing shard's entries, so skip the whole analysis.
			result.Message += fmt.Sprintf("; duration analysis skipped (%v)", err)
			return result
		}
		for _, d := range durations {
			byKey[d.key] = max(byKey[d.key], d.durMs)
		}
	}

	durations := make([]e2eTestDuration, 0, len(byKey))
	for _, key := range sortedKeys(byKey) {
		durations = append(durations, e2eTestDuration{key: key, durMs: byKey[key]})
	}

	allowlist := loadE2EDurationAllowlist(ctx.RootDir)
	analysis := analyzeE2EDurations(durations, allowlist.platformEntries(platform))

	var notes []string
	if len(analysis.slow) > 0 {
		var sb strings.Builder
		fmt.Fprintf(&sb, "%d %s over the %.1fs budget (warn-only):",
			len(analysis.slow), Pluralize(len(analysis.slow), "test", "tests"), float64(e2eSlowTestThresholdMs)/1000)
		for _, d := range analysis.slow {
			fmt.Fprintf(&sb, "\n  - %s (%.1fs)", formatE2ETestKey(d.key), float64(d.durMs)/1000)
		}
		sb.WriteString("\n  Speed the test up, or allowlist it with a reason in scripts/check/checks/e2e-duration-allowlist.json (new entries need David's OK).")
		notes = append(notes, sb.String())
	}
	if len(analysis.staleCandidates) > 0 {
		notes = append(notes, fmt.Sprintf("allowlist entries now well under the budget on %s (review the reason, remove if obsolete): %s",
			platform, strings.Join(formatE2ETestKeys(analysis.staleCandidates), ", ")))
	}
	if len(analysis.deadEntries) > 0 {
		if ctx.CI {
			notes = append(notes, fmt.Sprintf("dead allowlist entries (test gone; a local run removes them): %s",
				strings.Join(formatE2ETestKeys(analysis.deadEntries), ", ")))
		} else {
			entries := allowlist.platformEntries(platform)
			for _, key := range analysis.deadEntries {
				delete(entries, key)
			}
			allowlist.setPlatformEntries(platform, entries)
			if err := writeJSONAllowlist(e2eDurationAllowlistPath(ctx.RootDir), allowlist); err == nil {
				reformatWithOxfmt(ctx.RootDir, "scripts/check/checks/e2e-duration-allowlist.json")
				result.MadeChanges = true
				notes = append(notes, fmt.Sprintf("removed dead allowlist entries (test gone): %s",
					strings.Join(formatE2ETestKeys(analysis.deadEntries), ", ")))
			} else {
				notes = append(notes, fmt.Sprintf("could not rewrite allowlist: %v", err))
			}
		}
	}

	if len(notes) == 0 {
		if analysis.totalTests > 0 {
			suffix := fmt.Sprintf("; all within the %.1fs budget", float64(e2eSlowTestThresholdMs)/1000)
			if analysis.allowlisted > 0 {
				suffix += fmt.Sprintf(" (%d allowlisted)", analysis.allowlisted)
			}
			result.Message += suffix
		}
		return result
	}

	result.Code = ResultWarning
	result.Message += "\n" + strings.Join(notes, "\n")
	return result
}

// formatE2ETestKey renders an allowlist key for humans: the `::` separators
// become ` › ` and the empty describe-chain segment disappears.
func formatE2ETestKey(key string) string {
	parts := strings.SplitN(key, "::", 3)
	nonEmpty := make([]string, 0, len(parts))
	for _, p := range parts {
		if p != "" {
			nonEmpty = append(nonEmpty, p)
		}
	}
	return strings.Join(nonEmpty, " › ")
}

func formatE2ETestKeys(keys []string) []string {
	out := make([]string, len(keys))
	for i, key := range keys {
		out[i] = formatE2ETestKey(key)
	}
	return out
}
