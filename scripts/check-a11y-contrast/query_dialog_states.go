package main

// Search / Select dialog scenarios the rule walker can't see.
//
// These query dialogs (`lib/query-ui/`) compose several fg-on-bg pairs where
// the text color and the background it actually renders on live on different
// selectors (or get folded through `opacity`), so the generic same-selector
// pairing in `analyzer.go` never evaluates them. Each entry below names the
// real composited pair and runs it through the accent matrix + both modes,
// exactly like `dropdown_states.go`.
//
// To model an `opacity: N` on the text (CSS `opacity` composites the glyph
// against whatever is behind it), use `FgExpr` with a
// `color-mix(in srgb, var(--token), transparent (1-N)%)` term: the synthesizer
// then composites that translucent fg over the resolved bg, which is exactly
// what `opacity` produces. The hint below uses this to model its `opacity: 0.7`.
//
// Add a new entry when a query-dialog surface starts rendering muted text on a
// non-`bg-primary` (especially accent-tinted) background that the walker can't
// pair. The whole list runs against the accent matrix automatically.
var queryDialogScenarios = []ancestorBgScenario{
	{
		// `ToggleGroup` "AI" badge: `.tg-badge` sets its own
		// `color: var(--color-text-primary)` on a `--color-accent-subtle` bg.
		// The badge bg sits inside a `.tg-item` whose bg is `--color-bg-primary`,
		// so composite the translucent subtle accent over bg-primary.
		Selector: ".tg-badge (AI) on --color-accent-subtle",
		FgVar:    "color-text-primary",
		BgVar:    "color-accent-subtle",
	},
	{
		// `ToggleGroup` shortcut hint (`.tg-hint`, for example `⌥A`): tertiary
		// mono text on the resting `.tg-item` bg (`--color-bg-primary`). The
		// hint carries no `opacity` crutch (it failed AA at 0.7); the visible
		// quiet comes from tertiary itself. If you re-add `opacity: N`, mirror
		// it here with a `color-mix(in srgb, var(--color-text-tertiary),
		// transparent (1-N)%)` FgExpr so the check stays honest.
		Selector: ".tg-hint shortcut on --color-bg-primary",
		FgVar:    "color-text-tertiary",
		BgVar:    "color-bg-primary",
	},
	// Under-cursor result row (`.result-row.is-under-cursor`) in `QueryResults`:
	// the row bg flips to `--color-accent-subtle`, composited over the dialog
	// body (`--color-bg-secondary`). The muted columns (path / size / modified)
	// set their color on separate `.result-*` selectors, so the walker never
	// pairs them with the cursor bg. Under the cursor all three render at
	// `--color-text-primary` (the tertiary / secondary tokens don't clear AA on
	// the lightest accent tints); this entry pins that.
	{
		Selector: ".result-row.is-under-cursor .result-path/size/modified",
		FgVar:    "color-text-primary",
		// `--color-accent-subtle` is a 15%-alpha accent tint; the dialog body
		// behind the row is `--color-bg-secondary`. Compositing that tint over
		// the body is `mix(accent 15%, bg-secondary)` (an opaque, still
		// accent-dependent color, so the matrix sweep applies).
		BgExpr: "color-mix(in srgb, var(--color-accent) 15%, var(--color-bg-secondary))",
	},
	{
		// Footer shortcut hint (`.shortcut-hint`, for example the `⏎` next to
		// "Go to file"): tertiary mono text on the footer's `--color-bg-primary`.
		// Carries no `opacity` crutch (it dropped below AA at 0.8). Mirror any
		// re-added opacity with a transparent-mix FgExpr.
		Selector: ".shortcut-hint on footer --color-bg-primary",
		FgVar:    "color-text-tertiary",
		BgVar:    "color-bg-primary",
	},
	{
		// Footer shortcut hint ON the primary button (`.shortcut-on-primary`,
		// the `⏎` baked into the filled "Select these files" button):
		// `--color-accent-fg` on the `--color-accent` button bg. accent-fg is
		// auto-picked (black/white) for max contrast on the active accent, so
		// the matrix sweep confirms every accent clears AA.
		Selector: ".shortcut-hint.shortcut-on-primary on --color-accent",
		FgVar:    "color-accent-fg",
		BgVar:    "color-accent",
	},
}

// AnalyzeQueryDialogStates evaluates each query-dialog scenario against the
// accent matrix + both modes, returning the worst-case finding per
// (selector, mode). Reuses `evalDropdownSample` (the dropdown synthesizer is
// generic over `ancestorBgScenario`).
func (a *Analyzer) AnalyzeQueryDialogStates() []Finding {
	type key struct {
		selector string
		mode     Mode
	}
	worst := make(map[key]Finding)
	evaluated := 0

	for _, sc := range queryDialogScenarios {
		for _, mode := range []Mode{ModeLight, ModeDark} {
			for _, accent := range AccentVariants {
				f, ok := evalDropdownSample(a.Vars, mode, accent, sc)
				if !ok {
					continue
				}
				f.File = syntheticQueryDialogPath()
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

func syntheticQueryDialogPath() string {
	return "scripts/check-a11y-contrast/query_dialog_states.go"
}
