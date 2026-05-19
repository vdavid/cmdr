package main

import (
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
	"testing"
)

// TestPickRed sweeps a dense grid of red candidate hexes across the full
// row-state bg matrix and reports, for each candidate, the worst-case
// contrast against any bg (must stay above 4.5:1) and the diff vs
// `--color-text-primary` (we want to maximize this). Picks the candidate
// closest to the AA threshold from above — that's the red that nails
// "just above 4.5:1 on every bg" while pushing diff-vs-unselected as high
// as possible.
//
// Run: CMDR_PICK_RED=1 go test -run TestPickRed -v
func TestPickRed(t *testing.T) {
	if os.Getenv("CMDR_PICK_RED") == "" {
		t.Skip("set CMDR_PICK_RED=1 to run the red picker")
	}
	rootDir, _ := findRootDir()
	cssPath := filepath.Join(rootDir, "apps", "desktop", "src", "app.css")
	bytesCSS, _ := os.ReadFile(cssPath)
	vars := ParseAppCSS(string(bytesCSS))

	for _, mode := range []Mode{ModeLight, ModeDark} {
		fmt.Printf("\n=== %s mode ===\n", mode)
		worstBg := findWorstBg(vars, mode)
		fmt.Printf("Worst-case bg in the matrix: %s (luminance %.3f)\n", worstBg.Hex(), RelativeLuminance(worstBg))
		brightestBg := findBrightestBg(vars, mode)
		fmt.Printf("Brightest bg in the matrix: %s (luminance %.3f)\n", brightestBg.Hex(), RelativeLuminance(brightestBg))

		textPrimary, _ := resolveTextRole(vars, mode, "color-text-primary")
		fmt.Printf("Unselected text (`--color-text-primary`): %s (luminance %.3f)\n", textPrimary.Hex(), RelativeLuminance(textPrimary))

		// Candidate reds. In light mode we want darkish reds (low luminance)
		// to clear AA on white-ish bg; in dark mode we want bright reds.
		candidates := redCandidates(mode)
		// Score each candidate.
		type scored struct {
			hex       string
			rgba      RGBA
			minRatio  float64 // worst against any bg in the matrix
			diffRatio float64 // contrast against text-primary
		}
		var rows []scored
		for _, hex := range candidates {
			c, ok := ParseColor(hex)
			if !ok {
				continue
			}
			minR := minContrastAgainstMatrix(vars, mode, c)
			diff := ContrastRatio(c, textPrimary)
			rows = append(rows, scored{hex, c, minR, diff})
		}
		// Sort by diff desc, then filter to passing.
		sort.SliceStable(rows, func(i, j int) bool {
			return rows[i].diffRatio > rows[j].diffRatio
		})
		fmt.Printf("\n%-9s %-10s %-12s %-12s %s\n",
			"hex", "Y_fg", "min vs bg", "diff vs txt", "status")
		fmt.Println("-------------------------------------------------------")
		printed := 0
		var best string
		var bestDiff float64
		for _, r := range rows {
			pass := r.minRatio >= 4.5
			status := "✗"
			if pass {
				status = "✓"
				if best == "" || r.diffRatio > bestDiff {
					best = r.hex
					bestDiff = r.diffRatio
				}
			}
			if !pass && printed < 3 || pass && printed < 25 {
				fmt.Printf("%-9s %-10.3f %-12.2f %-12.2f %s\n",
					r.hex, RelativeLuminance(r.rgba), r.minRatio, r.diffRatio, status)
				printed++
			}
		}
		fmt.Printf("\nBest candidate (max diff while clearing AA on every bg): %s, diff=%.2f:1\n", best, bestDiff)
	}
}

// redCandidates returns a dense set of dark-to-medium reds (light mode)
// or bright-to-medium reds (dark mode).
func redCandidates(mode Mode) []string {
	if mode == ModeLight {
		// Dark reds, ordered roughly from darkest to brightest.
		return []string{
			"#700000", "#770000", "#7e0000", "#850000",
			"#8c0000", "#930000", "#9a0000", "#a10000",
			"#a80000", "#af0000", "#b60000", "#bd0000", "#c40000",
			// Slightly more orange variants.
			"#7a1000", "#871000", "#931000", "#a01000",
			"#ad1000", "#b91000",
			// Slightly more pink.
			"#8c1020", "#a01030", "#b01040",
		}
	}
	// Bright reds for dark mode.
	return []string{
		"#ff6060", "#ff7070", "#ff8080", "#ff9090",
		"#f08080", "#f09090", "#e87575", "#e88080",
		"#e89595", "#dc7a7a", "#d06868", "#d08585",
		"#ff5050", "#ff6666", "#ff7777", "#ff8888",
		"#ffa0a0", "#ffb0b0",
	}
}

// minContrastAgainstMatrix returns the lowest contrast ratio of `fg`
// against any bg in the row-state matrix for the given mode.
func minContrastAgainstMatrix(vars *VarTable, mode Mode, fg RGBA) float64 {
	minR := math.Inf(1)
	for _, tint := range rowPaneTints {
		for _, variant := range rowBgVariants {
			accents := []AccentVariant{{Name: "default", IsDefault: true}}
			if variant == "cursor-active" {
				accents = AccentVariants
			}
			for _, accent := range accents {
				v := vars
				if !accent.IsDefault {
					v = withAccentOverride(vars, accent)
				}
				bg, ok := resolveRowBg(v, mode, tint, variant)
				if !ok {
					continue
				}
				r := ContrastRatio(fg, bg)
				if r < minR {
					minR = r
				}
			}
		}
	}
	return minR
}

func findWorstBg(vars *VarTable, mode Mode) RGBA {
	var worst RGBA
	worstY := math.Inf(-1)
	if mode == ModeDark {
		// In dark mode, "worst" for a bright fg is the BRIGHTEST bg.
		worstY = -1
		for _, tint := range rowPaneTints {
			for _, variant := range rowBgVariants {
				accents := []AccentVariant{{Name: "default", IsDefault: true}}
				if variant == "cursor-active" {
					accents = AccentVariants
				}
				for _, accent := range accents {
					v := vars
					if !accent.IsDefault {
						v = withAccentOverride(vars, accent)
					}
					bg, ok := resolveRowBg(v, mode, tint, variant)
					if !ok {
						continue
					}
					y := RelativeLuminance(bg)
					if y > worstY {
						worstY = y
						worst = bg
					}
				}
			}
		}
		return worst
	}
	// Light mode: worst for a dark fg is the DARKEST bg.
	worstY = math.Inf(1)
	for _, tint := range rowPaneTints {
		for _, variant := range rowBgVariants {
			accents := []AccentVariant{{Name: "default", IsDefault: true}}
			if variant == "cursor-active" {
				accents = AccentVariants
			}
			for _, accent := range accents {
				v := vars
				if !accent.IsDefault {
					v = withAccentOverride(vars, accent)
				}
				bg, ok := resolveRowBg(v, mode, tint, variant)
				if !ok {
					continue
				}
				y := RelativeLuminance(bg)
				if y < worstY {
					worstY = y
					worst = bg
				}
			}
		}
	}
	return worst
}

func findBrightestBg(vars *VarTable, mode Mode) RGBA {
	var best RGBA
	bestY := math.Inf(-1)
	for _, tint := range rowPaneTints {
		for _, variant := range rowBgVariants {
			accents := []AccentVariant{{Name: "default", IsDefault: true}}
			if variant == "cursor-active" {
				accents = AccentVariants
			}
			for _, accent := range accents {
				v := vars
				if !accent.IsDefault {
					v = withAccentOverride(vars, accent)
				}
				bg, ok := resolveRowBg(v, mode, tint, variant)
				if !ok {
					continue
				}
				y := RelativeLuminance(bg)
				if y > bestY {
					bestY = y
					best = bg
				}
			}
		}
	}
	return best
}
