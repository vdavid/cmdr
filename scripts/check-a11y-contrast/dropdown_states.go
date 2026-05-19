package main

import "fmt"

// Dropdown / "secondary text on accent bg" scenarios.
//
// In `SettingSelect.svelte`, when a `.select-item` is `[data-highlighted]`
// (cursor over the option), its bg flips to `var(--color-accent)`. The
// item's primary label inherits the parent's `color: var(--color-accent-fg)`
// (auto-picked black/white per accent), but the inner `.option-description`
// span has its own rule `color: var(--color-text-tertiary)` that stays
// regardless of parent state. The result on Cmdr gold (and worse, on Apple
// Blue) is a gray description on an accent bg with ~2.4:1 contrast.
//
// The rule walker doesn't catch this because the description's color and the
// ancestor's bg are set on different selectors. This synthesizer hand-lists
// the (descendant-text-token, ancestor-bg-token) tuple and evaluates it
// against every accent variant + both modes.
//
// To add another "secondary text on accent bg" scenario in the future
// (e.g. a hover state of a custom dropdown), append a new entry to
// `dropdownScenarios` below. The whole list is run against the accent matrix
// automatically.

type ancestorBgScenario struct {
	// Selector is the human-readable selector for the report.
	Selector string
	// FgVar is the CSS custom-property name supplying the descendant's text
	// color (without `--`). Used when FgExpr is empty.
	FgVar string
	// FgExpr is an arbitrary CSS expression (e.g. a `color-mix(...)` chain)
	// that resolves to the descendant's text color. Takes priority over
	// FgVar when non-empty.
	FgExpr string
	// BgVar is the CSS custom-property name supplying the ancestor's bg
	// color (without `--`). The synthesizer reads this from the table after
	// applying any accent-variant override.
	BgVar string
}

// dropdownScenarios are the known "secondary text inside an accent-bg
// ancestor" pairs the rule walker can't see. Keep small and intentional —
// these are bespoke escape hatches, not a substitute for cascade-aware
// analysis.
// FgExpr is supplied for scenarios where the fg isn't a plain var() lookup
// but a small CSS expression (e.g. a color-mix). When non-empty, the
// synthesizer resolves FgExpr instead of FgVar.
var dropdownScenarios = []ancestorBgScenario{
	{
		Selector: ".select-item[data-highlighted] .option-description",
		FgVar:    "color-accent-fg",
		BgVar:    "color-accent",
	},
}

// AnalyzeDropdownStates evaluates each dropdown scenario against the accent
// matrix + both modes. Returns one Finding per (scenario, mode) keyed on the
// worst-case accent variant.
func (a *Analyzer) AnalyzeDropdownStates() []Finding {
	type key struct {
		selector string
		mode     Mode
	}
	worst := make(map[key]Finding)
	evaluated := 0

	for _, sc := range dropdownScenarios {
		for _, mode := range []Mode{ModeLight, ModeDark} {
			for _, accent := range AccentVariants {
				f, ok := evalDropdownSample(a.Vars, mode, accent, sc)
				if !ok {
					continue
				}
				k := key{selector: sc.Selector, mode: mode}
				if cur, exists := worst[k]; !exists || f.Ratio < cur.Ratio {
					worst[k] = f
				}
				evaluated++
			}
		}
	}

	a.RulesEvaluated += evaluated
	out := make([]Finding, 0, len(worst))
	for _, f := range worst {
		out = append(out, f)
	}
	return out
}

// evalDropdownSample evaluates one (scenario × mode × accent) sample.
// Returns ok=false when fg or bg fails to resolve.
func evalDropdownSample(baseVars *VarTable, mode Mode, accent AccentVariant, sc ancestorBgScenario) (Finding, bool) {
	vars := baseVars
	if !accent.IsDefault {
		vars = withAccentOverride(baseVars, accent)
	}
	bg, ok := resolveVar(vars, mode, sc.BgVar)
	if !ok {
		return Finding{}, false
	}
	if !bg.Opaque() {
		// Composite over `--color-bg-primary` like the main analyzer does
		// for translucent backgrounds.
		if primary, ok := resolveVar(vars, mode, "color-bg-primary"); ok {
			bg = CompositeOver(bg, primary)
		}
	}
	fg, ok := resolveDropdownFg(vars, mode, sc)
	if !ok {
		return Finding{}, false
	}
	if !fg.Opaque() {
		fg = CompositeOver(fg, bg)
	}
	ratio := ContrastRatio(fg, bg)
	accentTag := ""
	if !accent.IsDefault {
		accentTag = accent.Name
	}
	return Finding{
		File:          syntheticDropdownPath(),
		Line:          0,
		Selector:      fmt.Sprintf("%s (fg=%s, bg=%s)", sc.Selector, fgLabel(sc), sc.BgVar),
		Mode:          mode,
		FG:            fg,
		BG:            bg,
		Ratio:         ratio,
		Threshold:     4.5,
		IsPassing:     ratio >= 4.5,
		AccentVariant: accentTag,
	}, true
}

// resolveDropdownFg picks between the FgExpr (arbitrary CSS expression) and
// FgVar (named var) paths declared on the scenario.
func resolveDropdownFg(vars *VarTable, mode Mode, sc ancestorBgScenario) (RGBA, bool) {
	if sc.FgExpr != "" {
		r := NewResolver(vars, mode)
		c, err := r.Resolve(sc.FgExpr)
		if err != nil {
			return RGBA{}, false
		}
		return c, true
	}
	return resolveVar(vars, mode, sc.FgVar)
}

// fgLabel returns a short human-readable label for the report.
func fgLabel(sc ancestorBgScenario) string {
	if sc.FgExpr != "" {
		return sc.FgExpr
	}
	return sc.FgVar
}

func syntheticDropdownPath() string {
	return "scripts/check-a11y-contrast/dropdown_states.go"
}
