package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"unicode/utf8"
)

// Agent-facing docs are read by AI agents as a linear token stream, not a 2D
// layout, so markdown tables earn their keep only as genuine matrices. This
// check enforces two table-hygiene rules in that scope:
//
//  1. No two-column tables (and no three-column tables whose first column is a
//     pure sequential index, which are effectively two columns). The alignment
//     padding burns tokens on whitespace that helps nobody, and any edit reflows
//     every row's padding, causing spurious git merge conflicts. A bulleted list
//     (`- **Title**: details`) is cheaper to read, cheaper in tokens, and
//     diff-stable. There's no allowlist: a two-column table is always convertible.
//
//  2. No column wider than wideColMaxWidth characters. A column that wide holds
//     prose or a comma-list stuffed into a cell, not tabular data: oxfmt pads
//     every other cell in that column out to the widest one, so a single 600-char
//     cell can waste thousands of padding tokens across the table. The remedy is
//     to trim the cell, or destructure the table into sections/bullets. Genuine
//     3+-column matrices (capability grids, comparison tables) are legitimately 2D
//     and pass rule 1; they pass rule 2 too as long as no single cell balloons.
//
// Both rules are structural, not length-of-file based. Human-facing trees and
// vendored/generated trees are out of scope (see findAgentFacingDocs).

// wideColMaxWidth is the per-column character budget. oxfmt pads every cell in a
// column out to its widest cell, so one oversized cell taxes the whole column.
// 100 chars comfortably fits a real label, path, or short value; only a sentence
// or a comma-list blows past it, and that content reads better as prose or a list.
const wideColMaxWidth = 100

// tableHygieneSkipDirs are directory names whose subtree never holds agent-facing
// docs in this check's scope (vendored, generated, or human-facing trees).
// Mirrors the scope spelled out in docs/style-guide.md.
var tableHygieneSkipDirs = map[string]bool{
	"node_modules": true,
	"dist":         true,
	"website":      true, // apps/website/** is human-facing
	"brand":        true, // brand/** is human-facing assets + copy
}

// tableHygieneSkipFiles are human-facing markdown files excluded from the scope
// even though they live in otherwise-scanned directories.
var tableHygieneSkipFiles = map[string]bool{
	"README.md":       true,
	"CONTRIBUTING.md": true,
	"CHANGELOG.md":    true,
}

// fenceRe matches a fenced-code-block delimiter line (``` or ~~~, any indent,
// optional info string). Table-looking lines inside a fence are code, not tables.
var fenceRe = regexp.MustCompile("^\\s*(```|~~~)")

// tableSepRe matches a GitHub-flavored-markdown table separator row: a row of
// pipe-delimited cells each containing only dashes, colons, and spaces, with at
// least one dash. This is what turns the line above it into a table header.
var tableSepRe = regexp.MustCompile(`^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)+\|?\s*$`)

// indexCellRe matches a first-column cell that's a pure sequential index: empty,
// or a bare integer (optionally with a trailing dot or paren, e.g. "1", "1.",
// "1)"). Such a column carries no information a numbered bullet list wouldn't, so
// a 3-column table with an index first column counts as effectively two columns.
var indexCellRe = regexp.MustCompile(`^\d+[.)]?$`)

// tableHygieneHit records one flagged table for the failure report. For a wide
// column it also carries the offending column's header and width.
type tableHygieneHit struct {
	file    string // repo-relative, forward-slashed
	line    int    // 1-based line number of the header row
	kind    string // "2-column", "3-column with index first column", or "wide column"
	colName string // wide hits: the offending column's header text
	width   int    // wide hits: that column's widest-cell character width
}

// RunDocsTableHygiene scans the agent-facing markdown scope (CLAUDE.md,
// DETAILS.md, AGENTS.md, docs/**, .claude/rules/**) and fails when any table is
// two-column-equivalent or has a column wider than wideColMaxWidth characters.
func RunDocsTableHygiene(ctx *CheckContext) (CheckResult, error) {
	files, err := findAgentFacingDocs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list agent-facing docs: %w", err)
	}

	var hits []tableHygieneHit
	for _, rel := range files {
		data, readErr := os.ReadFile(filepath.Join(ctx.RootDir, filepath.FromSlash(rel)))
		if readErr != nil {
			return CheckResult{}, fmt.Errorf("failed to read %s: %w", rel, readErr)
		}
		hits = append(hits, scanTableHygiene(rel, string(data))...)
	}

	if len(hits) == 0 {
		return Success(fmt.Sprintf("%d markdown files scanned, all tables clean", len(files))), nil
	}

	var narrow, wide []tableHygieneHit
	for _, h := range hits {
		if h.kind == "wide column" {
			wide = append(wide, h)
		} else {
			narrow = append(narrow, h)
		}
	}

	var sb strings.Builder
	for _, h := range narrow {
		fmt.Fprintf(&sb, "%s:%d: %s table\n", h.file, h.line, h.kind)
	}
	for _, h := range wide {
		fmt.Fprintf(&sb, "%s:%d: wide column %q (%d chars, max %d)\n", h.file, h.line, h.colName, h.width, wideColMaxWidth)
	}

	body := fmt.Sprintf(
		"%s in agent-facing docs (see docs/style-guide.md):\n%s\n"+
			"Fixes: convert a 2-column (or index-first 3-column) table to a bullet list "+
			"(`- **Title**: details`, bold title, \": \" separator, no dash/em-dash). For a wide column, "+
			"trim the cell, or if it holds prose or a comma-list, destructure the table into sections or "+
			"bullets. Tables waste tokens on alignment padding and cause spurious merge conflicts.",
		Pluralize(len(hits), "1 table issue", fmt.Sprintf("%d table issues", len(hits))),
		indentOutput(strings.TrimRight(sb.String(), "\n")))
	return CheckResult{}, fmt.Errorf("%s", body)
}

// scanTableHygiene parses content as markdown and returns one hit per flagged
// table. It tracks fenced code blocks (table-shaped lines inside a fence are
// code) and recognizes a table by a separator row whose preceding line is a
// header row of the same column count.
func scanTableHygiene(rel, content string) []tableHygieneHit {
	lines := strings.Split(content, "\n")
	var hits []tableHygieneHit
	inFence := false
	for i, line := range lines {
		if fenceRe.MatchString(line) {
			inFence = !inFence
			continue
		}
		if inFence {
			continue
		}
		if !tableSepRe.MatchString(line) {
			continue
		}
		// A separator row makes a table only if the line above is a header row
		// with at least one pipe. i>0 is guaranteed (a leading separator has no
		// header, so it isn't a table).
		if i == 0 {
			continue
		}
		header := lines[i-1]
		if !strings.Contains(header, "|") {
			continue
		}
		headerCells := splitTableRow(header)
		if len(headerCells) < 2 {
			continue
		}
		if hit, ok := classifyTable(rel, i, headerCells, lines[i+1:]); ok {
			hits = append(hits, hit)
		}
	}
	return hits
}

// classifyTable decides whether a table (given its header cells and the data
// rows that follow the separator) is flagged, and how. A two-column-equivalent
// table is reported as such; otherwise a column wider than wideColMaxWidth is
// reported. The two-column rule takes priority: bulletizing such a table fixes
// any width problem too, so there's no point reporting both. sepIdx is the
// 0-based index of the separator row, which equals the 1-based line number of
// the header row one line above it.
func classifyTable(rel string, sepIdx int, headerCells []string, dataLines []string) (tableHygieneHit, bool) {
	cols := len(headerCells)
	headerLine := sepIdx

	switch {
	case cols == 2:
		return tableHygieneHit{file: rel, line: headerLine, kind: "2-column"}, true
	case cols == 3:
		if firstColumnIsIndex(headerCells, dataLines) {
			return tableHygieneHit{file: rel, line: headerLine, kind: "3-column with index first column"}, true
		}
	}

	if name, width, ok := widestOverBudgetColumn(headerCells, dataLines); ok {
		return tableHygieneHit{file: rel, line: headerLine, kind: "wide column", colName: name, width: width}, true
	}
	return tableHygieneHit{}, false
}

// widestOverBudgetColumn measures every column's widest cell (header plus data
// rows) and returns the widest column that exceeds wideColMaxWidth, with its
// header text and width. Returns ok=false when no column is over budget. Width is
// measured in runes (oxfmt aligns on character count), on the raw cell markdown
// since that's what oxfmt pads.
func widestOverBudgetColumn(headerCells []string, dataLines []string) (string, int, bool) {
	colWidth := make([]int, len(headerCells))
	for c, cell := range headerCells {
		colWidth[c] = utf8.RuneCountInString(strings.TrimSpace(cell))
	}
	for _, cells := range collectDataRows(len(headerCells), dataLines) {
		for c, cell := range cells {
			if w := utf8.RuneCountInString(strings.TrimSpace(cell)); w > colWidth[c] {
				colWidth[c] = w
			}
		}
	}
	worst, worstCol := 0, -1
	for c, w := range colWidth {
		if w > wideColMaxWidth && w > worst {
			worst, worstCol = w, c
		}
	}
	if worstCol == -1 {
		return "", 0, false
	}
	return strings.TrimSpace(headerCells[worstCol]), worst, true
}

// firstColumnIsIndex reports whether the first column of a 3-column table is a
// pure sequential index (every data cell empty or a bare integer). The header's
// own first cell is allowed to be a label (for example "#" or "Step"); only the
// data rows decide. Requires at least one data row, and every first cell to be
// index-shaped.
func firstColumnIsIndex(headerCells []string, dataLines []string) bool {
	rows := collectDataRows(len(headerCells), dataLines)
	if len(rows) == 0 {
		return false
	}
	for _, cells := range rows {
		first := strings.TrimSpace(cells[0])
		if first == "" {
			continue
		}
		if !indexCellRe.MatchString(first) {
			return false
		}
	}
	return true
}

// collectDataRows gathers the contiguous data rows of a table: every following
// line that's a pipe row, stopping at the first blank or non-row line. Rows are
// returned split into cells (only rows whose cell count matches colCount, to
// stay robust against ragged trailing content).
func collectDataRows(colCount int, dataLines []string) [][]string {
	var rows [][]string
	for _, line := range dataLines {
		if strings.TrimSpace(line) == "" || !strings.Contains(line, "|") {
			break
		}
		cells := splitTableRow(line)
		if len(cells) == colCount {
			rows = append(rows, cells)
		}
	}
	return rows
}

// splitTableRow splits a markdown table row into its cells, honoring backslash-
// escaped pipes (`\|`, a literal pipe inside a cell) and trimming the optional
// leading/trailing border pipes. Returns the inner cells (the count of real
// columns).
func splitTableRow(row string) []string {
	row = strings.TrimSpace(row)
	var cells []string
	var cur strings.Builder
	for i := 0; i < len(row); i++ {
		c := row[i]
		if c == '\\' && i+1 < len(row) {
			cur.WriteByte(c)
			cur.WriteByte(row[i+1])
			i++
			continue
		}
		if c == '|' {
			cells = append(cells, cur.String())
			cur.Reset()
			continue
		}
		cur.WriteByte(c)
	}
	cells = append(cells, cur.String())
	// A row written with border pipes (`| a | b |`) yields empty first and last
	// segments; drop a single leading/trailing empty so the count reflects real
	// columns. A borderless row (`a | b`) keeps both.
	if len(cells) >= 2 && strings.TrimSpace(cells[0]) == "" {
		cells = cells[1:]
	}
	if len(cells) >= 2 && strings.TrimSpace(cells[len(cells)-1]) == "" {
		cells = cells[:len(cells)-1]
	}
	return cells
}

// findAgentFacingDocs returns every in-scope agent-facing markdown file as a
// repo-relative, forward-slashed path: CLAUDE.md, DETAILS.md, AGENTS.md anywhere,
// plus everything under docs/ and .claude/rules/. Human-facing trees (website,
// brand) and files (README, CONTRIBUTING, CHANGELOG) are excluded, as are
// vendored/generated trees. In a git work tree it uses git ls-files so .gitignored
// scratch is skipped; outside git it walks the filesystem.
func findAgentFacingDocs(rootDir string) ([]string, error) {
	all, ok := gitListAllMarkdown(rootDir)
	if !ok {
		var walkErr error
		all, walkErr = walkAllMarkdown(rootDir)
		if walkErr != nil {
			return nil, walkErr
		}
	}
	var out []string
	for _, rel := range all {
		if inAgentFacingScope(rel) {
			out = append(out, rel)
		}
	}
	sort.Strings(out)
	return out, nil
}

// inAgentFacingScope decides whether a repo-relative markdown path is in the
// agent-facing scope this check enforces.
func inAgentFacingScope(rel string) bool {
	segs := strings.Split(rel, "/")
	base := segs[len(segs)-1]
	dirs := segs[:len(segs)-1]
	// Drop vendored/generated/human-facing trees. Drop hidden dirs too, with one
	// exception: the .claude/rules/ subtree is explicitly in scope.
	inClaudeRules := strings.HasPrefix(rel, ".claude/rules/")
	for _, seg := range dirs {
		if tableHygieneSkipDirs[seg] {
			return false
		}
		if strings.HasPrefix(seg, ".") && !inClaudeRules {
			return false
		}
	}
	if tableHygieneSkipFiles[base] {
		return false
	}
	if inClaudeRules {
		return true
	}
	if base == "CLAUDE.md" || base == "DETAILS.md" || base == "AGENTS.md" {
		return true
	}
	return strings.HasPrefix(rel, "docs/")
}

// gitListAllMarkdown lists tracked + untracked-not-ignored .md files. Returns
// (nil, false) when rootDir isn't a git work tree.
func gitListAllMarkdown(rootDir string) ([]string, bool) {
	cmd := exec.Command("git", "-C", rootDir, "ls-files", "--cached", "--others",
		"--exclude-standard", "-z", "--", "*.md")
	out, err := cmd.Output()
	if err != nil {
		return nil, false
	}
	var docs []string
	for _, rel := range strings.Split(string(out), "\x00") {
		if rel != "" {
			docs = append(docs, rel)
		}
	}
	return docs, true
}

// walkAllMarkdown is the non-git fallback: a filesystem walk of every .md file,
// skipping hidden dirs (except .claude) and the vendored/generated trees.
func walkAllMarkdown(rootDir string) ([]string, error) {
	var docs []string
	err := filepath.WalkDir(rootDir, func(p string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		rel, relErr := filepath.Rel(rootDir, p)
		if relErr != nil {
			return nil
		}
		rel = filepath.ToSlash(rel)
		if d.IsDir() {
			name := d.Name()
			if rel != "." && strings.HasPrefix(name, ".") && name != ".claude" {
				return filepath.SkipDir
			}
			if tableHygieneSkipDirs[name] {
				return filepath.SkipDir
			}
			return nil
		}
		if strings.HasSuffix(d.Name(), ".md") {
			docs = append(docs, rel)
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	return docs, nil
}
