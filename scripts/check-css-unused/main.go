// Package main provides a tool to detect unused and undefined CSS classes and custom properties.
// Run: cd scripts/check-css-unused && go run .
// Or:  go run -C scripts/check-css-unused .
package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
)

func main() {
	verbose := flag.Bool("verbose", false, "Show file locations for each issue")
	flag.Parse()

	rootDir, err := findRootDir()
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	srcDir := filepath.Join(rootDir, "apps", "desktop", "src")

	// Scan the codebase
	result, err := ScanDesktopApp(srcDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError scanning files: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	// Analyze for issues
	issues := AnalyzeResults(result)

	// Report findings
	issues.Report(*verbose)

	if issues.HasIssues() {
		fmt.Printf("%s❌ %s%s\n", colorRed, issues.Summary(), colorReset)
		fmt.Println()
		fmt.Println("To allowlist items, edit: scripts/check-css-unused/allowlist.go")
		os.Exit(1)
	}

	fmt.Printf("%s✅ No CSS issues found%s\n", colorGreen, colorReset)
}
