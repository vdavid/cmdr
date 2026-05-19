package main

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// TestSelectionDiff prints the contrast between the selected-row text
// color and the unselected-row text color (`--color-text-primary`) across
// modes and the runtime accent matrix. This measures how visually
// distinguishable a selected row is from a non-selected one — a different
// question from "is the text readable on its bg" that the rest of the
// check suite answers.
//
// Run: CMDR_PRINT_SELECTION_DIFF=1 go test -run TestSelectionDiff -v
func TestSelectionDiff(t *testing.T) {
	if os.Getenv("CMDR_PRINT_SELECTION_DIFF") == "" {
		t.Skip("set CMDR_PRINT_SELECTION_DIFF=1 to print the table")
	}
	rootDir, _ := findRootDir()
	cssPath := filepath.Join(rootDir, "apps", "desktop", "src", "app.css")
	bytesCSS, _ := os.ReadFile(cssPath)
	currentVars := ParseAppCSS(string(bytesCSS))
	beforeVars := pinPreFixSelectionFg(currentVars)

	fmt.Println()
	fmt.Println("Selected vs unselected text differentiation.")
	fmt.Println("Higher ratio = easier to tell selected and unselected apart.")
	fmt.Println("For reference: 4.5:1 = AA body text on bg, ~1.5:1 = visible but subtle,")
	fmt.Println("1.0:1 = identical colors (no differentiation).")
	fmt.Println()
	fmt.Printf("%-5s %-7s %-15s %-9s | %-16s ratio | %-16s ratio\n",
		"mode", "tint", "row state", "accent", "before (fg)", "after (fg)")
	fmt.Println("----------------------------------------------------------------------------------------------")

	type scenario struct {
		mode    Mode
		tint    paneTintHue
		variant string
		accent  AccentVariant
		label   string
	}
	scs := []scenario{
		{ModeLight, paneTintHue{Name: "none"}, "plain", AccentVariant{Name: "default", IsDefault: true}, "plain row, focused"},
		{ModeLight, paneTintHue{Name: "none"}, "cursor-active", AccentVariant{Name: "default", IsDefault: true}, "cursor on selected"},
		{ModeLight, paneTintHue{Name: "amber", VarName: "color-tint-amber"}, "cursor-active", AccentVariant{Name: "yellow", Light: "#ffc601", Dark: "#ffc601"}, "amber tint+yellow accent"},
		{ModeDark, paneTintHue{Name: "none"}, "plain", AccentVariant{Name: "default", IsDefault: true}, "plain row, focused"},
		{ModeDark, paneTintHue{Name: "none"}, "cursor-active", AccentVariant{Name: "default", IsDefault: true}, "cursor on selected"},
		{ModeDark, paneTintHue{Name: "amber", VarName: "color-tint-amber"}, "cursor-active", AccentVariant{Name: "yellow", Light: "#ffc601", Dark: "#ffc601"}, "amber tint+yellow (fallback fires)"},
	}
	for _, sc := range scs {
		bRatio, bFg := selectionDiffRatio(beforeVars, sc.mode, sc.tint, sc.variant, sc.accent, false)
		aRatio, aFg := selectionDiffRatio(currentVars, sc.mode, sc.tint, sc.variant, sc.accent, true)
		fmt.Printf("%-5s %-7s %-15s %-9s | %s  %5.2f | %s  %5.2f\n",
			sc.mode, sc.tint.Name, sc.label, sc.accent.Name, bFg.Hex(), bRatio, aFg.Hex(), aRatio,
		)
	}
}

// TestRedCandidates checks each candidate red against TWO constraints:
//  1. WCAG AA (≥4.5:1) on the worst-case row bg for its mode.
//  2. Differentiation ratio (≥some target) against `--color-text-primary`.
//
// Worst-case bgs are sampled from the existing matrix output; we don't
// recompute them here.
//
// Run: CMDR_PRINT_RED_CANDIDATES=1 go test -run TestRedCandidates -v
func TestRedCandidates(t *testing.T) {
	if os.Getenv("CMDR_PRINT_RED_CANDIDATES") == "" {
		t.Skip("set CMDR_PRINT_RED_CANDIDATES=1 to print the table")
	}

	// Worst light bg = a cursor-active row over a saturated pane tint with
	// the brightest accent. ~Y 0.85.
	worstLightBg, _ := ParseColor("#ffecbd") // light amber cursor-active w/ yellow accent
	// Worst dark bg = cursor-active over an amber-tinted pane with yellow
	// accent. ~Y 0.09.
	worstDarkBg, _ := ParseColor("#614e1d") // dark amber cursor-active w/ yellow accent
	white, _ := ParseColor("#ffffff")
	dark, _ := ParseColor("#1e1e1e")
	lightTextPrimary, _ := ParseColor("#1a1a1a")
	darkTextPrimary, _ := ParseColor("#e8e8e8")

	candidates := []struct {
		hex   string
		label string
	}{
		{"#5a4000", "current light primary (deep gold)"},
		{"#c9a227", "OLD light primary (bright gold)"},
		{"#8b0000", "darkred / Total Cmdr deep red"},
		{"#a00000", "deep red"},
		{"#b91c1c", "Tailwind red-700"},
		{"#c01e1e", "cmdr size-gb token (light)"},
		{"#d32f2f", "cmdr --color-error (light)"},
		{"#dc143c", "crimson"},
		{"#ff0000", "pure red"},
		{"#d4a82a", "current dark primary (bright gold)"},
		{"#ff5555", "bright red (dark mode candidate)"},
		{"#ff7777", "lighter red"},
		{"#fca5a5", "cmdr --color-error-text (dark)"},
	}

	fmt.Println()
	fmt.Println("Red candidate analysis. AA target = 4.5:1.")
	fmt.Println()
	fmt.Printf("%-12s %-40s | %s %s %s | %s %s %s\n",
		"hex", "label",
		"vs white", "vs worst-light", "vs text-primary",
		"vs #1e1e1e", "vs worst-dark", "vs text-primary",
	)
	fmt.Println("-------------------------------------------------------------------------------------------------------------------------------")
	for _, c := range candidates {
		fg, _ := ParseColor(c.hex)
		rWhite := ContrastRatio(fg, white)
		rWorstLight := ContrastRatio(fg, worstLightBg)
		rLightDiff := ContrastRatio(fg, lightTextPrimary)
		rDark := ContrastRatio(fg, dark)
		rWorstDark := ContrastRatio(fg, worstDarkBg)
		rDarkDiff := ContrastRatio(fg, darkTextPrimary)
		fmt.Printf("%-12s %-40s | %s%5.2f  %s%5.2f  %s%5.2f | %s%5.2f  %s%5.2f  %s%5.2f\n",
			c.hex, c.label,
			passMark(rWhite, 4.5), rWhite,
			passMark(rWorstLight, 4.5), rWorstLight,
			passMark(rLightDiff, 3.0), rLightDiff,
			passMark(rDark, 4.5), rDark,
			passMark(rWorstDark, 4.5), rWorstDark,
			passMark(rDarkDiff, 3.0), rDarkDiff,
		)
	}
}

func passMark(ratio, target float64) string {
	if ratio >= target {
		return "✓"
	}
	return "✗"
}

func selectionDiffRatio(vars *VarTable, mode Mode, tint paneTintHue, variant string, accent AccentVariant, applyFallback bool) (float64, RGBA) {
	work := vars
	if !accent.IsDefault {
		work = withAccentOverride(work, accent)
	}
	if applyFallback {
		work = withSelectionFgVariant(work, selectionFgTokenFor(mode, tint, variant))
	}
	fg, _ := resolveTextRole(work, mode, "color-selection-fg")
	textPrimary, _ := resolveTextRole(work, mode, "color-text-primary")
	return ContrastRatio(fg, textPrimary), fg
}
