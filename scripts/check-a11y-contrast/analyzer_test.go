package main

import "testing"

func TestSubsetInheritance(t *testing.T) {
	small := []string{"a", "b"}
	large := []string{"a", "b", "c"}
	if !isSubset(small, large) {
		t.Errorf("expected [a,b] subset of [a,b,c]")
	}
	if isSubset(large, small) {
		t.Errorf("[a,b,c] should not be subset of [a,b]")
	}
	if !isSubset([]string{}, large) {
		t.Errorf("empty set should be subset")
	}
}

// TestAnalyzerInheritsCompoundSelectorBg exercises the inheritance rule:
// `.item.sub.selected` must inherit bg from `.item.selected`, and the
// sibling compound `.item.sub` must NOT leak its inherited-from-`.item` bg.
func TestAnalyzerInheritsCompoundSelectorBg(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-text-primary"] = "#1a1a1a"
	vars.Light["color-text-secondary"] = "#666666"
	vars.Light["color-accent"] = "#d4a006"
	vars.Light["color-accent-fg"] = "#1a1a1a"
	vars.Dark["color-bg-primary"] = "#1e1e1e"
	vars.Dark["color-text-primary"] = "#e8e8e8"
	vars.Dark["color-accent"] = "#ffc206"

	svelte := `<style>
  .item { background: none; color: var(--color-text-primary); }
  .item.selected { background: var(--color-accent); color: var(--color-accent-fg); }
  .item.sub { color: var(--color-text-secondary); }
  .item.sub.selected { color: var(--color-accent-fg); }
</style>`
	pf := ParseSvelteFile("test.svelte", svelte)
	a := NewAnalyzer(vars)
	findings := a.AnalyzeFile(pf)

	want := map[string]map[Mode]string{
		".item.sub.selected": {ModeLight: "#d4a006", ModeDark: "#ffc206"},
	}
	for _, f := range findings {
		if m, ok := want[f.Selector]; ok {
			if got := f.BG.Hex(); got != m[f.Mode] {
				t.Errorf("%s (%s): bg = %s, want %s", f.Selector, f.Mode, got, m[f.Mode])
			}
		}
	}
}

// TestAnalyzerAiLabelNotFlagged reproduces the primary false-positive axe gave:
// `.ai-label` with text-primary on accent-subtle should be ~10:1, safely passing.
func TestAnalyzerAiLabelNotFlagged(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-bg-secondary"] = "#f5f5f5"
	vars.Light["color-text-primary"] = "#1a1a1a"
	vars.Light["color-text-tertiary"] = "#666666"
	vars.Light["color-accent"] = "#d4a006"
	vars.Light["color-accent-subtle"] = "color-mix(in oklch, var(--color-accent), transparent 85%)"

	svelte := `<style>
  .ai-label {
    color: var(--color-text-primary);
    background: var(--color-accent-subtle);
  }
</style>`
	pf := ParseSvelteFile("AiSearchRow.svelte", svelte)
	a := NewAnalyzer(vars)
	findings := a.AnalyzeFile(pf)

	var lightRatio float64
	for _, f := range findings {
		if f.Selector == ".ai-label" && f.Mode == ModeLight {
			lightRatio = f.Ratio
		}
	}
	if lightRatio < 8 {
		t.Errorf(".ai-label light contrast = %v, want >= 8 (safely passing AA)", lightRatio)
	}
}

// TestAnalyzerPlaceholderOkAtTertiary covers the second false-positive case:
// `.name-input::placeholder` with text-tertiary on bg-secondary is 5.26:1
// (well above 4.5) and should NOT flag.
func TestAnalyzerPlaceholderOkAtTertiary(t *testing.T) {
	vars := NewVarTable()
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-bg-secondary"] = "#f5f5f5"
	vars.Light["color-text-tertiary"] = "#666666"

	svelte := `<style>
  .name-input {
    background: var(--color-bg-secondary);
    color: var(--color-text-tertiary);
  }
  .name-input::placeholder {
    color: var(--color-text-tertiary);
  }
</style>`
	pf := ParseSvelteFile("AiSearchRow.svelte", svelte)
	a := NewAnalyzer(vars)
	findings := a.AnalyzeFile(pf)

	for _, f := range findings {
		if !f.IsPassing {
			t.Errorf("unexpected violation: %s fg=%s bg=%s ratio=%.2f", f.Selector, f.FG.Hex(), f.BG.Hex(), f.Ratio)
		}
	}
}
