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
	vt.Light["color-text-primary"] = "#1a1a1a"
	vt.Dark["color-text-primary"] = "#e8e8e8"
	vt.Light["color-accent-selection"] = "#b08505"
	vt.Dark["color-accent-selection"] = "#836403"
	return vt
}

// TestTextSelectionFillHexMatchesTheAppsValues pins the Go mirror to what
// `srgb-mix.ts:textSelectionFill` writes at runtime (its own tests hold the JS
// side to the ratios), so the gate measures the fill the app actually draws.
func TestTextSelectionFillHexMatchesTheAppsValues(t *testing.T) {
	light := struct{ field, text RGBA }{mustColor(t, "#ffffff"), mustColor(t, "#1a1a1a")}
	dark := struct{ field, text RGBA }{mustColor(t, "#1e1e1e"), mustColor(t, "#e8e8e8")}
	for _, tc := range []struct{ accent, wantLight, wantDark string }{
		{"#d4a006", "#b08505", ""},
		{"#ffc206", "", "#836403"},
		{"#087aff", "#278bff", "#0765d2"},
		{"#a54fa7", "#b874b9", "#99499c"},
		{"#ffc601", "#ad8601", "#816501"},
		{"#8b8c8c", "#8b8b8b", "#676968"},
	} {
		if tc.wantLight != "" {
			if got := textSelectionFillHex(tc.accent, light.field, light.text); got != tc.wantLight {
				t.Errorf("light %s: got %s, want %s", tc.accent, got, tc.wantLight)
			}
		}
		if tc.wantDark != "" {
			if got := textSelectionFillHex(tc.accent, dark.field, dark.text); got != tc.wantDark {
				t.Errorf("dark %s: got %s, want %s", tc.accent, got, tc.wantDark)
			}
		}
	}
}

// TestAnalyzeNonTextStatesFlagsTheTranslucentSelection is the regression anchor
// for the near-invisible text selection: the accent at 30% over the field was
// 1.16:1 on white for Apple Yellow.
func TestAnalyzeNonTextStatesFlagsTheTranslucentSelection(t *testing.T) {
	vt := nonTextFixture("#858585")
	vt.Light["color-accent-selection"] = "color-mix(in oklch, var(--color-accent), transparent 70%)"
	for _, f := range failingNonText(gatedNonText(vt)) {
		if f.Mode == ModeLight && f.Selector == "text selection fill in a field on color-bg-primary" {
			return
		}
	}
	t.Error("a 30% accent selection fill passed the 3:1 gate on a white field")
}

func mustColor(t *testing.T, hex string) RGBA {
	t.Helper()
	c, ok := ParseColor(hex)
	if !ok {
		t.Fatalf("bad color %q", hex)
	}
	return c
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
		if f.Threshold != 3.0 && f.Selector != "selected text in a field on color-accent-selection" {
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
