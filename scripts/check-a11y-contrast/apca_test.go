package main

import (
	"math"
	"testing"
)

func hexRGBA(t *testing.T, hex string) RGBA {
	t.Helper()
	c, ok := ParseColor(hex)
	if !ok {
		t.Fatalf("ParseColor(%q) failed", hex)
	}
	return c
}

// TestAPCAReferenceValues pins the implementation to APCA's well-known canonical
// outputs (apca-w3 0.1.9): black-on-white ≈ Lc 106, white-on-black ≈ Lc -108.
// If these drift, the port broke.
func TestAPCAReferenceValues(t *testing.T) {
	white := RGBA{R: 255, G: 255, B: 255, A: 1}
	black := RGBA{R: 0, G: 0, B: 0, A: 1}

	if got := APCALc(black, white); math.Abs(got-106.04) > 0.5 {
		t.Errorf("black on white: Lc = %.2f, want ≈106.04", got)
	}
	if got := APCALc(white, black); math.Abs(got-(-107.88)) > 0.5 {
		t.Errorf("white on black: Lc = %.2f, want ≈-107.88", got)
	}
}

// TestAPCAPolarityIsAsymmetric is the headline difference from WCAG 2 (which is
// symmetric): swapping fg/bg flips the sign AND changes the magnitude.
func TestAPCAPolarityIsAsymmetric(t *testing.T) {
	gray := hexRGBA(t, "#767676")
	white := RGBA{R: 255, G: 255, B: 255, A: 1}

	darkOnLight := APCALc(gray, white) // positive
	lightOnDark := APCALc(white, gray) // negative
	if darkOnLight <= 0 {
		t.Errorf("dark-on-light Lc should be positive, got %.2f", darkOnLight)
	}
	if lightOnDark >= 0 {
		t.Errorf("light-on-dark Lc should be negative, got %.2f", lightOnDark)
	}
	if math.Abs(math.Abs(darkOnLight)-math.Abs(lightOnDark)) < 1 {
		t.Errorf("expected polarity asymmetry, |%.1f| vs |%.1f| too close", darkOnLight, lightOnDark)
	}
}

// TestAPCATargetLadder sanity-checks the font-aware target tiers.
func TestAPCATargetLadder(t *testing.T) {
	cases := []struct {
		px     float64
		weight int
		want   float64
	}{
		{14, 400, 90}, // small body → strict band
		{18, 400, 75}, // body minimum
		{24, 400, 60}, // content
		{36, 400, 45}, // headline
		{24, 700, 45}, // large + heavy
	}
	for _, c := range cases {
		if got := apcaTargetLc(c.px, c.weight); got != c.want {
			t.Errorf("apcaTargetLc(%v, %d) = %v, want %v", c.px, c.weight, got, c.want)
		}
	}
}
