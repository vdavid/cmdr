package main

import (
	"fmt"
	"math"
	"sort"
)

// APCA (Accessible Perceptual Contrast Algorithm) — a perceptual second opinion
// alongside the WCAG 2.2 gate, plus an enforced Lc-45 floor.
//
// WCAG 2 stays the primary pass/fail contract (the ratified, legally-recognized
// standard). APCA is the better perceptual predictor of real readability
// (polarity-aware, font-size/weight-aware), so on top of WCAG we ENFORCE one
// conservative APCA bar: no text pair below Lc 45 ("the absolute minimum for any
// text"). Everything else APCA reports (the |Lc| distribution, blast radius,
// zoom sweep, per-pair shortfalls vs the size-aware target) stays advisory and
// prints only with -verbose.
//
// Status note (verified 2026-06-30): APCA was REMOVED from the WCAG 3 working
// draft (July 2023); the April 2026 WCAG 3 editor's draft says the contrast
// algorithm is "yet to be determined". APCA now develops independently (ARC,
// Inclusive Reading Technologies). The CORE MATH below has been stable since
// 2022 (apca-w3 0.1.9), which is why we're comfortable enforcing a single
// conservative floor while keeping the rest advisory.
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

// apcaFloor is the ENFORCED minimum |Lc| for any text pair — APCA's "absolute
// minimum for any text". Below this fails the check (alongside WCAG). The 45–60
// band (intentionally de-emphasized text: placeholders, hints, disabled) stays
// advisory; we don't force every label to content-level contrast.
const apcaFloor = 45.0

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

// apcaSizeTier is one "px ≥ pxMin and weight in [wMin,wMax] → required Lc" rule.
// First match in apcaTiers (ordered most-lenient first) wins; no match → Lc 90.
type apcaSizeTier struct {
	pxMin      float64
	wMin, wMax int
	lc         float64
}

// apcaTiers encodes APCA's published font-size/weight → minimum-Lc ladder
// (https://git.apcacontrast.com/documentation/APCA_in_a_Nutshell.html), the
// conservative "minimum readability" reading. 14px/400 body matches nothing and
// falls through to Lc 90 (the Nutshell lists Lc 75 minimums only from 18px/400
// and 14px/700), so ordinary small body text is held to the strict bar. Font px
// assumes ~0.52 x-height.
var apcaTiers = []apcaSizeTier{
	{36, 0, 400, 45}, {24, 700, 1000, 45}, // large / heavy headlines
	{24, 400, 1000, 60}, {21, 500, 1000, 60}, {18, 600, 1000, 60}, {16, 700, 1000, 60}, // content text
	{18, 400, 1000, 75}, {16, 500, 1000, 75}, {14, 700, 1000, 75}, // body minimum
}

// apcaTargetLc returns the minimum |Lc| APCA asks for at a font size and weight.
// This drives the ADVISORY shortfall counts only; the enforced gate is the flat
// apcaFloor.
func apcaTargetLc(px float64, weight int) float64 {
	if weight <= 0 {
		weight = 400
	}
	if px <= 0 {
		px = 14 // unresolved size: assume small body text
	}
	for _, t := range apcaTiers {
		if px >= t.pxMin && weight >= t.wMin && weight <= t.wMax {
			return t.lc
		}
	}
	return 90 // small/normal body (e.g. 14px/400) — strict band
}

// apcaLite is the (|Lc|, px, weight) of one evaluated pair, for the zoom sweep.
type apcaLite struct {
	lc     float64
	px     float64
	weight int
}

// apcaShortfall is one pair whose |Lc| is below a target (the size-aware target,
// or the flat floor).
type apcaShortfall struct {
	f      Finding
	lc     float64
	target float64
}

// ReportAPCA prints the APCA second opinion over every evaluated pair and
// enforces the Lc-45 floor. The advisory detail (distribution, blast radius,
// zoom sweep, per-pair shortfalls) prints only with verbose; the floor result
// always prints. Returns true if any pair is below the floor (a failure).
func ReportAPCA(findings []Finding, rootDir string, verbose bool) bool {
	if len(findings) == 0 {
		return false
	}

	// |Lc| distribution buckets.
	var b90, b75, b60, b45, b30, bLow int
	var shortfalls []apcaShortfall
	var floorViols []apcaShortfall
	var pairs []apcaLite // per-pair (|Lc|, px, weight) for the zoom sweep
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
		pairs = append(pairs, apcaLite{lc: lc, px: f.FontPx, weight: f.FontWeight})
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
		if target := apcaTargetLc(f.FontPx, f.FontWeight); lc < target {
			shortfalls = append(shortfalls, apcaShortfall{f: f, lc: lc, target: target})
		}
		if lc < apcaFloor {
			floorViols = append(floorViols, apcaShortfall{f: f, lc: lc, target: apcaFloor})
		}
	}
	total := len(seen)

	if verbose {
		printAPCAAdvisory(total, apcaBuckets{b90, b75, b60, b45, b30, bLow}, pairs, shortfalls, rootDir)
	} else {
		fmt.Printf("%sAPCA (advisory): %d pairs, %d in the Lc 45–60 muted band (run with -verbose for detail)%s\n",
			colorDim, total, b45, colorReset)
	}
	return printAPCAFloor(floorViols, rootDir)
}

// apcaBuckets holds the |Lc| distribution counts (≥90, 75–90, 60–75, 45–60, 30–45, <30).
type apcaBuckets struct{ b90, b75, b60, b45, b30, bLow int }

// printAPCAAdvisory prints the (verbose-only) perceptual detail: distribution,
// fixed-bar blast radius, the zoom sweep, and every per-pair shortfall.
func printAPCAAdvisory(total int, b apcaBuckets, pairs []apcaLite, shortfalls []apcaShortfall, rootDir string) {
	pct := func(n int) float64 { return 100 * float64(n) / float64(total) }
	below60 := b.b45 + b.b30 + b.bLow
	fmt.Printf("\n%s=== APCA (advisory) — perceptual second opinion ===%s\n", colorYellow, colorReset)
	fmt.Printf("%sAPCA 0.1.9 (W3). %d distinct pairs.%s\n", colorDim, total, colorReset)
	fmt.Printf("%s|Lc| distribution: ≥90:%d  75–90:%d  60–75:%d  45–60:%d  30–45:%d  <30:%d%s\n",
		colorDim, b.b90, b.b75, b.b60, b.b45, b.b30, b.bLow, colorReset)
	fmt.Printf("%sBelow a fixed bar: <60:%d (%.0f%%)  <45:%d  <30:%d%s\n",
		colorDim, below60, pct(below60), b.b30+b.bLow, b.bLow, colorReset)
	fmt.Printf("%sFont-aware shortfalls vs default zoom (Lc fixed; only the target relaxes): 100%%:%d 110%%:%d 125%%:%d 150%%:%d%s\n",
		colorDim, apcaShortfallAtZoom(pairs, 1.0), apcaShortfallAtZoom(pairs, 1.10), apcaShortfallAtZoom(pairs, 1.25), apcaShortfallAtZoom(pairs, 1.50), colorReset)
	sort.Slice(shortfalls, func(i, j int) bool {
		return (shortfalls[i].target - shortfalls[i].lc) > (shortfalls[j].target - shortfalls[j].lc)
	})
	fmt.Printf("%sFont-aware shortfalls (below the size/weight target): %d of %d%s\n", colorDim, len(shortfalls), total, colorReset)
	for _, s := range shortfalls {
		px := s.f.FontPx
		if px <= 0 {
			px = 14
		}
		fmt.Printf("    %s%s:%d  %s  %s  %.0fpx/%d%s  Lc %.0f need %.0f%s\n",
			colorDim, RelPath(rootDir, s.f.File), s.f.Line, s.f.Selector,
			s.f.Mode, px, weightOr400(s.f.FontWeight), apcaVariantTag(s.f), s.lc, s.target, colorReset)
	}
	fmt.Printf("%sNote: 14px/400 body sits in the Lc 90 band (APCA's preferred, not the floor).%s\n", colorDim, colorReset)
}

// printAPCAFloor prints the enforced Lc-45 floor result and returns true on any
// violation (a check failure, alongside the WCAG gate).
func printAPCAFloor(floorViols []apcaShortfall, rootDir string) bool {
	if len(floorViols) == 0 {
		fmt.Printf("%s✅ APCA Lc-45 floor: every text pair ≥ Lc 45%s\n", colorGreen, colorReset)
		return false
	}
	sort.Slice(floorViols, func(i, j int) bool { return floorViols[i].lc < floorViols[j].lc })
	fmt.Printf("%s❌ APCA Lc-45 floor: %d pair(s) below Lc 45%s\n", colorRed, len(floorViols), colorReset)
	for _, s := range floorViols {
		fmt.Printf("    %s%s:%d%s  %s%s%s  %s%s  Lc %.1f (need %.0f)\n",
			colorRed, RelPath(rootDir, s.f.File), s.f.Line, colorReset,
			colorDim, s.f.Selector, colorReset, s.f.Mode, apcaVariantTag(s.f), s.lc, apcaFloor)
	}
	return true
}

func apcaShortfallAtZoom(pairs []apcaLite, zoom float64) int {
	n := 0
	for _, p := range pairs {
		if p.lc < apcaTargetLc(p.px*zoom, p.weight) {
			n++
		}
	}
	return n
}

func apcaVariantTag(f Finding) string {
	if f.AccentVariant == "" {
		return ""
	}
	return fmt.Sprintf(" accent=%s", f.AccentVariant)
}

func weightOr400(w int) int {
	if w <= 0 {
		return 400
	}
	return w
}
