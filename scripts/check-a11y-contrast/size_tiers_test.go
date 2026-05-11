package main

import "testing"

// TestSizeTiersAnalyzerEmitsAllCombos guards the synthetic finding count so
// adding/removing a context or tier surfaces as a test failure (and forces a
// reviewer to confirm the intent).
func TestSizeTiersAnalyzerEmitsAllCombos(t *testing.T) {
	vars := NewVarTable()
	// Minimal palette so every var resolves in both modes.
	for _, m := range []map[string]string{vars.Light, vars.Dark} {
		m["color-bg-primary"] = "#ffffff"
		m["color-bg-secondary"] = "#f5f5f5"
		m["color-bg-tertiary"] = "#e8e8e8"
		m["color-error-bg"] = "#fef2f2"
		m["color-warning-bg"] = "#fdeee6"
		m["color-text-secondary"] = "#6b6b6b"
		m["color-selection-fg"] = "#d4a82a"
		m["color-size-bytes"] = "#5c805c"
		m["color-size-kb"] = "#8f8147"
		m["color-size-mb"] = "#947147"
		m["color-size-gb"] = "#945c5c"
		m["color-size-tb"] = "#9a5cd4"
	}

	a := NewAnalyzer(vars)
	findings := a.AnalyzeSizeTiers()

	wantCount := len(sizeBackgrounds) * len(sizeTiers) * 2 // ×2 for light + dark
	if len(findings) != wantCount {
		t.Fatalf("emitted %d findings, want %d (backgrounds=%d × tiers=%d × modes=2)",
			len(findings), wantCount, len(sizeBackgrounds), len(sizeTiers))
	}

	// Every finding must have both fg and bg resolved (non-zero).
	for _, f := range findings {
		if f.FG.A == 0 || f.BG.A == 0 {
			t.Errorf("unresolved color in %s (%s): fg=%v bg=%v", f.Selector, f.Mode, f.FG, f.BG)
		}
		if f.Threshold != 4.5 {
			t.Errorf("size tiers should use 4.5 threshold (normal body text), got %.1f for %s", f.Threshold, f.Selector)
		}
	}
}

// TestSizeTiersAnalyzerMissingVarWarns covers the safety path: if a background
// var is removed from app.css without updating sizeBackgrounds, we warn rather
// than silently drop the check.
func TestSizeTiersAnalyzerMissingVarWarns(t *testing.T) {
	vars := NewVarTable()
	// Only define one bg + one tier so the rest miss.
	vars.Light["color-bg-primary"] = "#ffffff"
	vars.Light["color-size-bytes"] = "#5c805c"
	vars.Dark["color-bg-primary"] = "#1e1e1e"
	vars.Dark["color-size-bytes"] = "#a8c4a8"

	a := NewAnalyzer(vars)
	_ = a.AnalyzeSizeTiers()
	if len(a.Warnings) == 0 {
		t.Fatalf("expected warnings for undefined size/bg vars, got none")
	}
}
