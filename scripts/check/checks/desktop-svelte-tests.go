package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

// CoverageThreshold is the minimum line coverage percentage required.
const CoverageThreshold = 70.0

// coverageSatisfiedMarginPct is the hysteresis band above CoverageThreshold:
// an allowlist entry is only reported as removable when the file's coverage is
// at least threshold+margin. Entries hovering right at the threshold would
// otherwise churn (removed this week, the check fails when coverage dips to
// 69% next week), so that band stays silently allowlisted.
const coverageSatisfiedMarginPct = 5.0

// CoverageMetric represents coverage data for a single metric.
type CoverageMetric struct {
	Total   int     `json:"total"`
	Covered int     `json:"covered"`
	Skipped int     `json:"skipped"`
	Pct     float64 `json:"pct"`
}

// FileCoverage represents coverage data for a single file.
type FileCoverage struct {
	Lines      CoverageMetric `json:"lines"`
	Statements CoverageMetric `json:"statements"`
	Functions  CoverageMetric `json:"functions"`
	Branches   CoverageMetric `json:"branches"`
}

// CoverageAllowlist represents the allowlist configuration.
type CoverageAllowlist struct {
	Comment string                    `json:"$comment"`
	Files   map[string]AllowlistEntry `json:"files"`
}

// AllowlistEntry represents a single allowlisted file entry.
type AllowlistEntry struct {
	Reason string `json:"reason"`
}

// coverageStaleness holds the allowlist entries that look unneeded: dead
// (the file no longer exists) and satisfied (the file now meets the coverage
// threshold with margin to spare, so the "can't be tested" reason no longer
// holds).
type coverageStaleness struct {
	dead      []string // relative paths (allowlist keys)
	satisfied []string // formatted "path (NN.N%)" lines
}

// findStaleCoverageEntries checks every allowlist entry against the filesystem
// and the freshly produced coverage data.
func findStaleCoverageEntries(desktopDir string, allowlist CoverageAllowlist, coverage map[string]FileCoverage) coverageStaleness {
	var stale coverageStaleness
	srcPrefix := filepath.Join(desktopDir, "src", "lib") + "/"

	for _, relPath := range sortedKeys(allowlist.Files) {
		if !fileExists(filepath.Join(desktopDir, "src", "lib", relPath)) {
			stale.dead = append(stale.dead, relPath)
			continue
		}
		if cov, ok := coverage[srcPrefix+relPath]; ok && cov.Lines.Pct >= CoverageThreshold+coverageSatisfiedMarginPct {
			stale.satisfied = append(stale.satisfied, fmt.Sprintf("%s (%.1f%%)", relPath, cov.Lines.Pct))
		}
	}
	return stale
}

// shrinkwrapCoverageAllowlist removes the given dead entries from the
// allowlist and rewrites coverage-allowlist.json (preserving the comment and
// the surviving entries' reasons).
func shrinkwrapCoverageAllowlist(desktopDir string, allowlist *CoverageAllowlist, dead []string) error {
	for _, relPath := range dead {
		delete(allowlist.Files, relPath)
	}
	return writeJSONAllowlist(filepath.Join(desktopDir, "coverage-allowlist.json"), allowlist)
}

// coverageRun bundles a single svelte-tests invocation's vitest command with
// its private coverage output location. Each run writes to its own temp
// reportsDirectory (via VITEST_COVERAGE_DIR, read by vitest.config.ts) so two
// concurrent `pnpm check svelte-tests` runs can't interact: v8 cleans
// reportsDirectory/.tmp at run boundaries, so a shared directory means one
// run's cleanup deletes the other's in-flight worker files, crashing it with
// ENOENT. Isolation, not serialization, keeps both runs green.
type coverageRun struct {
	cmd        *exec.Cmd
	reportsDir string
	summary    string
}

// newCoverageRun creates a fresh per-invocation coverage output dir under the
// OS temp dir and configures the vitest command to write there. The caller
// removes reportsDir when done.
func newCoverageRun(desktopDir string) (*coverageRun, error) {
	reportsDir, err := os.MkdirTemp("", "cmdr-svelte-coverage-*")
	if err != nil {
		return nil, fmt.Errorf("failed to create coverage temp dir: %w", err)
	}
	cmd := exec.Command("pnpm", "test:coverage")
	cmd.Dir = desktopDir
	cmd.Env = append(os.Environ(), "VITEST_COVERAGE_DIR="+reportsDir)
	return &coverageRun{
		cmd:        cmd,
		reportsDir: reportsDir,
		summary:    filepath.Join(reportsDir, "coverage-summary.json"),
	}, nil
}

// RunSvelteTests runs Svelte unit tests with Vitest and checks coverage.
func RunSvelteTests(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	// Run tests with coverage using pnpm, into a private per-invocation dir.
	run, err := newCoverageRun(desktopDir)
	if err != nil {
		return CheckResult{}, err
	}
	defer os.RemoveAll(run.reportsDir)
	output, err := RunCommand(run.cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("svelte tests failed\n%s", indentOutput(output))
	}

	// Extract test count from output. Strip ANSI first: vitest colorizes its
	// summary when it thinks the output is a terminal (the count is wrapped in
	// color codes like `Tests  \x1b[1m\x1b[32m3318\x1b[39m passed`), which the
	// raw regex can't match — that's why a contended full-suite run can fall
	// through to the "all" fallback while an isolated run shows the count.
	ansiRe := regexp.MustCompile(`\x1b\[[0-9;]*m`)
	clean := ansiRe.ReplaceAllString(output, "")
	testCountRe := regexp.MustCompile(`Tests\s+(\d+) passed`)
	testMatches := testCountRe.FindStringSubmatch(clean)
	testCount := "all"
	if len(testMatches) > 1 {
		testCount = testMatches[1]
	}

	// Parse coverage summary
	coverageData, err := os.ReadFile(run.summary)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to read coverage summary: %w", err)
	}

	var coverage map[string]FileCoverage
	if err := json.Unmarshal(coverageData, &coverage); err != nil {
		return CheckResult{}, fmt.Errorf("failed to parse coverage summary: %w", err)
	}

	// Load allowlist
	allowlistFile := filepath.Join(desktopDir, "coverage-allowlist.json")
	allowlist := CoverageAllowlist{Files: make(map[string]AllowlistEntry)}
	if allowlistData, err := os.ReadFile(allowlistFile); err == nil {
		if err := json.Unmarshal(allowlistData, &allowlist); err != nil {
			return CheckResult{}, fmt.Errorf("failed to parse coverage allowlist: %w", err)
		}
	}

	if err := checkCoverageThresholds(desktopDir, allowlist, coverage, clean); err != nil {
		return CheckResult{}, err
	}

	// Shrink-wrap the allowlist: drop entries whose file is gone (auto-fix
	// locally, report-only in CI) and surface entries whose file now clears
	// the threshold comfortably (warn; removal is a judgment call because the
	// reason may say "tested elsewhere" and coverage oscillates).
	stale := findStaleCoverageEntries(desktopDir, allowlist, coverage)
	staleNotes, madeChanges, err := applyCoverageShrinkwrap(ctx, desktopDir, &allowlist, stale)
	if err != nil {
		return CheckResult{}, err
	}

	return buildSvelteTestResult(ctx, testCount, staleNotes, madeChanges, stale), nil
}

// buildSvelteTestResult assembles the pass/warn CheckResult from the parsed
// test count and the allowlist shrink-wrap outcome. Warn (not fail) when a
// satisfied entry surfaced, or in CI when a dead entry would be auto-removed
// locally.
func buildSvelteTestResult(ctx *CheckContext, testCount string, staleNotes []string, madeChanges bool, stale coverageStaleness) CheckResult {
	passMsg := "All tests passed"
	count := 0
	if testCount != "all" {
		count, _ = strconv.Atoi(testCount)
		passMsg = fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))
	}
	result := Success(passMsg)
	if len(staleNotes) > 0 {
		result.Message = passMsg + "; " + strings.Join(staleNotes, "; ")
		result.MadeChanges = madeChanges
		if len(stale.satisfied) > 0 || (ctx.CI && len(stale.dead) > 0) {
			result.Code = ResultWarning
		}
	}
	if count > 0 {
		result.Total = count
	}
	return result
}

// checkCoverageThresholds returns an error listing every non-allowlisted file
// under src/lib whose line coverage is below the threshold. `cleanOutput` is
// the ANSI-stripped vitest output, used to append run diagnostics.
func checkCoverageThresholds(desktopDir string, allowlist CoverageAllowlist, coverage map[string]FileCoverage, cleanOutput string) error {
	var lowCoverageFiles []string
	srcPrefix := filepath.Join(desktopDir, "src", "lib") + "/"

	for filePath, fileCov := range coverage {
		if filePath == "total" {
			continue
		}

		relPath, _ := strings.CutPrefix(filePath, srcPrefix)

		if _, ok := allowlist.Files[relPath]; ok {
			continue
		}

		if fileCov.Lines.Pct < CoverageThreshold {
			lowCoverageFiles = append(lowCoverageFiles,
				fmt.Sprintf("  %s: %.1f%% (threshold: %.0f%%)", relPath, fileCov.Lines.Pct, CoverageThreshold))
		}
	}
	if len(lowCoverageFiles) == 0 {
		return nil
	}

	sort.Strings(lowCoverageFiles)
	errorMsg := "Files below coverage threshold:\n"
	for _, f := range lowCoverageFiles {
		errorMsg += "      " + f + "\n"
	}
	// Make the failure self-diagnosing. A contended run has been seen (once)
	// to leave a file that HAS a dedicated test reading 0% — i.e. the run was
	// incomplete, not a real coverage gap. It couldn't be reproduced under
	// CPU+memory load (see docs/notes/check-cpu-contention.md), so it's rare;
	// surfacing vitest's run tallies + any worker-death lines tells the next
	// occurrence apart from a genuine drop, instead of swallowing the output.
	errorMsg += "\n      If a below-threshold file has a dedicated test, the run was likely\n" +
		"      incomplete (rare, load-related) — re-run `--check svelte-tests` standalone\n" +
		"      before trusting this. Genuine gap? Add it to coverage-allowlist.json with a reason."
	if diag := vitestRunDiagnostics(cleanOutput); diag != "" {
		errorMsg += "\n\n      vitest run context (watch the skip count + any worker errors):\n" + diag
	}
	return fmt.Errorf("coverage below threshold for %d files\n%s", len(lowCoverageFiles), errorMsg)
}

// applyCoverageShrinkwrap turns the staleness verdicts into action: dead
// entries get removed from the allowlist file on local runs (report-only in
// CI), satisfied entries are always surfaced as a note for an agent to judge.
func applyCoverageShrinkwrap(ctx *CheckContext, desktopDir string, allowlist *CoverageAllowlist, stale coverageStaleness) (notes []string, madeChanges bool, err error) {
	if len(stale.dead) > 0 {
		if ctx.CI {
			notes = append(notes, fmt.Sprintf(
				"%d dead coverage-allowlist %s (file gone; a local run removes them): %s",
				len(stale.dead), Pluralize(len(stale.dead), "entry", "entries"), strings.Join(stale.dead, ", ")))
		} else {
			if err := shrinkwrapCoverageAllowlist(desktopDir, allowlist, stale.dead); err != nil {
				return nil, false, err
			}
			reformatWithOxfmt(ctx.RootDir, "apps/desktop/coverage-allowlist.json")
			madeChanges = true
			notes = append(notes, fmt.Sprintf(
				"removed %d dead coverage-allowlist %s: %s",
				len(stale.dead), Pluralize(len(stale.dead), "entry", "entries"), strings.Join(stale.dead, ", ")))
		}
	}
	if len(stale.satisfied) > 0 {
		notes = append(notes, fmt.Sprintf(
			"%d coverage-allowlist %s look unneeded (coverage ≥ %.0f%%) — remove from coverage-allowlist.json or keep with an updated reason:\n  %s",
			len(stale.satisfied), Pluralize(len(stale.satisfied), "entry", "entries"),
			CoverageThreshold+coverageSatisfiedMarginPct, strings.Join(stale.satisfied, "\n  ")))
	}
	return notes, madeChanges, nil
}

// vitestRunDiagnostics pulls the lines that reveal whether a vitest run was
// complete: the `Test Files` / `Tests` tallies (a skip count above the usual
// handful means files didn't run, so coverage is unreliable) plus any
// worker-death / heap-limit errors. Returned indented for the failure message.
// `cleanOutput` must already have ANSI stripped.
func vitestRunDiagnostics(cleanOutput string) string {
	var out []string
	for line := range strings.SplitSeq(cleanOutput, "\n") {
		t := strings.TrimSpace(line)
		if strings.HasPrefix(t, "Test Files ") || strings.HasPrefix(t, "Tests ") ||
			strings.Contains(t, "Worker terminated") || strings.Contains(t, "Channel closed") ||
			strings.Contains(t, "reached heap limit") || strings.Contains(t, "FATAL ERROR") ||
			strings.Contains(t, "closed unexpectedly") || strings.Contains(t, "worker exited") {
			out = append(out, "      "+t)
		}
	}
	return strings.Join(out, "\n")
}
