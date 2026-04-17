package main

import (
	"fmt"
	"strconv"
	"strings"
)

// Resolver evaluates a CSS value string (a mix of literals, var() references,
// and color-mix() calls) into a concrete RGBA color for a given mode.
//
// It's intentionally strict: on any parse failure it returns an error so the
// caller can log a warning rather than silently produce nonsense.
type Resolver struct {
	Vars *VarTable
	// seen guards against variable recursion within a single resolution.
	seen map[string]bool
	// Warnings accumulates non-fatal issues (oklch skips, unresolved vars
	// treated as opaque fallbacks, etc.) encountered during resolution.
	Warnings []string
	// CurrentMode is the mode for which we're resolving.
	CurrentMode Mode
}

// NewResolver creates a Resolver over the given variable table and mode.
func NewResolver(vars *VarTable, mode Mode) *Resolver {
	return &Resolver{
		Vars:        vars,
		CurrentMode: mode,
		seen:        make(map[string]bool),
	}
}

// Resolve evaluates a CSS value into an RGBA. Alpha may be < 1 for translucent
// colors (caller decides how to composite).
func (r *Resolver) Resolve(value string) (RGBA, error) {
	value = strings.TrimSpace(value)
	if value == "" {
		return RGBA{}, fmt.Errorf("empty value")
	}
	// `currentColor` and `inherit` are opaque black or opaque white depending
	// on cascade — we can't know. Surface a warning and return zero RGBA so
	// the caller can decide to skip this rule.
	lower := strings.ToLower(value)
	if lower == "currentcolor" || lower == "inherit" || lower == "unset" || lower == "initial" || lower == "revert" {
		return RGBA{}, fmt.Errorf("unresolvable keyword: %s", value)
	}
	// `background: none` is CSS shorthand meaning "no background layer".
	// Treat as transparent so the cascade falls back to the ancestor bg.
	if lower == "none" {
		return RGBA{A: 0}, nil
	}

	// Literal color?
	if c, ok := ParseColor(value); ok {
		return c, nil
	}

	// var(--name, fallback)
	if strings.HasPrefix(lower, "var(") {
		return r.resolveVar(value)
	}

	// color-mix(in <space>, A, B N%)
	if strings.HasPrefix(lower, "color-mix(") {
		return r.resolveColorMix(value)
	}

	return RGBA{}, fmt.Errorf("unsupported value: %q", value)
}

func (r *Resolver) resolveVar(value string) (RGBA, error) {
	open := strings.IndexByte(value, '(')
	close := matchParenEnd(value, open)
	if open < 0 || close < 0 {
		return RGBA{}, fmt.Errorf("malformed var(): %q", value)
	}
	inner := value[open+1 : close]
	args := splitTopLevelCommas(inner)
	if len(args) == 0 {
		return RGBA{}, fmt.Errorf("empty var()")
	}

	name := strings.TrimSpace(args[0])
	name = strings.TrimPrefix(name, "--")

	// Resolve the variable if defined.
	raw, ok := r.Vars.Raw(name, r.CurrentMode)
	if ok && raw != "" {
		if r.seen[name] {
			return RGBA{}, fmt.Errorf("recursive var(--%s)", name)
		}
		r.seen[name] = true
		defer delete(r.seen, name)
		c, err := r.Resolve(raw)
		if err == nil {
			return c, nil
		}
		// Variable value failed to resolve — try fallback.
		if len(args) >= 2 {
			return r.Resolve(strings.Join(args[1:], ","))
		}
		return RGBA{}, err
	}

	// Variable missing: use fallback if present.
	if len(args) >= 2 {
		return r.Resolve(strings.Join(args[1:], ","))
	}
	return RGBA{}, fmt.Errorf("undefined var --%s", name)
}

func (r *Resolver) resolveColorMix(value string) (RGBA, error) {
	args, err := parseColorMixArgs(value)
	if err != nil {
		return RGBA{}, err
	}

	aColor, aPct, aErr := parseColorStopWithPercent(strings.TrimSpace(args.a))
	if aErr != nil {
		return RGBA{}, fmt.Errorf("color-mix arg A: %w", aErr)
	}
	bColor, bPct, bErr := parseColorStopWithPercent(strings.TrimSpace(args.b))
	if bErr != nil {
		return RGBA{}, fmt.Errorf("color-mix arg B: %w", bErr)
	}

	bw, err := colorMixWeight(aPct, bPct)
	if err != nil {
		return RGBA{}, err
	}

	aRGBA, err := r.Resolve(aColor)
	if err != nil {
		return RGBA{}, fmt.Errorf("color-mix A resolve: %w", err)
	}
	bRGBA, err := r.Resolve(bColor)
	if err != nil {
		return RGBA{}, fmt.Errorf("color-mix B resolve: %w", err)
	}

	return r.applyMix(args.space, aRGBA, bRGBA, bw)
}

type colorMixArgs struct {
	space string
	a, b  string
}

func parseColorMixArgs(value string) (colorMixArgs, error) {
	open := strings.IndexByte(value, '(')
	close := matchParenEnd(value, open)
	if open < 0 || close < 0 {
		return colorMixArgs{}, fmt.Errorf("malformed color-mix(): %q", value)
	}
	parts := splitTopLevelCommas(value[open+1 : close])
	if len(parts) != 3 {
		return colorMixArgs{}, fmt.Errorf("color-mix expects 3 args, got %d", len(parts))
	}
	space := strings.TrimSpace(parts[0])
	spaceLower := strings.ToLower(space)
	if !strings.HasPrefix(spaceLower, "in ") {
		return colorMixArgs{}, fmt.Errorf("color-mix first arg must be `in <space>`: %q", space)
	}
	spaceName := strings.ToLower(strings.TrimSpace(strings.TrimPrefix(spaceLower, "in ")))
	if sp := strings.IndexByte(spaceName, ' '); sp >= 0 {
		spaceName = spaceName[:sp]
	}
	return colorMixArgs{space: spaceName, a: parts[1], b: parts[2]}, nil
}

// colorMixWeight returns the B-side weight in [0,1] given the raw percentages
// extracted from each side (or -1 if absent).
func colorMixWeight(aPct, bPct float64) (float64, error) {
	switch {
	case aPct < 0 && bPct < 0:
		return 0.5, nil
	case aPct >= 0 && bPct < 0:
		return 1 - aPct/100, nil
	case aPct < 0 && bPct >= 0:
		return bPct / 100, nil
	}
	sum := aPct + bPct
	if sum <= 0 {
		return 0, fmt.Errorf("color-mix weights sum to zero")
	}
	return bPct / sum, nil
}

func (r *Resolver) applyMix(space string, a, b RGBA, bw float64) (RGBA, error) {
	switch space {
	case "srgb":
		return MixSRGB(a, b, bw), nil
	case "oklch", "oklab", "hsl", "lch", "lab", "xyz":
		if space != "oklch" {
			r.Warnings = append(r.Warnings, fmt.Sprintf("color-mix in %s approximated as oklch", space))
		}
		return MixOKLCH(a, b, bw), nil
	default:
		return RGBA{}, fmt.Errorf("color-mix space %q not supported", space)
	}
}

// parseColorStopWithPercent parses a `<color> [<percent>]` or `<percent> <color>` term.
// Returns the color expression (unresolved) and the percent (-1 if absent).
func parseColorStopWithPercent(s string) (string, float64, error) {
	s = strings.TrimSpace(s)
	if s == "" {
		return "", 0, fmt.Errorf("empty color stop")
	}
	// Find a standalone trailing or leading percent that's *outside* any
	// nested parens. Walk the tokens respecting paren depth.
	pct := -1.0
	rest := s

	// Try trailing percent first.
	if idx := findTrailingPercentToken(s); idx >= 0 {
		tok := strings.TrimSpace(s[idx:])
		numStr := strings.TrimSuffix(tok, "%")
		v, err := strconv.ParseFloat(strings.TrimSpace(numStr), 64)
		if err == nil {
			pct = v
			rest = strings.TrimSpace(s[:idx])
		}
	}
	// Leading percent.
	if pct < 0 {
		fields := splitBySpaceTopLevel(s)
		if len(fields) >= 2 && strings.HasSuffix(fields[0], "%") {
			v, err := strconv.ParseFloat(strings.TrimSuffix(fields[0], "%"), 64)
			if err == nil {
				pct = v
				rest = strings.TrimSpace(strings.Join(fields[1:], " "))
			}
		}
	}
	return rest, pct, nil
}

// findTrailingPercentToken returns the byte index of the last percent token
// if it's a standalone trailing token outside parens, else -1.
func findTrailingPercentToken(s string) int {
	depth := 0
	lastStart := -1
	for i := 0; i < len(s); i++ {
		switch s[i] {
		case '(':
			depth++
			lastStart = -1
		case ')':
			if depth > 0 {
				depth--
			}
			lastStart = -1
		case ' ', '\t', '\n', '\r':
			if depth == 0 {
				lastStart = i + 1
			}
		default:
			if depth == 0 && lastStart < 0 {
				lastStart = i
			}
		}
	}
	if lastStart < 0 {
		return -1
	}
	tok := strings.TrimSpace(s[lastStart:])
	if !strings.HasSuffix(tok, "%") {
		return -1
	}
	// Make sure this is purely a number + %.
	body := strings.TrimSuffix(tok, "%")
	if _, err := strconv.ParseFloat(strings.TrimSpace(body), 64); err != nil {
		return -1
	}
	return lastStart
}

// splitBySpaceTopLevel splits on whitespace while respecting parens.
func splitBySpaceTopLevel(s string) []string {
	var out []string
	depth := 0
	start := 0
	flush := func(end int) {
		tok := strings.TrimSpace(s[start:end])
		if tok != "" {
			out = append(out, tok)
		}
		start = end + 1
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch c {
		case '(':
			depth++
		case ')':
			if depth > 0 {
				depth--
			}
		case ' ', '\t', '\n', '\r':
			if depth == 0 {
				flush(i)
			}
		}
	}
	if start < len(s) {
		tok := strings.TrimSpace(s[start:])
		if tok != "" {
			out = append(out, tok)
		}
	}
	return out
}

// splitTopLevelCommas splits s on commas that aren't inside parens.
func splitTopLevelCommas(s string) []string {
	var out []string
	depth := 0
	start := 0
	for i := 0; i < len(s); i++ {
		switch s[i] {
		case '(':
			depth++
		case ')':
			if depth > 0 {
				depth--
			}
		case ',':
			if depth == 0 {
				out = append(out, s[start:i])
				start = i + 1
			}
		}
	}
	if start <= len(s) {
		out = append(out, s[start:])
	}
	return out
}

// matchParenEnd returns the index of the `)` matching the `(` at openIdx.
func matchParenEnd(s string, openIdx int) int {
	if openIdx < 0 || openIdx >= len(s) || s[openIdx] != '(' {
		return -1
	}
	depth := 1
	for i := openIdx + 1; i < len(s); i++ {
		switch s[i] {
		case '(':
			depth++
		case ')':
			depth--
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}
