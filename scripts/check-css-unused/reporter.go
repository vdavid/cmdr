package main

import (
	"fmt"
	"sort"
	"strings"
)

const (
	colorRed    = "\033[31m"
	colorGreen  = "\033[32m"
	colorYellow = "\033[33m"
	colorReset  = "\033[0m"
)

// Issues holds all detected CSS issues.
type Issues struct {
	UnusedVars       []string
	UnusedClasses    []string
	UndefinedClasses []string

	// For verbose output
	VarDefs      map[string][]string
	ClassDefs    map[string][]string
	ClassUseLocs map[string][]string
}

// AnalyzeResults compares definitions against usages and returns issues.
func AnalyzeResults(result *ScanResult) *Issues {
	issues := &Issues{
		VarDefs:      result.VarDefs,
		ClassDefs:    result.ClassDefs,
		ClassUseLocs: result.ClassUseLocs,
	}

	// Find unused variables (defined but not used)
	for varName := range result.VarDefs {
		if !result.VarUses[varName] && !allowedUnusedVariables[varName] {
			issues.UnusedVars = append(issues.UnusedVars, varName)
		}
	}

	// Find unused classes (defined but not used)
	for className := range result.ClassDefs {
		if !result.ClassUses[className] && !allowedUnusedClasses[className] {
			issues.UnusedClasses = append(issues.UnusedClasses, className)
		}
	}

	// Find undefined classes (used but not defined)
	for className := range result.ClassUses {
		if _, defined := result.ClassDefs[className]; !defined {
			if !allowedUndefinedClasses[className] && !isLikelyExternalClass(className) {
				issues.UndefinedClasses = append(issues.UndefinedClasses, className)
			}
		}
	}

	sort.Strings(issues.UnusedVars)
	sort.Strings(issues.UnusedClasses)
	sort.Strings(issues.UndefinedClasses)

	return issues
}

// isLikelyExternalClass returns true if the class name looks like it comes from
// an external library or framework (Tailwind, etc.) rather than custom CSS.
func isLikelyExternalClass(className string) bool {
	// Common Tailwind/utility-first patterns
	utilityPrefixes := []string{
		"flex", "grid", "block", "inline", "hidden",
		"w-", "h-", "m-", "p-", "mx-", "my-", "px-", "py-",
		"text-", "font-", "bg-", "border-", "rounded-",
		"absolute", "relative", "fixed", "sticky",
		"top-", "right-", "bottom-", "left-",
		"z-", "opacity-", "overflow-",
		"cursor-", "pointer-events-",
		"transition-", "duration-", "ease-",
		"animate-", "transform", "scale-", "rotate-", "translate-",
		"shadow-", "ring-",
		"sr-only", "not-sr-only",
	}
	for _, prefix := range utilityPrefixes {
		if strings.HasPrefix(className, prefix) || className == strings.TrimSuffix(prefix, "-") {
			return true
		}
	}
	return false
}

// Report prints the issues to stdout.
func (issues *Issues) Report(verbose bool) {
	if len(issues.UnusedVars) > 0 {
		fmt.Printf("%s\n=== Unused CSS variables ===%s\n", colorYellow, colorReset)
		for _, varName := range issues.UnusedVars {
			if verbose {
				files := issues.VarDefs[varName]
				fmt.Printf("  --%s  (defined in: %s)\n", varName, strings.Join(files, ", "))
			} else {
				fmt.Printf("  --%s\n", varName)
			}
		}
		fmt.Println()
	}

	if len(issues.UnusedClasses) > 0 {
		fmt.Printf("%s\n=== Unused CSS classes ===%s\n", colorYellow, colorReset)
		for _, className := range issues.UnusedClasses {
			if verbose {
				files := issues.ClassDefs[className]
				fmt.Printf("  .%s  (defined in: %s)\n", className, strings.Join(files, ", "))
			} else {
				fmt.Printf("  .%s\n", className)
			}
		}
		fmt.Println()
	}

	if len(issues.UndefinedClasses) > 0 {
		fmt.Printf("%s\n=== Undefined CSS classes (used but not defined) ===%s\n", colorYellow, colorReset)
		for _, className := range issues.UndefinedClasses {
			if verbose {
				files := issues.ClassUseLocs[className]
				fmt.Printf("  .%s  (used in: %s)\n", className, strings.Join(files, ", "))
			} else {
				fmt.Printf("  .%s\n", className)
			}
		}
		fmt.Println()
	}
}

// HasIssues returns true if any issues were found.
func (issues *Issues) HasIssues() bool {
	return len(issues.UnusedVars) > 0 || len(issues.UnusedClasses) > 0 || len(issues.UndefinedClasses) > 0
}

// Summary returns a summary string of all issues.
func (issues *Issues) Summary() string {
	return fmt.Sprintf("Found %d unused variables, %d unused classes, %d undefined classes",
		len(issues.UnusedVars), len(issues.UnusedClasses), len(issues.UndefinedClasses))
}
