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
	printIssueSection("Unused CSS variables", issues.UnusedVars, "--", issues.VarDefs, "defined in", verbose)
	printIssueSection("Unused CSS classes", issues.UnusedClasses, ".", issues.ClassDefs, "defined in", verbose)
	printIssueSection("Undefined CSS classes (used but not defined)", issues.UndefinedClasses, ".", issues.ClassUseLocs, "used in", verbose)
}

func printIssueSection(title string, items []string, prefix string, locs map[string][]string, locLabel string, verbose bool) {
	if len(items) == 0 {
		return
	}
	fmt.Printf("%s\n=== %s ===%s\n", colorYellow, title, colorReset)
	for _, name := range items {
		if verbose {
			fmt.Printf("  %s%s  (%s: %s)\n", prefix, name, locLabel, strings.Join(locs[name], ", "))
		} else {
			fmt.Printf("  %s%s\n", prefix, name)
		}
	}
	fmt.Println()
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
