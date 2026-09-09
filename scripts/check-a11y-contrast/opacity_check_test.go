package main

import "testing"

// TestAnalyzeOpacity_DisabledStateExempt covers the WCAG 1.4.3 exemption for
// inactive/disabled UI components: a `:disabled` (or `[data-disabled]`,
// `.disabled`, `[aria-disabled='true']`, `[data-gated='true']`) rule dimming
// via `opacity` is expected and shouldn't be flagged.
func TestAnalyzeOpacity_DisabledStateExempt(t *testing.T) {
	cases := []Rule{
		{File: "Button.svelte", Line: 10, Selector: ".btn:disabled", Classes: []string{"btn"}, Opacity: 0.4, HasOpacity: true},
		{File: "Combobox.svelte", Line: 10, Selector: ":global(.combobox-input[data-disabled])", Classes: []string{"combobox-input"}, Opacity: 0.5, HasOpacity: true},
		{File: "SettingRow.svelte", Line: 10, Selector: ".setting-row.disabled", Classes: []string{"setting-row", "disabled"}, Opacity: 0.6, HasOpacity: true},
		{File: "TransferConflictDialog.svelte", Line: 10, Selector: ".danger-text[aria-disabled='true']", Classes: []string{"danger-text"}, Opacity: 0.4, HasOpacity: true},
		{File: "SectionCard.svelte", Line: 10, Selector: "[data-gated='true'] .section-card", Classes: []string{"section-card"}, Opacity: 0.5, HasOpacity: true},
		{File: "filter-popover.css", Line: 10, Selector: ".list-cell.is-disabled-look", Classes: []string{"list-cell", "is-disabled-look"}, Opacity: 0.5, HasOpacity: true},
		{File: "app-field.css", Line: 10, Selector: ".text-field-disabled", Classes: []string{"text-field-disabled"}, Opacity: 0.5, HasOpacity: true},
	}

	a := NewAnalyzer(NewVarTable())
	for _, rule := range cases {
		pf := &ParsedFile{Path: rule.File, Rules: []Rule{rule}}
		findings := a.AnalyzeOpacity(pf)
		if len(findings) != 0 {
			t.Errorf("selector %q: expected disabled/inactive-state exemption, got finding(s) %+v", rule.Selector, findings)
		}
	}
}

// TestAnalyzeOpacity_PlainTextReported is the core regression case: an
// opacity-dimmed rule with no disabled/inactive marker and no decorative
// exemption must be reported, since the rule walker can't fold `opacity`
// into its color/background pairing (see the README's "Detect, don't
// compute" note). This is the shape of the bug fixed in `4cdc00c0a` before
// its row got converted to a `--color-text-quiet` token.
func TestAnalyzeOpacity_PlainTextReported(t *testing.T) {
	rule := Rule{
		File:       "VolumeBreadcrumb.svelte",
		Line:       1363,
		Selector:   ".volume-item.is-restricted .volume-label",
		Classes:    []string{"volume-label"},
		Opacity:    0.6,
		HasOpacity: true,
	}
	a := NewAnalyzer(NewVarTable())
	pf := &ParsedFile{Path: rule.File, Rules: []Rule{rule}}
	findings := a.AnalyzeOpacity(pf)
	if len(findings) != 1 {
		t.Fatalf("expected 1 unmodeled-opacity finding, got %d", len(findings))
	}
	f := findings[0]
	if f.Selector != rule.Selector || f.Line != rule.Line || f.Opacity != rule.Opacity {
		t.Errorf("finding mismatch: %+v", f)
	}
}

// TestAnalyzeOpacity_DecorativeExempt covers the hand-verified non-text
// allowlist: an icon wrapper, an empty CSS-shape status dot, and an
// aria-hidden punctuation divider all render no text glyph, so this
// checker's text-contrast scope doesn't apply to them.
func TestAnalyzeOpacity_DecorativeExempt(t *testing.T) {
	cases := []Rule{
		{File: "apps/desktop/src/app-file-list.css", Line: 29, Selector: ".file-entry .restricted-indicator", Classes: []string{"restricted-indicator"}, Opacity: 0.7, HasOpacity: true},
		{File: "apps/desktop/src/lib/file-explorer/navigation/VolumeBreadcrumb.svelte", Line: 1369, Selector: ".restricted-indicator", Classes: []string{"restricted-indicator"}, Opacity: 0.6, HasOpacity: true},
		{File: "apps/desktop/src/lib/ask-cmdr/AskCmdrCostFooter.svelte", Line: 100, Selector: ".dot", Classes: []string{"dot"}, Opacity: 0.6, HasOpacity: true},
		// The hidden-entry icon dim: a raster OS icon plus badge glyphs, with the
		// row's own name carrying the meaning. It is the one allowlisted entry
		// that exists to make something LESS visible on purpose, so pin it.
		{File: "apps/desktop/src/lib/file-explorer/selection/FileIcon.svelte", Line: 142, Selector: ".icon-wrapper.is-dimmed", Classes: []string{"icon-wrapper", "is-dimmed"}, Opacity: 0.5, HasOpacity: true},
	}
	a := NewAnalyzer(NewVarTable())
	for _, rule := range cases {
		pf := &ParsedFile{Path: rule.File, Rules: []Rule{rule}}
		findings := a.AnalyzeOpacity(pf)
		if len(findings) != 0 {
			t.Errorf("selector %q in %s: expected decorative exemption, got finding(s) %+v", rule.Selector, rule.File, findings)
		}
	}
}

// TestAnalyzeOpacity_IgnoresOpaqueAndUnset covers the two non-findings: no
// `opacity` declared at all (HasOpacity false), and an explicit `opacity: 1`.
func TestAnalyzeOpacity_IgnoresOpaqueAndUnset(t *testing.T) {
	cases := []Rule{
		{File: "Foo.svelte", Line: 1, Selector: ".foo", Classes: []string{"foo"}},
		{File: "Foo.svelte", Line: 2, Selector: ".bar", Classes: []string{"bar"}, Opacity: 1, HasOpacity: true},
	}
	a := NewAnalyzer(NewVarTable())
	pf := &ParsedFile{Path: "Foo.svelte", Rules: cases}
	findings := a.AnalyzeOpacity(pf)
	if len(findings) != 0 {
		t.Errorf("expected no findings for unset/opaque opacity, got %+v", findings)
	}
}

// TestAnalyzeOpacity_ZeroOpacityIgnored covers the "hidden until :hover/
// :focus/a state class reveals it" idiom (a tab close button, a hover-only
// overlay action, a singleton tooltip before it's shown): `opacity: 0`
// renders nothing, so there's no visible glyph to have a contrast ratio
// against anything. The revealed state is a separate rule and is evaluated
// on its own.
func TestAnalyzeOpacity_ZeroOpacityIgnored(t *testing.T) {
	rule := Rule{
		File:       "TabBar.svelte",
		Line:       1,
		Selector:   ".close-btn",
		Classes:    []string{"close-btn"},
		Opacity:    0,
		HasOpacity: true,
	}
	a := NewAnalyzer(NewVarTable())
	pf := &ParsedFile{Path: rule.File, Rules: []Rule{rule}}
	findings := a.AnalyzeOpacity(pf)
	if len(findings) != 0 {
		t.Errorf("expected opacity:0 to be ignored, got %+v", findings)
	}
}

// TestAnalyzeOpacity_DedupesRepeatedSelector covers a selector re-declared
// under a second at-rule (for example a `prefers-reduced-motion` override of
// the same class): one physical component state, one finding.
func TestAnalyzeOpacity_DedupesRepeatedSelector(t *testing.T) {
	rules := []Rule{
		{File: "NewFolderDialog.svelte", Line: 295, Selector: ".suggestion-pending", Classes: []string{"suggestion-item", "suggestion-pending"}, Opacity: 0.5, HasOpacity: true},
		{File: "NewFolderDialog.svelte", Line: 309, Selector: ".suggestion-pending", Classes: []string{"suggestion-pending"}, Opacity: 0.4, HasOpacity: true, ModeOnly: ""},
	}
	a := NewAnalyzer(NewVarTable())
	pf := &ParsedFile{Path: "NewFolderDialog.svelte", Rules: rules}
	findings := a.AnalyzeOpacity(pf)
	if len(findings) != 1 {
		t.Fatalf("expected 1 deduped finding, got %d: %+v", len(findings), findings)
	}
}

// TestParseOpacity covers the literal-only parsing contract: numeric values
// resolve, `var()`/`calc()`/keywords are left unresolved rather than guessed.
func TestParseOpacity(t *testing.T) {
	cases := []struct {
		in      string
		want    float64
		wantOk  bool
		comment string
	}{
		{"0.5", 0.5, true, "plain decimal"},
		{"1", 1, true, "integer"},
		{"0", 0, true, "zero"},
		{".5", 0.5, true, "leading-dot decimal"},
		{"1.5", 0, false, "out of range"},
		{"var(--opacity-muted)", 0, false, "var() is non-literal"},
		{"calc(1 - 0.5)", 0, false, "calc() is non-literal"},
		{"", 0, false, "empty"},
	}
	for _, c := range cases {
		got, ok := parseOpacity(c.in)
		if ok != c.wantOk || (ok && got != c.want) {
			t.Errorf("%s: parseOpacity(%q) = (%v, %v), want (%v, %v)", c.comment, c.in, got, ok, c.want, c.wantOk)
		}
	}
}
