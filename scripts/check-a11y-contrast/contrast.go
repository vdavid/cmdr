package main

import (
	"fmt"
	"math"
	"strconv"
	"strings"
)

// RGBA is a linear-sRGB-adjacent representation with 0-255 channels and [0,1] alpha.
// We keep channels as float64 to avoid rounding between color-mix operations.
type RGBA struct {
	R, G, B float64 // 0..255
	A       float64 // 0..1
}

// Opaque returns true if the alpha is 1 (fully opaque).
func (c RGBA) Opaque() bool {
	return c.A >= 0.999999
}

// Hex returns the `#RRGGBB` representation (ignoring alpha).
func (c RGBA) Hex() string {
	return fmt.Sprintf("#%02x%02x%02x", clampByte(c.R), clampByte(c.G), clampByte(c.B))
}

func clampByte(v float64) uint8 {
	if v < 0 {
		return 0
	}
	if v > 255 {
		return 255
	}
	return uint8(math.Round(v))
}

// ParseColor parses a CSS color literal into RGBA (hex, rgb(), rgba(), or named).
// Returns ok=false if the string isn't a recognized literal.
func ParseColor(s string) (RGBA, bool) {
	s = strings.TrimSpace(s)
	if s == "" {
		return RGBA{}, false
	}
	lower := strings.ToLower(s)

	if lower == "transparent" {
		return RGBA{A: 0}, true
	}
	if c, ok := namedColors[lower]; ok {
		return c, true
	}
	if strings.HasPrefix(s, "#") {
		return parseHex(s[1:])
	}
	if strings.HasPrefix(lower, "rgb(") || strings.HasPrefix(lower, "rgba(") {
		return parseRGBFunc(s)
	}
	return RGBA{}, false
}

func parseHex(h string) (RGBA, bool) {
	switch len(h) {
	case 3, 4:
		return parseHexShort(h)
	case 6, 8:
		return parseHexLong(h)
	}
	return RGBA{}, false
}

func parseHexShort(h string) (RGBA, bool) {
	nibbles := make([]uint8, len(h))
	for i := 0; i < len(h); i++ {
		v, ok := hexNibble(h[i])
		if !ok {
			return RGBA{}, false
		}
		nibbles[i] = v
	}
	expand := func(n uint8) float64 { return float64(n*16 + n) }
	c := RGBA{R: expand(nibbles[0]), G: expand(nibbles[1]), B: expand(nibbles[2]), A: 1}
	if len(h) == 4 {
		c.A = expand(nibbles[3]) / 255
	}
	return c, true
}

func parseHexLong(h string) (RGBA, bool) {
	bytes := make([]uint8, len(h)/2)
	for i := range bytes {
		v, ok := hexByte(h[i*2 : i*2+2])
		if !ok {
			return RGBA{}, false
		}
		bytes[i] = v
	}
	c := RGBA{R: float64(bytes[0]), G: float64(bytes[1]), B: float64(bytes[2]), A: 1}
	if len(bytes) == 4 {
		c.A = float64(bytes[3]) / 255
	}
	return c, true
}

func hexNibble(c byte) (uint8, bool) {
	switch {
	case c >= '0' && c <= '9':
		return c - '0', true
	case c >= 'a' && c <= 'f':
		return c - 'a' + 10, true
	case c >= 'A' && c <= 'F':
		return c - 'A' + 10, true
	}
	return 0, false
}

func hexByte(s string) (uint8, bool) {
	hi, ok1 := hexNibble(s[0])
	lo, ok2 := hexNibble(s[1])
	if !ok1 || !ok2 {
		return 0, false
	}
	return hi*16 + lo, true
}

func parseRGBFunc(s string) (RGBA, bool) {
	open := strings.Index(s, "(")
	close := strings.LastIndex(s, ")")
	if open < 0 || close <= open {
		return RGBA{}, false
	}
	inner := s[open+1 : close]
	// Accept comma or space separation; accept optional `/` before alpha.
	inner = strings.ReplaceAll(inner, ",", " ")
	inner = strings.ReplaceAll(inner, "/", " ")
	fields := strings.Fields(inner)
	if len(fields) < 3 {
		return RGBA{}, false
	}
	r, ok1 := parseChannelByte(fields[0])
	g, ok2 := parseChannelByte(fields[1])
	b, ok3 := parseChannelByte(fields[2])
	if !ok1 || !ok2 || !ok3 {
		return RGBA{}, false
	}
	a := 1.0
	if len(fields) >= 4 {
		av, ok := parseAlpha(fields[3])
		if !ok {
			return RGBA{}, false
		}
		a = av
	}
	return RGBA{R: r, G: g, B: b, A: a}, true
}

func parseChannelByte(s string) (float64, bool) {
	s = strings.TrimSpace(s)
	if strings.HasSuffix(s, "%") {
		v, err := strconv.ParseFloat(strings.TrimSuffix(s, "%"), 64)
		if err != nil {
			return 0, false
		}
		return v / 100 * 255, true
	}
	v, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return 0, false
	}
	return v, true
}

func parseAlpha(s string) (float64, bool) {
	s = strings.TrimSpace(s)
	if strings.HasSuffix(s, "%") {
		v, err := strconv.ParseFloat(strings.TrimSuffix(s, "%"), 64)
		if err != nil {
			return 0, false
		}
		return v / 100, true
	}
	v, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return 0, false
	}
	return v, true
}

// MixSRGB mixes A and B in sRGB space following CSS `color-mix` rules:
// colors are premultiplied by their alpha, interpolated, then divided out.
// `bWeight` is in [0, 1] — the fraction attributed to B.
// Example: `color-mix(in srgb, A, B 65%)` => MixSRGB(A, B, 0.65) => 35%A + 65%B.
func MixSRGB(a, b RGBA, bWeight float64) RGBA {
	aw := 1 - bWeight
	alpha := a.A*aw + b.A*bWeight
	if alpha < 1e-9 {
		// Fully transparent result — keep hue from whichever side had non-zero
		// alpha originally (or zero everything).
		return RGBA{A: 0}
	}
	// Premultiplied mix.
	r := (a.R*a.A*aw + b.R*b.A*bWeight) / alpha
	g := (a.G*a.A*aw + b.G*b.A*bWeight) / alpha
	bch := (a.B*a.A*aw + b.B*b.A*bWeight) / alpha
	return RGBA{R: r, G: g, B: bch, A: alpha}
}

// MixOKLCH mixes two sRGB colors by converting to OKLab, interpolating in
// OKLCH polar coordinates (L, C, h), then converting back to sRGB.
// Matches CSS `color-mix(in oklch, A, B N%)` including:
//   - premultiplied-alpha interpolation (translucent inputs don't bleed
//     color information at reduced opacity)
//   - achromatic hue fallback (when one side has ~0 chroma, use the other's
//     hue so the mix doesn't drift through a random direction)
//   - shorter-arc hue interpolation (the CSS default).
//
// Special case: if one side has alpha 0 (for example `transparent`), we drop
// its color contribution entirely and take the other side's color with a
// blended alpha — this is what CSS actually produces and what designers mean
// when writing `color-mix(in oklch, #foo, transparent N%)`.
func MixOKLCH(a, b RGBA, bWeight float64) RGBA {
	aw := 1 - bWeight
	alpha := a.A*aw + b.A*bWeight
	if alpha < 1e-9 {
		return RGBA{A: 0}
	}

	// If one side is fully transparent, its color doesn't enter the mix.
	// The result is the other side's hue/chroma/L, with the blended alpha.
	if a.A < 1e-9 {
		L, C, H := srgbToOKLCH(b)
		out := oklchToSRGB(L, C, H, alpha)
		return out
	}
	if b.A < 1e-9 {
		L, C, H := srgbToOKLCH(a)
		out := oklchToSRGB(L, C, H, alpha)
		return out
	}

	aL, aC, aH := srgbToOKLCH(a)
	bL, bC, bH := srgbToOKLCH(b)

	if aC < 1e-6 {
		aH = bH
	}
	if bC < 1e-6 {
		bH = aH
	}

	if bH-aH > 180 {
		aH += 360
	} else if aH-bH > 180 {
		bH += 360
	}

	// Premultiply L and C by alpha for color mixing; hue is interpolated
	// straight.
	apw := a.A * aw
	bpw := b.A * bWeight
	wSum := apw + bpw
	L := (aL*apw + bL*bpw) / wSum
	C := (aC*apw + bC*bpw) / wSum
	H := aH*aw + bH*bWeight

	return oklchToSRGB(L, C, H, alpha)
}

// CompositeOver alpha-composites `fg` (possibly translucent) over solid `bg`.
// Returns the opaque result seen by the eye.
func CompositeOver(fg, bg RGBA) RGBA {
	if fg.A >= 0.999999 {
		return fg
	}
	if fg.A <= 1e-9 {
		out := bg
		out.A = 1
		return out
	}
	a := fg.A
	return RGBA{
		R: fg.R*a + bg.R*(1-a),
		G: fg.G*a + bg.G*(1-a),
		B: fg.B*a + bg.B*(1-a),
		A: 1,
	}
}

// --- OKLab / OKLCH conversions (Björn Ottosson's OKLab). ---

func srgbToLinear(c float64) float64 {
	c = c / 255
	if c <= 0.04045 {
		return c / 12.92
	}
	return math.Pow((c+0.055)/1.055, 2.4)
}

func linearToSRGB(c float64) float64 {
	var v float64
	if c <= 0.0031308 {
		v = 12.92 * c
	} else {
		v = 1.055*math.Pow(c, 1/2.4) - 0.055
	}
	return v * 255
}

// srgbToOKLCH returns (L, C, h) with h in degrees [0, 360).
func srgbToOKLCH(c RGBA) (float64, float64, float64) {
	r := srgbToLinear(c.R)
	g := srgbToLinear(c.G)
	b := srgbToLinear(c.B)

	l := 0.4122214708*r + 0.5363325363*g + 0.0514459929*b
	m := 0.2119034982*r + 0.6806995451*g + 0.1073969566*b
	s := 0.0883024619*r + 0.2817188376*g + 0.6299787005*b

	l = cbrt(l)
	m = cbrt(m)
	s = cbrt(s)

	L := 0.2104542553*l + 0.7936177850*m - 0.0040720468*s
	A := 1.9779984951*l - 2.4285922050*m + 0.4505937099*s
	B := 0.0259040371*l + 0.7827717662*m - 0.8086757660*s

	C := math.Sqrt(A*A + B*B)
	h := math.Atan2(B, A) * 180 / math.Pi
	if h < 0 {
		h += 360
	}
	return L, C, h
}

func oklchToSRGB(L, C, h, alpha float64) RGBA {
	hr := h * math.Pi / 180
	A := C * math.Cos(hr)
	B := C * math.Sin(hr)

	l := L + 0.3963377774*A + 0.2158037573*B
	m := L - 0.1055613458*A - 0.0638541728*B
	s := L - 0.0894841775*A - 1.2914855480*B

	l = l * l * l
	m = m * m * m
	s = s * s * s

	r := +4.0767416621*l - 3.3077115913*m + 0.2309699292*s
	g := -1.2684380046*l + 2.6097574011*m - 0.3413193965*s
	bl := -0.0041960863*l - 0.7034186147*m + 1.7076147010*s

	return RGBA{
		R: clampLinearToSrgb(r),
		G: clampLinearToSrgb(g),
		B: clampLinearToSrgb(bl),
		A: alpha,
	}
}

func clampLinearToSrgb(c float64) float64 {
	v := linearToSRGB(c)
	if v < 0 {
		return 0
	}
	if v > 255 {
		return 255
	}
	return v
}

func cbrt(x float64) float64 {
	if x < 0 {
		return -math.Pow(-x, 1.0/3)
	}
	return math.Pow(x, 1.0/3)
}

// --- WCAG 2.2 contrast ratio ---

// RelativeLuminance returns the WCAG relative luminance of an opaque sRGB color.
func RelativeLuminance(c RGBA) float64 {
	r := srgbChannelLum(c.R / 255)
	g := srgbChannelLum(c.G / 255)
	b := srgbChannelLum(c.B / 255)
	return 0.2126*r + 0.7152*g + 0.0722*b
}

func srgbChannelLum(c float64) float64 {
	if c <= 0.03928 {
		return c / 12.92
	}
	return math.Pow((c+0.055)/1.055, 2.4)
}

// ContrastRatio returns the WCAG 2.2 contrast ratio between two opaque colors.
// Result is in [1, 21].
func ContrastRatio(a, b RGBA) float64 {
	la := RelativeLuminance(a)
	lb := RelativeLuminance(b)
	if la < lb {
		la, lb = lb, la
	}
	return (la + 0.05) / (lb + 0.05)
}

// namedColors — the subset of CSS named colors we need. We don't ship all 147
// because CSS variables overwhelmingly use hex/rgb/color-mix, and adding every
// name just expands our attack surface for typos.
var namedColors = map[string]RGBA{
	"black":       {R: 0, G: 0, B: 0, A: 1},
	"white":       {R: 255, G: 255, B: 255, A: 1},
	"transparent": {A: 0},
	"red":         {R: 255, G: 0, B: 0, A: 1},
	"green":       {R: 0, G: 128, B: 0, A: 1},
	"blue":        {R: 0, G: 0, B: 255, A: 1},
	"gray":        {R: 128, G: 128, B: 128, A: 1},
	"grey":        {R: 128, G: 128, B: 128, A: 1},
	"silver":      {R: 192, G: 192, B: 192, A: 1},
	"yellow":      {R: 255, G: 255, B: 0, A: 1},
	"cyan":        {R: 0, G: 255, B: 255, A: 1},
	"magenta":     {R: 255, G: 0, B: 255, A: 1},
}
