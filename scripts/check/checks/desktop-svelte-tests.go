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

	// Extract test count from output
	testCountRe := regexp.MustCompile(`Tests\s+(\d+) passed`)
	testMatches := testCountRe.FindStringSubmatch(output)
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
		errorMsg += "\n      To allowlist a file, add it to coverage-allowlist.json with a reason."
		return CheckResult{}, fmt.Errorf("coverage below threshold for %d files\n%s", len(lowCoverageFiles), errorMsg)
	}

	if testCount == "all" {
		return Success("All tests passed"), nil
	}
	count, _ := strconv.Atoi(testCount)
	return Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))), nil
}
