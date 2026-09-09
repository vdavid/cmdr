package main

import "strings"

// OpacityFinding flags a rule that dims (or fully hides) text via a static
// `opacity: N < 1` the rule walker never sees: `analyzer.go` pairs a `color`
// and a `background` declared on the SAME selector and has no notion of
// `opacity` at all, so a translucent text color composites against whatever
// is behind it (its own ancestor's bg, at runtime) without this checker ever
// evaluating the result. This is a structural "we can't see this" flag, not
// a computed WCAG/APCA verdict like `Finding` — see the README's "Detect,
// don't compute" note.
type OpacityFinding struct {
	File     string
	Line     int
	Selector string
	Opacity  float64
}

// opacityInactiveMarkers are selector substrings that mark a disabled or
// otherwise inactive UI component. WCAG 1.4.3 explicitly exempts "inactive
// user interface components" from the text contrast requirement, and this is
// where the large majority of the codebase's `opacity` dimming lives
// (buttons, inputs, toggles, menu items). Checked against the lowercased
// selector text for attribute/pseudo-class forms.
var opacityInactiveMarkers = []string{
	":disabled",
	"[disabled]",
	"aria-disabled",
	"data-disabled",
	// A gated (locked/paywalled) feature card: its controls are genuinely
	// inactive while gated, same as `:disabled` (`SectionCard.svelte`).
	"data-gated",
}

// opacityInactiveClassPatterns are class-name shapes that mark the same
// inactive-component state as `opacityInactiveMarkers`, but via a plain
// class rather than an attribute/pseudo-class (`.disabled`, `.is-disabled`,
// `.is-disabled-look`, `.text-field-disabled`).
func opacityInactiveSelector(rule Rule) bool {
	sel := strings.ToLower(rule.Selector)
	for _, marker := range opacityInactiveMarkers {
		if strings.Contains(sel, marker) {
			return true
		}
	}
	for _, c := range rule.Classes {
		cl := strings.ToLower(c)
		if cl == "disabled" || strings.HasPrefix(cl, "is-disabled") || strings.HasSuffix(cl, "-disabled") {
			return true
		}
	}
	return false
}

// opacityDecorativeEntry is one hand-verified non-text element: an <Icon>
// component, an empty CSS-shape status indicator (a colored dot/bar/swatch
// with no child content), or an aria-hidden punctuation divider carrying no
// informational content. WCAG's text contrast requirement (and this
// checker's stated scope) is about TEXT; none of these render a text glyph,
// so an opacity dim on them is the right tool and out of scope here.
//
// Each entry was verified by reading the component's markup (not guessed
// from the class name) during the 2026-09 opacity survey — see
// `scripts/check-a11y-contrast/README.md` § "Add a new opacity exemption"
// for how to add one and what evidence it needs.
type opacityDecorativeEntry struct {
	fileSuffix string
	selector   string
	why        string
}

var opacityDecorativeAllowlist = []opacityDecorativeEntry{
	{"app-file-list.css", ".file-entry .restricted-indicator", "wraps an <Icon>, aria-hidden"},
	{"VolumeBreadcrumb.svelte", ".restricted-indicator", "wraps an <Icon>, aria-hidden"},
	{"VolumeBreadcrumb.svelte", ".read-only-indicator", `wraps <Icon name="lock">`},
	{"VolumeBreadcrumb.svelte", ".smb-indicator", "empty span, pure CSS-colored status dot"},
	{"VolumeBreadcrumb.svelte", ".smb-indicator-saved", "empty span, pure CSS-colored dot outline"},
	{"VolumeBreadcrumb.svelte", ".usb-speed-indicator", "empty span, pure CSS-colored status dot"},
	{"DriveIndexBadge.svelte", ".drive-index-badge", "empty <button>, pure CSS-colored status dot"},
	{"ImageIndexDriveBadge.svelte", ".image-index-drive-badge", `empty <span role="img">, pure CSS-colored status dot`},
	{"TabBar.svelte", ".warning-icon", "wraps an <Icon>"},
	{"TabBar.svelte", ".pin-icon", "wraps an <Icon>"},
	{"AiLocalSection.svelte", ".ram-projected", "empty bar-chart segment / legend swatch"},
	{"AiLocalSection.svelte", ".ram-freed", "empty bar-chart segment / legend swatch"},
	{"AskCmdrCostFooter.svelte", ".dot", `aria-hidden "·" divider, no informational content`},
	{"RepoChip.svelte", ".sep", `aria-hidden "·" divider, no informational content`},
	{"IndexingStatusBody.svelte", ".step-pending .step-marker", "wraps a <Spinner>/<Icon>, aria-hidden"},
	{"FileIcon.svelte", ".icon-wrapper.is-dimmed", `holds an alt="" <img> plus badge glyphs, no text; the row's name carries the meaning`},
}

func opacityDecorativeReason(rule Rule) (string, bool) {
	for _, e := range opacityDecorativeAllowlist {
		if strings.HasSuffix(rule.File, e.fileSuffix) && rule.Selector == e.selector {
			return e.why, true
		}
	}
	return "", false
}

// opacityModeledElsewhere lists (file-suffix, selector) pairs whose real
// composited contrast is already covered by a scenario synthesizer (a
// `FgExpr` in `dropdown_states.go` / `query_dialog_states.go` that models the
// same `opacity: N` as a `color-mix(..., transparent (1-N)%)` term). Empty
// today: `ToggleGroup.svelte`'s `.tg-hint` dropped its `opacity: 0.7` crutch
// after it failed AA (see the comment at `ToggleGroup.svelte` ~line 329) and
// nothing has re-added a modeled case since. Add an entry here when a
// synthesizer picks up a real `opacity` rule, so this check doesn't
// double-report what the synthesizer already verifies.
var opacityModeledElsewhere []opacityDecorativeEntry

func opacityIsModeledElsewhere(rule Rule) bool {
	for _, e := range opacityModeledElsewhere {
		if strings.HasSuffix(rule.File, e.fileSuffix) && rule.Selector == e.selector {
			return true
		}
	}
	return false
}

// AnalyzeOpacity walks every parsed rule for a static `opacity: N < 1` that
// the WCAG/APCA rule walker can't fold into its color/background pairing. A
// rule is reported unless it's a disabled/inactive-component state (WCAG
// 1.4.3's own exemption), a hand-verified non-text/decorative element, or
// already covered by a scenario synthesizer. Deduplicates by (file,
// selector): a `prefers-reduced-motion` or other at-rule override of the
// same class state is one physical component, not a second finding.
func (a *Analyzer) AnalyzeOpacity(pf *ParsedFile) []OpacityFinding {
	var out []OpacityFinding
	seen := make(map[string]bool)

	for _, rule := range pf.Rules {
		if !rule.HasOpacity || rule.Opacity >= 1 {
			continue
		}
		// `opacity: 0` renders nothing: there's no visible glyph to have a
		// contrast ratio against anything. This is the CSS idiom for
		// "hidden until :hover/:focus/a state class reveals it" (a close
		// button, a hover-only overlay action, a singleton tooltip element
		// before it's positioned and shown), not a dimmed-but-readable text
		// case. The revealed state is a separate rule (its own selector,
		// commonly `:hover`/`:focus`/`.is-open`) and gets evaluated on its
		// own merits like any other rule.
		if rule.Opacity == 0 {
			continue
		}
		if opacityInactiveSelector(rule) {
			continue
		}
		if _, ok := opacityDecorativeReason(rule); ok {
			continue
		}
		if opacityIsModeledElsewhere(rule) {
			continue
		}
		key := rule.File + "|" + rule.Selector
		if seen[key] {
			continue
		}
		seen[key] = true
		out = append(out, OpacityFinding{
			File:     rule.File,
			Line:     rule.Line,
			Selector: rule.Selector,
			Opacity:  rule.Opacity,
		})
	}
	return out
}
