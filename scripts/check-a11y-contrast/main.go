// Package main is the design-time WCAG contrast checker for Cmdr.
//
// Parses `apps/desktop/src/app.css` for --color-* design tokens (light and
// dark), walks `apps/desktop/src/**/*.svelte`, and for each element that
// declares both a text color and a background computes the WCAG 2.2 contrast
// ratio in both modes. Flags pairs below 4.5:1 (3:1 for large text).
//
// Run: go run ./scripts/check-a11y-contrast
// Exit: 0 on clean, 1 on violations.
package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

func main() {
	verbose := flag.Bool("verbose", false, "Show warnings and per-pair detail")
	flag.Parse()

	rootDir, err := findRootDir()
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	appCSSPath := filepath.Join(rootDir, "apps", "desktop", "src", "app.css")
	cssContent, err := os.ReadFile(appCSSPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError reading app.css: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	vars := ParseAppCSS(string(cssContent))
	analyzer := NewAnalyzer(vars)

	srcDir := filepath.Join(rootDir, "apps", "desktop", "src")

	var allFindings []Finding
	var opacityFindings []OpacityFinding
	fileCount := 0

	err = filepath.Walk(srcDir, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if info.IsDir() {
			return nil
		}
		result, readErr := analyzeSourceFile(analyzer, path, appCSSPath)
		if readErr != nil {
			return readErr
		}
		if result.isSvelteFile {
			fileCount++
		}
		allFindings = append(allFindings, result.findings...)
		opacityFindings = append(opacityFindings, result.opacityFindings...)
		return nil
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError walking src: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	// Also check app.css global class rules (like `.cmdr-tooltip`,
	// `.cmdr-tooltip-kbd`). They're not scoped but they're design tokens too.
	appCSSRules := parseRulesFromCSS(appCSSPath, string(cssContent))
	if len(appCSSRules) > 0 {
		pf := &ParsedFile{Path: appCSSPath, Rules: appCSSRules}
		allFindings = append(allFindings, analyzer.AnalyzeFile(pf)...)
		opacityFindings = append(opacityFindings, analyzer.AnalyzeOpacity(pf)...)
	}

	// Size tier utility classes (`.size-bytes` .. `.size-tb`) declare only a
	// `color:` and inherit the background of their container. The rule
	// walker never pairs them with a bg. Cover every (tier × known bg × mode)
	// combo explicitly. See `size_tiers.go` for the context list.
	allFindings = append(allFindings, analyzer.AnalyzeSizeTiers()...)

	// File-list selected-row text colors (`--color-selection-fg` and the
	// `--color-size-*-selected` tiers) are set on different selectors from
	// the row bg (which depends on pane tint + stripe + cursor state + the
	// active macOS accent). The rule walker never pairs them. See
	// `row_state_matrix.go` for the composition.
	allFindings = append(allFindings, analyzer.AnalyzeRowStates()...)

	// Secondary text inside an accent-bg ancestor (the highlighted dropdown
	// option's description, today). See `dropdown_states.go` for the list.
	allFindings = append(allFindings, analyzer.AnalyzeDropdownStates()...)

	// Search / Select dialog (`lib/query-ui/`) fg-on-bg pairs that span
	// selectors or fold through `opacity`: the ToggleGroup "AI" badge + hint,
	// and the under-cursor result row's muted columns on the accent-tinted
	// cursor bg. See `query_dialog_states.go`.
	allFindings = append(allFindings, analyzer.AnalyzeQueryDialogStates()...)

	violations := FilterViolations(allFindings)
	warnings := append([]string{}, analyzer.Warnings...)
	for _, f := range allFindings {
		warnings = append(warnings, f.Warnings...)
	}
	warnings = joinWarnings(warnings)

	hasViolations := Report(violations, warnings, rootDir, *verbose)

	// APCA: print the perceptual second opinion (detail under -verbose) and
	// enforce the Lc-45 floor alongside the WCAG gate. See apca.go.
	apcaFloorFail := ReportAPCA(allFindings, rootDir, *verbose)

	// Unmodeled `opacity` dimming: a rule the rule walker can't fold into its
	// color/background pairing (see opacity_check.go). Enforced alongside
	// WCAG and APCA: an opacity dim on live text is exactly as invisible to a
	// user as a bad color pairing.
	opacityFail := ReportOpacity(opacityFindings, rootDir)

	summary := Summary(fileCount, analyzer.RulesEvaluated, len(allFindings), len(violations))
	if hasViolations || apcaFloorFail || opacityFail {
		extra := ""
		if opacityFail {
			extra = fmt.Sprintf(", %d unmodeled %s", len(opacityFindings), plural(len(opacityFindings), "opacity dim", "opacity dims"))
		}
		fmt.Printf("%s❌ %s%s%s\n", colorRed, summary, extra, colorReset)
		os.Exit(1)
	}
	fmt.Printf("%s✅ No contrast violations. %s%s\n", colorGreen, summary, colorReset)
}

// sourceFileResult is what one file contributes to the walk in main(): WCAG
// findings (Svelte components only), opacity findings (both), and whether it
// counted toward the Svelte file total in the summary line.
type sourceFileResult struct {
	findings        []Finding
	opacityFindings []OpacityFinding
	isSvelteFile    bool
}

// analyzeSourceFile handles one file from the `apps/desktop/src` walk in
// main(): a `.svelte` component feeds both the WCAG rule walker and the
// opacity check; a `.css` file other than app.css (`app-field.css`,
// `app-file-list.css`, a component-local `filter-popover.css`, ...) feeds
// only the opacity check, since these global stylesheets are imported
// directly by `+layout.svelte` rather than scoped to a component and aren't
// covered by the WCAG walker below (app.css's own global class rules are
// handled separately via `parseRulesFromCSS` in main()). Extracted out of
// main()'s filepath.Walk callback to keep that function's complexity down.
func analyzeSourceFile(analyzer *Analyzer, path, appCSSPath string) (sourceFileResult, error) {
	switch filepath.Ext(path) {
	case ".svelte":
		content, err := os.ReadFile(path)
		if err != nil {
			return sourceFileResult{}, err
		}
		parsed := ParseSvelteFile(path, string(content))
		if len(parsed.Rules) == 0 {
			return sourceFileResult{}, nil
		}
		return sourceFileResult{
			findings:        analyzer.AnalyzeFile(parsed),
			opacityFindings: analyzer.AnalyzeOpacity(parsed),
			isSvelteFile:    true,
		}, nil
	case ".css":
		if path == appCSSPath {
			return sourceFileResult{}, nil
		}
		content, err := os.ReadFile(path)
		if err != nil {
			return sourceFileResult{}, err
		}
		parsed := &ParsedFile{Path: path, Rules: parseRulesFromCSS(path, string(content))}
		if len(parsed.Rules) == 0 {
			return sourceFileResult{}, nil
		}
		return sourceFileResult{opacityFindings: analyzer.AnalyzeOpacity(parsed)}, nil
	default:
		return sourceFileResult{}, nil
	}
}

// parseRulesFromCSS reuses the Svelte rule parser on a raw CSS file.
// app.css isn't inside a `<style>` block, so we emulate one.
func parseRulesFromCSS(path, content string) []Rule {
	// Strip the dark-mode block so we don't double-attribute rules. The
	// dark-mode block only contains variable overrides and a few .cmdr-tooltip
	// overrides; we already evaluate per-mode via the variable table, and
	// .cmdr-tooltip dark overrides use rgba() literals that we don't track.
	wrapped := "<style>\n" + content + "\n</style>"
	pf := ParseSvelteFile(path, wrapped)
	return pf.Rules
}

// findRootDir walks up looking for the monorepo marker.
func findRootDir() (string, error) {
	dir, err := os.Getwd()
	if err != nil {
		return "", err
	}
	for {
		marker := filepath.Join(dir, "apps", "desktop", "src-tauri", "Cargo.toml")
		if _, err := os.Stat(marker); err == nil {
			return dir, nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", fmt.Errorf("could not find project root (missing %s)", strings.TrimPrefix(marker, dir))
		}
		dir = parent
	}
}
