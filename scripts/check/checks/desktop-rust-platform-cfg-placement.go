package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// The platform-seam convention (`docs/guides/platform-seams.md`) says two things this check
// can see in source:
//
//   - An arm names the OSes it's for. A NEGATED OS predicate (`not(target_os = "macos")`,
//     `not(unix)`, `not(any(target_os = "macos", target_os = "linux"))`) is an arm for "every
//     other OS", which today silently means Linux and tomorrow silently means Windows too, with
//     whatever answer the Linux code happened to give. Refused everywhere in production code,
//     seams included.
//   - The platform split lives in a seam: a `platform/` directory whose `mod.rs` picks one arm
//     per OS and pins its signatures. An OS cfg anywhere else is business logic deciding the
//     platform for itself.
//
// Both are counted per file against `platform-cfg-placement-allowlist.json`, seeded at the
// convention's introduction, so the numbers only go down. Test code is out of jurisdiction:
// a test of one OS's behavior is allowed to say so.

// platformCfgAllowlistRelPath is where the per-file budget lives.
const platformCfgAllowlistRelPath = "scripts/check/checks/platform-cfg-placement-allowlist.json"

// platformCfgCounts is one file's tally, and also its allowlist entry: the number of
// negated OS predicates, and the number of OS cfgs outside a `platform/` seam. A site
// that's both counts in both, so each later refactor visibly moves its own number.
type platformCfgCounts struct {
	Negated     int `json:"negated,omitempty"`
	OutsideSeam int `json:"outsideSeam,omitempty"`
}

func (c platformCfgCounts) isZero() bool { return c.Negated == 0 && c.OutsideSeam == 0 }

// platformCfgAllowlist is the on-disk shape of the allowlist, keyed on the repo-relative
// file. A per-site key would move on every edit above the site; a per-file count only moves
// when somebody adds or removes a platform cfg.
type platformCfgAllowlist struct {
	Comment string                       `json:"$comment,omitempty"`
	Files   map[string]platformCfgCounts `json:"files"`
}

// platformCfgSite is one counted cfg.
type platformCfgSite struct {
	relPath     string
	line        int
	predicate   string
	negated     bool
	outsideSeam bool
}

// platformCfgOpener finds each `cfg(`, `cfg_attr(`, and `cfg!(` in masked source. The
// leading boundary keeps `my_cfg(` and `cfg_aliases!(` out.
var platformCfgOpener = regexp.MustCompile(`(^|[^A-Za-z0-9_])(cfg_attr|cfg)\s*(!\s*)?\(`)

// RunPlatformCfgPlacement counts every OS cfg in production Rust code, app and crates alike,
// and fails when a file holds more negated predicates, or more cfgs outside a `platform/`
// seam, than its allowlist entry. Local runs shrink-wrap the allowlist.
func RunPlatformCfgPlacement(ctx *CheckContext) (CheckResult, error) {
	roots, err := ScannerRoots(ctx.RootDir, "desktop-rust-platform-cfg-placement")
	if err != nil {
		return CheckResult{}, err
	}

	var sites []platformCfgSite
	scanned := 0
	for _, root := range roots {
		rootSites, rootScanned, scanErr := scanForPlatformCfgs(ctx.RootDir, root)
		if scanErr != nil {
			return CheckResult{}, fmt.Errorf("failed to scan Rust files: %w", scanErr)
		}
		sites = append(sites, rootSites...)
		scanned += rootScanned
	}
	counts := countPlatformCfgsPerFile(sites)

	allowlist := loadPlatformCfgAllowlist(ctx.RootDir)
	staleChanges := shrinkwrapPlatformCfgAllowlist(&allowlist, counts)
	if len(staleChanges) > 0 && !ctx.CI {
		if err := writeJSONAllowlist(filepath.Join(ctx.RootDir, platformCfgAllowlistRelPath), allowlist); err != nil {
			return CheckResult{}, err
		}
		reformatWithOxfmt(ctx.RootDir, platformCfgAllowlistRelPath)
	}
	staleMsg := formatPlatformCfgStaleness(staleChanges, ctx.CI)

	if over := platformCfgsOverBudget(sites, counts, allowlist); len(over) > 0 {
		parts := appendNonEmpty([]string{formatPlatformCfgsOverBudget(over, counts, allowlist)}, staleMsg)
		return CheckResult{}, fmt.Errorf("%s", strings.Join(parts, "\n"))
	}

	var total platformCfgCounts
	for _, c := range allowlist.Files {
		total.Negated += c.Negated
		total.OutsideSeam += c.OutsideSeam
	}
	okMsg := fmt.Sprintf(
		"%d Rust %s scanned, no new platform cfg outside a seam (%d negated and %d unseamed allowlisted across %d %s)",
		scanned, Pluralize(scanned, "file", "files"),
		total.Negated, total.OutsideSeam, len(allowlist.Files), Pluralize(len(allowlist.Files), "file", "files"),
	)
	if staleMsg != "" {
		return SuccessWithChanges(okMsg + "; " + staleMsg), nil
	}
	return Success(okMsg), nil
}

// scanForPlatformCfgs returns every counted site under one source root, and how many
// production files it read.
func scanForPlatformCfgs(rootDir, srcDir string) ([]platformCfgSite, int, error) {
	var sites []platformCfgSite
	scanned := 0
	err := corpus.WalkDir(srcDir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() || !strings.HasSuffix(d.Name(), ".rs") || isRustTestFile(path) {
			return nil
		}
		source, readErr := corpus.ReadFile(path)
		if readErr != nil {
			return readErr
		}
		scanned++
		relPath, relErr := filepath.Rel(rootDir, path)
		if relErr != nil {
			relPath = path
		}
		relPath = filepath.ToSlash(relPath)
		sites = append(sites, platformCfgSitesIn(relPath, source)...)
		return nil
	})
	return sites, scanned, err
}

// platformCfgSitesIn finds the counted cfgs in one file's source. Comments and string
// bodies are masked first so prose can't produce a site; the predicate text is then read
// from the ORIGINAL source at the same offsets, since masking blanks the `"macos"` too.
func platformCfgSitesIn(relPath, source string) []platformCfgSite {
	// Most files name no cfg at all; skip the lexing for them.
	if !strings.Contains(source, "cfg") {
		return nil
	}
	masked, _ := maskRustSource(source)
	cfgs := parsedCfgsIn(masked, source)
	testRegions, wholeFileIsTest := testOnlyRegions(masked, cfgs)
	if wholeFileIsTest {
		return nil
	}

	seam := isPlatformSeamPath(relPath)
	lineStarts := lineStartOffsets(source)
	var sites []platformCfgSite
	for _, c := range cfgs {
		if !c.node.mentionsOS() || c.node.isTestOnly() || inRegions(testRegions, c.start) {
			continue
		}
		site := platformCfgSite{
			relPath:     relPath,
			line:        lineForOffset(lineStarts, c.start),
			predicate:   c.kind + "(" + c.predicate + ")",
			negated:     c.node.hasNegatedOS(),
			outsideSeam: !seam,
		}
		if site.negated || site.outsideSeam {
			sites = append(sites, site)
		}
	}
	return sites
}

// parsedCfg is one `cfg(`, `cfg_attr(`, or `cfg!(` with a predicate the parser could read.
// start is the offset of the `cfg` word, close that of the closing parenthesis.
type parsedCfg struct {
	start, close int
	kind         string
	attr, inner  bool
	predicate    string
	node         *cfgNode
}

func parsedCfgsIn(masked, source string) []parsedCfg {
	var out []parsedCfg
	for _, m := range platformCfgOpener.FindAllStringSubmatchIndex(masked, -1) {
		open := m[1] - 1
		closeAt := matchDelim(masked, open, '(', ')')
		if closeAt < 0 {
			continue
		}
		kind := masked[m[4]:m[5]]
		body := source[open+1 : closeAt]
		if kind == "cfg_attr" {
			body = firstTopLevelArg(body)
		}
		node := parseCfgPredicate(body)
		if node == nil {
			continue
		}
		attr, inner := attributeBefore(masked, m[4])
		out = append(out, parsedCfg{
			start: m[4], close: closeAt, kind: kind, attr: attr, inner: inner,
			predicate: strings.Join(strings.Fields(body), " "), node: node,
		})
	}
	return out
}

// testOnlyRegions returns the test items of a file: `findRustTestRegions`' plus every item
// behind a test-only gate (`all(test, …)`), which is test code exactly like `#[cfg(test)]`.
// An inner test-only gate (`#![cfg(test)]`) makes the whole file test code.
func testOnlyRegions(masked string, cfgs []parsedCfg) ([][2]int, bool) {
	regions := findRustTestRegions(masked)
	for _, c := range cfgs {
		if c.kind != "cfg" || !c.attr || !c.node.isTestOnly() {
			continue
		}
		if c.inner {
			return nil, true
		}
		if open := testItemBodyBrace(masked, c.close+1); open >= 0 {
			if end := matchBrace(masked, open); end >= 0 {
				regions = append(regions, [2]int{c.start, end})
			}
		}
	}
	return regions, false
}

// attributeBefore reports whether the `cfg` at start sits in an attribute (`#[` or `#![`
// right before it, whitespace aside) and whether that attribute is an inner one.
func attributeBefore(masked string, start int) (attr, inner bool) {
	i := start - 1
	for i >= 0 && (masked[i] == ' ' || masked[i] == '\t' || masked[i] == '\n' || masked[i] == '\r') {
		i--
	}
	if i < 1 || masked[i] != '[' {
		return false, false
	}
	if masked[i-1] == '#' {
		return true, false
	}
	return i >= 2 && masked[i-1] == '!' && masked[i-2] == '#', true
}

// isPlatformSeamPath reports whether a file sits in a platform seam: any directory named
// exactly `platform` on its path.
func isPlatformSeamPath(relPath string) bool {
	segments := strings.Split(relPath, "/")
	for _, s := range segments[:len(segments)-1] {
		if s == "platform" {
			return true
		}
	}
	return false
}

// firstTopLevelArg returns a `cfg_attr`'s predicate: everything before its first top-level
// comma.
func firstTopLevelArg(body string) string {
	depth := 0
	inString := false
	for i := 0; i < len(body); i++ {
		switch c := body[i]; {
		case inString:
			if c == '\\' {
				i++
			} else if c == '"' {
				inString = false
			}
		case c == '"':
			inString = true
		case c == '(':
			depth++
		case c == ')':
			depth--
		case c == ',' && depth == 0:
			return body[:i]
		}
	}
	return body
}

// ── The cfg predicate ────────────────────────────────────────────────

// cfgNode is one node of a parsed cfg predicate: an `all` / `any` / `not` with children,
// or a leaf (`unix`, `target_os = "macos"`).
type cfgNode struct {
	name     string
	value    string
	hasValue bool
	children []*cfgNode
}

// isOSLeaf reports whether a leaf chooses an operating system. `target_family = "wasm"` is
// deliberately not one: it chooses between the desktop and the browser build, a feature
// boundary rather than an OS arm.
func (n *cfgNode) isOSLeaf() bool {
	if n.children != nil {
		return false
	}
	switch n.name {
	case "unix", "windows":
		return !n.hasValue
	case "target_os", "target_vendor":
		return n.hasValue
	case "target_family":
		return n.hasValue && (n.value == "unix" || n.value == "windows")
	}
	return false
}

func (n *cfgNode) isCombinator() bool {
	return n.children != nil && (n.name == "all" || n.name == "any" || n.name == "not")
}

func (n *cfgNode) mentionsOS() bool {
	if n.isOSLeaf() {
		return true
	}
	for _, c := range n.children {
		if c.mentionsOS() {
			return true
		}
	}
	return false
}

// hasNegatedOS reports whether any `not(…)` in the predicate wraps an OS choice.
func (n *cfgNode) hasNegatedOS() bool {
	if n.name == "not" && n.isCombinator() {
		for _, c := range n.children {
			if c.mentionsOS() {
				return true
			}
		}
	}
	for _, c := range n.children {
		if c.hasNegatedOS() {
			return true
		}
	}
	return false
}

// isTestOnly reports whether the predicate can only hold in a test build: `test` itself,
// or an `all(…)` with `test` as one of its terms.
func (n *cfgNode) isTestOnly() bool {
	if n.children == nil {
		return n.name == "test" && !n.hasValue
	}
	if n.name != "all" {
		return false
	}
	for _, c := range n.children {
		if c.isTestOnly() {
			return true
		}
	}
	return false
}

// parseCfgPredicate parses a cfg predicate, or returns nil for anything it can't read
// whole: a shape the parser doesn't know is left alone rather than guessed at.
func parseCfgPredicate(src string) *cfgNode {
	p := &cfgParser{tokens: tokenizeCfg(src)}
	node := p.parse()
	if node == nil || p.pos != len(p.tokens) {
		return nil
	}
	return node
}

type cfgParser struct {
	tokens []string
	pos    int
}

func (p *cfgParser) peek() string {
	if p.pos < len(p.tokens) {
		return p.tokens[p.pos]
	}
	return ""
}

func (p *cfgParser) parse() *cfgNode {
	name := p.peek()
	if name == "" || !isCfgIdent(name) {
		return nil
	}
	p.pos++
	node := &cfgNode{name: name}
	switch p.peek() {
	case "=":
		p.pos++
		value := p.peek()
		if !strings.HasPrefix(value, `"`) {
			return nil
		}
		p.pos++
		node.value = strings.Trim(value, `"`)
		node.hasValue = true
	case "(":
		p.pos++
		node.children = []*cfgNode{}
		for p.peek() != ")" {
			child := p.parse()
			if child == nil {
				return nil
			}
			node.children = append(node.children, child)
			if p.peek() == "," {
				p.pos++
			} else if p.peek() != ")" {
				return nil
			}
		}
		p.pos++
	}
	return node
}

func isCfgIdent(tok string) bool {
	for i := 0; i < len(tok); i++ {
		if !isRustIdentByte(tok[i]) {
			return false
		}
	}
	return tok != ""
}

// tokenizeCfg splits a predicate into identifiers, string literals (quotes kept), and the
// punctuation `(`, `)`, `,`, `=`. Anything else (`::` in a path-shaped predicate, a stray
// symbol, an unterminated string) ends the list with a token the parser rejects, which
// leaves the whole predicate alone.
func tokenizeCfg(src string) []string {
	var tokens []string
	for i := 0; i < len(src); {
		c := src[i]
		next := i + 1
		switch {
		case c == ' ' || c == '\t' || c == '\n' || c == '\r':
			i = next
			continue
		case c == '"':
			next = endOfCfgString(src, i)
			if next < 0 {
				return append(tokens, src[i:])
			}
		case isRustIdentByte(c):
			for next < len(src) && isRustIdentByte(src[next]) {
				next++
			}
		case !strings.ContainsRune("(),=", rune(c)):
			return append(tokens, string(c))
		}
		tokens = append(tokens, src[i:next])
		i = next
	}
	return tokens
}

// endOfCfgString returns the offset just past the string literal opening at i, or -1 when
// it never closes.
func endOfCfgString(src string, i int) int {
	for j := i + 1; j < len(src); j++ {
		switch src[j] {
		case '\\':
			j++
		case '"':
			return j + 1
		}
	}
	return -1
}

// ── The allowlist ────────────────────────────────────────────────────

func countPlatformCfgsPerFile(sites []platformCfgSite) map[string]platformCfgCounts {
	counts := map[string]platformCfgCounts{}
	for _, s := range sites {
		c := counts[s.relPath]
		if s.negated {
			c.Negated++
		}
		if s.outsideSeam {
			c.OutsideSeam++
		}
		counts[s.relPath] = c
	}
	return counts
}

// loadPlatformCfgAllowlist reads the budget. A missing or unparsable file yields an empty
// allowlist, so every counted site gets reported.
func loadPlatformCfgAllowlist(rootDir string) platformCfgAllowlist {
	list := platformCfgAllowlist{Files: map[string]platformCfgCounts{}}
	data, err := os.ReadFile(filepath.Join(rootDir, platformCfgAllowlistRelPath))
	if err != nil {
		return list
	}
	if err := json.Unmarshal(data, &list); err != nil || list.Files == nil {
		return platformCfgAllowlist{Files: map[string]platformCfgCounts{}}
	}
	return list
}

// shrinkwrapPlatformCfgAllowlist lowers each entry to the file's current counts and drops
// a file that reaches zero. No slack buffer: the numbers only move when somebody adds or
// removes a platform cfg, so there's no drift to absorb.
func shrinkwrapPlatformCfgAllowlist(list *platformCfgAllowlist, counts map[string]platformCfgCounts) []string {
	var changes []string
	for _, path := range sortedKeys(list.Files) {
		allowed := list.Files[path]
		current := counts[path]
		next := platformCfgCounts{
			Negated:     min(allowed.Negated, current.Negated),
			OutsideSeam: min(allowed.OutsideSeam, current.OutsideSeam),
		}
		switch {
		case next.isZero():
			delete(list.Files, path)
			changes = append(changes, fmt.Sprintf("removed %s (no platform cfgs left to allow)", path))
		case next != allowed:
			list.Files[path] = next
			changes = append(changes, fmt.Sprintf("ratcheted %s: %s → %s", path, allowed, next))
		}
	}
	return changes
}

func formatPlatformCfgStaleness(changes []string, ci bool) string {
	if len(changes) == 0 {
		return ""
	}
	verb := "Shrink-wrapped allowlist"
	if ci {
		verb = "Stale allowlist entries (a local run shrink-wraps them)"
	}
	return fmt.Sprintf("%s:\n  - %s", verb, strings.Join(changes, "\n  - "))
}

func (c platformCfgCounts) String() string {
	return fmt.Sprintf("%d negated, %d outside a seam", c.Negated, c.OutsideSeam)
}

// platformCfgsOverBudget returns the sites of every file over its entry in either count. A
// file absent from the allowlist has a budget of zero.
func platformCfgsOverBudget(sites []platformCfgSite, counts map[string]platformCfgCounts, list platformCfgAllowlist) []platformCfgSite {
	var out []platformCfgSite
	for _, s := range sites {
		c, allowed := counts[s.relPath], list.Files[s.relPath]
		if (s.negated && c.Negated > allowed.Negated) || (s.outsideSeam && c.OutsideSeam > allowed.OutsideSeam) {
			out = append(out, s)
		}
	}
	return out
}

// formatPlatformCfgsOverBudget lists each over-budget file, then every site that counts
// against the exceeded number: the budget is per file, so which site to move is the
// reader's pick.
func formatPlatformCfgsOverBudget(sites []platformCfgSite, counts map[string]platformCfgCounts, list platformCfgAllowlist) string {
	byFile := map[string][]platformCfgSite{}
	for _, s := range sites {
		byFile[s.relPath] = append(byFile[s.relPath], s)
	}
	var sb strings.Builder
	for _, relPath := range sortedKeys(byFile) {
		fileSites := byFile[relPath]
		sort.Slice(fileSites, func(i, j int) bool { return fileSites[i].line < fileSites[j].line })
		sb.WriteString(fmt.Sprintf("  - %s: %s (allowlist: %s)\n", relPath, counts[relPath], list.Files[relPath]))
		for _, s := range fileSites {
			var why []string
			if s.negated {
				why = append(why, "negated")
			}
			if s.outsideSeam {
				why = append(why, "outside a seam")
			}
			sb.WriteString(fmt.Sprintf("      %s:%d [%s] %s\n", relPath, s.line, strings.Join(why, ", "), s.predicate))
		}
	}
	return fmt.Sprintf(
		"%d %s over budget for platform cfgs. Name the OSes an arm is for instead of negating one "+
			"(`target_os = \"linux\"` and `target_os = \"windows\"`, never `not(target_os = \"macos\")`), and put "+
			"the split behind a `<feature>/platform/` seam whose `mod.rs` picks one arm per OS "+
			"(docs/guides/platform-seams.md):\n%s",
		len(byFile), Pluralize(len(byFile), "file", "files"), strings.TrimRight(sb.String(), "\n"),
	)
}
