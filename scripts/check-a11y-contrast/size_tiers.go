package main

import (
	"fmt"
	"path/filepath"
)

// Size tier coverage.
//
// The size-tier utility classes (`.size-bytes`, `.size-kb`, ...) are pure text
// utilities defined globally in `app.css` — they only declare `color:` and
// inherit the background of whatever container they're rendered in. The Svelte
// rule walker can't see this pairing because the global rules don't set a
// background.
//
// To plug the gap, we maintain an explicit list of every container background
// the size text actually renders against (footer, dialogs, tooltips, dropdowns,
// search results, viewer, AI toast, settings panels, error report). For each
// (tier × background × mode) we synthesize a Finding using the resolver +
// contrast math, so violations are reported in the same pipeline as the
// generic rule walker.
//
// Update this list whenever a new context starts using `<Size>` or a `size-*`
// class against a background different from the ones already listed.

// sizeBackground names a (CSS var, where-it-renders) pair. We resolve the var
// to a concrete color and report any violation with the label for context.
type sizeBackground struct {
	// VarName is the CSS custom-property name (without leading `--`).
	VarName string
	// Label is the user-facing context name used in violation output.
	Label string
}

// sizeTier names one tier class.
type sizeTier struct {
	// VarName is the size color CSS custom-property name (without `--`).
	VarName string
	// Class is the corresponding utility class name (for display).
	Class string
}

// Backgrounds where `<Size>` / `.size-*` text is rendered. Keep alphabetized
// by VarName for predictable output.
//
// Notes:
//   - `color-bg-primary` covers the file list, transfer/delete dialog bodies,
//     viewer body, settings pages, and the fallback transfer-error dialog.
//   - `color-bg-secondary` covers the SelectionInfo footer, viewer status bar,
//     search results panel, AiLocalSection rows, ErrorReportDialog body,
//     DriveIndexingSection sub-panel.
//   - `color-bg-tertiary` covers the VolumeBreadcrumb dropdown items, AI toast,
//     ErrorReportDialog file rows, AiLocalSection inner cards.
//   - `color-error-bg` covers the DeleteDialog danger row.
//   - `color-warning-bg` covers the TransferDialog / DeleteDialog warning rows
//     and the TransferProgressDialog warning summary.
var sizeBackgrounds = []sizeBackground{
	{"color-bg-primary", "page bg (file list, dialog body, viewer)"},
	{"color-bg-secondary", "secondary panel (SelectionInfo, viewer footer, search, settings sub-panel)"},
	{"color-bg-tertiary", "tertiary surface (volume dropdown, AI toast, error-report file rows)"},
	{"color-error-bg", "danger row (DeleteDialog)"},
	{"color-warning-bg", "warning row (TransferDialog, DeleteDialog, transfer progress)"},
}

// Tier order matches the rainbow palette in app.css.
var sizeTiers = []sizeTier{
	{"color-size-bytes", "size-bytes"},
	{"color-size-kb", "size-kb"},
	{"color-size-mb", "size-mb"},
	{"color-size-gb", "size-gb"},
	{"color-size-tb", "size-tb"},
}

// AnalyzeSizeTiers returns findings for every (tier × background × mode) combo.
// Pseudo file path "<size-tiers>" makes the source obvious in the report.
//
// The rainbow palette is the default and the only one the checker validates —
// the `accent` and `none` palettes are user opt-ins that resolve via the same
// `--color-size-*` vars (different definitions in scoped `:root[data-...]`
// blocks), so testing the default is sufficient unless a user changes their
// setting. The frontend setting docs note this trade-off.
func (a *Analyzer) AnalyzeSizeTiers() []Finding {
	var findings []Finding
	for _, bg := range sizeBackgrounds {
		for _, tier := range sizeTiers {
			for _, mode := range []Mode{ModeLight, ModeDark} {
				if f, ok := a.evaluateSizeTier(tier, bg, mode); ok {
					findings = append(findings, f)
				}
			}
		}
	}
	a.RulesEvaluated += len(sizeBackgrounds) * len(sizeTiers)
	return findings
}

// evaluateSizeTier resolves one (tier, bg, mode) into a Finding. The text is
// normal body weight at 12–14px, so the threshold is the standard 4.5:1.
func (a *Analyzer) evaluateSizeTier(tier sizeTier, bg sizeBackground, mode Mode) (Finding, bool) {
	tierRaw, ok := a.Vars.Raw(tier.VarName, mode)
	if !ok {
		a.Warnings = append(a.Warnings, fmt.Sprintf("size tier: undefined var --%s (%s)", tier.VarName, mode))
		return Finding{}, false
	}
	bgRaw, ok := a.Vars.Raw(bg.VarName, mode)
	if !ok {
		a.Warnings = append(a.Warnings, fmt.Sprintf("size tier: undefined bg var --%s (%s)", bg.VarName, mode))
		return Finding{}, false
	}

	selector := fmt.Sprintf(".%s on --%s (%s)", tier.Class, bg.VarName, bg.Label)
	// 0 fontPx, 400 fontWeight → 4.5:1 threshold (matches `evaluate`).
	return a.evaluate(syntheticSizeTierPath(), 0, selector, tierRaw, bgRaw, 0, 400, false, mode)
}

// syntheticSizeTierPath is a marker path used in the Finding so the reporter
// shows where this came from. The file doesn't exist — `RelPath` falls through
// to the absolute path and we tag it with a leading marker.
func syntheticSizeTierPath() string {
	// Anchor to repo-relative for readable output.
	return filepath.Join("scripts", "check-a11y-contrast", "size_tiers.go")
}
