package main

import "testing"

// TestAccentMatrixPassesPrimaryButtonWithRuntimeFg verifies the matrix
// passes a primary button (color-accent-fg on color-accent) under every
// variant, because the matrix mirrors the runtime fg picker that switches
// between black and white per accent. This is the post-fix state of the
// regression the user reported on the License page.
func TestAccentMatrixPassesPrimaryButtonWithRuntimeFg(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-accent"] = "#d4a006" // static fallback only
	vars.Light["color-accent-fg"] = "#1a1a1a"

	svelte := `<style>
  .btn-primary {
    background: var(--color-accent);
    color: var(--color-accent-fg);
  }
</style>`
	pf := ParseSvelteFile("Button.svelte", svelte)
	a := NewAnalyzer(vars)
	findings := a.AnalyzeFile(pf)

	var lightFinding *Finding
	for i, f := range findings {
		if f.Selector == ".btn-primary" && f.Mode == ModeLight {
			lightFinding = &findings[i]
			break
		}
	}
	if lightFinding == nil {
		t.Fatalf(".btn-primary light-mode finding missing")
	}
	if !lightFinding.IsPassing {
		t.Errorf(
			".btn-primary should pass under every accent variant with the runtime fg picker; worst variant=%q ratio=%.2f fg=%s bg=%s",
			lightFinding.AccentVariant, lightFinding.Ratio, lightFinding.FG.Hex(), lightFinding.BG.Hex(),
		)
	}
}

// TestReadableFgOnApplePurple pins the picker's choice for the one variant
// where it diverges from black: Apple Purple (#a54fa7) needs white text.
func TestReadableFgOnApplePurple(t *testing.T) {
	if got := readableFgOn("#a54fa7"); got != "#ffffff" {
		t.Errorf("readableFgOn(Apple Purple) = %s, want #ffffff", got)
	}
	if got := readableFgOn("#087aff"); got != "#000000" {
		t.Errorf("readableFgOn(Apple Blue) = %s, want #000000", got)
	}
	if got := readableFgOn("#d4a006"); got != "#000000" {
		t.Errorf("readableFgOn(Cmdr gold) = %s, want #000000", got)
	}
}

// TestAccentMatrixIgnoresNonAccentPairs verifies a pair with no
// `--color-accent` dependency in the chain doesn't get re-evaluated against
// the matrix (no AccentVariant tag, and the resolved colors match the static
// pass).
func TestAccentMatrixIgnoresNonAccentPairs(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-text-primary"] = "#1a1a1a"

	svelte := `<style>
  .body {
    background: var(--color-bg-primary);
    color: var(--color-text-primary);
  }
</style>`
	pf := ParseSvelteFile("Body.svelte", svelte)
	a := NewAnalyzer(vars)
	findings := a.AnalyzeFile(pf)

	for _, f := range findings {
		if f.Selector != ".body" || f.Mode != ModeLight {
			continue
		}
		if f.AccentVariant != "" {
			t.Errorf(".body shouldn't depend on accent; got AccentVariant=%q", f.AccentVariant)
		}
	}
}

// TestDependsOnAccentDetectsTransitive verifies the dep tracker flags pairs
// that go through a derived token (e.g. `--color-accent-hover`).
func TestDependsOnAccentDetectsTransitive(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-accent"] = "#d4a006"
	vars.Light["color-accent-hover"] = "color-mix(in oklch, var(--color-accent), white 15%)"

	r := NewResolver(vars, ModeLight)
	if _, err := r.Resolve("var(--color-accent-hover)"); err != nil {
		t.Fatalf("resolve failed: %v", err)
	}
	if !dependsOnAccent(r.Deps) {
		t.Errorf("dependsOnAccent should detect transitive --color-accent via --color-accent-hover")
	}
}
