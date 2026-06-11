package checks

import (
	"os"
	"path"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// This file builds the doc-discoverability graph the `docs-reachable` check
// enforces and the `--docs-graph` renderer draws. The contract: starting from
// the single root CLAUDE.md (the repo-root loader, the real entry point: Claude
// Code reads it first and it `@import`s AGENTS.md), every CLAUDE.md, DETAILS.md,
// and docs/ file must be reachable by following references between docs, so a
// reader (human or agent) entering there can find every doc by link-walking.
//
// A "reference" is any mention, treated equally whether it's a Markdown link, an
// `@import`, a backtick path, or a bare path token: we watch intent, not syntax.
// The one asymmetry: a DETAILS.md or a docs/ file must be named, but a CLAUDE.md
// is reached by a mention of its *directory* too (architecture.md lists most
// subsystems as `` `some/dir/` ``, and Claude Code auto-injects a CLAUDE.md from
// its directory anyway). Such an edge is tagged ViaDir so the graph view can show
// "(dir reference)".

// docGraphRoot is the single entry point: the repo-root CLAUDE.md, which Claude
// Code loads first and which `@import`s AGENTS.md. Everything must trace back here.
const docGraphRoot = "CLAUDE.md"

// docSkipDirs are never walked: vendored, generated, and build-output trees hold
// no first-party docs. Mirrors reminderSkipDirs plus the per-worktree dirs.
var docSkipDirs = map[string]bool{
	"vendor":        true,
	"node_modules":  true,
	".cargo-docker": true,
	"target":        true,
	"build":         true,
	"dist":          true,
}

// DocReach records how a doc was reached from the root.
type DocReach struct {
	Path   string
	Parent string // "" for the root itself
	Depth  int    // hop count from the root (root = 0)
	ViaDir bool   // reached through a directory reference (CLAUDE.md only)
}

// DocGraph is the analyzed discoverability graph rooted at AGENTS.md.
type DocGraph struct {
	Root     string               // always docGraphRoot
	Reached  map[string]*DocReach // every reachable doc, keyed by repo-relative path
	Orphans  []string             // enforced candidates not reachable, sorted
	children map[string][]string  // BFS tree: parent path -> child paths, sorted
}

// Children returns the BFS-tree children of a doc (closest-to-root edges only),
// sorted by path. Used by the renderer.
func (g *DocGraph) Children(docPath string) []string { return g.children[docPath] }

type docEdge struct {
	to     string
	viaDir bool
}

// BuildDocGraph walks rootDir, builds the reference graph between Markdown docs,
// and computes reachability from AGENTS.md. Returns the reached set (with parent
// + depth + via-dir tagging for the closest-to-root path) and the sorted orphans
// among the enforced candidates (CLAUDE.md, DETAILS.md, docs/ minus ephemeral).
func BuildDocGraph(rootDir string) (*DocGraph, error) {
	allDocs, err := findMarkdownDocs(rootDir)
	if err != nil {
		return nil, err
	}

	contents := make(map[string]string, len(allDocs))
	for _, rel := range allDocs {
		data, readErr := os.ReadFile(filepath.Join(rootDir, filepath.FromSlash(rel)))
		if readErr != nil {
			continue
		}
		contents[rel] = string(data)
	}

	adj := buildDocEdges(allDocs, contents)

	g := &DocGraph{
		Root:     docGraphRoot,
		Reached:  map[string]*DocReach{},
		children: map[string][]string{},
	}
	if _, ok := contents[docGraphRoot]; ok {
		bfsDocGraph(g, adj)
	}

	for _, rel := range allDocs {
		if rel == docGraphRoot {
			continue
		}
		if isEnforcedCandidate(rel) && g.Reached[rel] == nil {
			g.Orphans = append(g.Orphans, rel)
		}
	}
	sort.Strings(g.Orphans)
	return g, nil
}

// bfsDocGraph runs the breadth-first traversal from the root, recording the
// closest-to-root parent (BFS guarantees minimal depth; sorted adjacency makes
// ties deterministic) and whether that winning edge was a directory reference.
func bfsDocGraph(g *DocGraph, adj map[string][]docEdge) {
	g.Reached[g.Root] = &DocReach{Path: g.Root, Depth: 0}
	queue := []string{g.Root}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		curReach := g.Reached[cur]
		if curReach == nil {
			continue // unreachable: cur was enqueued only after being recorded in Reached
		}
		for _, e := range adj[cur] {
			if g.Reached[e.to] != nil {
				continue // already reached closer to the root; ignore cycles + longer paths
			}
			g.Reached[e.to] = &DocReach{Path: e.to, Parent: cur, Depth: curReach.Depth + 1, ViaDir: e.viaDir}
			g.children[cur] = append(g.children[cur], e.to)
			queue = append(queue, e.to)
		}
	}
}

// buildDocEdges resolves every doc's references into graph edges. Adjacency is
// sorted by target path (file edges before dir edges for the same target) so BFS
// is deterministic and prefers a named reference over a directory one.
func buildDocEdges(allDocs []string, contents map[string]string) map[string][]docEdge {
	docSet := make(map[string]bool, len(allDocs))
	for _, d := range allDocs {
		docSet[d] = true
	}
	var claudeDirs []string // dirs that hold a CLAUDE.md, for dir-reference resolution
	for _, d := range allDocs {
		if path.Base(d) == "CLAUDE.md" {
			claudeDirs = append(claudeDirs, path.Dir(d))
		}
	}

	adj := make(map[string][]docEdge, len(allDocs))
	for _, src := range allDocs {
		srcDir := path.Dir(src)
		seen := map[string]bool{} // dedupe; once a target has a file edge, a dir edge is redundant
		add := func(to string, viaDir bool) {
			if to == "" || to == src || seen[to] {
				return
			}
			seen[to] = true
			adj[src] = append(adj[src], docEdge{to: to, viaDir: viaDir})
		}
		fileRefs, dirRefs := extractDocRefs(contents[src])
		for _, ref := range fileRefs {
			for _, to := range resolveFileRef(srcDir, ref, docSet) {
				add(to, false)
			}
		}
		for _, ref := range dirRefs {
			for _, to := range resolveDirRef(srcDir, ref, claudeDirs) {
				add(to, true)
			}
		}
		sort.Slice(adj[src], func(i, j int) bool {
			if adj[src][i].to != adj[src][j].to {
				return adj[src][i].to < adj[src][j].to
			}
			return !adj[src][i].viaDir && adj[src][j].viaDir
		})
	}
	return adj
}

var (
	// mdFileRefRe matches any path token ending in .md (Markdown link target,
	// @import, backtick path, or bare). Stops at .md so trailing #anchors drop.
	mdFileRefRe = regexp.MustCompile(`[@A-Za-z0-9._/-]*\.md`)
	// backtickDirRe matches a backtick-wrapped directory path (trailing slash),
	// the form architecture.md uses to list subsystems: `` `file-explorer/` ``.
	backtickDirRe = regexp.MustCompile("`([@A-Za-z0-9._/-]+/)`")
	// mdLinkTargetRe matches a Markdown link target; a slash-terminated one is a
	// directory reference (e.g. `[rules](.claude/rules/)`).
	mdLinkTargetRe = regexp.MustCompile(`\]\(([^)\s]+)\)`)
)

// extractDocRefs returns the file-reference and directory-reference tokens found
// in a doc's text. Tokens are raw (resolution happens later, against the source's
// directory and the candidate set).
func extractDocRefs(text string) (fileRefs, dirRefs []string) {
	fileRefs = mdFileRefRe.FindAllString(text, -1)
	for _, m := range backtickDirRe.FindAllStringSubmatch(text, -1) {
		dirRefs = append(dirRefs, m[1])
	}
	for _, m := range mdLinkTargetRe.FindAllStringSubmatch(text, -1) {
		if strings.HasSuffix(m[1], "/") {
			dirRefs = append(dirRefs, m[1])
		}
	}
	return fileRefs, dirRefs
}

// normalizeRef strips reference decoration (`@` import marker, leading `./` or
// `/`) and cleans the path so it can be matched against repo-relative doc paths.
func normalizeRef(ref string) string {
	ref = strings.TrimPrefix(ref, "@")
	ref = strings.TrimPrefix(ref, "/")
	ref = strings.TrimSuffix(ref, "/")
	if ref == "" {
		return ""
	}
	return path.Clean(ref)
}

// resolveFileRef maps a .md reference token to the doc paths it names. It tries,
// in order: relative to the source's directory, repo-root-relative, and (only for
// multi-segment tokens, to keep bare `DETAILS.md` from matching every sibling) a
// path-suffix match. Returns every candidate it resolves to (generous by design:
// over-connecting only hides a would-be orphan, never invents a false one).
func resolveFileRef(srcDir, ref string, docSet map[string]bool) []string {
	tok := normalizeRef(ref)
	if tok == "" {
		return nil
	}
	var out []string
	seen := map[string]bool{}
	consider := func(p string) {
		if p != "" && docSet[p] && !seen[p] {
			seen[p] = true
			out = append(out, p)
		}
	}
	consider(path.Clean(path.Join(srcDir, tok)))
	consider(tok)
	if strings.Contains(tok, "/") {
		suffix := "/" + tok
		for d := range docSet {
			if d == tok || strings.HasSuffix(d, suffix) {
				consider(d)
			}
		}
	}
	return out
}

// resolveDirRef maps a directory reference to the CLAUDE.md files it connects: a
// CLAUDE.md whose directory the token names, resolved relative to the source dir,
// repo-root-relative, or by path-suffix (so “ `search/` “ connects every
// search/CLAUDE.md). Directory references reach CLAUDE.md only, never other docs.
func resolveDirRef(srcDir, ref string, claudeDirs []string) []string {
	tok := normalizeRef(ref)
	if tok == "" {
		return nil
	}
	rel := path.Clean(path.Join(srcDir, tok))
	suffix := "/" + tok
	var out []string
	for _, dir := range claudeDirs {
		if dir == tok || dir == rel || strings.HasSuffix(dir, suffix) {
			out = append(out, path.Join(dir, "CLAUDE.md"))
		}
	}
	return out
}

// isEnforcedCandidate reports whether a doc must be reachable: every CLAUDE.md
// and DETAILS.md, plus everything under docs/. The repo-root CLAUDE.md is the
// graph root, never an orphan, so it's filtered out before this is consulted.
func isEnforcedCandidate(rel string) bool {
	base := path.Base(rel)
	if base == "CLAUDE.md" || base == "DETAILS.md" {
		return true
	}
	return strings.HasPrefix(rel, "docs/")
}

// findMarkdownDocs walks rootDir and returns every .md file as a repo-relative,
// forward-slashed path, skipping vendored/generated/hidden dirs.
func findMarkdownDocs(rootDir string) ([]string, error) {
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
			if rel != "." && (strings.HasPrefix(name, ".") || docSkipDirs[name]) {
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
	sort.Strings(docs)
	return docs, nil
}
