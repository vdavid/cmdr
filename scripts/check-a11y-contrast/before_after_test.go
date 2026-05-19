package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"testing"
)

// TestBeforeAfterTable prints a comparison table of selection-fg contrast
// across a curated set of (mode, tint, variant, accent) combinations. Run
// with `go test -run TestBeforeAfterTable -v` to see the table.
//
// The table compares the current (post-fix) `--color-selection-fg` against
// the pre-fix baseline (`#c9a227` light / `#d4a82a` dark, no switching).
// It picks ≤80 scenarios that cover the most meaningful axes the user asked
// about: light vs. dark, no-tint vs. a few representative tints, every row
// variant (plain / striped / cursor-inactive / cursor-active), and the
// accents that drive the worst-case cursor-active bg.
//
// Skipped by default in `go test ./...` (use the explicit `-run` flag).
// beforeAfterRow is one rendered line in the comparison table.
type beforeAfterRow struct {
	mode    Mode
	tint    string
	variant string
	accent  string
	role    string

	beforeFg, beforeBg RGBA
	beforeRatio        float64
	beforePass         bool

	afterFg, afterBg RGBA
	afterRatio       float64
	afterPass        bool
}

func TestBeforeAfterTable(t *testing.T) {
	if os.Getenv("CMDR_PRINT_BEFORE_AFTER") == "" {
		t.Skip("set CMDR_PRINT_BEFORE_AFTER=1 to print the table")
	}

	rootDir, err := findRootDir()
	if err != nil {
		t.Fatalf("findRootDir: %v", err)
	}
	cssPath := filepath.Join(rootDir, "apps", "desktop", "src", "app.css")
	bytesCSS, err := os.ReadFile(cssPath)
	if err != nil {
		t.Fatalf("read app.css: %v", err)
	}
	currentVars := ParseAppCSS(string(bytesCSS))
	beforeVars := pinPreFixSelectionFg(currentVars)

	rows := collectBeforeAfterRows(beforeVars, currentVars, buildBeforeAfterScenarios())
	sortBeforeAfterRows(rows)

	fmt.Println()
	fmt.Println("Before / after contrast for selected-row text. AA target = 4.5:1.")
	fmt.Println("Rows where 'before' passed and 'after' would too are omitted to keep the table tight.")
	printBeforeAfterTable(rows)
}

// pinPreFixSelectionFg returns a VarTable derived from `current` with
// `--color-selection-fg` (and its primary alias) pinned back to the pre-fix
// hex values, simulating the codebase before the fix without re-parsing an
// old checkout.
func pinPreFixSelectionFg(current *VarTable) *VarTable {
	out := cloneVars(current)
	out.Light["color-selection-fg"] = "#c9a227"
	out.Dark["color-selection-fg"] = "#d4a82a"
	out.Light["color-selection-fg-primary"] = "#c9a227"
	out.Dark["color-selection-fg-primary"] = "#d4a82a"
	return out
}

// collectBeforeAfterRows runs each scenario through `evalScenario` twice and
// returns the rendered table rows.
func collectBeforeAfterRows(beforeVars, afterVars *VarTable, scenarios []beforeAfterScenario) []beforeAfterRow {
	rows := make([]beforeAfterRow, 0, len(scenarios))
	for _, sc := range scenarios {
		bF, ok := evalScenario(beforeVars, sc, false)
		if !ok {
			continue
		}
		aF, ok := evalScenario(afterVars, sc, true)
		if !ok {
			continue
		}
		rows = append(rows, beforeAfterRow{
			mode: sc.mode, tint: sc.tint.Name, variant: sc.variant,
			accent: sc.accentName, role: sc.role,
			beforeFg: bF.FG, beforeBg: bF.BG, beforeRatio: bF.Ratio, beforePass: bF.Ratio >= 4.5,
			afterFg: aF.FG, afterBg: aF.BG, afterRatio: aF.Ratio, afterPass: aF.Ratio >= 4.5,
		})
	}
	return rows
}

// sortBeforeAfterRows sorts in stable (mode, role, tint, variant, accent)
// order so the table reads like a tidy spreadsheet.
func sortBeforeAfterRows(rows []beforeAfterRow) {
	sort.SliceStable(rows, func(i, j int) bool {
		if rows[i].mode != rows[j].mode {
			return rows[i].mode < rows[j].mode
		}
		if rows[i].role != rows[j].role {
			return rows[i].role < rows[j].role
		}
		if rows[i].tint != rows[j].tint {
			return rows[i].tint < rows[j].tint
		}
		if rows[i].variant != rows[j].variant {
			return rows[i].variant < rows[j].variant
		}
		return rows[i].accent < rows[j].accent
	})
}

// printBeforeAfterTable writes the comparison to stdout, dropping all but a
// handful of "before-passed AND after-passed" rows so the table stays focused
// on the regressions the fix is closing.
func printBeforeAfterTable(rows []beforeAfterRow) {
	fmt.Println()
	fmt.Printf("%-5s %-7s %-15s %-9s %-25s | %s %s %-7s | %s %s %-7s\n",
		"mode", "tint", "variant", "accent", "text role",
		"  bg→fg before    ", "  ratio", "(AA?)",
		"  bg→fg after     ", "  ratio", "(AA?)",
	)
	fmt.Println("------------------------------------------------------------------------" +
		"------------------------------------------------------------------------")
	printedBeforePassed := 0
	for _, r := range rows {
		if r.beforePass && r.afterPass {
			printedBeforePassed++
			if printedBeforePassed > 5 {
				continue
			}
		}
		fmt.Printf("%-5s %-7s %-15s %-9s %-25s | %s→%s %5.2f %-7s | %s→%s %5.2f %-7s\n",
			r.mode, r.tint, r.variant, r.accent, r.role,
			r.beforeBg.Hex(), r.beforeFg.Hex(), r.beforeRatio, passStr(r.beforePass),
			r.afterBg.Hex(), r.afterFg.Hex(), r.afterRatio, passStr(r.afterPass),
		)
	}
	var beforePass, afterPass int
	for _, r := range rows {
		if r.beforePass {
			beforePass++
		}
		if r.afterPass {
			afterPass++
		}
	}
	fmt.Println()
	fmt.Printf("Summary: %d scenarios. Before: %d/%d pass AA. After: %d/%d pass AA.\n",
		len(rows), beforePass, len(rows), afterPass, len(rows))
}

func passStr(p bool) string {
	if p {
		return "✓"
	}
	return "✗ FAIL"
}

func cloneVars(v *VarTable) *VarTable {
	out := NewVarTable()
	for k, val := range v.Light {
		out.Light[k] = val
	}
	for k, val := range v.Dark {
		out.Dark[k] = val
	}
	return out
}

type beforeAfterScenario struct {
	mode       Mode
	tint       paneTintHue
	variant    string
	accentName string // empty when not cursor-active
	role       string
}

func buildBeforeAfterScenarios() []beforeAfterScenario {
	// Three tints: none + a warm one (amber, drives the dark cursor-active
	// worst case) + a cool one (purple, drives the light cursor-active
	// case under Apple Blue accent). 3 tints × 2 modes × 4 variants × up to
	// 3 accents × 2 roles = 72 rows — under the 80-row cap.
	tintSet := []paneTintHue{
		{Name: "none", VarName: ""},
		{Name: "amber", VarName: "color-tint-amber"},
		{Name: "purple", VarName: "color-tint-purple"},
	}
	variants := []string{"plain", "striped", "cursor-inactive", "cursor-active"}
	// Cursor-active accents: default (Cmdr gold), Apple Purple (the only
	// white-text accent — drives one direction of failure), Apple Yellow
	// (drives dark warm-tint failure). Apple Blue tested elsewhere; it
	// patterns with Cmdr gold here.
	accents := []AccentVariant{
		{Name: "default", IsDefault: true},
		{Name: "purple", Light: "#a54fa7", Dark: "#a54fa7"},
		{Name: "yellow", Light: "#ffc601", Dark: "#ffc601"},
	}
	// Two roles: the main selection-fg + size-bytes-selected (the darkest
	// derived mix). size-tb-selected is alias for selection-fg, so omit.
	roles := []string{"color-selection-fg", "color-size-bytes-selected"}

	var out []beforeAfterScenario
	for _, mode := range []Mode{ModeLight, ModeDark} {
		for _, tint := range tintSet {
			for _, variant := range variants {
				accentList := []AccentVariant{{Name: "", IsDefault: true}}
				if variant == "cursor-active" {
					accentList = accents
				}
				for _, accent := range accentList {
					for _, role := range roles {
						out = append(out, beforeAfterScenario{
							mode: mode, tint: tint, variant: variant,
							accentName: accent.Name, role: role,
						})
					}
				}
			}
		}
	}
	return out
}

// evalScenario resolves one scenario's fg/bg and returns the Finding.
// `applyFallback` controls whether the synthesizer's selection-fg-fallback
// rule (the new behavior) is applied. The "before" pass calls with false.
func evalScenario(base *VarTable, sc beforeAfterScenario, applyFallback bool) (Finding, bool) {
	vars := base
	// Apply accent override if specified.
	if sc.accentName != "" {
		for _, av := range AccentVariants {
			if av.Name == sc.accentName {
				vars = withAccentOverride(vars, av)
				break
			}
		}
		// Custom accents from buildBeforeAfterScenarios may not be in
		// AccentVariants; handle the explicit set we know about.
		switch sc.accentName {
		case "blue":
			vars = withAccentOverride(vars, AccentVariant{Name: "blue", Light: "#087aff", Dark: "#087aff"})
		case "purple":
			vars = withAccentOverride(vars, AccentVariant{Name: "purple", Light: "#a54fa7", Dark: "#a54fa7"})
		case "yellow":
			vars = withAccentOverride(vars, AccentVariant{Name: "yellow", Light: "#ffc601", Dark: "#ffc601"})
		}
	}
	if applyFallback && shouldUseSelectionFgFallback(sc.mode, sc.tint, sc.variant) {
		vars = withSelectionFgFallback(vars)
	}
	bg, ok := resolveRowBg(vars, sc.mode, sc.tint, sc.variant)
	if !ok {
		return Finding{}, false
	}
	fg, ok := resolveTextRole(vars, sc.mode, sc.role)
	if !ok {
		return Finding{}, false
	}
	if !fg.Opaque() {
		fg = CompositeOver(fg, bg)
	}
	ratio := ContrastRatio(fg, bg)
	return Finding{FG: fg, BG: bg, Ratio: ratio}, true
}
