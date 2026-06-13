package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// Agent-facing docs are read by AI agents as a linear token stream, not a 2D
// layout. A two-column markdown table is pure waste there: the column-alignment
// padding burns tokens on whitespace that helps nobody, and any edit reflows
// every row's padding, which causes spurious git merge conflicts. A bulleted
// list (`- **Title**: details`) is cheaper to read, cheaper in tokens, and
// diff-stable. This check forbids two-column tables (and three-column tables
// whose first column is a pure sequential index, which are effectively two
// columns) in the agent-facing markdown scope. Genuine 3+-column matrices
// (capability grids, comparison tables) are legitimately 2D and pass.
//
// There's no allowlist: a two-column table is always convertible to bullets, so
// it's zero-tolerance. The check is structural (it parses table rows), never
// length-based.

// twoColTablesSkipDirs are directory names whose subtree never holds
// agent-facing docs in this check's scope (vendored, generated, or human-facing
// trees). Mirrors the scope spelled out in docs/style-guide.md.
var twoColTablesSkipDirs = map[string]bool{
	"node_modules": true,
	"dist":         true,
	"website":      true, // apps/website/** is human-facing
	"brand":        true, // brand/** is human-facing assets + copy
}

// twoColTablesSkipFiles are human-facing markdown files excluded from the scope
// even though they live in otherwise-scanned directories.
var twoColTablesSkipFiles = map[string]bool{
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

// twoColTableHit records one flagged table for the failure report.
type twoColTableHit struct {
	file string // repo-relative, forward-slashed
	line int    // 1-based line number of the header row
	cols int    // data-column count after collapsing an index column
	kind string // "2-column" or "3-column with index first column"
}

// RunDocsNoTwoColTables scans the agent-facing markdown scope (CLAUDE.md,
// DETAILS.md, AGENTS.md, docs/**, .claude/rules/**) and fails when any table has
// exactly two data columns, or three columns whose first is a pure sequential
// index. Those reformat losslessly to `- **Title**: details` bullets, which read
// cheaper and diff cleaner for agents. Genuine 3+-column matrices pass.
func RunDocsNoTwoColTables(ctx *CheckContext) (CheckResult, error) {
	files, err := findAgentFacingDocs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list agent-facing docs: %w", err)
	}

	var hits []twoColTableHit
	for _, rel := range files {
		data, readErr := os.ReadFile(filepath.Join(ctx.RootDir, filepath.FromSlash(rel)))
		if readErr != nil {
			return CheckResult{}, fmt.Errorf("failed to read %s: %w", rel, readErr)
		}
		hits = append(hits, scanTwoColTables(rel, string(data))...)
	}

	if len(hits) == 0 {
		return Success(fmt.Sprintf("%d markdown files scanned, no 2-column tables", len(files))), nil
	}

	var sb strings.Builder
	for _, h := range hits {
		fmt.Fprintf(&sb, "%s:%d: %s table\n", h.file, h.line, h.kind)
	}
	body := fmt.Sprintf(
		"%d %s in agent-facing docs. Convert each to a bullet list (`- **Title**: details`, bold title, "+
			"\": \" separator, no dash/em-dash); see docs/style-guide.md. Two-column tables waste tokens on "+
			"alignment padding and cause spurious merge conflicts:\n%s",
		len(hits), Pluralize(len(hits), "2-column table", "2-column tables"),
		indentOutput(strings.TrimRight(sb.String(), "\n")))
	return CheckResult{}, fmt.Errorf("%s", body)
}

// scanTwoColTables parses content as markdown and returns one hit per flagged
// table. It tracks fenced code blocks (table-shaped lines inside a fence are
// code) and recognizes a table by a separator row whose preceding line is a
// header row of the same column count.
func scanTwoColTables(rel, content string) []twoColTableHit {
	lines := strings.Split(content, "\n")
	var hits []twoColTableHit
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
// rows that follow the separator) is a flagged two-column-equivalent table.
// headerLineIdx is the 0-based index of the separator row; the header sits one
// line above, so the reported 1-based header line is headerLineIdx (0-based
// separator + 1 for the header above it, then +1 again would be the separator,
// so header line number == headerLineIdx).
func classifyTable(rel string, sepIdx int, headerCells []string, dataLines []string) (twoColTableHit, bool) {
	cols := len(headerCells)
	headerLine := sepIdx // 1-based line number of the header row (sepIdx is 0-based separator index)

	switch {
	case cols == 2:
		return twoColTableHit{file: rel, line: headerLine, cols: 2, kind: "2-column"}, true
	case cols == 3:
		if firstColumnIsIndex(headerCells, dataLines) {
			return twoColTableHit{
				file: rel, line: headerLine, cols: 2,
				kind: "3-column with index first column",
			}, true
		}
	}
	return twoColTableHit{}, false
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
		if twoColTablesSkipDirs[seg] {
			return false
		}
		if strings.HasPrefix(seg, ".") && !inClaudeRules {
			return false
		}
	}
	if twoColTablesSkipFiles[base] {
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
			if twoColTablesSkipDirs[name] {
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
