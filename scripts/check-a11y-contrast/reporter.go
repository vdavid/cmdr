package main

import (
	"fmt"
	"sort"
)

const (
	colorRed    = "\033[31m"
	colorGreen  = "\033[32m"
	colorYellow = "\033[33m"
	colorDim    = "\033[2m"
	colorReset  = "\033[0m"
)

// Report prints violations to stdout in a human-readable format.
// Returns true if any violations were reported.
func Report(violations []Finding, warnings []string, rootDir string, verbose bool) bool {
	if len(violations) == 0 && len(warnings) == 0 {
		return false
	}

	// Sort: file, then line, then mode.
	sort.SliceStable(violations, func(i, j int) bool {
		if violations[i].File != violations[j].File {
			return violations[i].File < violations[j].File
		}
		if violations[i].Line != violations[j].Line {
			return violations[i].Line < violations[j].Line
		}
		return violations[i].Mode < violations[j].Mode
	})

	if len(violations) > 0 {
		fmt.Printf("%s=== Contrast violations ===%s\n", colorYellow, colorReset)
		for _, v := range violations {
			delta := v.Threshold - v.Ratio
			tag := ""
			if v.Placeholder {
				tag = " [placeholder]"
			}
			fmt.Printf(
				"  %s%s:%d%s  %s%s%s  mode=%s  fg=%s  bg=%s  ratio=%.2f  need=%.1f  delta=-%.2f%s\n",
				colorRed, RelPath(rootDir, v.File), v.Line, colorReset,
				colorDim, v.Selector, colorReset,
				v.Mode,
				v.FG.Hex(), v.BG.Hex(),
				v.Ratio, v.Threshold, delta,
				tag,
			)
		}
		fmt.Println()
	}

	if verbose && len(warnings) > 0 {
		fmt.Printf("%s=== Warnings ===%s\n", colorYellow, colorReset)
		for _, w := range warnings {
			fmt.Printf("  %s%s%s\n", colorDim, w, colorReset)
		}
		fmt.Println()
	}

	return len(violations) > 0
}

// Summary returns a one-line summary for the final status line.
func Summary(fileCount, ruleCount, findingCount, violationCount int) string {
	return fmt.Sprintf(
		"%d %s, %d %s checked, %d %s evaluated, %d %s",
		fileCount, plural(fileCount, "file", "files"),
		ruleCount, plural(ruleCount, "rule", "rules"),
		findingCount, plural(findingCount, "pair", "pairs"),
		violationCount, plural(violationCount, "violation", "violations"),
	)
}

func plural(n int, s, p string) string {
	if n == 1 {
		return s
	}
	return p
}

// joinWarnings collapses duplicate warnings into a sorted deduplicated list.
func joinWarnings(ws []string) []string {
	seen := make(map[string]int, len(ws))
	for _, w := range ws {
		seen[w]++
	}
	out := make([]string, 0, len(seen))
	for w, n := range seen {
		if n > 1 {
			out = append(out, fmt.Sprintf("%s (x%d)", w, n))
		} else {
			out = append(out, w)
		}
	}
	sort.Strings(out)
	return out
}
