package main

import (
	"fmt"
	"sort"
	"strings"
)

// Finding is one contrast verdict (either a violation or a passing pair we
// tracked). Only violations make it to the report.
type Finding struct {
	File        string
	Line        int
	Selector    string
	Mode        Mode
	FG          RGBA
	BG          RGBA
	Ratio       float64
	Threshold   float64
	IsPassing   bool
	FontPx      float64
	FontWeight  int
	Placeholder bool
	Warnings    []string
}

// Analyzer runs per-file: for each rule that sets both color and background
// (or resolves a background from a sibling/ancestor rule in the same <style>
// block), check contrast in light and dark modes.
type Analyzer struct {
	Vars     *VarTable
	Warnings []string
	// Total rules evaluated (for reporter summary).
	RulesEvaluated int
}

// NewAnalyzer creates an Analyzer bound to a variable table.
func NewAnalyzer(vars *VarTable) *Analyzer {
	return &Analyzer{Vars: vars}
}

// AnalyzeFile returns contrast findings for one parsed Svelte file.
//
// We walk the rules twice — once per mode — maintaining a separate cascade
// state per mode. Rules tagged with `ModeOnly` contribute only to their own
// mode's pass.
func (a *Analyzer) AnalyzeFile(pf *ParsedFile) []Finding {
	if len(pf.Rules) == 0 {
		return nil
	}

	var findings []Finding
	for _, mode := range []Mode{ModeLight, ModeDark} {
		findings = append(findings, a.analyzeFileForMode(pf, mode)...)
	}
	a.RulesEvaluated += len(pf.Rules)
	return findings
}

type classState struct {
	color      string
	background string
	fontSize   float64
	fontWeight int
	selector   string
	line       int
	// Direct* track only what THIS compound-selector entry set itself (not
	// inherited). Inheritance folds `directColor` / `directBackground` of
	// subset-matching entries, so sibling rules don't leak each other's
	// inherited properties.
	directColor      string
	directBackground string
	directFontSize   float64
	directFontWeight int
}

// stateEntry couples the state payload with the class set it applies to,
// enabling subset-based inheritance.
type stateEntry struct {
	classes []string // sorted for set ops
	pseudo  string
	st      *classState
}

func (a *Analyzer) analyzeFileForMode(pf *ParsedFile, mode Mode) []Finding {
	// Per-mode cascade: one state per unique compound-class-set (with pseudo
	// suffix). Inheritance on first-encounter walks all previously-seen
	// entries whose class set is a subset of the current rule's, so
	// `.section-item.subsection.selected` inherits bg from `.section-item.selected`.
	entries := []stateEntry{}
	byKey := make(map[string]int) // key -> index in entries
	latest := make(map[string]Finding)
	order := []string{}

	for i := range pf.Rules {
		rule := pf.Rules[i]
		if rule.ModeOnly != "" && rule.ModeOnly != mode {
			continue
		}

		key := stateKeyFor(rule)
		idx, exists := byKey[key]
		if !exists {
			// Compute inherited state by merging (in rule-order) all previous
			// entries whose classes are a subset AND whose pseudo matches.
			inherited := inheritFrom(entries, rule)
			entries = append(entries, stateEntry{
				classes: sortedCopy(rule.Classes),
				pseudo:  rule.Pseudo,
				st:      inherited,
			})
			idx = len(entries) - 1
			byKey[key] = idx
		}
		st := entries[idx].st
		applyRule(&rule, st)
		st.selector = rule.Selector
		st.line = rule.Line

		if rule.Color == "" && rule.Background == "" {
			continue
		}
		if st.color == "" || st.background == "" {
			continue
		}
		isPlaceholder := rule.Pseudo == "placeholder"
		if f, ok := a.evaluate(pf.Path, rule.Line, rule.Selector, st.color, st.background, st.fontSize, st.fontWeight, isPlaceholder, mode); ok {
			if _, seen := latest[key]; !seen {
				order = append(order, key)
			}
			latest[key] = f
		}
	}

	findings := make([]Finding, 0, len(latest))
	for _, k := range order {
		findings = append(findings, latest[k])
	}
	return findings
}

// inheritFrom folds all previously-seen entries whose class set is a subset of
// `rule`'s, producing a new classState. Source order matters: later rules
// override earlier ones. Only direct* contributions of each subset entry are
// merged — inherited values aren't re-propagated, so sibling compound
// selectors don't leak each other's inherited defaults.
func inheritFrom(entries []stateEntry, rule Rule) *classState {
	out := &classState{}
	ruleClasses := sortedCopy(rule.Classes)
	for _, e := range entries {
		if e.pseudo != "" && e.pseudo != rule.Pseudo {
			continue
		}
		if !isSubset(e.classes, ruleClasses) {
			continue
		}
		if e.st.directColor != "" {
			out.color = e.st.directColor
		}
		if e.st.directBackground != "" {
			out.background = e.st.directBackground
		}
		if e.st.directFontSize > 0 {
			out.fontSize = e.st.directFontSize
		}
		if e.st.directFontWeight > 0 {
			out.fontWeight = e.st.directFontWeight
		}
	}
	return out
}

func sortedCopy(in []string) []string {
	out := make([]string, len(in))
	copy(out, in)
	sort.Strings(out)
	return out
}

func isSubset(small, large []string) bool {
	if len(small) > len(large) {
		return false
	}
	i := 0
	for _, v := range large {
		if i < len(small) && small[i] == v {
			i++
		}
	}
	return i == len(small)
}

// stateKeyFor returns the identifier used to group state for a rule's selector.
// Multi-class compound selectors (`.pill.empty`) get a distinct key from the
// base class alone. Pseudo-element selectors get their own key. Pseudo-classes
// (`.pill:hover`) share the base class's state key — hover is a transient
// runtime state, not a parallel configuration.
func stateKeyFor(rule Rule) string {
	sorted := sortedCopy(rule.Classes)
	key := strings.Join(sorted, ".")
	if rule.Pseudo != "" {
		key += "::" + rule.Pseudo
	}
	return key
}

func applyRule(rule *Rule, st *classState) {
	if rule.Color != "" {
		st.color = rule.Color
		st.directColor = rule.Color
	}
	if rule.Background != "" {
		st.background = rule.Background
		st.directBackground = rule.Background
	}
	if rule.FontSizePx > 0 {
		st.fontSize = rule.FontSizePx
		st.directFontSize = rule.FontSizePx
	}
	if rule.FontWeight > 0 {
		st.fontWeight = rule.FontWeight
		st.directFontWeight = rule.FontWeight
	}
}

func (a *Analyzer) evaluate(file string, line int, selector, colorExpr, bgExpr string, fontPx float64, fontWeight int, isPlaceholder bool, mode Mode) (Finding, bool) {
	fgRes := NewResolver(a.Vars, mode)
	fg, err := fgRes.Resolve(colorExpr)
	if err != nil {
		a.Warnings = append(a.Warnings, fmt.Sprintf("%s:%d [%s] fg %q (%s): %v", file, line, selector, colorExpr, mode, err))
		return Finding{}, false
	}
	bgRes := NewResolver(a.Vars, mode)
	bg, err := bgRes.Resolve(bgExpr)
	if err != nil {
		a.Warnings = append(a.Warnings, fmt.Sprintf("%s:%d [%s] bg %q (%s): %v", file, line, selector, bgExpr, mode, err))
		return Finding{}, false
	}

	// Composite bg over `--color-bg-primary` if translucent. Same for fg
	// (fg-on-translucent-bg is rare but can happen when color-mix uses
	// `transparent` on the fg side).
	if !bg.Opaque() {
		primaryRes := NewResolver(a.Vars, mode)
		if raw, ok := a.Vars.Raw("color-bg-primary", mode); ok {
			if solid, err := primaryRes.Resolve(raw); err == nil {
				bg = CompositeOver(bg, solid)
			} else {
				bg.A = 1
			}
		} else {
			bg.A = 1
		}
	}
	if !fg.Opaque() {
		fg = CompositeOver(fg, bg)
	}

	ratio := ContrastRatio(fg, bg)
	threshold := 4.5
	// WCAG "large text" threshold: >= 18pt (24px) OR >= 14pt bold (~18.66px bold).
	if fontPx >= 24 || (fontPx >= 18.66 && fontWeight >= 700) {
		threshold = 3.0
	}

	warnings := append([]string{}, fgRes.Warnings...)
	warnings = append(warnings, bgRes.Warnings...)

	return Finding{
		File:        file,
		Line:        line,
		Selector:    selector,
		Mode:        mode,
		FG:          fg,
		BG:          bg,
		Ratio:       ratio,
		Threshold:   threshold,
		IsPassing:   ratio >= threshold,
		FontPx:      fontPx,
		FontWeight:  fontWeight,
		Placeholder: isPlaceholder,
		Warnings:    warnings,
	}, true
}

// FilterViolations returns only the findings that failed their threshold.
func FilterViolations(findings []Finding) []Finding {
	var out []Finding
	for _, f := range findings {
		if !f.IsPassing {
			out = append(out, f)
		}
	}
	return out
}

// RelPath returns a display-friendly relative path from root.
func RelPath(root, abs string) string {
	if rel, ok := strings.CutPrefix(abs, root); ok {
		return strings.TrimPrefix(rel, "/")
	}
	return abs
}
