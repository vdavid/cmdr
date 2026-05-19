package main

import (
	"fmt"
	"maps"
)

// Row-state matrix.
//
// `.file-entry` rows in the file pane render with bgs that depend on:
//   - the pane tint (12 hues + "none"), set inline on `.file-pane` from
//     `volume-tint.svelte.ts`.
//   - whether the row is striped (`.is-striped`).
//   - whether the row is under the cursor and whether the pane is focused
//     (`.is-under-cursor` + `.is-focused`); these route to either
//     `--color-cursor-inactive` or `--color-cursor-active`.
//   - the active macOS accent (cursor-active is `color-mix(in oklch,
//     var(--color-accent), transparent 80%)`).
//
// The text on a selected row comes from a different selector
// (`.file-entry.is-selected .col-name { color: var(--color-selection-fg) }`),
// so the static checker doesn't pair them in `evaluate`. This synthesizer
// composes the bg the row will actually render with — pane tint, optional
// stripe, optional cursor overlay — and pairs each text role against it.
//
// We report the worst-case finding per (textRole, mode) so the output stays
// small even though the synthesizer evaluates a few thousand combos.

type paneTintHue struct {
	// Name is the human-readable tint label (matches the user setting value).
	Name string
	// VarName is the `--color-tint-<X>` token, or "" when no tint.
	VarName string
}

// rowPaneTints mirrors the 12 hues in `--color-tint-*` plus a "none" sentinel.
// Keep in sync with the `appearance.tint{Local,Smb,Mtp}` enum values in
// `settings-registry.ts`.
var rowPaneTints = []paneTintHue{
	{Name: "none", VarName: ""},
	{Name: "red", VarName: "color-tint-red"},
	{Name: "orange", VarName: "color-tint-orange"},
	{Name: "amber", VarName: "color-tint-amber"},
	{Name: "lime", VarName: "color-tint-lime"},
	{Name: "green", VarName: "color-tint-green"},
	{Name: "teal", VarName: "color-tint-teal"},
	{Name: "cyan", VarName: "color-tint-cyan"},
	{Name: "blue", VarName: "color-tint-blue"},
	{Name: "indigo", VarName: "color-tint-indigo"},
	{Name: "purple", VarName: "color-tint-purple"},
	{Name: "pink", VarName: "color-tint-pink"},
	{Name: "brown", VarName: "color-tint-brown"},
}

// rowBgVariants enumerates the four row bg states. Cursor wins over stripe in
// the cascade (the cursor rules have higher specificity), so the (striped +
// cursor) combo is folded into the cursor variant — the visible bg is the
// translucent cursor composited over the pane-tinted bg, never the stripe.
var rowBgVariants = []string{
	"plain",           // no stripe, no cursor — row shows pane bg
	"striped",         // `.is-striped` — row bg overrides pane bg with `--color-bg-stripe`
	"cursor-inactive", // `.is-under-cursor` with pane NOT focused
	"cursor-active",   // `.is-under-cursor` with pane focused
}

// rowSelectedTextRoles are the tokens the file-list selection styling reads.
// They are NOT paired with any bg by `.svelte` rules in our codebase
// (the rule that sets them only sets `color`, leaving bg to ancestors), so
// the generic walker skips them — hence this scenario list.
var rowSelectedTextRoles = []string{
	"color-selection-fg",
	"color-size-bytes-selected",
	"color-size-kb-selected",
	"color-size-mb-selected",
	"color-size-gb-selected",
	"color-size-tb-selected",
}

// AnalyzeRowStates evaluates every (mode × pane tint × bg variant × accent ×
// text role) combination and returns the worst-case Finding per (textRole,
// mode, tint, variant) — accent is collapsed into the worst variant
// internally. The result is roughly 600 findings (well under the rule walker's
// total) and surfaces every failing combo without exploding the report.
func (a *Analyzer) AnalyzeRowStates() []Finding {
	worst, evaluated := make(map[rowWorstKey]Finding), 0
	for _, mode := range []Mode{ModeLight, ModeDark} {
		for _, tint := range rowPaneTints {
			for _, variant := range rowBgVariants {
				evaluated += a.evalRowCell(mode, tint, variant, worst)
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

// resolveRowBg composes the row's bg from the pane-tint layer + the optional
// stripe override or cursor overlay. Returns the opaque RGBA the renderer
// would actually paint.
func resolveRowBg(vars *VarTable, mode Mode, tint paneTintHue, variant string) (RGBA, bool) {
	paneBg, ok := resolvePaneBg(vars, mode, tint)
	if !ok {
		return RGBA{}, false
	}
	switch variant {
	case "plain":
		// Selected rows get a darker bg via `--color-selection-bg`
		// (dark mode only; transparent in light). Since the matrix's
		// text roles (`color-selection-fg` and `color-size-*-selected`)
		// only render on SELECTED rows, the relevant bg here is the
		// selection-bg if defined.
		if sel, ok := resolveSelectionBg(vars, mode, paneBg); ok {
			return sel, true
		}
		return paneBg, true
	case "striped":
		// On a selected row the stripe is overridden by the selection
		// bg (same rule + same specificity, but `.is-selected` appears
		// later in `FullList.svelte`'s `<style>`). The matrix's text
		// roles only apply to selected rows, so model that.
		if sel, ok := resolveSelectionBg(vars, mode, paneBg); ok {
			return sel, true
		}
		// Fallback for modes where selection-bg is transparent (light):
		// render the stripe color on top of the pane bg.
		c, ok := resolveVar(vars, mode, "color-bg-stripe")
		if !ok {
			return RGBA{}, false
		}
		if !c.Opaque() {
			c = CompositeOver(c, paneBg)
		}
		return c, true
	case "cursor-inactive":
		c, ok := resolveVar(vars, mode, "color-cursor-inactive")
		if !ok {
			return RGBA{}, false
		}
		if !c.Opaque() {
			c = CompositeOver(c, paneBg)
		}
		return c, true
	case "cursor-active":
		c, ok := resolveVar(vars, mode, "color-cursor-active")
		if !ok {
			return RGBA{}, false
		}
		if !c.Opaque() {
			c = CompositeOver(c, paneBg)
		}
		return c, true
	}
	return RGBA{}, false
}

// resolveSelectionBg returns the opaque selection bg for selected rows, or
// ok=false when `--color-selection-bg` is undefined or fully transparent
// (which is the light-mode default — light keeps the pane bg under selected
// text, the new red foreground carries the signal on its own).
func resolveSelectionBg(vars *VarTable, mode Mode, fallbackPaneBg RGBA) (RGBA, bool) {
	c, ok := resolveVar(vars, mode, "color-selection-bg")
	if !ok {
		return RGBA{}, false
	}
	if c.A <= 0 {
		// `transparent` means "no selection-bg in this mode" — caller
		// should use the pane bg.
		return RGBA{}, false
	}
	if !c.Opaque() {
		c = CompositeOver(c, fallbackPaneBg)
	}
	return c, true
}

// resolvePaneBg returns the opaque pane background: `--color-bg-primary` when
// the tint is "none", otherwise the same `color-mix(in oklch, ...)` formula
// `volume-tint.svelte.ts` writes inline on `.file-pane`.
func resolvePaneBg(vars *VarTable, mode Mode, tint paneTintHue) (RGBA, bool) {
	if tint.VarName == "" {
		return resolveVar(vars, mode, "color-bg-primary")
	}
	// Light = 10% tint, dark = 15% (both default; prefers-contrast: more
	// would bump these to 15/25 but isn't modelled here — see Scope below).
	tintPct := 10
	if mode == ModeDark {
		tintPct = 15
	}
	expr := fmt.Sprintf(
		"color-mix(in oklch, var(--color-bg-primary) %d%%, var(--%s) %d%%)",
		100-tintPct, tint.VarName, tintPct,
	)
	r := NewResolver(vars, mode)
	c, err := r.Resolve(expr)
	if err != nil {
		return RGBA{}, false
	}
	return c, true
}

// resolveVar fetches a named var from the table and resolves it through the
// usual color-mix / var-chain pipeline. Returns ok=false if the var is
// undefined or the value fails to resolve.
func resolveVar(vars *VarTable, mode Mode, name string) (RGBA, bool) {
	raw, ok := vars.Raw(name, mode)
	if !ok {
		return RGBA{}, false
	}
	r := NewResolver(vars, mode)
	c, err := r.Resolve(raw)
	if err != nil {
		return RGBA{}, false
	}
	return c, true
}

// resolveTextRole is a thin alias for resolveVar; kept distinct so callers
// reading the scenario loop can tell apart bg-side and fg-side resolutions.
func resolveTextRole(vars *VarTable, mode Mode, role string) (RGBA, bool) {
	return resolveVar(vars, mode, role)
}

// rowWorstKey identifies one "worst-case slot" in the matrix. The synthesizer
// collapses all accent variants for cursor-active into a single worst-case
// finding per (role, mode, tint, variant) tuple, so the report stays short.
type rowWorstKey struct {
	role    string
	mode    Mode
	tint    string
	variant string
}

// evalRowCell evaluates one (mode, tint, variant) cell across all roles and
// all relevant accent variants. Returns how many (role × accent) pairs were
// evaluated, and updates `worst` in place with the lowest-ratio finding per
// (role × mode × tint × variant) tuple.
func (a *Analyzer) evalRowCell(mode Mode, tint paneTintHue, variant string, worst map[rowWorstKey]Finding) int {
	accents := []AccentVariant{{Name: "default", IsDefault: true}}
	if variant == "cursor-active" {
		accents = AccentVariants
	}
	evaluated := 0
	for _, accent := range accents {
		evaluated += a.evalRowCellForAccent(mode, tint, variant, accent, worst)
	}
	return evaluated
}

// evalRowCellForAccent runs one (mode, tint, variant, accent) sample across
// every text role. Splits out of `evalRowCell` so gocyclo doesn't fire.
func (a *Analyzer) evalRowCellForAccent(
	mode Mode, tint paneTintHue, variant string, accent AccentVariant,
	worst map[rowWorstKey]Finding,
) int {
	vars := a.Vars
	if !accent.IsDefault {
		vars = withAccentOverride(a.Vars, accent)
	}
	// Mirror the `selection-fg fallback` rule in `app.css`: in dark mode,
	// when the pane is tinted AND the row is cursor-active + selected,
	// `--color-selection-fg` swaps to `--color-selection-fg-fallback`.
	// The resolver doesn't model rule-level CSS overrides; apply by hand.
	if shouldUseSelectionFgFallback(mode, tint, variant) {
		vars = withSelectionFgFallback(vars)
	}
	bg, ok := resolveRowBg(vars, mode, tint, variant)
	if !ok {
		return 0
	}
	evaluated := 0
	for _, role := range rowSelectedTextRoles {
		f, ok := evalRowText(vars, mode, role, bg, accent, tint, variant)
		if !ok {
			continue
		}
		k := rowWorstKey{role: role, mode: mode, tint: tint.Name, variant: variant}
		if cur, exists := worst[k]; !exists || f.Ratio < cur.Ratio {
			worst[k] = f
		}
		evaluated++
	}
	return evaluated
}

// evalRowText turns a (vars, role) pair into a Finding tagged with the
// scenario coordinates.
func evalRowText(
	vars *VarTable, mode Mode, role string, bg RGBA,
	accent AccentVariant, tint paneTintHue, variant string,
) (Finding, bool) {
	fg, ok := resolveTextRole(vars, mode, role)
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
		File:          syntheticRowMatrixPath(),
		Line:          0,
		Selector:      describeRowScenario(role, tint, variant),
		Mode:          mode,
		FG:            fg,
		BG:            bg,
		Ratio:         ratio,
		Threshold:     4.5,
		IsPassing:     ratio >= 4.5,
		AccentVariant: accentTag,
	}, true
}

// shouldUseSelectionFgFallback mirrors the `app.css` rule that swaps
// `--color-selection-fg` to its fallback. Keep this predicate in sync with
// the CSS selector (see the "selection-fg fallback" block in `app.css`).
//
// Today the rule triggers when ALL of these hold:
//   - dark mode (light mode's primary value is dark enough to clear AA on
//     every tinted bg without help)
//   - the pane has a tint applied
//   - the row is the focused cursor-active row (`.is-under-cursor` +
//     focused container, in our matrix terms: `variant == "cursor-active"`).
func shouldUseSelectionFgFallback(mode Mode, tint paneTintHue, variant string) bool {
	return mode == ModeDark && tint.VarName != "" && variant == "cursor-active"
}

// withSelectionFgFallback returns a VarTable derived from v with
// `--color-selection-fg` pointing at `--color-selection-fg-fallback` so the
// `--color-size-*-selected` mixes (which reference `--color-selection-fg`)
// pick up the swap automatically.
func withSelectionFgFallback(v *VarTable) *VarTable {
	out := NewVarTable()
	maps.Copy(out.Light, v.Light)
	maps.Copy(out.Dark, v.Dark)
	out.Light["color-selection-fg"] = "var(--color-selection-fg-fallback)"
	out.Dark["color-selection-fg"] = "var(--color-selection-fg-fallback)"
	return out
}

func describeRowScenario(role string, tint paneTintHue, variant string) string {
	return fmt.Sprintf(".file-entry.is-selected .%s (tint=%s, %s)", role, tint.Name, variant)
}

// syntheticRowMatrixPath is the marker path embedded in synthesized Findings
// so the reporter can tell where they came from. The file doesn't exist; the
// path just anchors the report to this file in case a reader follows it.
func syntheticRowMatrixPath() string {
	return "scripts/check-a11y-contrast/row_state_matrix.go"
}
