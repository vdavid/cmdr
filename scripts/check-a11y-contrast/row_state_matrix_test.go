package main

import (
	"math"
	"testing"
)

// findRowFinding locates the AnalyzeRowStates() finding for one
// (role, mode, tint, variant, selected) scenario, using the same describe
// string the synthesizer tags findings with.
func findRowFinding(t *testing.T, findings []Finding, role string, mode Mode, tint, variant string, selected bool) *Finding {
	t.Helper()
	want := describeRowScenario(role, paneTintHue{Name: tint}, variant, selected)
	for i := range findings {
		if findings[i].Selector == want && findings[i].Mode == mode {
			return &findings[i]
		}
	}
	return nil
}

// TestAnalyzeRowStatesCatchesQuietTextBelowAPCAFloor is the regression anchor
// for the gap this matrix closes: before the unselected-row roles were
// modeled at all, nothing evaluated `--color-text-quiet` (the token the
// hidden-entry dim and the TCC-restricted row treatment both use) against the
// bg it actually renders on. The restricted row's PRE-FIX `opacity: 0.6`
// treatment composited to roughly `#979797` in dark mode against the pane bg
// — APCA Lc ~44, under the enforced Lc-45 floor — and nothing caught it,
// because opacity isn't folded into the walker and unselected rows weren't
// synthesized either. This test pins a value in that same failing range and
// asserts the matrix now flags it, so a future regression back toward
// opacity-shaped dimming fails loudly instead of silently.
func TestAnalyzeRowStatesCatchesQuietTextBelowAPCAFloor(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-text-quiet"] = "#4d4d4d"
	vars.Dark["color-bg-primary"] = "#1e1e1e"
	// The pre-fix opacity-composited gray, not the real `--color-text-quiet`
	// value (`#aaaaaa`) — this is deliberately the bad case.
	vars.Dark["color-text-quiet"] = "#979797"

	a := NewAnalyzer(vars)
	findings := a.AnalyzeRowStates()

	f := findRowFinding(t, findings, "color-text-quiet", ModeDark, "none", "plain", false)
	if f == nil {
		t.Fatalf("no unselected-row finding for color-text-quiet in dark mode (tint=none, variant=plain); scenario isn't being evaluated")
	}
	lc := math.Abs(APCALc(f.FG, f.BG))
	if lc >= apcaFloor {
		t.Errorf("expected the pre-fix opacity-composited gray to fail the Lc-45 floor, got Lc=%.1f (fg=%s bg=%s)", lc, f.FG.Hex(), f.BG.Hex())
	}
}

// TestAnalyzeRowStatesQuietTextClearsAPCAFloor is the green twin: the real
// `--color-text-quiet` token (calibrated in `app.css`) clears the floor
// against the plain pane bg in both modes, once resolved through the
// unselected-row bg composition rather than left unevaluated.
func TestAnalyzeRowStatesQuietTextClearsAPCAFloor(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-text-quiet"] = "#4d4d4d"
	vars.Dark["color-bg-primary"] = "#1e1e1e"
	vars.Dark["color-text-quiet"] = "#aaaaaa"

	a := NewAnalyzer(vars)
	findings := a.AnalyzeRowStates()

	for _, mode := range []Mode{ModeLight, ModeDark} {
		f := findRowFinding(t, findings, "color-text-quiet", mode, "none", "plain", false)
		if f == nil {
			t.Fatalf("no unselected-row finding for color-text-quiet in %s mode (tint=none, variant=plain)", mode)
		}
		lc := math.Abs(APCALc(f.FG, f.BG))
		if lc < apcaFloor {
			t.Errorf("%s: expected the real color-text-quiet value to clear the Lc-45 floor, got Lc=%.1f (fg=%s bg=%s)", mode, lc, f.FG.Hex(), f.BG.Hex())
		}
		if !f.IsPassing {
			t.Errorf("%s: expected the real color-text-quiet value to pass WCAG too, got ratio=%.2f", mode, f.Ratio)
		}
	}
}

// TestResolveRowBgUnselectedIgnoresSelectionBg verifies the `selected` arg
// actually gates the `--color-selection-bg` substitution: an unselected row
// renders on the plain pane bg even when a (non-transparent) selection bg is
// defined, since `--color-selection-bg` only ever applies to `.is-selected`
// rows.
func TestResolveRowBgUnselectedIgnoresSelectionBg(t *testing.T) {
	vars := NewVarTable()
	vars.Dark["color-bg-primary"] = "#1e1e1e"
	vars.Dark["color-selection-bg"] = "#3a1414" // opaque, dark-mode-only selection fill

	tint := paneTintHue{Name: "none"}

	selectedBg, ok := resolveRowBg(vars, ModeDark, tint, "plain", true)
	if !ok {
		t.Fatalf("resolveRowBg(selected=true) failed to resolve")
	}
	if got := selectedBg.Hex(); got != "#3a1414" {
		t.Errorf("selected plain row bg = %s, want the selection-bg #3a1414", got)
	}

	unselectedBg, ok := resolveRowBg(vars, ModeDark, tint, "plain", false)
	if !ok {
		t.Fatalf("resolveRowBg(selected=false) failed to resolve")
	}
	if got := unselectedBg.Hex(); got != "#1e1e1e" {
		t.Errorf("unselected plain row bg = %s, want the plain pane bg #1e1e1e (selection-bg must not leak into unselected rows)", got)
	}
}

// TestDescribeRowScenarioReflectsSelectedState guards the report-readability
// contract: a selected-row finding's selector names `.is-selected`, an
// unselected-row finding's doesn't (see the hardcoded string this replaced,
// which claimed every scenario was `.is-selected` regardless of the group).
func TestDescribeRowScenarioReflectsSelectedState(t *testing.T) {
	tint := paneTintHue{Name: "none"}
	selected := describeRowScenario("color-selection-fg", tint, "plain", true)
	unselected := describeRowScenario("color-text-quiet", tint, "plain", false)

	if want := ".file-entry.is-selected .color-selection-fg (tint=none, plain)"; selected != want {
		t.Errorf("selected scenario = %q, want %q", selected, want)
	}
	if want := ".file-entry .color-text-quiet (tint=none, plain)"; unselected != want {
		t.Errorf("unselected scenario = %q, want %q", unselected, want)
	}
}
