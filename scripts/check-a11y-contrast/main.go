// Package main is the design-time WCAG contrast checker for Cmdr.
//
// Parses `apps/desktop/src/app.css` for --color-* design tokens (light and
// dark), walks `apps/desktop/src/**/*.svelte`, and for each element that
// declares both a text color and a background computes the WCAG 2.2 contrast
// ratio in both modes. Flags pairs below 4.5:1 (3:1 for large text).
//
// Run: go run ./scripts/check-a11y-contrast
// Exit: 0 on clean, 1 on violations.
package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func main() {
	verbose := flag.Bool("verbose", false, "Show warnings and per-pair detail")
	flag.Parse()

	rootDir, err := findRootDir()
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	appCSSPath := filepath.Join(rootDir, "apps", "desktop", "src", "app.css")
	cssContent, err := os.ReadFile(appCSSPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError reading app.css: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	vars := ParseAppCSS(string(cssContent))
	analyzer := NewAnalyzer(vars)

	srcDir := filepath.Join(rootDir, "apps", "desktop", "src")

	var allFindings []Finding
	fileCount := 0

	err = filepath.Walk(srcDir, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if info.IsDir() {
			return nil
		}
		if filepath.Ext(path) != ".svelte" {
			return nil
		}
		content, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		parsed := ParseSvelteFile(path, string(content))
		if len(parsed.Rules) == 0 {
			return nil
		}
		fileCount++
		allFindings = append(allFindings, analyzer.AnalyzeFile(parsed)...)
		return nil
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError walking src: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	// Also check app.css global class rules (like `.cmdr-tooltip`,
	// `.cmdr-tooltip-kbd`). They're not scoped but they're design tokens too.
	appCSSRules := parseRulesFromCSS(appCSSPath, string(cssContent))
	if len(appCSSRules) > 0 {
		pf := &ParsedFile{Path: appCSSPath, Rules: appCSSRules}
		allFindings = append(allFindings, analyzer.AnalyzeFile(pf)...)
	}

	violations := FilterViolations(allFindings)
	warnings := append([]string{}, analyzer.Warnings...)
	for _, f := range allFindings {
		warnings = append(warnings, f.Warnings...)
	}
	warnings = joinWarnings(warnings)

	hasViolations := Report(violations, warnings, rootDir, *verbose)

	summary := Summary(fileCount, analyzer.RulesEvaluated, len(allFindings), len(violations))
	if hasViolations {
		fmt.Printf("%s❌ %s%s\n", colorRed, summary, colorReset)
		os.Exit(1)
	}
	fmt.Printf("%s✅ No contrast violations. %s%s\n", colorGreen, summary, colorReset)
}

// parseRulesFromCSS reuses the Svelte rule parser on a raw CSS file.
// app.css isn't inside a `<style>` block, so we emulate one.
func parseRulesFromCSS(path, content string) []Rule {
	// Strip the dark-mode block so we don't double-attribute rules. The
	// dark-mode block only contains variable overrides and a few .cmdr-tooltip
	// overrides; we already evaluate per-mode via the variable table, and
	// .cmdr-tooltip dark overrides use rgba() literals that we don't track.
	wrapped := "<style>\n" + content + "\n</style>"
	pf := ParseSvelteFile(path, wrapped)
	return pf.Rules
}

// findRootDir walks up looking for the monorepo marker.
func findRootDir() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}
	for {
		marker := filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")
		if _, err := os.Stat(marker); err == nil {
			return dir, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("could not find project root (missing %s)", strings.TrimPrefix(marker, dir))
		}
		dir = parent
	}
}
