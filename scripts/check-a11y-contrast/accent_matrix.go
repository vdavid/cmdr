package main

import (
	"maps"
	"math"
)

// Accent matrix: the runtime value of `--color-accent` is overridden by
// `apps/desktop/src/lib/accent-color.ts:applyAccentForCurrentSetting` to one
// of the user's macOS system accent colors (or the Cmdr brand gold). The
// static `app.css` value (`#d4a006`) we parse for the rest of the run is the
// CSS fallback only; on the running app every value derived from
// `--color-accent` (the primary button bg, hover, subtle, text, the size /
// age tier mixes, etc.) shifts as the user's system accent changes.
//
// To catch low-contrast pairs that the static fallback hides, this matrix
// re-evaluates accent-sensitive rules under each runtime variant and reports
// the worst case.
//
// Variants must mirror:
//   - `apps/desktop/src/app.css` --color-apple-*    (the 8 macOS systems)
//   - `apps/desktop/src/lib/accent-color.ts:34-35`  (CMDR_GOLD_LIGHT/DARK)
//
// macOS doesn't ship separate light/dark hexes for system accents, so the
// 8 system variants reuse the same hex in both modes. The Cmdr gold variant
// uses the per-mode constants from accent-color.ts (the only variant that
// shifts between modes today).

// AccentVariant is one accent color the runtime might pick.
type AccentVariant struct {
	Name      string // short label for the report (e.g. "blue")
	Light     string // hex used when light mode is active
	Dark      string // hex used when dark mode is active
	IsDefault bool   // the static app.css fallback (don't re-evaluate; baseline)
}

// AccentVariants enumerates every accent color the runtime can land on. Keep
// in sync with the sources called out above.
//
// "default" is the static app.css value the rest of the check already
// evaluates; it's listed here so callers can iterate uniformly and skip the
// duplicate baseline pass via IsDefault.
var AccentVariants = []AccentVariant{
	{Name: "default", Light: "#d4a006", Dark: "#ffc206", IsDefault: true},
	{Name: "blue", Light: "#087aff", Dark: "#087aff"},
	{Name: "purple", Light: "#a54fa7", Dark: "#a54fa7"},
	{Name: "pink", Light: "#f74f9f", Dark: "#f74f9f"},
	{Name: "red", Light: "#ff5157", Dark: "#ff5157"},
	{Name: "orange", Light: "#f6821b", Dark: "#f6821b"},
	{Name: "yellow", Light: "#ffc601", Dark: "#ffc601"},
	{Name: "green", Light: "#61bb46", Dark: "#61bb46"},
	{Name: "graphite", Light: "#8b8c8c", Dark: "#8b8c8c"},
}

// accentSeedVars are the leaf tokens accent-color.ts rewrites on `:root` at
// runtime. Overriding these by hand and letting the existing `color-mix(...)`
// resolution in app.css propagate gives us the derived tokens
// (`--color-accent-hover`, `-subtle`, `-text`) without re-implementing the
// formulas. `accent-color.ts` also writes those derived tokens directly with
// slightly different (sRGB-mix) formulas, but the WCAG ratios produced by the
// CSS oklch mix and the JS sRGB mix are close enough that the check's verdict
// doesn't change.
var accentSeedVars = []string{
	"color-accent",
	"color-system-accent",
	"color-cmdr-gold",
}

// accentSensitiveDeps lists tokens whose value changes when `--color-accent`
// changes. A rule is accent-sensitive if its fg or bg resolution walked
// through any of these (transitively, via the resolver's dep tracker).
//
// `color-accent-fg` is intentionally excluded: it's `#1a1a1a` in app.css with
// no dependency on `--color-accent`. A rule using `--color-accent-fg` against
// `--color-accent` IS accent-sensitive (via the latter); a rule using
// `--color-accent-fg` against a non-accent bg shouldn't be flagged just
// because of the token's name.
var accentSensitiveDeps = func() map[string]bool {
	out := map[string]bool{}
	for _, v := range accentSeedVars {
		out[v] = true
	}
	// Derived tokens defined in app.css that mix `--color-accent`. Their
	// resolution would already mark `color-accent` as a dep — listing them
	// here is belt-and-braces in case a token reference is added that
	// doesn't go through the mix (for example, a future direct alias).
	for _, name := range []string{
		"color-accent-hover", "color-accent-subtle", "color-accent-text", "color-accent-selection",
		"color-cursor-active", "color-cursor-inactive",
		"color-age-fresh", "color-age-recent", "color-age-aging", "color-age-old",
		"color-size-bytes", "color-size-kb", "color-size-mb", "color-size-gb", "color-size-tb",
	} {
		out[name] = true
	}
	return out
}()

// dependsOnAccent reports whether any of the resolver's visited vars was
// accent-sensitive.
func dependsOnAccent(deps map[string]bool) bool {
	for name := range deps {
		if accentSensitiveDeps[name] {
			return true
		}
	}
	return false
}

// withAccentOverride returns a VarTable derived from v with the accent seed
// tokens pinned to the variant's hexes for both modes. The returned table
// shares no maps with the original, so per-variant resolution is independent.
//
// Mirrors `accent-color.ts:applyDerivedAccentTokens`: `--color-accent-fg` is
// recomputed per variant (black or white, whichever gives higher contrast),
// because the JS picks it dynamically based on the active accent. Without
// this the matrix would always test the static app.css fallback `#1a1a1a`
// and re-flag the Apple-Blue/Purple issues that the JS pick fixes.
func withAccentOverride(v *VarTable, variant AccentVariant) *VarTable {
	out := NewVarTable()
	maps.Copy(out.Light, v.Light)
	maps.Copy(out.Dark, v.Dark)
	for _, name := range accentSeedVars {
		out.Light[name] = variant.Light
		out.Dark[name] = variant.Dark
	}
	// Mirror `accent-color.ts:applyDerivedAccentTokens` exactly, per mode.
	// We can't rely on the parsed app.css formulas (`color-mix(...)`) for
	// these tokens because the table also picks up the `@supports not (...)`
	// fallback values (the old-WebKit hex literals) which overwrite the
	// formulas at parse time. Computing the values here matches what the
	// JS actually writes onto `:root` at runtime, regardless of how the
	// parser folded the fallbacks.
	for _, mode := range []Mode{ModeLight, ModeDark} {
		accentHex := variant.Light
		if mode == ModeDark {
			accentHex = variant.Dark
		}
		bucket := out.Light
		if mode == ModeDark {
			bucket = out.Dark
		}
		fg := readableFgOn(accentHex)
		bucket["color-accent-fg"] = fg
		bucket["color-accent-subtle"] = withAlphaHex(accentHex, 0.15)
		// Hover: shift AWAY from the readable-fg color so contrast holds.
		// 15% in light, 10% in dark. See the JS analog in
		// accent-color.ts:applyDerivedAccentTokens for the rationale.
		hoverPct := 0.15
		if mode == ModeDark {
			hoverPct = 0.10
		}
		hoverTowards := "#ffffff"
		if fg == "#ffffff" {
			hoverTowards = "#000000"
		}
		bucket["color-accent-hover"] = mixSrgbHex(accentHex, hoverTowards, hoverPct)
		// Text-on-bg: light mixes 65% black; dark lightens dark accents by
		// 45% toward white so Apple Purple (the dimmest system accent)
		// clears WCAG AA and the APCA Lc-45 floor for accent-text.
		if mode == ModeDark {
			bucket["color-accent-text"] = mixSrgbHex(accentHex, "#ffffff", 0.45)
		} else {
			bucket["color-accent-text"] = mixSrgbHex(accentHex, "#000000", 0.65)
		}
		field, okField := resolveVar(out, mode, "color-bg-primary")
		text, okText := resolveVar(out, mode, "color-text-primary")
		if okField && okText {
			bucket["color-accent-selection"] = textSelectionFillHex(accentHex, field, text)
		}
	}
	return out
}

// Mirrors `srgb-mix.ts:textSelectionFill`'s targets and margin; keep the two in sync.
const (
	selectionVsField = 3.0
	selectionVsText  = 4.5
	selectionMargin  = 0.0003
)

// textSelectionFillHex mirrors `srgb-mix.ts:textSelectionFill`: the accent shaded toward black or
// white into the luminance window where it clears 3:1 on the field and 4.5:1 under the text, the
// rounded hex searched for when the window is narrower than an 8-bit step. The text wins when no
// shade clears both.
func textSelectionFillHex(accentHex string, field, text RGBA) string {
	accent, ok := ParseColor(accentHex)
	if !ok {
		return accentHex
	}
	lo, hi := selectionWindow(RelativeLuminance(field), RelativeLuminance(text))
	target := (lo + hi) / 2
	return nearestInWindowHex(shadeToLuminance(accent, target), lo, hi, target)
}

// selectionWindow is the luminance range a fill may take between the field and its text, or
// only the text's side of it when the two bounds cross.
func selectionWindow(field, text float64) (lo, hi float64) {
	lightField := field >= text
	fromField := selectionVsField*(field+0.05) - 0.05
	fromText := (text+0.05)/selectionVsText - 0.05
	if lightField {
		fromField = (field+0.05)/selectionVsField - 0.05
		fromText = selectionVsText*(text+0.05) - 0.05
	}
	reachable := fromField <= fromText
	if lightField {
		reachable = fromText <= fromField
	}
	lo, hi = min(fromField, fromText)+selectionMargin, max(fromField, fromText)-selectionMargin
	if reachable && lo <= hi {
		return lo, hi
	}
	if lightField {
		return fromText + selectionMargin, 1
	}
	return 0, fromText - selectionMargin
}

// roundedLuminance is the luminance of `c` once rounded to the 8-bit hex the app writes.
func roundedLuminance(c RGBA) float64 {
	rounded, _ := ParseColor(c.Hex())
	return RelativeLuminance(rounded)
}

// shadeToLuminance mixes `accent` toward black or white until its luminance is `target`.
func shadeToLuminance(accent RGBA, target float64) RGBA {
	end := 0.0
	towardWhite := RelativeLuminance(accent) < target
	if towardWhite {
		end = 255
	}
	at := func(t float64) RGBA {
		return RGBA{R: accent.R + (end-accent.R)*t, G: accent.G + (end-accent.G)*t, B: accent.B + (end-accent.B)*t, A: 1}
	}
	a, b := 0.0, 1.0
	for range 40 {
		mid := (a + b) / 2
		l := roundedLuminance(at(mid))
		if (towardWhite && l >= target) || (!towardWhite && l <= target) {
			b = mid
		} else {
			a = mid
		}
	}
	return at(b)
}

// nearestInWindowHex is the rounded color nearest `shaded` whose luminance is in `[lo, hi]`,
// smallest nudge first, then closest to `target`.
func nearestInWindowHex(shaded RGBA, lo, hi, target float64) string {
	if l := roundedLuminance(shaded); l >= lo && l <= hi {
		return shaded.Hex()
	}
	lum := roundedLuminance
	best, bestScore := shaded.Hex(), math.Inf(1)
	for dr := -3; dr <= 3; dr++ {
		for dg := -3; dg <= 3; dg++ {
			for db := -3; db <= 3; db++ {
				candidate := RGBA{R: math.Round(shaded.R) + float64(dr), G: math.Round(shaded.G) + float64(dg), B: math.Round(shaded.B) + float64(db), A: 1}
				l := lum(candidate)
				if l < lo || l > hi {
					continue
				}
				score := float64(dr*dr+dg*dg+db*db) + math.Abs(l-target)
				if score < bestScore {
					best, bestScore = candidate.Hex(), score
				}
			}
		}
	}
	return best
}

// mixSrgbHex mirrors `srgb-mix.ts:mixSrgb`: linear sRGB interpolation,
// returning `#rrggbb`. `t` is the share of `b` in [0,1].
func mixSrgbHex(a, b string, t float64) string {
	ac, ok1 := ParseColor(a)
	bc, ok2 := ParseColor(b)
	if !ok1 || !ok2 {
		return a
	}
	mixed := RGBA{
		R: ac.R*(1-t) + bc.R*t,
		G: ac.G*(1-t) + bc.G*t,
		B: ac.B*(1-t) + bc.B*t,
		A: 1,
	}
	return mixed.Hex()
}

// withAlphaHex mirrors `srgb-mix.ts:withAlpha`: returns `rgba(r,g,b,a)`.
func withAlphaHex(hex string, alpha float64) string {
	c, ok := ParseColor(hex)
	if !ok {
		return hex
	}
	c.A = alpha
	return rgbaString(c)
}

func rgbaString(c RGBA) string {
	return "rgba(" + ftoa(c.R) + ", " + ftoa(c.G) + ", " + ftoa(c.B) + ", " + alphaToa(c.A) + ")"
}

func ftoa(v float64) string {
	if v < 0 {
		v = 0
	}
	if v > 255 {
		v = 255
	}
	return itoa(int(v + 0.5))
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	neg := n < 0
	if neg {
		n = -n
	}
	var buf [16]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	if neg {
		i--
		buf[i] = '-'
	}
	return string(buf[i:])
}

func alphaToa(a float64) string {
	if a <= 0 {
		return "0"
	}
	if a >= 1 {
		return "1"
	}
	// Two-decimal precision is plenty for design-time use.
	hundredths := int(a*100 + 0.5)
	return "0." + leftPadDigit(hundredths)
}

func leftPadDigit(n int) string {
	if n < 10 {
		return "0" + itoa(n)
	}
	return itoa(n)
}

// readableFgOn returns `#000000` or `#ffffff`, whichever gives the higher
// WCAG contrast on top of the given accent hex. Mirrors the same-named
// helper in `apps/desktop/src/lib/utils/srgb-mix.ts`; keep the two in sync.
func readableFgOn(accentHex string) string {
	bg, ok := ParseColor(accentHex)
	if !ok {
		return "#000000"
	}
	black := RGBA{R: 0, G: 0, B: 0, A: 1}
	white := RGBA{R: 255, G: 255, B: 255, A: 1}
	if ContrastRatio(black, bg) >= ContrastRatio(white, bg) {
		return "#000000"
	}
	return "#ffffff"
}
