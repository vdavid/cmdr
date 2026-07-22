package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
)

// Agent-facing docs are read as raw Markdown, never rendered, so a link whose
// text is the target path prints that path twice: `[`a/b/CLAUDE.md`](a/b/CLAUDE.md)`
// costs double the tokens and double the visual noise for zero extra information.
// This check bans path-shaped link text. Two fixes, both cheaper to read:
//
//  1. Drop the link, keep the path: `` `a/b/CLAUDE.md` ``. The doc graph treats a
//     backtick path as a first-class reference (see docs_graph.go), so
//     docs-reachable and claude-md-details-sibling still connect through it, and
//     docs-dead-links still verifies it exists.
//  2. Keep the link, give it descriptive text: `[the subsystem map](docs/architecture.md)`.
//     Same token cost as the bare path, and it says why the link is worth following.
//
// Which one is editorial. Prefer (2) when a phrase reads better in the sentence,
// (1) when the path itself is the point. A link target with an #anchor needs (2),
// since a bare backtick path can't carry the fragment.
//
// Error-level, no allowlist: once the corpus is clean there's no legitimate
// exception, and human-facing docs (README, CONTRIBUTING, CHANGELOG, website,
// brand) are out of scope entirely, since those render and need real link text.

var (
	// linkTextFenceRe matches a fenced-code-block delimiter line. Links inside a
	// fence are examples, not references.
	linkTextFenceRe = regexp.MustCompile("^\\s*(```|~~~)")
	// doubleBacktickSpanRe matches a ``…`` inline-code span: the form a doc uses to
	// show a literal example that itself contains backticks, e.g. ``[`x.md`](x.md)``.
	// Blanked before scanning so documenting this rule doesn't violate it.
	doubleBacktickSpanRe = regexp.MustCompile("``[^\n]*?``")
	// linkTextRe matches a Markdown inline link, capturing text and target. The text
	// group forbids brackets, so nested/reference links are left alone.
	linkTextRe = regexp.MustCompile(`\[([^\[\]]*)\]\(([^)\s]+)\)`)
	// pathTokenRe matches a token made only of path characters. Anything with a
	// space, comma, or other prose punctuation isn't a path.
	pathTokenRe = regexp.MustCompile(`^[@A-Za-z0-9._/()+-]+$`)
	// pathExtRe matches a file extension that marks a token as a path even without a
	// slash, so a bare `DETAILS.md` or `Cargo.toml` link text is caught too.
	pathExtRe = regexp.MustCompile(`\.(md|mdx|rs|ts|tsx|js|mjs|cjs|svelte|go|json|jsonc|toml|yml|yaml|sh|astro|css|html|sql|lock|txt)$`)
)

// linkTextHit records one link whose text duplicates its target path.
type linkTextHit struct {
	file    string // repo-relative, forward-slashed
	line    int    // 1-based
	text    string // the link text as written, backticks included
	target  string // the link target as written
	anchor  bool   // target carries a #fragment, so only descriptive text can replace it
	sameStr bool   // text (backticks stripped) is byte-identical to the target
}

// blankLinkExamples empties every inline-code span on a line whose content holds
// a `](` — a code span wrapping a whole link, which is a doc showing a literal
// example (docs/doc-system.md spells out a CLAUDE.md's closing line that way), not
// a live reference. Backtick pairing is done by splitting rather than by regex,
// because a regex can just as happily start a span at a *closing* backtick and
// swallow the gap between two adjacent real links. A genuine `[`x.md`](x.md)`
// survives: its code span holds only the path, with the `](` outside it.
func blankLinkExamples(line string) string {
	parts := strings.Split(line, "`")
	if len(parts) < 3 {
		return line
	}
	var sb strings.Builder
	for i, p := range parts {
		if i > 0 {
			sb.WriteByte('`')
		}
		if i%2 == 1 && strings.Contains(p, "](") {
			continue // odd index = code-span content; drop the example, keep delimiters
		}
		sb.WriteString(p)
	}
	return sb.String()
}

// isPathShapedLinkText reports whether a link's text is a path rather than prose:
// a bare path token, optionally wrapped in single backticks. Surrounding backticks
// are the common form but not required, since `[DETAILS.md](DETAILS.md)` is just as
// redundant as its backticked twin.
func isPathShapedLinkText(text string) bool {
	t := strings.TrimSpace(text)
	if strings.HasPrefix(t, "`") && strings.HasSuffix(t, "`") && len(t) > 2 {
		t = t[1 : len(t)-1]
	}
	if t == "" || strings.Contains(t, "`") || !pathTokenRe.MatchString(t) {
		return false
	}
	// A lone "." or ".." isn't a reference worth flagging, and a token with neither
	// a separator nor a known extension is a name (`serde`, `--fast`), not a path.
	if t == "." || t == ".." {
		return false
	}
	return strings.Contains(t, "/") || pathExtRe.MatchString(t)
}

// scanLinkText returns every path-shaped-link-text hit in one doc's content.
// Fenced blocks and double-backtick spans are blanked first so documented
// examples don't register as real links. Pure and line-oriented, so hits carry
// line numbers.
func scanLinkText(rel, content string) []linkTextHit {
	var hits []linkTextHit
	inFence := false
	for i, line := range strings.Split(content, "\n") {
		if linkTextFenceRe.MatchString(line) {
			inFence = !inFence
			continue
		}
		if inFence {
			continue
		}
		scanned := blankLinkExamples(doubleBacktickSpanRe.ReplaceAllString(line, ""))
		for _, m := range linkTextRe.FindAllStringSubmatch(scanned, -1) {
			text, target := m[1], m[2]
			if !isPathShapedLinkText(text) {
				continue
			}
			// External links keep their text: `[`serde_json`](https://docs.rs/...)`
			// names a crate, not a repo path, and there's no bare-path form for a URL.
			if localLinkTarget(target) == "" {
				continue
			}
			bare := strings.Trim(strings.TrimSpace(text), "`")
			hits = append(hits, linkTextHit{
				file:    rel,
				line:    i + 1,
				text:    text,
				target:  target,
				anchor:  strings.Contains(target, "#"),
				sameStr: bare == target,
			})
		}
	}
	return hits
}

// RunDocsLinkText scans every agent-facing doc for Markdown links whose text is a
// path, and fails (error-level) listing each one.
func RunDocsLinkText(ctx *CheckContext) (CheckResult, error) {
	files, err := findAgentFacingDocs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list agent-facing docs: %w", err)
	}

	var hits []linkTextHit
	for _, rel := range files {
		data, readErr := os.ReadFile(filepath.Join(ctx.RootDir, filepath.FromSlash(rel)))
		if readErr != nil {
			return CheckResult{}, fmt.Errorf("failed to read %s: %w", rel, readErr)
		}
		hits = append(hits, scanLinkText(rel, string(data))...)
	}

	if len(hits) == 0 {
		return Success(fmt.Sprintf("%d agent-facing %s scanned, no path-shaped link text",
			len(files), Pluralize(len(files), "doc", "docs"))), nil
	}

	var sb strings.Builder
	for _, h := range hits {
		suffix := ""
		switch {
		case h.anchor:
			suffix = "  (target has an #anchor: only descriptive link text works)"
		case h.sameStr:
			suffix = "  (exact duplicate: drop the link, keep the text)"
		default:
			suffix = "  (text and target differ: keep the TARGET, drop the text)"
		}
		fmt.Fprintf(&sb, "%s:%d: [%s](%s)%s\n", h.file, h.line, h.text, h.target, suffix)
	}

	body := fmt.Sprintf(
		"%d %s with path-shaped link text:\n%s\n"+
			"Fix each one by dropping the link and keeping the target as a backticked path "+
			"(`` `docs/architecture.md` ``, which the doc graph still follows), or by giving the link "+
			"descriptive text (`[the subsystem map](docs/architecture.md)`). Take the path from the "+
			"TARGET, not the old text, which is often stale: keep it as written when it's a sibling or "+
			"child, and use the repo-rooted path when the target climbs two or more directories. "+
			"Repeating the path as link text costs double the tokens for no extra information.",
		len(hits), Pluralize(len(hits), "link", "links"),
		indentOutput(strings.TrimRight(sb.String(), "\n")))
	return CheckResult{}, fmt.Errorf("%s", body)
}
