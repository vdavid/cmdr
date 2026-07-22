package checks

import (
	"fmt"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// This check is the companion to docs-reachable: that one fails when a doc can't
// be reached from the root (an orphan), this one fails when a doc's link points at
// a target that doesn't exist (a dead link). Together they keep the doc graph both
// connected and intact, so an agent or human link-walking the docs never hits a
// missing file. Error-level, no allowlist: a dead link is always a fix, not an
// exemption.

var (
	// fencedCodeBlockRe matches ``` or ~~~ fenced code blocks (non-greedy, across
	// lines). Stripped before link extraction so a Markdown-link EXAMPLE inside a
	// code block isn't mistaken for a real link.
	fencedCodeBlockRe = regexp.MustCompile("(?s)```.*?```|~~~.*?~~~")
	// inlineCodeRe matches an inline code span. Stripped too, so a documented
	// example like "use `[text](path)`" in prose doesn't read as a live link.
	inlineCodeRe = regexp.MustCompile("`[^`]*`")
)

// backtickPathRe matches an inline-code span (single or double backticks). Its
// content is a path candidate, checked by isRepoPathToken below.
var backtickPathRe = regexp.MustCompile("`{1,2}([^`\n]+)`{1,2}")

// repoPathTokenRe matches a token made only of path characters: anything with a
// space or prose punctuation isn't a path.
var repoPathTokenRe = regexp.MustCompile(`^[@A-Za-z0-9._/()+-]+$`)

// isRepoPathToken reports whether an inline-code span names an in-repo doc worth
// verifying: a multi-segment path ending in .md or .mdx.
//
// The scope is deliberately narrow. A bare backtick path in prose is ambiguous in
// a way a Markdown link never is: docs legitimately spell out paths that aren't
// repo files at all, including Cmdr's virtual git filesystem (`.git/branches/`),
// example user paths (`/Documents/notes.txt`), build output, and files in sibling
// repos. Doc-to-doc references have none of that ambiguity, and they're the ones
// that carry the doc graph, so they're where verification pays.
//
// Excluded on purpose: absolute and `~`-prefixed paths (another machine, another
// repo, nothing here to check them against), node_modules (untracked), and elided
// paths written with a `...` segment. docs/specs/ is excluded at the call site.
func isRepoPathToken(tok string) bool {
	if tok == "" || !repoPathTokenRe.MatchString(tok) {
		return false
	}
	if !strings.HasSuffix(tok, ".md") && !strings.HasSuffix(tok, ".mdx") {
		return false
	}
	if !strings.Contains(tok, "/") {
		return false // single-segment `DETAILS.md`: too easy to confuse with prose like `C.md`
	}
	if strings.HasPrefix(tok, "~") || strings.HasPrefix(tok, "/") {
		return false
	}
	if strings.Contains(tok, "/.../") || strings.HasPrefix(tok, ".../") {
		return false
	}
	return !strings.HasPrefix(tok, "node_modules/") && !strings.Contains(tok, "/node_modules/")
}

// buildPathSuffixSet indexes every tracked file and every directory on the way to
// it by all of its path suffixes, so a subtree-relative reference resolves without
// the doc having to spell out the full path. Falls back to nil (which disables the
// suffix tier, leaving strict resolution) outside a git work tree.
func buildPathSuffixSet(rootDir string) map[string]bool {
	out, err := runGit(rootDir, "ls-files")
	if err != nil {
		return nil
	}
	set := map[string]bool{}
	add := func(p string, isDir bool) {
		segs := strings.Split(p, "/")
		for i := range segs {
			suffix := strings.Join(segs[i:], "/")
			if isDir {
				suffix += "/"
			}
			set[suffix] = true
		}
	}
	for f := range strings.SplitSeq(strings.TrimSpace(out), "\n") {
		if f == "" {
			continue
		}
		add(f, false)
		for dir := path.Dir(f); dir != "." && dir != "/"; dir = path.Dir(dir) {
			add(dir, true)
			add(dir, false) // a directory named without its trailing slash
		}
	}
	return set
}

// specsDir is the per-development plan area, periodically wiped once each plan's
// durable intent lands in the colocated docs (AGENTS.md § File structure).
const specsDir = "docs/specs/"

// isSpecScratchPath reports whether a backtick path is one that's expected to
// dangle. Two shapes qualify, both around docs/specs/:
//
//   - A reference TO a spec, which module docs use on purpose to name the wiped
//     plan a decision came from ("Design history is in git (former …)").
//   - A reference FROM a spec, since a plan names the files it intends to create.
//
// Markdown LINKS in and out of docs/specs/ are still verified: a link promises
// something to click, so it has to resolve either way.
func isSpecScratchPath(srcDoc, tok string) bool {
	return strings.HasPrefix(tok, specsDir) || strings.HasPrefix(srcDoc, specsDir)
}

// barePathResolves reports whether a backtick path names something real. It tries
// strict resolution first (relative to the doc, then repo-rooted), then falls back
// to a repo-wide path-suffix match, because docs routinely reference a file by the
// part of its path that's meaningful in context (`routes/+page.svelte` inside the
// analytics dashboard's own docs). A Markdown link gets no such fallback: it has to
// resolve as written, or clicking it breaks.
func barePathResolves(rootDir, srcDoc, tok string, suffixes map[string]bool) bool {
	if resolved, checkable := linkResolves(rootDir, srcDoc, tok); checkable && resolved {
		return true
	}
	return suffixes[strings.TrimPrefix(tok, "/")]
}

// deadLink records a reference whose local target doesn't exist on disk. A
// Markdown link and a bare backtick path are both references (the doc graph
// treats them identically, see docs_graph.go), so both are verified here.
type deadLink struct {
	doc    string
	target string
	bare   bool // a backtick path rather than a Markdown link
}

// localLinkTarget strips a Markdown link target down to a bare local path, or
// returns "" if it's something we can't or shouldn't resolve to a file: an
// in-page #anchor, a URL (`https:`, `mailto:`, a protocol-relative `//host`), or
// empty. The #fragment and ?query are dropped so `foo.md#section` checks `foo.md`.
func localLinkTarget(raw string) string {
	t := strings.TrimSpace(raw)
	t = strings.TrimPrefix(t, "<")
	t = strings.TrimSuffix(t, ">")
	if t == "" || strings.HasPrefix(t, "#") || strings.HasPrefix(t, "//") {
		return ""
	}
	// A scheme prefix (`scheme:...`) marks a URL. A real relative path's first
	// segment never contains a colon, so a leading `scheme:` with no slash or dot
	// before the colon is an external link (http, https, mailto, tel, vscode, ...).
	if i := strings.IndexByte(t, ':'); i > 0 && !strings.ContainsAny(t[:i], "/.") {
		return ""
	}
	if i := strings.IndexAny(t, "#?"); i >= 0 {
		t = t[:i]
	}
	return strings.TrimSpace(t)
}

// blogContentDir is the Astro content-collection path whose posts render at the
// site route /blog/<slug>.
const blogContentDir = "src/content/blog/"

// blogLinkCandidate maps a site-absolute /blog/<slug> link to the post's source
// directory. A blog post that links to a sibling post should use the rendered URL
// (the only form that's robust under Astro's trailingSlash:ignore, where the index
// links posts without a trailing slash), but that URL doesn't name a file on disk.
// So when the link lives in a blog post, resolve /blog/<slug> to that post's
// content/blog dir + slug. Derived from the source doc's own path, so there's no
// hardcoded app prefix. Returns "" when it doesn't apply.
func blogLinkCandidate(srcDoc, target string) string {
	const route = "/blog/"
	if !strings.HasPrefix(target, route) {
		return ""
	}
	slug := strings.Trim(target[len(route):], "/")
	if slug == "" || strings.Contains(slug, "/") {
		return ""
	}
	i := strings.Index(srcDoc, blogContentDir)
	if i < 0 {
		return ""
	}
	return path.Join(srcDoc[:i]+blogContentDir, slug)
}

// pageRouteCandidates maps a site-absolute link (`/pricing`, `/roadmap`) in a
// website doc to the Astro page source that renders it: `src/pages/<route>.astro`
// or `src/pages/<route>/index.astro`. Astro page routes have no file at the URL
// path, so a Markdown doc must link the rendered URL (the form that's also right in
// rendered HTML) even though it doesn't name a file on disk: same situation as
// blogLinkCandidate, resolved the same way. The site's `src/` root is derived from
// the doc's own content-collection path, so there's no hardcoded app prefix.
// Returns nil when it doesn't apply.
func pageRouteCandidates(srcDoc, target string) []string {
	if !strings.HasPrefix(target, "/") {
		return nil
	}
	route := strings.Trim(target, "/")
	if route == "" {
		return nil // bare "/" is the site root, not a page file
	}
	const contentMarker = "src/content/"
	i := strings.Index(srcDoc, contentMarker)
	if i < 0 {
		return nil // not a doc under the Astro site's content tree
	}
	pagesDir := srcDoc[:i] + "src/pages"
	return []string{
		path.Join(pagesDir, route+".astro"),
		path.Join(pagesDir, route, "index.astro"),
	}
}

// linkResolves reports whether a local link target names an existing file or
// directory. It tries the target relative to the source doc's directory (standard
// Markdown), then repo-root-relative, then (for a blog post) as a /blog/<slug>
// route, then as an Astro page route. checkable is false when every candidate
// escapes the repo root (a `../`-heavy path we can't verify), so the caller skips
// it rather than flagging a false positive.
func linkResolves(rootDir, srcDoc, target string) (resolved, checkable bool) {
	srcDir := path.Dir(srcDoc)
	cands := []string{
		path.Clean(path.Join(srcDir, target)),
		path.Clean(target),
	}
	if blogCand := blogLinkCandidate(srcDoc, target); blogCand != "" {
		cands = append(cands, blogCand)
	}
	cands = append(cands, pageRouteCandidates(srcDoc, target)...)
	for _, cand := range cands {
		if strings.HasPrefix(cand, "..") {
			continue // escapes the repo root; not verifiable via this form
		}
		checkable = true
		if fileExists(filepath.Join(rootDir, filepath.FromSlash(cand))) {
			return true, true
		}
	}
	return false, checkable
}

// scanDocForDeadRefs returns one doc's references that don't resolve, in both
// forms: Markdown link targets, then bare backtick paths. The two share a `seen`
// set, so a doc that links AND mentions the same target is reported once.
func scanDocForDeadRefs(rootDir, doc, content string, suffixes map[string]bool) []deadLink {
	var dead []deadLink
	unfenced := fencedCodeBlockRe.ReplaceAllString(content, "")
	seen := map[string]bool{}

	// Inline code is stripped here so a Markdown link written inside a code span
	// reads as the example it is, not as a live link.
	text := inlineCodeRe.ReplaceAllString(unfenced, "")
	for _, m := range mdLinkTargetRe.FindAllStringSubmatch(text, -1) {
		target := localLinkTarget(m[1])
		if target == "" || seen[target] {
			continue
		}
		seen[target] = true
		if resolved, checkable := linkResolves(rootDir, doc, target); checkable && !resolved {
			dead = append(dead, deadLink{doc: doc, target: target})
		}
	}

	// Bare backtick paths are references too: house style prefers `a/b/CLAUDE.md`
	// over a link whose text repeats its target (see docs-link-text.go), so without
	// this pass most of the doc corpus would go unverified.
	for _, m := range backtickPathRe.FindAllStringSubmatch(unfenced, -1) {
		tok := strings.TrimSpace(m[1])
		if !isRepoPathToken(tok) || seen[tok] || isSpecScratchPath(doc, tok) {
			continue
		}
		seen[tok] = true
		if !barePathResolves(rootDir, doc, tok, suffixes) {
			dead = append(dead, deadLink{doc: doc, target: tok, bare: true})
		}
	}
	return dead
}

// RunDocsDeadLinks scans every first-party Markdown doc for references whose
// target file or directory doesn't exist, and fails (error-level) listing each
// one. Both Markdown links and bare backtick paths count as references. External
// URLs, in-page #anchors, and anything inside a fenced block are skipped, as is a
// Markdown link written inside an inline-code span (a documented example, not a
// live link). Reuses the doc set and link regex from the doc-graph machinery.
func RunDocsDeadLinks(ctx *CheckContext) (CheckResult, error) {
	docs, err := findMarkdownDocs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list docs: %w", err)
	}

	suffixes := buildPathSuffixSet(ctx.RootDir)

	var dead []deadLink
	for _, doc := range docs {
		data, readErr := os.ReadFile(filepath.Join(ctx.RootDir, filepath.FromSlash(doc)))
		if readErr != nil {
			continue
		}
		dead = append(dead, scanDocForDeadRefs(ctx.RootDir, doc, string(data), suffixes)...)
	}

	if len(dead) == 0 {
		return Success(fmt.Sprintf("All local links and backtick paths resolve (%d %s scanned)",
			len(docs), Pluralize(len(docs), "doc", "docs"))), nil
	}

	sort.Slice(dead, func(i, j int) bool {
		if dead[i].doc != dead[j].doc {
			return dead[i].doc < dead[j].doc
		}
		return dead[i].target < dead[j].target
	})
	var sb strings.Builder
	for _, d := range dead {
		kind := ""
		if d.bare {
			kind = " (backtick path)"
		}
		sb.WriteString(fmt.Sprintf("  - %s -> %s%s\n", d.doc, d.target, kind))
	}
	return CheckResult{}, fmt.Errorf(
		"%d dead doc %s (the target doesn't exist; fix the path or drop the reference):\n%s",
		len(dead), Pluralize(len(dead), "reference", "references"), strings.TrimRight(sb.String(), "\n"))
}
