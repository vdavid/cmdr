package main

import (
	"os"
	"path/filepath"
	"testing"
)

// loadAppCSSVars loads the real app.css token table for synthesizer tests.
func loadAppCSSVars(t *testing.T) *VarTable {
	t.Helper()
	root, err := findRootDir()
	if err != nil {
		t.Fatalf("findRootDir: %v", err)
	}
	css, err := os.ReadFile(filepath.Join(root, "apps", "desktop", "src", "app.css"))
	if err != nil {
		t.Fatalf("read app.css: %v", err)
	}
	return ParseAppCSS(string(css))
}

// TestQueryDialogScenariosClearAA pins that every modeled Search / Select
// dialog fg-on-bg pair clears WCAG AA across all accents + both modes. These
// pairs span selectors (badge / hint / under-cursor row) so the generic rule
// walker can't catch them; this synthesizer is the only gate.
func TestQueryDialogScenariosClearAA(t *testing.T) {
	a := NewAnalyzer(loadAppCSSVars(t))
	findings := a.AnalyzeQueryDialogStates()
	if len(findings) == 0 {
		t.Fatal("expected query-dialog findings, got none (scenario wiring broken?)")
	}
	for _, f := range findings {
		if !f.IsPassing {
			t.Errorf("query-dialog contrast fails AA: %s [%s accent=%q] ratio=%.2f (need >= %.1f)",
				f.Selector, f.Mode, f.AccentVariant, f.Ratio, f.Threshold)
		}
	}
}
