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

// RunSvelteTests runs Svelte unit tests with Vitest and checks coverage.
func RunSvelteTests(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	// Run tests with coverage using pnpm
	cmd := exec.Command("pnpm", "test:coverage")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
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
	coverageFile := filepath.Join(desktopDir, "coverage", "coverage-summary.json")
	coverageData, err := os.ReadFile(coverageFile)
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

	// Check coverage for each file
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

	if len(lowCoverageFiles) > 0 {
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
		if diag := vitestRunDiagnostics(clean); diag != "" {
			errorMsg += "\n\n      vitest run context (watch the skip count + any worker errors):\n" + diag
		}
		return CheckResult{}, fmt.Errorf("coverage below threshold for %d files\n%s", len(lowCoverageFiles), errorMsg)
	}

	if testCount == "all" {
		return Success("All tests passed"), nil
	}
	count, _ := strconv.Atoi(testCount)
	result := Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests")))
	result.Total = count
	return result, nil
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
