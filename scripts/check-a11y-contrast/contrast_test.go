package main

import (
	"math"
	"testing"
)

func TestParseColorHex(t *testing.T) {
	tests := []struct {
		in string
		r  float64
		g  float64
		b  float64
		a  float64
		ok bool
	}{
		{"#000000", 0, 0, 0, 1, true},
		{"#ffffff", 255, 255, 255, 1, true},
		{"#1a1a1a", 26, 26, 26, 1, true},
		{"#fff", 255, 255, 255, 1, true},
		{"#000", 0, 0, 0, 1, true},
		{"#f5f5f5", 245, 245, 245, 1, true},
		{"transparent", 0, 0, 0, 0, true},
		{"not-a-color", 0, 0, 0, 0, false},
	}
	for _, tc := range tests {
		c, ok := ParseColor(tc.in)
		if ok != tc.ok {
			t.Errorf("ParseColor(%q): ok=%v, want %v", tc.in, ok, tc.ok)
			continue
		}
		if !ok {
			continue
		}
		if c.R != tc.r || c.G != tc.g || c.B != tc.b || math.Abs(c.A-tc.a) > 1e-9 {
			t.Errorf("ParseColor(%q) = %+v, want {%v %v %v %v}", tc.in, c, tc.r, tc.g, tc.b, tc.a)
		}
	}
}

func TestParseRGBFunc(t *testing.T) {
	c, ok := ParseColor("rgba(255, 213, 0, 0.4)")
	if !ok {
		t.Fatalf("ParseColor rgba failed")
	}
	if c.R != 255 || c.G != 213 || c.B != 0 || math.Abs(c.A-0.4) > 1e-9 {
		t.Errorf("rgba parsed wrong: %+v", c)
	}
}

func TestContrastBlackOnWhite(t *testing.T) {
	white, _ := ParseColor("#ffffff")
	black, _ := ParseColor("#000000")
	r := ContrastRatio(white, black)
	if math.Abs(r-21) > 0.01 {
		t.Errorf("black-on-white contrast = %v, want 21", r)
	}
}

func TestContrastDarkTextOnWhite(t *testing.T) {
	// --color-text-primary (#1a1a1a) on --color-bg-primary (#ffffff) -> ~17.4:1
	// (WCAG 2.x math; some tooling reports ~16 due to rounding different
	// channels to 8 bits before applying the transfer curve).
	fg, _ := ParseColor("#1a1a1a")
	bg, _ := ParseColor("#ffffff")
	r := ContrastRatio(fg, bg)
	if r < 17 || r > 18 {
		t.Errorf("dark-text-on-white contrast = %v, want ~17.4", r)
	}
}

func TestContrastTertiaryOnSecondary(t *testing.T) {
	// #666666 on #f5f5f5 -> ~5.26:1 (well above 4.5 AA).
	fg, _ := ParseColor("#666666")
	bg, _ := ParseColor("#f5f5f5")
	r := ContrastRatio(fg, bg)
	if r < 5.0 || r > 5.6 {
		t.Errorf("tertiary-on-secondary contrast = %v, want ~5.26", r)
	}
}

func TestMixSRGB_AccentText(t *testing.T) {
	// --color-accent-text = color-mix(in srgb, #d4a006, black 65%) -> 35% A + 65% B
	// A = (212, 160, 6)  B = (0, 0, 0)
	// result = (212*0.35, 160*0.35, 6*0.35) = (74.2, 56, 2.1)
	a, _ := ParseColor("#d4a006")
	b, _ := ParseColor("#000000")
	m := MixSRGB(a, b, 0.65)
	if math.Abs(m.R-74.2) > 0.5 || math.Abs(m.G-56) > 0.5 || math.Abs(m.B-2.1) > 0.5 {
		t.Errorf("sRGB mix wrong: got %+v, want ~(74,56,2)", m)
	}
}

func TestCompositeOverTranslucentOnWhite(t *testing.T) {
	// Mixing accent gold with transparent 85% gives alpha 0.15 with rgb near accent.
	// Composite over white should produce a pale version of the accent.
	accent, _ := ParseColor("#d4a006")
	white, _ := ParseColor("#ffffff")

	// Simulate MixOKLCH of (accent, transparent 85%) producing ~(accent, alpha=0.15)
	// The sRGB result of mixing accent at 15% with transparent is approx accent*0.15
	// composited over white in rgb space:
	// result = accent * 0.15 + white * 0.85
	expectedR := 0.15*212 + 0.85*255
	expectedG := 0.15*160 + 0.85*255
	expectedB := 0.15*6 + 0.85*255
	translucent := RGBA{R: accent.R, G: accent.G, B: accent.B, A: 0.15}
	out := CompositeOver(translucent, white)
	if math.Abs(out.R-expectedR) > 1 || math.Abs(out.G-expectedG) > 1 || math.Abs(out.B-expectedB) > 1 {
		t.Errorf("composite wrong: got %+v, want approx (%v,%v,%v)", out, expectedR, expectedG, expectedB)
	}
}

func TestOKLCHRoundTripPreservesColor(t *testing.T) {
	inputs := []string{"#d4a006", "#1a1a1a", "#ffffff", "#087aff", "#4a9eff"}
	for _, hex := range inputs {
		c, _ := ParseColor(hex)
		L, C, h := srgbToOKLCH(c)
		back := oklchToSRGB(L, C, h, 1)
		if math.Abs(back.R-c.R) > 1.5 || math.Abs(back.G-c.G) > 1.5 || math.Abs(back.B-c.B) > 1.5 {
			t.Errorf("oklch round-trip for %s: in=%+v out=%+v", hex, c, back)
		}
	}
}

func TestMixOKLCH_AccentSubtle(t *testing.T) {
	// --color-accent-subtle = color-mix(in oklch, #d4a006, transparent 85%)
	// bWeight = 0.85. The color-side is accent, the other is transparent.
	// Alpha of result ~= 1*0.15 + 0*0.85 = 0.15.
	accent, _ := ParseColor("#d4a006")
	transparent, _ := ParseColor("transparent")
	m := MixOKLCH(accent, transparent, 0.85)
	if math.Abs(m.A-0.15) > 0.01 {
		t.Errorf("accent-subtle alpha = %v, want ~0.15", m.A)
	}
	// With transparent premultiplied-out, the mix result equals the accent
	// color at alpha 0.15. Composited on white:
	//   result = accent * 0.15 + white * 0.85
	//   ~= (212*0.15+255*0.85, 160*0.15+255*0.85, 6*0.15+255*0.85)
	//   ~= (248.6, 240.8, 217.7)  — pale warm yellow, luminance ~0.87.
	white, _ := ParseColor("#ffffff")
	out := CompositeOver(m, white)
	lum := RelativeLuminance(out)
	if lum < 0.80 {
		t.Errorf("accent-subtle composited luminance = %v, want >= 0.80", lum)
	}
	// Red should be greater than blue (warm).
	if out.R < out.B {
		t.Errorf("expected warm tint, got R=%v B=%v", out.R, out.B)
	}
}
