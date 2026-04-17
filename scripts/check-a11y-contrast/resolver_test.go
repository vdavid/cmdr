package main

import (
	"math"
	"testing"
)

func makeVars() *VarTable {
	v := NewVarTable()
	// Copy a minimal subset of app.css tokens.
	v.Light["color-bg-primary"] = "#ffffff"
	v.Light["color-bg-secondary"] = "#f5f5f5"
	v.Light["color-text-primary"] = "#1a1a1a"
	v.Light["color-text-secondary"] = "#666666"
	v.Light["color-text-tertiary"] = "#666666"
	v.Light["color-accent"] = "#d4a006"
	v.Light["color-accent-text"] = "color-mix(in srgb, var(--color-accent), black 65%)"
	v.Light["color-accent-subtle"] = "color-mix(in oklch, var(--color-accent), transparent 85%)"

	v.Dark["color-bg-primary"] = "#1e1e1e"
	v.Dark["color-bg-secondary"] = "#2a2a2a"
	v.Dark["color-text-primary"] = "#e8e8e8"
	v.Dark["color-text-tertiary"] = "#a0a0a0"
	v.Dark["color-accent"] = "#ffc206"
	return v
}

func TestResolveLiteral(t *testing.T) {
	r := NewResolver(makeVars(), ModeLight)
	c, err := r.Resolve("#1a1a1a")
	if err != nil {
		t.Fatalf("Resolve hex err: %v", err)
	}
	if c.R != 26 || c.G != 26 || c.B != 26 {
		t.Errorf("wrong hex: %+v", c)
	}
}

func TestResolveVarLight(t *testing.T) {
	r := NewResolver(makeVars(), ModeLight)
	c, err := r.Resolve("var(--color-text-primary)")
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if c.R != 26 {
		t.Errorf("got %+v", c)
	}
}

func TestResolveVarDarkFallsThroughToLight(t *testing.T) {
	v := makeVars()
	// --color-bg-primary has a dark override.
	r := NewResolver(v, ModeDark)
	c, err := r.Resolve("var(--color-bg-primary)")
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if c.R != 30 {
		t.Errorf("dark bg-primary got %+v, want #1e1e1e", c)
	}

	// --color-text-secondary is light-only; dark should inherit.
	r = NewResolver(v, ModeDark)
	c, err = r.Resolve("var(--color-text-secondary)")
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if c.R != 102 {
		t.Errorf("dark text-secondary got %+v, want #666 (inherited)", c)
	}
}

func TestResolveVarFallback(t *testing.T) {
	r := NewResolver(makeVars(), ModeLight)
	c, err := r.Resolve("var(--not-defined, #ff0000)")
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	if c.R != 255 {
		t.Errorf("fallback not honored: %+v", c)
	}
}

func TestResolveNestedColorMix(t *testing.T) {
	r := NewResolver(makeVars(), ModeLight)
	// --color-accent-text = color-mix(in srgb, var(--color-accent), black 65%)
	c, err := r.Resolve("var(--color-accent-text)")
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	// 35% of (212, 160, 6) = (74, 56, 2)
	if math.Abs(c.R-74) > 1 || math.Abs(c.G-56) > 1 || math.Abs(c.B-2) > 1 {
		t.Errorf("accent-text got %+v, want (~74, ~56, ~2)", c)
	}
}

func TestResolveOKLCHTransparent(t *testing.T) {
	r := NewResolver(makeVars(), ModeLight)
	c, err := r.Resolve("var(--color-accent-subtle)")
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	// alpha should be ~0.15
	if math.Abs(c.A-0.15) > 0.02 {
		t.Errorf("accent-subtle alpha = %v, want ~0.15", c.A)
	}
}

func TestResolveLeadingPercent(t *testing.T) {
	r := NewResolver(makeVars(), ModeLight)
	// color-mix(in srgb, 50% red, 50% blue) — both explicit.
	c, err := r.Resolve("color-mix(in srgb, 50% red, 50% blue)")
	if err != nil {
		t.Fatalf("err: %v", err)
	}
	// 50/50 red/blue in sRGB = (127.5, 0, 127.5)
	if math.Abs(c.R-127.5) > 1 || c.G > 1 || math.Abs(c.B-127.5) > 1 {
		t.Errorf("50/50 red/blue got %+v, want (~128, 0, ~128)", c)
	}
}

func TestSplitTopLevelCommas(t *testing.T) {
	got := splitTopLevelCommas("a, b(x, y), c")
	want := []string{"a", " b(x, y)", " c"}
	if len(got) != len(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
	for i := range got {
		if got[i] != want[i] {
			t.Errorf("idx %d: got %q want %q", i, got[i], want[i])
		}
	}
}
