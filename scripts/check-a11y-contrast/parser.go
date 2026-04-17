package main

import (
	"fmt"
	"regexp"
	"strings"
)

// Mode identifies which color-scheme a set of variable values belongs to.
type Mode string

const (
	ModeLight Mode = "light"
	ModeDark  Mode = "dark"
)

// VarTable holds raw variable definitions (name -> raw CSS value string) per mode.
type VarTable struct {
	Light map[string]string
	Dark  map[string]string
}

// NewVarTable returns an empty VarTable.
func NewVarTable() *VarTable {
	return &VarTable{
		Light: make(map[string]string),
		Dark:  make(map[string]string),
	}
}

// Raw returns the raw value string for a variable in a given mode.
// If the dark mode doesn't override it, falls back to the light value.
func (v *VarTable) Raw(name string, mode Mode) (string, bool) {
	if mode == ModeDark {
		if val, ok := v.Dark[name]; ok {
			return val, true
		}
	}
	val, ok := v.Light[name]
	return val, ok
}

// Rule represents a single selector block in a Svelte `<style>` section.
// We keep simple class selectors and optional pseudo-element markers.
type Rule struct {
	File       string // absolute path
	Line       int    // line number of the opening brace, 1-indexed
	Selector   string // full selector text, as written (for display)
	Classes    []string
	Pseudo     string // empty, or "placeholder" (future: "before", "after")
	Color      string // raw CSS value (empty if not set)
	Background string // raw CSS value (empty if not set)
	// FontSizePx is the parsed `font-size` declared in this rule (0 if absent).
	FontSizePx float64
	// FontWeight is the parsed numeric weight (0 if absent).
	FontWeight int
	// HasBackgroundColorProp tracks whether the property was explicitly
	// `background-color` vs the shorthand `background`. Purely informational.
	HasBackgroundColorProp bool
	// ModeOnly restricts this rule to one color-scheme mode. Empty means
	// "applies to all modes". Rules inside `@media (prefers-color-scheme: X)`
	// carry this tag so overrides don't pollute the other mode.
	ModeOnly Mode
}

// ParsedFile is the parsed output of one Svelte file's `<style>` block(s).
type ParsedFile struct {
	Path  string
	Rules []Rule
}

// --- app.css parsing ---

// ParseAppCSS extracts --color-* and related design tokens from app.css for
// both light and dark modes.
func ParseAppCSS(content string) *VarTable {
	content = stripComments(content)
	table := NewVarTable()

	// Pull the dark-mode block out first so we don't accidentally eat it into
	// the light-mode pass.
	darkBody, darkStart, darkEnd := extractDarkModeBlock(content)
	if darkStart >= 0 {
		darkRoot := extractRootBlock(darkBody)
		extractVarDefs(darkRoot, table.Dark)

		// Replace the dark block with an equivalent number of spaces so that
		// line numbers don't shift (not strictly needed, but tidy) and so the
		// next pass doesn't see it.
		content = content[:darkStart] + strings.Repeat(" ", darkEnd-darkStart) + content[darkEnd:]
	}

	// Now parse all `:root { ... }` blocks remaining (light mode).
	for {
		body, start, end := findNextRootBlock(content)
		if start < 0 {
			break
		}
		extractVarDefs(body, table.Light)
		content = content[:start] + strings.Repeat(" ", end-start) + content[end:]
	}

	return table
}

func stripComments(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	i := 0
	for i < len(s) {
		if i+1 < len(s) && s[i] == '/' && s[i+1] == '*' {
			end := strings.Index(s[i+2:], "*/")
			if end < 0 {
				break
			}
			i += end + 4
			continue
		}
		b.WriteByte(s[i])
		i++
	}
	return b.String()
}

// extractDarkModeBlock locates `@media (prefers-color-scheme: dark) { ... }`
// and returns its inner body + the [start, end) byte range of the full block
// (including the `@media ... { ... }` braces).
func extractDarkModeBlock(content string) (string, int, int) {
	re := regexp.MustCompile(`@media\s*\(\s*prefers-color-scheme\s*:\s*dark\s*\)\s*\{`)
	loc := re.FindStringIndex(content)
	if loc == nil {
		return "", -1, -1
	}
	bodyStart := loc[1]
	bodyEnd := matchingBrace(content, bodyStart-1)
	if bodyEnd < 0 {
		return "", -1, -1
	}
	return content[bodyStart:bodyEnd], loc[0], bodyEnd + 1
}

// extractRootBlock returns the inside of the first `:root { ... }` in s.
func extractRootBlock(s string) string {
	body, _, _ := findNextRootBlock(s)
	return body
}

func findNextRootBlock(s string) (string, int, int) {
	re := regexp.MustCompile(`:root\s*\{`)
	loc := re.FindStringIndex(s)
	if loc == nil {
		return "", -1, -1
	}
	bodyStart := loc[1]
	bodyEnd := matchingBrace(s, bodyStart-1)
	if bodyEnd < 0 {
		return "", -1, -1
	}
	return s[bodyStart:bodyEnd], loc[0], bodyEnd + 1
}

// matchingBrace returns the index of the `}` matching the `{` at openIdx.
func matchingBrace(s string, openIdx int) int {
	if openIdx >= len(s) || s[openIdx] != '{' {
		return -1
	}
	depth := 1
	for i := openIdx + 1; i < len(s); i++ {
		switch s[i] {
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return i
			}
		}
	}
	return -1
}

// extractVarDefs scans a block body for `--name: value;` pairs (at depth 0)
// and writes them into `out`. Nested blocks (like keyframes) are skipped.
func extractVarDefs(body string, out map[string]string) {
	// We do a depth-aware scan so we don't pull vars from nested `@media`
	// queries inside :root (rare but possible).
	depth := 0
	i := 0
	for i < len(body) {
		c := body[i]
		switch {
		case c == '{':
			depth++
			i++
		case c == '}':
			if depth > 0 {
				depth--
			}
			i++
		case depth == 0 && c == '-' && i+1 < len(body) && body[i+1] == '-':
			// Custom property definition.
			end := strings.IndexByte(body[i:], ';')
			if end < 0 {
				end = len(body) - i
			}
			decl := body[i : i+end]
			colon := strings.IndexByte(decl, ':')
			if colon > 2 {
				name := strings.TrimSpace(decl[:colon])
				value := strings.TrimSpace(decl[colon+1:])
				if rest, ok := strings.CutPrefix(name, "--"); ok {
					out[rest] = value
				}
			}
			i += end + 1
		default:
			i++
		}
	}
}

// --- Svelte `<style>` block parsing ---

// ParseSvelteFile extracts all rules-of-interest from a Svelte file.
func ParseSvelteFile(path, content string) *ParsedFile {
	out := &ParsedFile{Path: path}

	styleBlocks := findStyleBlocks(content)
	for _, sb := range styleBlocks {
		clean := stripComments(sb.body)
		rules := parseRules(path, sb.lineOffset, clean)
		out.Rules = append(out.Rules, rules...)
	}
	return out
}

type styleBlock struct {
	body       string
	lineOffset int // 1-indexed line number of the first byte of `body`
}

func findStyleBlocks(content string) []styleBlock {
	var out []styleBlock
	i := 0
	for {
		start := strings.Index(content[i:], "<style")
		if start < 0 {
			return out
		}
		start += i
		tagEnd := strings.Index(content[start:], ">")
		if tagEnd < 0 {
			return out
		}
		bodyStart := start + tagEnd + 1
		end := strings.Index(content[bodyStart:], "</style>")
		if end < 0 {
			return out
		}
		bodyEnd := bodyStart + end
		out = append(out, styleBlock{
			body:       content[bodyStart:bodyEnd],
			lineOffset: 1 + strings.Count(content[:bodyStart], "\n"),
		})
		i = bodyEnd + len("</style>")
	}
}

// parseRules walks a CSS body and returns one Rule per top-level selector block.
// Keyframes and @supports wrappers are flattened; @media blocks are descended
// into, and their rules inherit the modeFilter of their wrapper (so a dark
// @media's rules only apply in dark mode).
func parseRules(file string, lineOffset int, body string) []Rule {
	return parseRulesInner(file, lineOffset, body, "")
}

func parseRulesInner(file string, lineOffset int, body string, modeFilter Mode) []Rule {
	var rules []Rule
	i := 0
	for i < len(body) {
		for i < len(body) && isSpace(body[i]) {
			i++
		}
		if i >= len(body) {
			break
		}
		if body[i] == '@' {
			advanced, nested, ok := handleAtRule(file, lineOffset, body, i, modeFilter)
			if !ok {
				return rules
			}
			rules = append(rules, nested...)
			i = advanced
			continue
		}
		advanced, blockRules, ok := handleSelectorBlock(file, lineOffset, body, i, modeFilter)
		if !ok {
			return rules
		}
		rules = append(rules, blockRules...)
		i = advanced
	}
	return rules
}

// handleAtRule processes an at-rule starting at body[i]. Returns the advanced
// index, rules emitted from inside it (if we descend), and ok=false if the
// input is malformed.
func handleAtRule(file string, lineOffset int, body string, i int, modeFilter Mode) (int, []Rule, bool) {
	atEnd := strings.IndexAny(body[i:], "{;")
	if atEnd < 0 {
		return 0, nil, false
	}
	atRule := strings.TrimSpace(body[i : i+atEnd])
	if body[i+atEnd] == ';' {
		return i + atEnd + 1, nil, true
	}
	blockOpen := i + atEnd
	blockClose := matchingBrace(body, blockOpen)
	if blockClose < 0 {
		return 0, nil, false
	}
	lower := strings.ToLower(atRule)
	nestedOffset := lineOffset + strings.Count(body[:blockOpen+1], "\n")
	inner := body[blockOpen+1 : blockClose]
	switch {
	case strings.HasPrefix(lower, "@media"):
		filter := mediaModeFilter(lower, modeFilter)
		return blockClose + 1, parseRulesInner(file, nestedOffset, inner, filter), true
	case strings.HasPrefix(lower, "@supports"), strings.HasPrefix(lower, "@layer"):
		return blockClose + 1, parseRulesInner(file, nestedOffset, inner, modeFilter), true
	}
	// @keyframes / unknown: skip body.
	return blockClose + 1, nil, true
}

// mediaModeFilter chooses the mode filter for an @media block.
func mediaModeFilter(lowerAtRule string, parent Mode) Mode {
	if !strings.Contains(lowerAtRule, "prefers-color-scheme") {
		return parent
	}
	if strings.Contains(lowerAtRule, "dark") {
		return ModeDark
	}
	if strings.Contains(lowerAtRule, "light") {
		return ModeLight
	}
	return parent
}

func handleSelectorBlock(file string, lineOffset int, body string, i int, modeFilter Mode) (int, []Rule, bool) {
	blockOpen := strings.IndexByte(body[i:], '{')
	if blockOpen < 0 {
		return 0, nil, false
	}
	blockOpen += i
	selector := collapseWhitespace(strings.TrimSpace(body[i:blockOpen]))
	blockClose := matchingBrace(body, blockOpen)
	if blockClose < 0 {
		return 0, nil, false
	}
	decls := body[blockOpen+1 : blockClose]
	selLine := lineOffset + strings.Count(body[:blockOpen], "\n")

	var out []Rule
	for _, singleSel := range splitSelectorList(selector) {
		rule := buildRule(file, selLine, singleSel, decls)
		if rule != nil {
			rule.ModeOnly = modeFilter
			out = append(out, *rule)
		}
	}
	return blockClose + 1, out, true
}

func splitSelectorList(sel string) []string {
	// Simple split on commas outside of parens/brackets.
	var out []string
	depth := 0
	start := 0
	for i := 0; i < len(sel); i++ {
		switch sel[i] {
		case '(', '[':
			depth++
		case ')', ']':
			if depth > 0 {
				depth--
			}
		case ',':
			if depth == 0 {
				out = append(out, strings.TrimSpace(sel[start:i]))
				start = i + 1
			}
		}
	}
	out = append(out, strings.TrimSpace(sel[start:]))
	return out
}

// buildRule constructs a Rule from one selector and its declarations.
// Returns nil if the selector doesn't name any class we can track.
func buildRule(file string, line int, sel, decls string) *Rule {
	classes, pseudo := extractClassesAndPseudo(sel)
	if len(classes) == 0 {
		return nil
	}
	// For v1 we only handle ::placeholder. Rules with other pseudo-elements
	// (::before, ::after, ::selection, ...) are skipped.
	if pseudo != "" && pseudo != "placeholder" {
		return nil
	}

	r := &Rule{
		File:     file,
		Line:     line,
		Selector: sel,
		Classes:  classes,
		Pseudo:   pseudo,
	}

	for _, d := range splitDeclarations(decls) {
		prop, val := splitDecl(d)
		if prop == "" {
			continue
		}
		switch prop {
		case "color":
			r.Color = val
		case "background":
			r.Background = val
		case "background-color":
			r.Background = val
			r.HasBackgroundColorProp = true
		case "font-size":
			r.FontSizePx = parseFontSizePx(val)
		case "font-weight":
			r.FontWeight = parseFontWeight(val)
		}
	}
	return r
}

// extractClassesAndPseudo pulls the set of classes referenced by the LAST
// compound selector (the element we're actually styling), plus any
// pseudo-element suffix.
//
// Examples:
//
//	.foo.bar                 -> classes=[foo bar], pseudo=""
//	.wrap .baz               -> classes=[baz], pseudo=""       (we style .baz)
//	.name-input::placeholder -> classes=[name-input], pseudo="placeholder"
//	button.primary           -> classes=[primary], pseudo=""
func extractClassesAndPseudo(sel string) ([]string, string) {
	// Split into compound selectors (last wins for our purposes).
	parts := splitCombinators(sel)
	if len(parts) == 0 {
		return nil, ""
	}
	last := parts[len(parts)-1]

	pseudo := ""
	if idx := strings.Index(last, "::"); idx >= 0 {
		pseudoName := strings.TrimSpace(last[idx+2:])
		// Strip any trailing `(` args.
		if p := strings.IndexAny(pseudoName, "( \t"); p >= 0 {
			pseudoName = pseudoName[:p]
		}
		pseudo = strings.ToLower(pseudoName)
		last = last[:idx]
	}
	// Strip single-colon pseudo-classes like `:hover`, `:focus`, `:not(...)`.
	if idx := strings.IndexByte(last, ':'); idx >= 0 {
		last = last[:idx]
	}

	re := regexp.MustCompile(`\.([a-zA-Z_][a-zA-Z0-9_-]*)`)
	var classes []string
	for _, m := range re.FindAllStringSubmatch(last, -1) {
		classes = append(classes, m[1])
	}
	return classes, pseudo
}

// splitCombinators splits a compound selector chain by descendant / child /
// sibling combinators and returns each segment trimmed.
func splitCombinators(sel string) []string {
	// Replace > + ~ with spaces for a simple whitespace split.
	replaced := sel
	for _, c := range []string{">", "+", "~"} {
		replaced = strings.ReplaceAll(replaced, c, " ")
	}
	fields := strings.Fields(replaced)
	return fields
}

// splitDeclarations splits a CSS declaration block body on `;`, but respects
// nested parens (for example, `color-mix(in srgb, ..., ...)`).
func splitDeclarations(body string) []string {
	var out []string
	depth := 0
	start := 0
	for i := 0; i < len(body); i++ {
		switch body[i] {
		case '(':
			depth++
		case ')':
			if depth > 0 {
				depth--
			}
		case ';':
			if depth == 0 {
				out = append(out, body[start:i])
				start = i + 1
			}
		}
	}
	if start < len(body) {
		out = append(out, body[start:])
	}
	return out
}

func splitDecl(d string) (string, string) {
	d = strings.TrimSpace(d)
	if d == "" {
		return "", ""
	}
	colon := strings.IndexByte(d, ':')
	if colon < 0 {
		return "", ""
	}
	prop := strings.ToLower(strings.TrimSpace(d[:colon]))
	val := strings.TrimSpace(d[colon+1:])
	// Strip `!important` — doesn't affect the value.
	if idx := strings.LastIndex(strings.ToLower(val), "!important"); idx >= 0 {
		val = strings.TrimSpace(val[:idx])
	}
	return prop, val
}

func parseFontSizePx(v string) float64 {
	v = strings.TrimSpace(v)
	// If it's a var(), we can't resolve here — we rely on well-known tokens.
	// Many rules use the tokens directly; we handle them by name.
	if strings.HasPrefix(v, "var(") {
		return fontSizeFromVar(v)
	}
	if strings.HasSuffix(v, "px") {
		return atof(strings.TrimSuffix(v, "px"))
	}
	if strings.HasSuffix(v, "rem") {
		return 16 * atof(strings.TrimSuffix(v, "rem"))
	}
	if strings.HasSuffix(v, "em") {
		return 16 * atof(strings.TrimSuffix(v, "em"))
	}
	return 0
}

func fontSizeFromVar(v string) float64 {
	open := strings.Index(v, "(")
	close := strings.LastIndex(v, ")")
	if open < 0 || close <= open {
		return 0
	}
	inner := strings.TrimSpace(v[open+1 : close])
	name := strings.TrimSpace(inner)
	if comma := strings.IndexByte(name, ','); comma >= 0 {
		name = strings.TrimSpace(name[:comma])
	}
	// These values must stay in sync with app.css `--font-size-*` tokens.
	switch name {
	case "--font-size-xs":
		return 10
	case "--font-size-sm":
		return 12
	case "--font-size-md":
		return 14
	case "--font-size-lg":
		return 16
	case "--font-size-xl":
		return 20
	}
	return 0
}

func parseFontWeight(v string) int {
	v = strings.TrimSpace(strings.ToLower(v))
	switch v {
	case "bold", "bolder":
		return 700
	case "normal", "lighter":
		return 400
	}
	n, err := strconvAtoi(v)
	if err != nil {
		return 0
	}
	return n
}

// --- small helpers ---

func isSpace(c byte) bool {
	return c == ' ' || c == '\t' || c == '\n' || c == '\r'
}

func collapseWhitespace(s string) string {
	var b strings.Builder
	prevSpace := false
	for i := 0; i < len(s); i++ {
		if isSpace(s[i]) {
			if !prevSpace {
				b.WriteByte(' ')
			}
			prevSpace = true
		} else {
			b.WriteByte(s[i])
			prevSpace = false
		}
	}
	return strings.TrimSpace(b.String())
}

func atof(s string) float64 {
	s = strings.TrimSpace(s)
	var f float64
	_, err := fmt.Sscanf(s, "%f", &f)
	if err != nil {
		return 0
	}
	return f
}

func strconvAtoi(s string) (int, error) {
	var n int
	_, err := fmt.Sscanf(strings.TrimSpace(s), "%d", &n)
	return n, err
}
