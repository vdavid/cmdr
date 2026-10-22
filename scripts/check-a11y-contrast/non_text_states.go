package main

import "fmt"

// Non-text contrast (WCAG 1.4.11): the edges and rings that carry a control's
// shape or state, held to 3:1 against the surfaces they sit on.
//
// Text pairs are the rule walker's job; nothing there measures a border or a
// focus ring, so a field edge drawn in the decorative `--color-border` (1.36:1
// on white) shipped unseen. Each scenario names the mark's token and the
// surfaces it meets, and runs in both modes, under every accent variant, and
// in each look it names. These findings stay out of the APCA text floor:
// APCA's Lc scale is for text, not a 1px edge. Marks on file-list rows, whose
// bg depends on the pane tint and the row state, live in `row_mark_states.go`.

type nonTextScenario struct {
	// Selector is the human-readable owner of the mark, for the report.
	Selector string
	// MarkVar is the token the edge or ring is painted in (without `--`).
	MarkVar string
	// Surfaces are the bg tokens the mark sits on.
	Surfaces []string
	// Gated looks are held to 3:1; Measured looks are only reported (a cue
	// kept subtle there by design). A look in neither isn't painted.
	Gated, Measured []rowContrastTier
}

// fieldSurfaces are the backgrounds a text field sits on: the main window, a
// card or the settings pane, and a dialog panel.
var fieldSurfaces = []string{"color-bg-primary", "color-bg-secondary", "color-bg-dialog"}

// activeTabSurface is the active tab's own surface, which its accent band sits
// on (`TabBar.svelte`).
var activeTabSurface = []string{"color-bg-secondary"}

var nonTextScenarios = []nonTextScenario{
	{
		Selector: ".text-field / Combobox / NumberInput edge",
		MarkVar:  "color-field-border",
		Surfaces: fieldSurfaces,
		Gated:    []rowContrastTier{tierDefault, tierIncrease},
	},
	{
		Selector: "focus ring ink layer",
		MarkVar:  "color-focus-ink",
		Surfaces: fieldSurfaces,
		Gated:    []rowContrastTier{tierIncrease},
	},
	// A focused ToggleGroup cell that's also the active one: its fill is the accent,
	// so the ring is drawn inside it in `--color-accent-fg` (`ToggleGroup.svelte`).
	{
		Selector: "focused active ToggleGroup cell's ring",
		MarkVar:  "color-accent-fg",
		Surfaces: []string{"color-accent"},
		Gated:    []rowContrastTier{tierDefault, tierIncrease},
	},
	// The focused pane's active-tab band: the raw accent by default (a pane-level
	// extra, measured: Apple Yellow is 1.45:1 on the tab), `--color-accent-pop`
	// under Differentiate without color, where it must clear 3:1.
	{
		Selector: "focused pane's tab accent band",
		MarkVar:  "color-tab-band",
		Surfaces: activeTabSurface,
		Gated:    []rowContrastTier{tierDifferentiate},
		Measured: []rowContrastTier{tierDefault},
	},
	// The unfocused pane's band, at half the accent: faint on purpose.
	{
		Selector: "unfocused pane's tab accent band",
		MarkVar:  "color-tab-band-unfocused",
		Surfaces: activeTabSurface,
		Measured: []rowContrastTier{tierDefault},
	},
}

// AnalyzeNonTextStates evaluates every non-text scenario and returns the
// worst-case finding per (scenario, surface, mode, look), split into the gated
// ones and the measured-only ones.
func (a *Analyzer) AnalyzeNonTextStates() (gated, measured []Finding) {
	for _, sc := range nonTextScenarios {
		for _, tier := range sc.Gated {
			gated = append(gated, a.nonTextFindings(sc, tier)...)
		}
		for _, tier := range sc.Measured {
			measured = append(measured, a.nonTextFindings(sc, tier)...)
		}
	}
	return gated, measured
}

func (a *Analyzer) nonTextFindings(sc nonTextScenario, tier rowContrastTier) []Finding {
	base := tableFor(a.Vars, tier)
	var out []Finding
	for _, surface := range sc.Surfaces {
		for _, mode := range []Mode{ModeLight, ModeDark} {
			out = append(out, a.worstNonText(base, sc, surface, mode, tier))
		}
	}
	return out
}

// worstNonText sweeps the accent variants for one (scenario, surface, mode,
// tier) and returns the lowest ratio. A token that doesn't resolve is a
// failure, not a skip: a renamed token must not turn the gate green.
func (a *Analyzer) worstNonText(base *VarTable, sc nonTextScenario, surface string, mode Mode, tier rowContrastTier) Finding {
	selector := fmt.Sprintf("%s on %s%s", sc.Selector, surface, tierTag(tier))
	worst := Finding{File: syntheticNonTextPath(), Selector: selector, Mode: mode, Threshold: 3.0, Ratio: -1}
	for _, accent := range AccentVariants {
		vars := base
		if !accent.IsDefault {
			vars = withAccentOverride(base, accent)
		}
		a.RulesEvaluated++
		bg, okBg := resolveVar(vars, mode, surface)
		mark, okMark := resolveVar(vars, mode, sc.MarkVar)
		if !okBg || !okMark {
			worst.Ratio, worst.IsPassing = 0, false
			worst.Warnings = append(worst.Warnings, fmt.Sprintf("%s: couldn't resolve --%s or --%s", selector, sc.MarkVar, surface))
			return worst
		}
		if !bg.Opaque() {
			if primary, ok := resolveVar(vars, mode, "color-bg-primary"); ok {
				bg = CompositeOver(bg, primary)
			}
		}
		if !mark.Opaque() {
			mark = CompositeOver(mark, bg)
		}
		ratio := ContrastRatio(mark, bg)
		if worst.Ratio < 0 || ratio < worst.Ratio {
			worst.FG, worst.BG, worst.Ratio = mark, bg, ratio
			worst.AccentVariant = ""
			if !accent.IsDefault {
				worst.AccentVariant = accent.Name
			}
		}
	}
	worst.IsPassing = worst.Ratio >= worst.Threshold
	return worst
}

func syntheticNonTextPath() string {
	return "scripts/check-a11y-contrast/non_text_states.go"
}
