// Package main scans `.svelte` files for scoped CSS that restyles the
// canonical `<Button>` component (the `.btn` / `.btn-primary` / `.btn-secondary`
// / `.btn-danger` classes).
//
// Why: `<Button>` is the single source of truth for our button visuals
// (variant colors, focus ring, hover states, disabled opacity). Every primary
// button shares one accent-mixed color pair, so a contrast fix made in
// `Button.svelte` reaches every consumer. A scoped override in a feature
// component re-introduces the same problem we just paid to solve in one
// place — only this time silent, because the feature owner isn't usually
// thinking about contrast under macOS-Purple-accent dark mode.
//
// What it flags: any rule inside a `<style>` block (in any `.svelte` file
// other than `Button.svelte` itself) whose selector matches `.btn`,
// `.btn-primary`, `.btn-secondary`, or `.btn-danger` AND that sets `color`,
// `background`, or `background-color`. Layout-only overrides on `<Button>` via
// `:global(button)` (flex, width, margin, etc.) are fine and won't trip the
// check; only color/background restyles do.
//
// Opt-out: a comment `/* allowed-btn-restyle: <rationale> */` placed
// immediately before the rule. The rationale must be non-empty — empty
// allowlist comments defeat the purpose. The check prints any allowlisted
// rules at the end of a clean run so they remain visible.
package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

const (
	colorRed    = "\033[31m"
	colorGreen  = "\033[32m"
	colorYellow = "\033[33m"
	colorDim    = "\033[2m"
	colorReset  = "\033[0m"
)

// bannedProps are the properties that turn a `.btn-*` selector into a
// "restyle" rather than a layout tweak. Anything else (flex, width, padding,
// margin, font-size, etc.) is fine — feature components legitimately resize
// or align buttons.
var bannedProps = map[string]bool{
	"color":            true,
	"background":       true,
	"background-color": true,
}

// btnClasses are the canonical class names the `<Button>` component declares.
// Restyling any of these from a scoped feature component is the regression we
// want to catch.
var btnClasses = []string{"btn", "btn-primary", "btn-secondary", "btn-danger"}

// btnSelectorRE matches any of the canonical button classes as a CSS class
// reference. The negative lookbehind is approximated with a leading boundary
// so `.btn-mini`, `.btn-regular` (Button.svelte's size classes) and
// `.toggle-button` don't false-match.
//
// Class names are alphanumeric; the class ends at the first non-class char.
// We rely on the explicit list above to avoid `.btn-regular` matching.
var btnSelectorRE = func() *regexp.Regexp {
	parts := make([]string, len(btnClasses))
	for i, c := range btnClasses {
		parts[i] = regexp.QuoteMeta(c)
	}
	// Match `.` then one of the class names, with no name char following.
	return regexp.MustCompile(`\.(` + strings.Join(parts, "|") + `)(?:$|[^a-zA-Z0-9_-])`)
}()

// allowlistRE matches `/* allowed-btn-restyle: <rationale> */`.
var allowlistRE = regexp.MustCompile(`/\*\s*allowed-btn-restyle:\s*([^*]+?)\s*\*/`)

// Violation is one banned rule, ready to print.
type Violation struct {
	File     string // path relative to repo root
	Line     int    // selector line, 1-indexed
	Selector string
	Property string
	Value    string
}

// Allowed is one allowlisted rule (kept visible in the summary so reasons
// don't rot silently).
type Allowed struct {
	File      string
	Line      int
	Selector  string
	Rationale string
}

// Orphan is an `allowed-btn-restyle` comment that excused nothing: the rule it
// precedes doesn't touch a banned property on a canonical button class (or the
// rationale is empty, or the comment floats unattached). Stale comments must
// go, or they'll silently excuse a future restyle.
type Orphan struct {
	File string
	Line int
}

func main() {
	verbose := flag.Bool("verbose", false, "Show allowlisted rules and rationales")
	flag.Parse()

	rootDir, err := findRootDir()
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	violations, allowed, orphans, err := scanTree(rootDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%sError walking src: %v%s\n", colorRed, err, colorReset)
		os.Exit(1)
	}

	if len(violations) > 0 {
		printViolations(violations)
	}
	if len(orphans) > 0 {
		printOrphans(orphans)
	}
	if *verbose && len(allowed) > 0 {
		printAllowed(allowed)
	}

	if len(violations) > 0 || len(orphans) > 0 {
		os.Exit(1)
	}
	fmt.Printf("%s✅ No .btn-* restyles. (%d allowlisted, run with --verbose to list)%s\n", colorGreen, len(allowed), colorReset)
}

// scanTree walks every non-Button `.svelte` file under apps/desktop/src and
// aggregates the scan results, violations sorted by file and line.
func scanTree(rootDir string) ([]Violation, []Allowed, []Orphan, error) {
	srcDir := filepath.Join(rootDir, "apps", "desktop", "src")
	var violations []Violation
	var allowed []Allowed
	var orphans []Orphan

	err := filepath.Walk(srcDir, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if info.IsDir() || filepath.Ext(path) != ".svelte" {
			return nil
		}
		// `Button.svelte` IS the canonical source of these styles; skip it.
		if filepath.Base(path) == "Button.svelte" {
			return nil
		}
		content, readErr := os.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		v, a, o := scanFile(rootDir, path, string(content))
		violations = append(violations, v...)
		allowed = append(allowed, a...)
		orphans = append(orphans, o...)
		return nil
	})

	sort.SliceStable(violations, func(i, j int) bool {
		if violations[i].File != violations[j].File {
			return violations[i].File < violations[j].File
		}
		return violations[i].Line < violations[j].Line
	})
	return violations, allowed, orphans, err
}

func printViolations(violations []Violation) {
	fmt.Printf("%s=== .btn-* restyle violations ===%s\n", colorYellow, colorReset)
	for _, v := range violations {
		fmt.Printf(
			"  %s%s:%d%s  %s%s%s  sets %s: %s\n",
			colorRed, v.File, v.Line, colorReset,
			colorDim, v.Selector, colorReset,
			v.Property, trimToOneLine(v.Value),
		)
	}
	fmt.Println()
	fmt.Println("Each rule above changes `color`, `background`, or `background-color` on a")
	fmt.Println("canonical Button class (.btn / .btn-primary / .btn-secondary / .btn-danger)")
	fmt.Println("from a scoped `<style>` block. Restyling the shared component breaks the")
	fmt.Println("contrast guarantees Button.svelte ships with (notably under runtime macOS")
	fmt.Println("accent overrides). Either:")
	fmt.Println("  - drop the override and use Button's own variants, OR")
	fmt.Println("  - add `/* allowed-btn-restyle: <reason> */` on the line above with a")
	fmt.Println("    concrete written rationale.")
	fmt.Printf("%s❌ %d violation(s) across %d file(s).%s\n", colorRed, len(violations), countDistinctFiles(violations), colorReset)
}

func printOrphans(orphans []Orphan) {
	sort.SliceStable(orphans, func(i, j int) bool {
		if orphans[i].File != orphans[j].File {
			return orphans[i].File < orphans[j].File
		}
		return orphans[i].Line < orphans[j].Line
	})
	fmt.Printf("%s=== Unused allowed-btn-restyle comments ===%s\n", colorYellow, colorReset)
	for _, o := range orphans {
		fmt.Printf("  %s%s:%d%s\n", colorRed, o.File, o.Line, colorReset)
	}
	fmt.Println()
	fmt.Println("Each `/* allowed-btn-restyle: ... */` comment above excuses nothing: the rule")
	fmt.Println("it precedes doesn't set a banned property on a canonical button class (or the")
	fmt.Println("rationale is empty). Remove the stale comment so it can't silently excuse a")
	fmt.Println("future restyle.")
	fmt.Printf("%s❌ %d unused allowlist comment(s).%s\n", colorRed, len(orphans), colorReset)
}

func printAllowed(allowed []Allowed) {
	sort.SliceStable(allowed, func(i, j int) bool {
		if allowed[i].File != allowed[j].File {
			return allowed[i].File < allowed[j].File
		}
		return allowed[i].Line < allowed[j].Line
	})
	fmt.Printf("%s=== Allowlisted overrides ===%s\n", colorYellow, colorReset)
	for _, a := range allowed {
		fmt.Printf("  %s%s:%d%s  %s%s%s  — %s\n",
			colorDim, a.File, a.Line, colorReset,
			colorDim, a.Selector, colorReset,
			a.Rationale,
		)
	}
	fmt.Println()
}

func countDistinctFiles(violations []Violation) int {
	seen := map[string]bool{}
	for _, v := range violations {
		seen[v.File] = true
	}
	return len(seen)
}

// scanFile returns the violations, allowlisted rules, and orphaned allowlist
// comments from one `.svelte`.
//
// The path is converted to repo-relative before populating outputs.
func scanFile(rootDir, absPath, content string) ([]Violation, []Allowed, []Orphan) {
	relPath := relativeTo(rootDir, absPath)
	styleBlocks := findStyleBlocks(content)

	var vs []Violation
	var as []Allowed
	var os []Orphan
	for _, sb := range styleBlocks {
		blockVs, blockAs, blockOs := scanStyleBlock(relPath, sb.body, sb.lineOffset)
		vs = append(vs, blockVs...)
		as = append(as, blockAs...)
		os = append(os, blockOs...)
	}
	return vs, as, os
}

type styleBlock struct {
	body       string
	lineOffset int // 1-indexed line number of body[0]
}

// findStyleBlocks returns every `<style>...</style>` body in a Svelte file
// (Svelte allows multiple `<style>` blocks: one root, plus `<style module>`
// or `<style lang="...">`).
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

// scanStyleBlock walks the body of a `<style>` block, locates each top-level
// rule whose selector touches a canonical button class, and flags the rule
// if it sets a banned property. The allowlist comment is consumed from the
// gap immediately before the rule. Allowlist comments that excused nothing
// (wrong selector, layout-only rule, empty rationale, or floating free) come
// back as orphans.
//
// The walker uses a depth-aware brace scan so nested `@media`/`@supports`
// rules are descended into without confusing the matcher.
func scanStyleBlock(relPath, body string, lineOffset int) ([]Violation, []Allowed, []Orphan) {
	var vs []Violation
	var as []Allowed
	consumed := map[int]bool{} // body offsets of allowlist comments that excused a rule
	depth := 0
	selStart := 0
	for i := 0; i < len(body); i++ {
		switch body[i] {
		case '{':
			if depth == 0 {
				selectorRaw := strings.TrimSpace(body[selStart:i])
				selector := collapseWhitespace(stripComments(selectorRaw))
				// Skip `@at-rules` (we descend their body instead).
				if strings.HasPrefix(selector, "@") {
					selStart = i + 1
					depth++
					continue
				}
				end := matchingBrace(body, i)
				if end < 0 {
					return vs, as, collectOrphans(relPath, body, lineOffset, consumed)
				}
				ruleBody := body[i+1 : end]
				lineNo := lineOffset + strings.Count(body[:i], "\n")
				if btnSelectorRE.MatchString(selector) {
					rationale, matchStart := findPrecedingAllowlist(body[selStart:i])
					if rationale != "" {
						// Always record allowlisted rules touching banned
						// properties (so rationales stay visible). Layout-only
						// overrides aren't flagged at all, so they don't need
						// rationales.
						if rps := bannedPropDecls(ruleBody); len(rps) > 0 {
							as = append(as, Allowed{File: relPath, Line: lineNo, Selector: selector, Rationale: rationale})
							consumed[selStart+matchStart] = true
						}
					} else {
						for _, rp := range bannedPropDecls(ruleBody) {
							vs = append(vs, Violation{File: relPath, Line: lineNo, Selector: selector, Property: rp.prop, Value: rp.value})
						}
					}
				}
				// Skip past the rule body.
				i = end
				selStart = i + 1
				continue
			}
			depth++
		case '}':
			if depth > 0 {
				depth--
				if depth == 0 {
					selStart = i + 1
				}
			}
		case ';':
			if depth == 0 {
				// Bare declaration at block top-level (shouldn't really
				// happen in scoped CSS, but stay safe).
				selStart = i + 1
			}
		}
	}
	return vs, as, collectOrphans(relPath, body, lineOffset, consumed)
}

// collectOrphans returns every allowlist comment in the block whose start
// offset never got consumed by a banned-prop rule on a canonical button class.
func collectOrphans(relPath, body string, lineOffset int, consumed map[int]bool) []Orphan {
	var orphans []Orphan
	for _, m := range allowlistRE.FindAllStringIndex(body, -1) {
		if consumed[m[0]] {
			continue
		}
		orphans = append(orphans, Orphan{
			File: relPath,
			Line: lineOffset + strings.Count(body[:m[0]], "\n"),
		})
	}
	return orphans
}

type propDecl struct {
	prop  string
	value string
}

// bannedPropDecls returns every banned property declaration in a rule body.
// Multiple declarations of the same property are all returned; the check
// surfaces every one.
func bannedPropDecls(body string) []propDecl {
	var out []propDecl
	for _, decl := range splitDeclarations(body) {
		prop, val := splitDecl(decl)
		if prop == "" {
			continue
		}
		if bannedProps[prop] {
			out = append(out, propDecl{prop: prop, value: val})
		}
	}
	return out
}

// findPrecedingAllowlist scans the selector-prefix region (everything between
// the previous rule's `}` and this rule's `{`, which contains both the
// allowlist comment and the selector text) for the most recent
// `/* allowed-btn-restyle: <reason> */` comment. The comment must immediately
// precede the selector — only whitespace allowed between the closing `*/` and
// the first non-whitespace char of the selector — so an allowlist comment
// attached to a previous rule can't accidentally cover the next one.
//
// Returns the trimmed rationale plus the comment's start offset within
// prefix, or ("", -1) if no valid allowlist comment was found (or its
// rationale was empty, which we treat as no comment).
func findPrecedingAllowlist(prefix string) (string, int) {
	// Find the last allowlist comment in the prefix region.
	matches := allowlistRE.FindAllStringSubmatchIndex(prefix, -1)
	if len(matches) == 0 {
		return "", -1
	}
	last := matches[len(matches)-1]
	// last[1] is the byte index right after the closing `*/` of the
	// allowlist comment. Verify everything between there and end-of-prefix
	// is whitespace OR another comment + whitespace (so trailing layout
	// comments don't break attachment). The selector itself begins at the
	// first non-whitespace, non-comment byte AFTER our anchor and would
	// have been collapsed by collapseWhitespace; here we just sanity-check
	// there's no stray declaration or rule end.
	after := stripComments(prefix[last[1]:])
	for _, c := range after {
		if c != ' ' && c != '\t' && c != '\n' && c != '\r' {
			// Selector class chars are fine; declarations / braces are not.
			if c == '{' || c == '}' || c == ';' {
				return "", -1
			}
		}
	}
	rationale := strings.TrimSpace(prefix[last[2]:last[3]])
	if rationale == "" {
		return "", -1
	}
	return rationale, last[0]
}

// --- helpers below mirror scripts/check-a11y-contrast/parser.go in style. ---

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
	d = strings.TrimSpace(stripComments(d))
	if d == "" {
		return "", ""
	}
	colon := strings.IndexByte(d, ':')
	if colon < 0 {
		return "", ""
	}
	prop := strings.ToLower(strings.TrimSpace(d[:colon]))
	val := strings.TrimSpace(d[colon+1:])
	if idx := strings.LastIndex(strings.ToLower(val), "!important"); idx >= 0 {
		val = strings.TrimSpace(val[:idx])
	}
	return prop, val
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

func collapseWhitespace(s string) string {
	var b strings.Builder
	prevSpace := false
	for i := 0; i < len(s); i++ {
		c := s[i]
		if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
			if !prevSpace {
				b.WriteByte(' ')
			}
			prevSpace = true
		} else {
			b.WriteByte(c)
			prevSpace = false
		}
	}
	return strings.TrimSpace(b.String())
}

func trimToOneLine(s string) string {
	s = collapseWhitespace(s)
	if len(s) > 80 {
		s = s[:77] + "..."
	}
	return s
}

func relativeTo(root, abs string) string {
	if rel, ok := strings.CutPrefix(abs, root); ok {
		return strings.TrimPrefix(rel, "/")
	}
	return abs
}

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
