package main

import (
	"fmt"
	"math"
	"sort"
)

// APCA (Accessible Perceptual Contrast Algorithm) — advisory, report-only.
//
// This is a SECOND opinion alongside the WCAG 2.2 gate, not a replacement.
// WCAG 2 stays the pass/fail contract (it's the ratified, legally-recognized
// standard); APCA is the better perceptual predictor of real readability
// (polarity-aware, font-size/weight-aware). We print its verdict so we can see
// where the two disagree, without affecting the exit code.
//
// Status note (verified 2026-06-30): APCA was REMOVED from the WCAG 3 working
// draft (July 2023); the April 2026 WCAG 3 editor's draft says the contrast
// algorithm is "yet to be determined". APCA now develops independently (ARC,
// Inclusive Reading Technologies). The CORE MATH below has been stable since
// 2022 (apca-w3 0.1.9), but its thresholds/role may still change — which is
// exactly why this is advisory only.
//
// Math: the canonical apca-w3 0.1.9 "W3" constants (revision 2022-07-03),
// https://github.com/Myndex/apca-w3. Output is Lc ("lightness contrast"),
// signed: positive for dark-text-on-light, negative for light-text-on-dark,
// |Lc| ranging ~0..108.

const (
	apcaMainTRC = 2.4 // per-channel exponent (NOT WCAG's piecewise sRGB curve)
	apcaSRco    = 0.2126729
	apcaSGco    = 0.7151522
	apcaSBco    = 0.0721750

	apcaNormBG  = 0.56 // normal-polarity background exponent
	apcaNormTXT = 0.57 // normal-polarity text exponent
	apcaRevTXT  = 0.62 // reverse-polarity (dark mode) text exponent
	apcaRevBG   = 0.65 // reverse-polarity background exponent

	apcaBlkThrs = 0.022 // soft black clamp threshold
	apcaBlkClmp = 1.414 // soft black clamp exponent
	apcaScale   = 1.14  // BoW == WoB scale
	apcaLoOff   = 0.027 // low-contrast offset
	apcaLoClip  = 0.1   // contrasts below this collapse to 0
	apcaDeltaY  = 0.0005
)

// apcaY converts an opaque sRGB color to APCA screen luminance.
func apcaY(c RGBA) float64 {
	return apcaSRco*math.Pow(c.R/255, apcaMainTRC) +
		apcaSGco*math.Pow(c.G/255, apcaMainTRC) +
		apcaSBco*math.Pow(c.B/255, apcaMainTRC)
}

// APCALc returns the signed APCA Lc for text over an opaque background.
func APCALc(txt, bg RGBA) float64 {
	yTxt := apcaY(txt)
	yBg := apcaY(bg)

	// Soft-clamp very dark colors (the fix for WCAG over-crediting near-blacks).
	if yTxt <= apcaBlkThrs {
		yTxt += math.Pow(apcaBlkThrs-yTxt, apcaBlkClmp)
	}
	if yBg <= apcaBlkThrs {
		yBg += math.Pow(apcaBlkThrs-yBg, apcaBlkClmp)
	}

	if math.Abs(yBg-yTxt) < apcaDeltaY {
		return 0
	}

	var out float64
	if yBg > yTxt {
		// Normal polarity: dark text on a lighter background.
		s := (math.Pow(yBg, apcaNormBG) - math.Pow(yTxt, apcaNormTXT)) * apcaScale
		if s < apcaLoClip {
			return 0
		}
		out = s - apcaLoOff
	} else {
		// Reverse polarity: light text on a darker background (dark mode).
		s := (math.Pow(yBg, apcaRevBG) - math.Pow(yTxt, apcaRevTXT)) * apcaScale
		if s > -apcaLoClip {
			return 0
		}
		out = s + apcaLoOff
	}
	return out * 100
}

// apcaTargetLc returns the minimum |Lc| APCA asks for at a given font size and
// weight — the "minimum readability" reading of the published ladder
// (https://git.apcacontrast.com/documentation/APCA_in_a_Nutshell.html). It's a
// conservative interpretation: 14px/400 body lands in the Lc 90 band (the
// Nutshell lists Lc 75 minimums only from 18px/400 and 14px/700), so ordinary
// small body text is held to the strict bar. Returns 0 only for sizes we don't
// classify (treated as "no APCA opinion"). Font px assumes ~0.52 x-height.
func apcaTargetLc(px float64, weight int) float64 {
	if weight <= 0 {
		weight = 400
	}
	if px <= 0 {
		px = 14 // unresolved size: assume small body text
	}
	switch {
	case (px >= 36 && weight <= 400) || (px >= 24 && weight >= 700):
		return 45 // large / heavy headlines
	case (px >= 24 && weight >= 400) || (px >= 21 && weight >= 500) ||
		(px >= 18 && weight >= 600) || (px >= 16 && weight >= 700):
		return 60 // content text
	case (px >= 18 && weight >= 400) || (px >= 16 && weight >= 500) ||
		(px >= 14 && weight >= 700):
		return 75 // body minimum
	default:
		return 90 // small/normal body (e.g. 14px/400) — strict band
	}
}

// apcaShortfall is one pair whose |Lc| is below its font-aware target.
type apcaShortfall struct {
	f      Finding
	lc     float64
	target float64
}

// ReportAPCA prints the advisory APCA section over every evaluated pair. It
// NEVER affects the exit code — it's a second opinion, printed so we can see
// where APCA and WCAG 2 disagree and size the blast radius before deciding
// whether any of it should ever become a gate.
func ReportAPCA(findings []Finding, rootDir string) {
	if len(findings) == 0 {
		return
	}

	// |Lc| distribution buckets.
	var b90, b75, b60, b45, b30, bLow int
	var shortfalls []apcaShortfall
	// Per-pair (|Lc|, px, weight) for the zoom sweep below.
	type lite struct {
		lc     float64
		px     float64
		weight int
	}
	var pairs []lite
	// Dedup identical pairs (same file:line:selector:mode) so counts mean
	// "distinct evaluated pairs", matching how the WCAG report reads.
	seen := map[string]bool{}

	for _, f := range findings {
		key := fmt.Sprintf("%s:%d:%s:%s", f.File, f.Line, f.Selector, f.Mode)
		if seen[key] {
			continue
		}
		seen[key] = true

		lc := math.Abs(APCALc(f.FG, f.BG))
		pairs = append(pairs, lite{lc: lc, px: f.FontPx, weight: f.FontWeight})
		switch {
		case lc >= 90:
			b90++
		case lc >= 75:
			b75++
		case lc >= 60:
			b60++
		case lc >= 45:
			b45++
		case lc >= 30:
			b30++
		default:
			bLow++
		}
		target := apcaTargetLc(f.FontPx, f.FontWeight)
		if lc < target {
			shortfalls = append(shortfalls, apcaShortfall{f: f, lc: lc, target: target})
		}
	}
	total := len(seen)

	fmt.Printf("\n%s=== APCA (advisory, report-only — does NOT affect pass/fail) ===%s\n", colorYellow, colorReset)
	fmt.Printf("%sAPCA 0.1.9 (W3) second opinion alongside the WCAG 2.2 gate. %d distinct pairs.%s\n", colorDim, total, colorReset)
	fmt.Printf("%s|Lc| distribution:%s\n", colorDim, colorReset)
	fmt.Printf("    %s≥90  body preferred : %d%s\n", colorDim, b90, colorReset)
	fmt.Printf("    %s75–90 body minimum  : %d%s\n", colorDim, b75, colorReset)
	fmt.Printf("    %s60–75 large/content : %d%s\n", colorDim, b60, colorReset)
	fmt.Printf("    %s45–60 headline only : %d%s\n", colorDim, b45, colorReset)
	fmt.Printf("    %s30–45 weak          : %d%s\n", colorDim, b30, colorReset)
	fmt.Printf("    %s<30   sub-minimum   : %d%s\n", colorRed, bLow, colorReset)

	// Cumulative "below a fixed bar" — the target-independent blast radius at
	// each strictness, far more honest than the single font-aware number
	// (which our 14px/400 base body text dominates by demanding Lc 90).
	pct := func(n int) float64 { return 100 * float64(n) / float64(total) }
	fmt.Printf("%sPairs below a fixed Lc bar (blast radius if that bar were the gate):%s\n", colorDim, colorReset)
	fmt.Printf("    %s< Lc 90 (body preferred): %d (%.0f%%)%s\n", colorDim, b75+b60+b45+b30+bLow, pct(b75+b60+b45+b30+bLow), colorReset)
	fmt.Printf("    %s< Lc 75 (body minimum)  : %d (%.0f%%)%s\n", colorDim, b60+b45+b30+bLow, pct(b60+b45+b30+bLow), colorReset)
	fmt.Printf("    %s< Lc 60 (content/UI)    : %d (%.0f%%)%s\n", colorDim, b45+b30+bLow, pct(b45+b30+bLow), colorReset)
	fmt.Printf("    %s< Lc 45 (headline only) : %d (%.0f%%)%s\n", colorDim, b30+bLow, pct(b30+bLow), colorReset)
	fmt.Printf("    %s< Lc 30 (any text fails): %d (%.0f%%)%s\n", colorDim, bLow, pct(bLow), colorReset)

	// Zoom sweep. Zoom doesn't change Lc (contrast); it enlarges the rendered
	// text, which lowers the size-based APCA target each element must clear. So
	// bumping the app's default zoom is a legitimate way to lift our APCA
	// standing without recoloring anything. Count font-aware shortfalls at a
	// few default-zoom levels (each pair's px scaled, target re-looked-up).
	shortfallAtZoom := func(zoom float64) int {
		n := 0
		for _, p := range pairs {
			if p.lc < apcaTargetLc(p.px*zoom, p.weight) {
				n++
			}
		}
		return n
	}
	fmt.Printf("%sFont-aware shortfalls vs default zoom (Lc unchanged; only the size-based target relaxes):%s\n", colorDim, colorReset)
	for _, z := range []float64{1.0, 1.10, 1.25, 1.50} {
		n := shortfallAtZoom(z)
		fmt.Printf("    %s%3.0f%%: %d (%.0f%%)%s\n", colorDim, z*100, n, pct(n), colorReset)
	}
	// Why zoom helps so little: split the 100%-zoom shortfalls into the ones a
	// bigger render could ever rescue (|Lc| ≥ 60, so a large-text target would
	// pass) vs the contrast-bound ones (|Lc| < 60, below even the most lenient
	// content target — no amount of zoom fixes them; only recoloring does).
	var sizeBound, contrastBound int
	for _, p := range pairs {
		if p.lc < apcaTargetLc(p.px, p.weight) {
			if p.lc >= 60 {
				sizeBound++
			} else {
				contrastBound++
			}
		}
	}
	fmt.Printf("    %sof the 100%% shortfalls: %d are size-fixable (|Lc|≥60), %d are contrast-bound (|Lc|<60 — zoom can't fix, only recolor)%s\n",
		colorDim, sizeBound, contrastBound, colorReset)

	// Sort shortfalls by how far below target (largest gap first).
	sort.Slice(shortfalls, func(i, j int) bool {
		return (shortfalls[i].target - shortfalls[i].lc) > (shortfalls[j].target - shortfalls[j].lc)
	})

	fmt.Printf("%sFont-aware shortfalls (|Lc| below the APCA target for the element's size/weight): %s%d of %d%s\n",
		colorDim, colorRed, len(shortfalls), total, colorReset)

	const maxShown = 30
	shown := shortfalls
	if len(shown) > maxShown {
		shown = shown[:maxShown]
	}
	for _, s := range shown {
		px := s.f.FontPx
		if px <= 0 {
			px = 14
		}
		variant := ""
		if s.f.AccentVariant != "" {
			variant = fmt.Sprintf(" accent=%s", s.f.AccentVariant)
		}
		fmt.Printf("    %s%s:%d%s  %s%s%s  %s  %.0fpx/%d%s  Lc %.0f  need %.0f  (short %.0f)\n",
			colorRed, RelPath(rootDir, s.f.File), s.f.Line, colorReset,
			colorDim, s.f.Selector, colorReset,
			s.f.Mode, px, weightOr400(s.f.FontWeight), variant,
			s.lc, s.target, s.target-s.lc)
	}
	if len(shortfalls) > maxShown {
		fmt.Printf("    %s… and %d more%s\n", colorDim, len(shortfalls)-maxShown, colorReset)
	}
	fmt.Printf("%sNote: target ladder is APCA's MINIMUM reading; 14px/400 body sits in the Lc 90 band.%s\n", colorDim, colorReset)
	fmt.Printf("%sNote: accent pairs use the worst-WCAG accent variant, so their Lc is indicative, not an APCA sweep.%s\n", colorDim, colorReset)
}

func weightOr400(w int) int {
	if w <= 0 {
		return 400
	}
	return w
}
