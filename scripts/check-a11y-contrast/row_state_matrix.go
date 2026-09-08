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
// Row text comes from a different selector than the bg in two shapes the
// rule walker can't pair on its own:
//   - Selected-row text (`.file-entry.is-selected .col-name { color:
//     var(--color-selection-fg) }`): the bg is on the row, the color on a
//     `.is-selected` descendant.
//   - Unselected-row text that isn't `--color-text-primary` (today: the
//     shared quiet-text token `--color-text-quiet`, which both the
//     hidden-entry name dim and the TCC-restricted row treatment set):
//     the bg is on the row, the color on a plain descendant with no
//     selection state involved at all.
//
// This synthesizer composes the bg the row will actually render with — pane
// tint, optional stripe, optional cursor overlay, and (selected roles only)
// the selection-bg override — and pairs each text role in `rowRoleGroups`
// against it.
//
// We report the worst-case finding per (textRole, mode, tint, variant) so the
// output stays small even though the synthesizer evaluates a few thousand
// combos.

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
// the generic walker skips them — hence this scenario list. They render only
// on `.is-selected` rows, so their bg composition includes the
// `--color-selection-bg` override (see `resolveRowBg`'s `selected` arg).
var rowSelectedTextRoles = []string{
	"color-selection-fg",
	"color-size-bytes-selected",
	"color-size-kb-selected",
	"color-size-mb-selected",
	"color-size-gb-selected",
	"color-size-tb-selected",
}

// rowUnselectedTextRoles are text roles set on a plain (non-`.is-selected`)
// row descendant, so their bg is pane tint × stripe × cursor state with no
// selection fill — a shape the generic rule walker never pairs either, since
// the color and the bg still live on different selectors.
//
// Today this holds the shared quiet-text token (`--color-text-quiet`,
// `app.css`): both the hidden-entry name dim and the TCC-restricted row
// treatment set it, and neither is on a selector the walker can pair with its
// actual row bg. Extend this list when another unselected-row text role needs
// the same treatment.
var rowUnselectedTextRoles = []string{
	"color-text-quiet",
}

// rowUnselectedVariants excludes "cursor-active" from the unselected-role
// sweep: a restricted row (the only unselected-row use of the quiet token
// that ever reaches an active cursor — the hidden-entry dim already excludes
// every cursor state in `isHiddenNameDimmed`) reverts to full-strength text
// there, via `.file-entry.is-under-cursor.is-restricted` in `FullList.svelte`
// / `BriefList.svelte`. That rule exists BECAUSE the quiet token composited
// against the accent-tinted cursor-active overlay drops below the enforced
// floor for some accent/tint combinations (Apple Yellow + a warm tint).
// Evaluating "cursor-active" here would check a bg the renderer no longer
// pairs with this token; re-add it (and re-add the CSS rule that excludes it)
// together if a future unselected-row role needs to render under an active
// cursor.
var rowUnselectedVariants = []string{"plain", "striped", "cursor-inactive"}

// rowRoleGroup pairs a set of text-role tokens with whether they render only
// on selected rows, and the bg variants they're actually evaluated against.
// `selected` governs two things downstream: whether `resolveRowBg` may
// substitute `--color-selection-bg` for the pane/stripe bg, and whether the
// three-tier `--color-selection-fg` cascade override applies (irrelevant to a
// role that isn't derived from selection-fg).
type rowRoleGroup struct {
	roles    []string
	selected bool
	variants []string // nil means every `rowBgVariants` entry
}

func (g rowRoleGroup) bgVariants() []string {
	if g.variants != nil {
		return g.variants
	}
	return rowBgVariants
}

var rowRoleGroups = []rowRoleGroup{
	{roles: rowSelectedTextRoles, selected: true},
	{roles: rowUnselectedTextRoles, selected: false, variants: rowUnselectedVariants},
}

// AnalyzeRowStates evaluates every (mode × pane tint × bg variant × role
// group × accent × text role) combination and returns the worst-case Finding
// per (textRole, mode, tint, variant) — accent is collapsed into the worst
// variant internally. The result is roughly 700 findings (well under the rule
// walker's total) and surfaces every failing combo without exploding the
// report.
func (a *Analyzer) AnalyzeRowStates() []Finding {
	worst, evaluated := make(map[rowWorstKey]Finding), 0
	for _, mode := range []Mode{ModeLight, ModeDark} {
		for _, tint := range rowPaneTints {
			for _, group := range rowRoleGroups {
				for _, variant := range group.bgVariants() {
					evaluated += a.evalRowCell(mode, tint, variant, group, worst)
				}
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
// would actually paint. `selected` is the role group's `selected` flag: only
// a role that renders on `.is-selected` rows may see `--color-selection-bg`
// substituted in; an unselected-row role always renders on the plain
// pane/stripe/cursor bg, even in the "plain"/"striped" variants.
func resolveRowBg(vars *VarTable, mode Mode, tint paneTintHue, variant string, selected bool) (RGBA, bool) {
	paneBg, ok := resolvePaneBg(vars, mode, tint)
	if !ok {
		return RGBA{}, false
	}
	switch variant {
	case "plain":
		return resolvePlainOrStripedBg(vars, mode, paneBg, selected, "")
	case "striped":
		return resolvePlainOrStripedBg(vars, mode, paneBg, selected, "color-bg-stripe")
	case "cursor-inactive":
		return resolveOverlayBg(vars, mode, paneBg, "color-cursor-inactive")
	case "cursor-active":
		return resolveOverlayBg(vars, mode, paneBg, "color-cursor-active")
	}
	return RGBA{}, false
}

// resolvePlainOrStripedBg handles the "plain" (`stripeVar == ""`) and
// "striped" variants, which share the same selected-row override: a selected
// row gets a darker bg via `--color-selection-bg` (dark mode only;
// transparent in light) regardless of stripe, since the stripe rule and the
// selection-bg rule carry the same specificity and `.is-selected` appears
// later in `apps/desktop/src/app-file-list.css`. Unselected rows never see
// selection-bg — they render the pane bg, or the stripe color on top of it.
func resolvePlainOrStripedBg(vars *VarTable, mode Mode, paneBg RGBA, selected bool, stripeVar string) (RGBA, bool) {
	if selected {
		if sel, ok := resolveSelectionBg(vars, mode, paneBg); ok {
			return sel, true
		}
	}
	if stripeVar == "" {
		return paneBg, true
	}
	return resolveOverlayBg(vars, mode, paneBg, stripeVar)
}

// resolveOverlayBg resolves a translucent overlay var (the stripe color or a
// cursor highlight) and composites it over paneBg when it isn't already
// opaque. Shared by the stripe fallback and both cursor variants.
func resolveOverlayBg(vars *VarTable, mode Mode, paneBg RGBA, varName string) (RGBA, bool) {
	c, ok := resolveVar(vars, mode, varName)
	if !ok {
		return RGBA{}, false
	}
	if !c.Opaque() {
		c = CompositeOver(c, paneBg)
	}
	return c, true
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

// evalRowCell evaluates one (mode, tint, variant, group) cell across the
// group's roles and all relevant accent variants. Returns how many
// (role × accent) pairs were evaluated, and updates `worst` in place with the
// lowest-ratio finding per (role × mode × tint × variant) tuple.
func (a *Analyzer) evalRowCell(mode Mode, tint paneTintHue, variant string, group rowRoleGroup, worst map[rowWorstKey]Finding) int {
	accents := []AccentVariant{{Name: "default", IsDefault: true}}
	if variant == "cursor-active" {
		accents = AccentVariants
	}
	evaluated := 0
	for _, accent := range accents {
		evaluated += a.evalRowCellForAccent(mode, tint, variant, accent, group, worst)
	}
	return evaluated
}

// evalRowCellForAccent runs one (mode, tint, variant, accent) sample across
// every role in `group`. Splits out of `evalRowCell` so gocyclo doesn't fire.
func (a *Analyzer) evalRowCellForAccent(
	mode Mode, tint paneTintHue, variant string, accent AccentVariant, group rowRoleGroup,
	worst map[rowWorstKey]Finding,
) int {
	vars := a.Vars
	if !accent.IsDefault {
		vars = withAccentOverride(a.Vars, accent)
	}
	if group.selected {
		// Mirror the three-tier `--color-selection-fg` cascade from
		// `app.css` + `app-file-list.css`. The resolver doesn't model
		// rule-level CSS overrides, so apply the right tier by hand per
		// scenario. Irrelevant to an unselected-row role: none of them
		// derive from `--color-selection-fg`.
		vars = withSelectionFgVariant(vars, selectionFgTokenFor(mode, tint, variant))
	}
	bg, ok := resolveRowBg(vars, mode, tint, variant, group.selected)
	if !ok {
		return 0
	}
	evaluated := 0
	for _, role := range group.roles {
		f, ok := evalRowText(vars, mode, role, bg, accent, tint, variant, group.selected)
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
	accent AccentVariant, tint paneTintHue, variant string, selected bool,
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
		Selector:      describeRowScenario(role, tint, variant, selected),
		Mode:          mode,
		FG:            fg,
		BG:            bg,
		Ratio:         ratio,
		Threshold:     4.5,
		IsPassing:     ratio >= 4.5,
		AccentVariant: accentTag,
	}, true
}

// selectionFgTokenFor picks which `--color-selection-fg-*` variant applies
// to a given scenario, mirroring the cascade in `app.css` + `app-file-list.css`.
// Keep this in sync with the CSS rules: least- to most-specific is
// primary → cursor.
//
// The cascade today:
//   - default: `--color-selection-fg-primary` (the strong red, AA-safe vs
//     `--color-selection-bg`).
//   - row is under cursor: `--color-selection-fg-cursor` (a hair darker
//     red in light, lighter in dark, AA-safe vs the translucent cursor bg).
//
// There's a fourth tier in the CSS — dark + tinted + cursor-active drops to
// `--color-selection-fg-fallback` (= `--color-text-primary`) — but it fires
// ONLY under `prefers-contrast: more` (25% tint), which this matrix doesn't
// model (it evaluates the 15% baseline; see `resolvePaneBg`). At 15% the red
// cursor variant clears AA on every tint, so the matrix uses it here too.
func selectionFgTokenFor(_ Mode, _ paneTintHue, variant string) string {
	if variant == "cursor-active" || variant == "cursor-inactive" {
		return "color-selection-fg-cursor"
	}
	return "color-selection-fg-primary"
}

// withSelectionFgVariant returns a VarTable derived from v with
// `--color-selection-fg` pointing at the named variant token. Both modes
// get the override so the `--color-size-*-selected` mixes (which reference
// `--color-selection-fg`) pick up the right tier automatically.
func withSelectionFgVariant(v *VarTable, tokenName string) *VarTable {
	out := NewVarTable()
	maps.Copy(out.Light, v.Light)
	maps.Copy(out.Dark, v.Dark)
	expr := fmt.Sprintf("var(--%s)", tokenName)
	out.Light["color-selection-fg"] = expr
	out.Dark["color-selection-fg"] = expr
	return out
}

func describeRowScenario(role string, tint paneTintHue, variant string, selected bool) string {
	scope := ".file-entry"
	if selected {
		scope += ".is-selected"
	}
	return fmt.Sprintf("%s .%s (tint=%s, %s)", scope, role, tint.Name, variant)
}

// syntheticRowMatrixPath is the marker path embedded in synthesized Findings
// so the reporter can tell where they came from. The file doesn't exist; the
// path just anchors the report to this file in case a reader follows it.
func syntheticRowMatrixPath() string {
	return "scripts/check-a11y-contrast/row_state_matrix.go"
}
