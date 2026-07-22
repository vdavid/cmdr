package main

import (
	"bufio"
	"encoding/json"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"

	"cmdr/scripts/check/checks"
)

// This file enriches the `--docs-graph` tree with real usage: for every doc node,
// how many agent sessions in a recent window read it and how many wrote it. The
// data source is the Claude Code transcript store at `~/.claude/projects/`.
//
// Model (see docs_graph_render.go's header and scripts/check/DETAILS.md for the
// rationale):
//   - A "session" is one transcript JSONL file (top-level or subagent), so
//     subagents and team members already count separately.
//   - A file is "touched" when a session's Read/Edit/Write/MultiEdit/NotebookEdit
//     names it. Every touch of a file loads the CLAUDE.md of that file's directory
//     and every ancestor directory (Claude Code's autoload), plus those CLAUDE.md
//     files' transitive `@`-imports (root CLAUDE.md `@import`s AGENTS.md). Plain
//     Markdown links do NOT autoload, so DETAILS.md and docs/ files are counted as
//     read only when a session explicitly Reads them.
//   - The denominator is sessions that touched at least one repo file. Because "."
//     is an ancestor of every path, the root CLAUDE.md (and thus AGENTS.md) is
//     loaded by every such session, so both sit at exactly 100% — a built-in
//     correctness check on this whole computation.
//
// Worktree copies and the target clone are folded back to canonical repo-relative
// paths (see normalizeRepoRel), so a doc edited in `.claude/worktrees/x/` or in
// `/private/tmp/ab/cmdr/` counts toward the same node. Files in since-moved
// locations simply miss the current node set; that data loss is accepted.

// usageWindow is the rough recency horizon. Transcript files are filtered by mtime
// (a session that last wrote within the window is included whole, even if it began
// earlier), so the boundary is coarse by design.
const usageWindow = 30 * 24 * time.Hour

// docUsage is the per-node read/write session tally.
type docUsage struct {
	readSessions  int
	writeSessions int
}

// usageReport is the full result: per-node tallies plus the session denominator.
type usageReport struct {
	usage         map[string]*docUsage
	totalSessions int // sessions that touched >=1 repo file (the % denominator)
	scannedFiles  int // transcript files parsed
	available     bool
}

// fileToolInput captures the two file-path shapes across the file tools.
type fileToolInput struct {
	FilePath     string `json:"file_path"`     // Read/Edit/Write/MultiEdit
	NotebookPath string `json:"notebook_path"` // NotebookEdit
}

// contentBlock is one entry in a message's content array; we only care about
// tool_use blocks that name a file.
type contentBlock struct {
	Type  string        `json:"type"`
	Name  string        `json:"name"`
	Input fileToolInput `json:"input"`
}

// transcriptLine is the minimal shape we decode from each JSONL record: the
// assistant message's tool_use blocks. Everything else is ignored.
type transcriptLine struct {
	Message struct {
		Content []contentBlock `json:"content"`
	} `json:"message"`
}

var readToolNames = map[string]bool{"Read": true}
var writeToolNames = map[string]bool{
	"Edit": true, "Write": true, "MultiEdit": true, "NotebookEdit": true,
}

// importRefRe matches an `@`-import token (`@AGENTS.md`, `@../foo/BAR.md`). Only
// `@`-prefixed refs autoload; plain Markdown links are deliberately excluded.
var importRefRe = regexp.MustCompile(`@([A-Za-z0-9._/-]+\.md)`)

// usageAnnotation formats one node's tally, e.g. "Read 25x (2%), written 2x". The
// whole string is dim for scannability except the read percentage, which carries
// readColor (a percentile bucket, see readColorBuckets). c applies ANSI codes (or
// not, when color is disabled).
func usageAnnotation(r usageReport, node, readColor string, c func(code, s string) string) string {
	u := r.usage[node]
	if u == nil {
		u = &docUsage{}
	}
	return c(colorDim, "Read "+itoa(u.readSessions)+"x (") +
		c(readColor, percentStr(u.readSessions, r.totalSessions)) +
		c(colorDim, "), written "+itoa(u.writeSessions)+"x")
}

// readColorBuckets maps each node to the color for its read percentage, by
// absolute thresholds tuned to what the buckets mean here: never read is red
// (dead weight), read by up to 20% of sessions is yellow (domain-specific docs
// only a fraction of sessions load), and above 20% is green (broadly loaded).
// Orange is deliberately unused: too close to red to tell apart.
func readColorBuckets(r usageReport, nodes []string) map[string]string {
	colors := make(map[string]string, len(nodes))
	for _, node := range nodes {
		colors[node] = readColorFor(readCount(r, node), r.totalSessions)
	}
	return colors
}

// readColorFor buckets one read count against the session total: 0 → red,
// (0, 20%] → yellow, >20% → green.
func readColorFor(count, total int) string {
	switch {
	case count == 0:
		return colorRed
	case total <= 0 || count*100 <= total*20: // <= 20% of sessions
		return colorYellow
	default:
		return colorGreen
	}
}

func readCount(r usageReport, node string) int {
	if u := r.usage[node]; u != nil {
		return u.readSessions
	}
	return 0
}

// percentStr renders n/total as a whole-percent string, with "<1%" for a nonzero
// fraction that rounds below 1 and "0%" for a true zero.
func percentStr(n, total int) string {
	if total <= 0 || n == 0 {
		return "0%"
	}
	pct := float64(n) * 100 / float64(total)
	if pct < 1 {
		return "<1%"
	}
	return itoa(int(pct+0.5)) + "%"
}

func itoa(n int) string { return strconv.Itoa(n) }

// computeDocUsage scans the transcript store and returns per-node usage for the
// graph's nodes. A non-available report (store missing, no sessions) is returned
// with available=false so the renderer degrades to the plain tree.
func computeDocUsage(rootDir string, g *checks.DocGraph) usageReport {
	report := usageReport{usage: map[string]*docUsage{}}

	nodes := make([]string, 0, len(g.Reached))
	for n := range g.Reached {
		nodes = append(nodes, n)
		report.usage[n] = &docUsage{}
	}
	nodeSet := make(map[string]bool, len(nodes))
	for _, n := range nodes {
		nodeSet[n] = true
	}

	claudeDirToNode, importEdges := buildAutoloadModel(rootDir, nodes)

	transcripts := findTranscriptFiles()
	if len(transcripts) == 0 {
		return report
	}

	cutoff := time.Now().Add(-usageWindow)
	for _, tf := range transcripts {
		info, err := os.Stat(tf)
		if err != nil || info.ModTime().Before(cutoff) {
			continue
		}
		touched, read, wrote := scanTranscript(tf, nodeSet)
		if len(touched) == 0 {
			continue // no repo interaction: not an "agent session on this repo"
		}
		report.scannedFiles++
		report.totalSessions++

		loaded := loadedDocsForSession(touched, read, claudeDirToNode, importEdges)
		for node := range loaded {
			if u := report.usage[node]; u != nil {
				u.readSessions++
			}
		}
		for node := range wrote {
			if u := report.usage[node]; u != nil {
				u.writeSessions++
			}
		}
	}

	report.available = report.totalSessions > 0
	return report
}

// loadedDocsForSession resolves the set of doc nodes considered "read" by a
// session: autoloaded CLAUDE.md ancestors of every touched file, their transitive
// `@`-imports, and any explicitly-read doc node.
func loadedDocsForSession(touched, explicitRead map[string]bool, claudeDirToNode map[string]string, importEdges map[string][]string) map[string]bool {
	loaded := map[string]bool{}
	for f := range touched {
		for _, dir := range ancestorDirs(f) {
			if node, ok := claudeDirToNode[dir]; ok {
				loaded[node] = true
			}
		}
	}
	// Transitive @-imports of everything loaded so far (BFS).
	queue := make([]string, 0, len(loaded))
	for n := range loaded {
		queue = append(queue, n)
	}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		for _, imp := range importEdges[cur] {
			if !loaded[imp] {
				loaded[imp] = true
				queue = append(queue, imp)
			}
		}
	}
	for r := range explicitRead {
		loaded[r] = true
	}
	return loaded
}

// buildAutoloadModel derives, from the doc node set, the dir->CLAUDE.md map used
// for ancestor autoload and the `@`-import edges between nodes.
func buildAutoloadModel(rootDir string, nodes []string) (claudeDirToNode map[string]string, importEdges map[string][]string) {
	claudeDirToNode = map[string]string{}
	importEdges = map[string][]string{}
	nodeSet := make(map[string]bool, len(nodes))
	for _, n := range nodes {
		nodeSet[n] = true
	}
	for _, n := range nodes {
		if path.Base(n) == "CLAUDE.md" {
			claudeDirToNode[path.Dir(n)] = n
		}
		data, err := os.ReadFile(filepath.Join(rootDir, filepath.FromSlash(n)))
		if err != nil {
			continue
		}
		srcDir := path.Dir(n)
		for _, m := range importRefRe.FindAllStringSubmatch(string(data), -1) {
			target := path.Clean(path.Join(srcDir, m[1]))
			if nodeSet[target] && target != n {
				importEdges[n] = append(importEdges[n], target)
			}
		}
	}
	return claudeDirToNode, importEdges
}

// scanTranscript streams one JSONL transcript and returns the repo-relative paths
// it touched (any file tool), the doc nodes it explicitly Read, and the doc nodes
// it wrote. Non-repo paths and non-doc touches beyond the touched set are dropped.
func scanTranscript(pathname string, nodeSet map[string]bool) (touched, read, wrote map[string]bool) {
	touched, read, wrote = map[string]bool{}, map[string]bool{}, map[string]bool{}
	f, err := os.Open(pathname)
	if err != nil {
		return touched, read, wrote
	}
	defer f.Close()

	r := bufio.NewReader(f)
	for {
		line, err := r.ReadBytes('\n')
		if len(line) > 0 && bytesContainsToolUse(line) {
			var tl transcriptLine
			if json.Unmarshal(line, &tl) == nil {
				for _, c := range tl.Message.Content {
					recordToolFile(c, nodeSet, touched, read, wrote)
				}
			}
		}
		if err != nil {
			break // io.EOF or a read error: either way we're done with this file
		}
	}
	return touched, read, wrote
}

// recordToolFile folds one content block into the touched/read/wrote sets when it
// is a file-naming tool_use. Non-file, non-repo, and unknown-tool blocks no-op.
func recordToolFile(c contentBlock, nodeSet, touched, read, wrote map[string]bool) {
	if c.Type != "tool_use" {
		return
	}
	raw := c.Input.FilePath
	if raw == "" {
		raw = c.Input.NotebookPath
	}
	if raw == "" {
		return
	}
	rel, ok := normalizeRepoRel(raw)
	if !ok {
		return
	}
	isRead := readToolNames[c.Name]
	isWrite := writeToolNames[c.Name]
	if !isRead && !isWrite {
		return
	}
	touched[rel] = true
	if isRead && nodeSet[rel] {
		read[rel] = true
	}
	if isWrite && nodeSet[rel] {
		wrote[rel] = true
	}
}

// bytesContainsToolUse is a cheap prefilter so we only JSON-decode lines that
// carry a tool_use block (assistant turns), skipping large tool_result payloads.
func bytesContainsToolUse(line []byte) bool {
	return strings.Contains(string(line), `"tool_use"`)
}

// normalizeRepoRel folds any transcript file path into a canonical repo-relative
// path, or returns ok=false when the path is not inside the cmdr repo. It handles
// the main clone, worktrees under `.claude/worktrees/<name>/`, and the target
// clone (`/private/tmp/ab/cmdr/`) uniformly by splitting on `/cmdr/`.
func normalizeRepoRel(p string) (string, bool) {
	const marker = "/cmdr/"
	idx := strings.Index(p, marker)
	if idx < 0 {
		return "", false
	}
	rel := p[idx+len(marker):]
	const wt = ".claude/worktrees/"
	if strings.HasPrefix(rel, wt) {
		rest := rel[len(wt):]
		if slash := strings.IndexByte(rest, '/'); slash >= 0 {
			rel = rest[slash+1:]
		} else {
			return "", false // ".claude/worktrees/<name>" with no file under it
		}
	}
	if rel == "" {
		return "", false
	}
	return path.Clean(rel), true
}

// ancestorDirs returns the file's directory and every ancestor up to "." (the repo
// root), which is where per-directory CLAUDE.md autoload applies.
func ancestorDirs(rel string) []string {
	var dirs []string
	d := path.Dir(rel)
	for {
		dirs = append(dirs, d)
		if d == "." || d == "/" {
			break
		}
		d = path.Dir(d)
	}
	return dirs
}

// findTranscriptFiles returns every `*.jsonl` under `~/.claude/projects/*cmdr*/`
// (recursively, so subagent transcripts in per-session subdirs are included).
func findTranscriptFiles() []string {
	home, err := os.UserHomeDir()
	if err != nil {
		return nil
	}
	projectsDir := filepath.Join(home, ".claude", "projects")
	slugs, err := filepath.Glob(filepath.Join(projectsDir, "*cmdr*"))
	if err != nil {
		return nil
	}
	var files []string
	for _, slug := range slugs {
		_ = filepath.WalkDir(slug, func(p string, d os.DirEntry, err error) error {
			if err != nil {
				return nil
			}
			if !d.IsDir() && strings.HasSuffix(p, ".jsonl") {
				files = append(files, p)
			}
			return nil
		})
	}
	return files
}
