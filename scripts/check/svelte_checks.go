package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// PrettierCheck formats code with Prettier.
type PrettierCheck struct{}

func (c *PrettierCheck) Name() string {
	return "Prettier"
}

func (c *PrettierCheck) Run(ctx *CheckContext) error {
	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "format:check")
	} else {
		cmd = exec.Command("pnpm", "format")
	}
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		if ctx.CI {
			return fmt.Errorf("code is not formatted, run pnpm format locally")
		}
		return fmt.Errorf("prettier formatting failed")
	}
	return nil
}

// ESLintCheck lints and fixes code with ESLint.
type ESLintCheck struct{}

func (c *ESLintCheck) Name() string {
	return "ESLint"
}

func (c *ESLintCheck) Run(ctx *CheckContext) error {
	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "lint")
	} else {
		cmd = exec.Command("pnpm", "lint:fix")
	}
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		if ctx.CI {
			return fmt.Errorf("lint errors found, run pnpm lint:fix locally")
		}
		return fmt.Errorf("eslint found unfixable errors")
	}
	return nil
}

// StylelintCheck validates CSS and catches undefined custom properties.
type StylelintCheck struct{}

func (c *StylelintCheck) Name() string {
	return "stylelint"
}

func (c *StylelintCheck) Run(ctx *CheckContext) error {
	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "stylelint")
	} else {
		cmd = exec.Command("pnpm", "stylelint:fix")
	}
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		if ctx.CI {
			return fmt.Errorf("CSS lint errors found, run pnpm stylelint:fix locally")
		}
		return fmt.Errorf("stylelint found unfixable errors")
	}
	return nil
}

// SvelteCheck runs svelte-check for type and a11y validation.
type SvelteCheck struct{}

func (c *SvelteCheck) Name() string {
	return "svelte-check"
}

func (c *SvelteCheck) Run(ctx *CheckContext) error {
	cmd := exec.Command("pnpm", "check")
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := runCommand(cmd, true)
	// svelte-check returns 0 even with warnings, so check output for warnings
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("svelte-check failed")
	}
	// Check for warnings in output (svelte-check reports "X warnings")
	if strings.Contains(output, " warning") && !strings.Contains(output, "0 warnings") {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("svelte-check found warnings")
	}
	return nil
}

// KnipCheck finds unused code, dependencies, and exports.
type KnipCheck struct{}

func (c *KnipCheck) Name() string {
	return "knip"
}

func (c *KnipCheck) Run(ctx *CheckContext) error {
	cmd := exec.Command("pnpm", "knip")
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("knip found unused code or dependencies")
	}
	return nil
}

// SvelteTestsCheck runs Svelte unit tests with Vitest and checks coverage.
type SvelteTestsCheck struct{}

func (c *SvelteTestsCheck) Name() string {
	return "tests"
}

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

func (c *SvelteTestsCheck) Run(ctx *CheckContext) error {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	// Run tests with coverage
	cmd := exec.Command("pnpm", "test:coverage")
	cmd.Dir = desktopDir
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("svelte tests failed")
	}

	// Parse coverage summary
	coverageFile := filepath.Join(desktopDir, "coverage", "coverage-summary.json")
	coverageData, err := os.ReadFile(coverageFile)
	if err != nil {
		return fmt.Errorf("failed to read coverage summary: %w", err)
	}

	var coverage map[string]FileCoverage
	if err := json.Unmarshal(coverageData, &coverage); err != nil {
		return fmt.Errorf("failed to parse coverage summary: %w", err)
	}

	// Load allowlist
	allowlistFile := filepath.Join(desktopDir, "coverage-allowlist.json")
	allowlist := CoverageAllowlist{Files: make(map[string]AllowlistEntry)}
	if allowlistData, err := os.ReadFile(allowlistFile); err == nil {
		if err := json.Unmarshal(allowlistData, &allowlist); err != nil {
			return fmt.Errorf("failed to parse coverage allowlist: %w", err)
		}
	}

	// Check coverage for each file
	var lowCoverageFiles []string
	srcPrefix := filepath.Join(desktopDir, "src", "lib") + "/"

	for filePath, fileCov := range coverage {
		// Skip the "total" entry
		if filePath == "total" {
			continue
		}

		// Get relative path for display and allowlist lookup
		relPath, _ := strings.CutPrefix(filePath, srcPrefix)

		// Check if allowlisted
		if _, ok := allowlist.Files[relPath]; ok {
			continue
		}

		// Check coverage threshold
		if fileCov.Lines.Pct < CoverageThreshold {
			lowCoverageFiles = append(lowCoverageFiles,
				fmt.Sprintf("  %s: %.1f%% (threshold: %.0f%%)", relPath, fileCov.Lines.Pct, CoverageThreshold))
		}
	}

	if len(lowCoverageFiles) > 0 {
		sort.Strings(lowCoverageFiles)
		fmt.Println()
		fmt.Println("      Files below coverage threshold:")
		for _, f := range lowCoverageFiles {
			fmt.Println("      " + f)
		}
		fmt.Println()
		fmt.Println("      To allowlist a file, add it to coverage-allowlist.json with a reason.")
		return fmt.Errorf("coverage below threshold for %d files", len(lowCoverageFiles))
	}

	return nil
}

// E2ETestsCheck runs end-to-end tests with Playwright.
type E2ETestsCheck struct{}

func (c *E2ETestsCheck) Name() string {
	return "E2E tests"
}

func (c *E2ETestsCheck) Run(ctx *CheckContext) error {
	cmd := exec.Command("pnpm", "test:e2e")
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := runCommand(cmd, true)
	if err != nil {
		fmt.Println()
		fmt.Print(indentOutput(output, "      "))
		return fmt.Errorf("e2e tests failed")
	}
	return nil
}
