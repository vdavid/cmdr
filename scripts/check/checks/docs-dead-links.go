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

// deadLink records a Markdown link whose local target doesn't exist on disk.
type deadLink struct {
	doc    string
	target string
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

// linkResolves reports whether a local link target names an existing file or
// directory. It tries the target relative to the source doc's directory (standard
// Markdown), then repo-root-relative, then (for a blog post) as a /blog/<slug>
// route. checkable is false when every candidate escapes the repo root (a
// `../`-heavy path we can't verify), so the caller skips it rather than flagging a
// false positive.
func linkResolves(rootDir, srcDoc, target string) (resolved, checkable bool) {
	srcDir := path.Dir(srcDoc)
	cands := []string{
		path.Clean(path.Join(srcDir, target)),
		path.Clean(target),
	}
	if blogCand := blogLinkCandidate(srcDoc, target); blogCand != "" {
		cands = append(cands, blogCand)
	}
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

// RunDocsDeadLinks scans every first-party Markdown doc for relative links whose
// target file or directory doesn't exist, and fails (error-level) listing each
// one. External URLs, in-page #anchors, and links inside code (fenced or inline)
// are skipped. Reuses the doc set and link regex from the doc-graph machinery.
func RunDocsDeadLinks(ctx *CheckContext) (CheckResult, error) {
	docs, err := findMarkdownDocs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list docs: %w", err)
	}

	var dead []deadLink
	for _, doc := range docs {
		data, readErr := os.ReadFile(filepath.Join(ctx.RootDir, filepath.FromSlash(doc)))
		if readErr != nil {
			continue
		}
		text := inlineCodeRe.ReplaceAllString(fencedCodeBlockRe.ReplaceAllString(string(data), ""), "")
		seen := map[string]bool{}
		for _, m := range mdLinkTargetRe.FindAllStringSubmatch(text, -1) {
			target := localLinkTarget(m[1])
			if target == "" || seen[target] {
				continue
			}
			seen[target] = true
			if resolved, checkable := linkResolves(ctx.RootDir, doc, target); checkable && !resolved {
				dead = append(dead, deadLink{doc: doc, target: target})
			}
		}
	}

	if len(dead) == 0 {
		return Success(fmt.Sprintf("All local links resolve (%d %s scanned)",
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
		sb.WriteString(fmt.Sprintf("  - %s -> %s\n", d.doc, d.target))
	}
	return CheckResult{}, fmt.Errorf(
		"%d dead doc %s (the link target doesn't exist; fix the path or remove the link):\n%s",
		len(dead), Pluralize(len(dead), "link", "links"), strings.TrimRight(sb.String(), "\n"))
}
