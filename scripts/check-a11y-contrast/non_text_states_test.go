package main

import "testing"

func nonTextFixture(fieldBorder string) *VarTable {
	vt := NewVarTable()
	vt.Light["color-bg-primary"] = "#ffffff"
	vt.Light["color-bg-secondary"] = "#f5f5f5"
	vt.Light["color-bg-dialog"] = "#ebebeb"
	vt.Light["color-field-border"] = fieldBorder
	vt.Dark["color-bg-primary"] = "#1e1e1e"
	vt.Dark["color-bg-secondary"] = "#2a2a2a"
	vt.Dark["color-bg-dialog"] = "#232323"
	vt.Dark["color-field-border"] = "#808080"
	vt.Light["color-accent"] = "#d4a006"
	vt.Light["color-accent-fg"] = "#1a1a1a"
	vt.Light["color-accent-pop"] = "color-mix(in oklch, var(--color-accent), black 40%)"
	vt.Dark["color-accent"] = "#ffc206"
	vt.Dark["color-accent-pop"] = "color-mix(in oklch, var(--color-accent), white 40%)"
	vt.ContrastLight["color-focus-ink"] = "#1a1a1a"
	vt.ContrastDark["color-focus-ink"] = "#e8e8e8"
	vt.Light["color-tab-band"] = "var(--color-accent)"
	vt.Light["color-tab-band-unfocused"] = "color-mix(in oklch, var(--color-accent), transparent 50%)"
	vt.Differentiate["color-tab-band"] = "var(--color-accent-pop)"
	return vt
}

// TestAnalyzeNonTextStatesGatesTheTabBandUnderDifferentiateWithoutColor: the
// focused pane's tab band only promises 3:1 under Differentiate without color,
// so a band left at the raw accent there fails, while the default look's raw
// accent band is measured, not gated.
func TestAnalyzeNonTextStatesGatesTheTabBandUnderDifferentiateWithoutColor(t *testing.T) {
	vt := nonTextFixture("#858585")
	gated, measured := NewAnalyzer(vt).AnalyzeNonTextStates()
	for _, f := range failingNonText(gated) {
		t.Errorf("the accent-pop band failed: %s (%s) = %.2f:1", f.Selector, f.Mode, f.Ratio)
	}
	if len(measured) == 0 {
		t.Error("the default-look tab bands aren't measured")
	}
	vt.Differentiate["color-tab-band"] = "var(--color-accent)"
	gated, _ = NewAnalyzer(vt).AnalyzeNonTextStates()
	if len(failingNonText(gated)) == 0 {
		t.Error("a raw-accent band passed the Differentiate without color gate")
	}
}

func gatedNonText(vt *VarTable) []Finding {
	gated, _ := NewAnalyzer(vt).AnalyzeNonTextStates()
	return gated
}

func failingNonText(findings []Finding) []Finding {
	var out []Finding
	for _, f := range findings {
		if !f.IsPassing {
			out = append(out, f)
		}
	}
	return out
}

// TestAnalyzeNonTextStatesFlagsAFaintFieldBorder is the regression anchor for
// the audit's 1.4.11 finding: text fields drew their edge in the decorative
// `--color-border` (#ddd, 1.36:1 on white), and no check measured a border at
// all. The non-text scenarios hold every field edge to 3:1 on the surfaces
// fields sit on.
func TestAnalyzeNonTextStatesFlagsAFaintFieldBorder(t *testing.T) {
	failing := failingNonText(gatedNonText(nonTextFixture("#dddddd")))
	if len(failing) == 0 {
		t.Fatal("a #ddd field border passed the 3:1 non-text gate")
	}
	for _, f := range failing {
		if f.Threshold != 3.0 {
			t.Errorf("non-text finding %q uses threshold %.1f, want 3.0", f.Selector, f.Threshold)
		}
	}
}

// TestAnalyzeNonTextStatesPassesTheControlBorder is the green twin: the 3:1
// control border clears every surface in both modes.
func TestAnalyzeNonTextStatesPassesTheControlBorder(t *testing.T) {
	if failing := failingNonText(gatedNonText(nonTextFixture("#858585"))); len(failing) > 0 {
		for _, f := range failing {
			t.Errorf("%s (%s) = %.2f:1", f.Selector, f.Mode, f.Ratio)
		}
	}
}
