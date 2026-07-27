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

// sectionRefRe matches a cross-doc section pointer: a backticked doc path followed
// by `§` and the section it names, either quoted or bare. House style writes
// `foo/DETAILS.md` § The writer, so the path and the heading arrive together.
//
// A bare claim runs until a delimiter that can't appear mid-heading. The trailing
// sentence is trimmed separately (claimText), since a heading name is routinely
// followed by prose in the same sentence.
var sectionRefRe = regexp.MustCompile("`([^`\n]+\\.mdx?)`\\s*§\\s*(?:\"([^\"\n]+)\"|([^\n,;:)]+))")

// headingLineRe strips the leading #s off an ATX heading.
var headingLineRe = regexp.MustCompile(`^#+\s*`)

// boldLeadRe matches a bold lead-in used as a subsection marker, at the start of a
// line or of a list item: `**The rule.** …` and `- **The rule.** …`. Both are real §
// targets here, and the bullet form is the more common of the two, so a check that
// knew only about `#` headings (or only about unindented bold) would flag a pile of
// correct pointers.
var boldLeadRe = regexp.MustCompile(`^\s*(?:[-*+]\s+)?\*\*(.+?)\*\*(.*)$`)

// labelledBlockRe matches a bold LABEL introducing a named block: `**Decision**: Live
// -toggleable portal …`. The name is what a § pointer cites, and one doc can hold
// twenty `**Decision**` blocks, so the label alone is useless as a target. Captures
// the text after the colon.
var labelledBlockRe = regexp.MustCompile(`^\s*:\s*(.+)$`)

// sentenceBreakRe splits a bare claim at its first sentence end, so
// "§ Gotchas. If you ever need to…" claims "Gotchas", not the paragraph.
var sentenceBreakRe = regexp.MustCompile(`\.\s`)

// maxClaimWords caps how much of a bare claim counts as the heading name. Long
// enough for every real heading here, short enough that a runaway match can't turn
// a whole sentence into a "missing heading".
const maxClaimWords = 10

// headingWords normalizes a heading or a claim to comparable words: formatting
// markers dropped, lowercased, trailing punctuation off each word. Internal
// punctuation stays, so `dir_stats` and `copy_file_range(2)` keep their shape.
func headingWords(s string) []string {
	s = strings.NewReplacer("`", "", `"`, "", "“", "", "”", "", "*", "").Replace(s)
	var out []string
	for w := range strings.FieldsSeq(s) {
		w = strings.TrimRight(strings.ToLower(w), ".,;:!?)")
		if w != "" {
			out = append(out, w)
		}
	}
	return out
}

// docHeadings returns every § target a doc offers: its ATX headings plus its bold
// lead-ins.
func docHeadings(content string) [][]string {
	var out [][]string
	add := func(raw string) {
		if w := headingWords(raw); len(w) > 0 {
			out = append(out, w)
		}
	}
	for line := range strings.SplitSeq(content, "\n") {
		if strings.HasPrefix(line, "#") {
			add(headingLineRe.ReplaceAllString(line, ""))
			continue
		}
		m := boldLeadRe.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		add(m[1])
		// `**Decision**: <name>` — the name after the label is the real target, so
		// index it too rather than leaving twenty identical `Decision` entries.
		if rest := labelledBlockRe.FindStringSubmatch(m[2]); rest != nil {
			add(rest[1])
		}
	}
	return out
}

// claimText reduces a matched § pointer to the words it claims are a heading, and
// returns the author's own wording alongside them so a report quotes what's written
// in the doc rather than the normalized form (the thing you'd grep for).
func claimText(quoted, bare string) (words []string, raw string) {
	raw = quoted
	if raw == "" {
		if parts := sentenceBreakRe.Split(bare, 2); len(parts) > 0 {
			raw = strings.TrimSpace(parts[0])
		}
	}
	w := headingWords(raw)
	if len(w) > maxClaimWords {
		w = w[:maxClaimWords]
		raw = strings.Join(strings.Fields(raw)[:maxClaimWords], " ")
	}
	return w, raw
}

// isOrderedSubsequence reports whether every word of needle appears in hay, in the
// same order, not necessarily adjacent. That's how docs shorten a long heading:
// "§ The source contract" for "The full-aggregate source contract".
//
// Order is what keeps this from decaying into "shares some words with a heading" —
// a claim whose words appear scrambled doesn't match. Needles under two words are
// rejected outright, since a single common word would subsequence into almost
// anything.
func isOrderedSubsequence(needle, hay []string) bool {
	if len(needle) < 2 || len(needle) > len(hay) {
		return false
	}
	i := 0
	for _, w := range hay {
		if w == needle[i] {
			if i++; i == len(needle) {
				return true
			}
		}
	}
	return false
}

func slicesEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

// claimMatches reports whether a claim names one of a doc's headings. The rule is
// deliberately generous, because the cost of a false positive (an author rewording
// a correct pointer to satisfy a check) is higher than the cost of missing one:
//
//   - a shared prefix, so "§ Platform constraints" finds "Platform constraints
//     (filesystem and IPC)" and vice versa
//   - either one an in-order subsequence of the other, so "§ The source contract"
//     finds "The full-aggregate source contract"
//
// What it still catches is the case that matters: a heading that isn't there at
// all, because it was renamed, moved to another doc, or never existed.
func claimMatches(claim []string, headings [][]string) bool {
	for _, h := range headings {
		n := min(len(claim), len(h))
		if n > 0 && slicesEqual(claim[:n], h[:n]) {
			return true
		}
		if isOrderedSubsequence(claim, h) || isOrderedSubsequence(h, claim) {
			return true
		}
	}
	return false
}

// sectionRef is one pointer whose named heading isn't in the target doc.
type sectionRef struct {
	doc     string
	target  string
	heading string
}

// resolveDocTarget maps a backticked doc path to a repo-relative doc, doc-relative
// first then repo-rooted. Returns "" when it names nothing that exists: that's
// docs-dead-links' finding, and reporting it here too would make one broken
// reference look like two separate failures.
func resolveDocTarget(rootDir, srcDoc, target string) string {
	for _, cand := range []string{
		path.Clean(path.Join(path.Dir(srcDoc), target)),
		path.Clean(target),
	} {
		if strings.HasPrefix(cand, "..") {
			continue
		}
		if fileExists(filepath.Join(rootDir, filepath.FromSlash(cand))) {
			return cand
		}
	}
	return ""
}

// scanDocForSectionRefs returns the § pointers in one doc that name a heading the
// target doesn't have.
func scanDocForSectionRefs(rootDir, doc, content string, headings map[string][][]string) []sectionRef {
	var bad []sectionRef
	unfenced := fencedCodeBlockRe.ReplaceAllString(content, "")
	for _, m := range sectionRefRe.FindAllStringSubmatch(unfenced, -1) {
		target := resolveDocTarget(rootDir, doc, strings.TrimSpace(m[1]))
		if target == "" {
			continue
		}
		claim, claimRaw := claimText(strings.TrimSpace(m[2]), strings.TrimSpace(m[3]))
		if len(claim) == 0 {
			continue
		}
		heads, ok := headings[target]
		if !ok {
			data, err := os.ReadFile(filepath.Join(rootDir, filepath.FromSlash(target)))
			if err != nil {
				continue
			}
			heads = docHeadings(string(data))
			headings[target] = heads
		}
		if !claimMatches(claim, heads) {
			bad = append(bad, sectionRef{doc: doc, target: target, heading: claimRaw})
		}
	}
	return bad
}

// RunDocsSectionRefs verifies cross-doc section pointers: when a doc says
// `other/DETAILS.md` § Some heading, that heading has to exist in that doc.
//
// A path can resolve while the section it names is long gone, so docs-dead-links
// passes and the pointer still leads nowhere. Both ATX headings and bold lead-ins
// count as targets, and matching is prefix- and substring-tolerant so the house
// habit of naming a heading's distinctive opening keeps working.
//
// Skipped: docs/specs/ (a plan names headings it intends to write) and docs/notes/
// (a dated record cites a doc as it read then), matching docs-dead-links' carve-outs.
func RunDocsSectionRefs(ctx *CheckContext) (CheckResult, error) {
	docs, err := findMarkdownDocs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list docs: %w", err)
	}

	headings := map[string][][]string{}
	var bad []sectionRef
	checked := 0
	for _, doc := range docs {
		if strings.HasPrefix(doc, specsDir) || strings.HasPrefix(doc, notesDir) {
			continue
		}
		data, readErr := os.ReadFile(filepath.Join(ctx.RootDir, filepath.FromSlash(doc)))
		if readErr != nil {
			continue
		}
		checked++
		bad = append(bad, scanDocForSectionRefs(ctx.RootDir, doc, string(data), headings)...)
	}

	if len(bad) == 0 {
		return Success(fmt.Sprintf("Every § section pointer names a real heading (%d %s scanned)",
			checked, Pluralize(checked, "doc", "docs"))), nil
	}

	sort.Slice(bad, func(i, j int) bool {
		if bad[i].doc != bad[j].doc {
			return bad[i].doc < bad[j].doc
		}
		return bad[i].heading < bad[j].heading
	})
	var sb strings.Builder
	for _, b := range bad {
		sb.WriteString(fmt.Sprintf("  - %s -> %s § %s\n", b.doc, b.target, b.heading))
	}
	return CheckResult{}, fmt.Errorf(
		"%d § %s at a heading that doesn't exist (rename the pointer, or restore the heading):\n%s",
		len(bad), Pluralize(len(bad), "pointer", "pointers"), strings.TrimRight(sb.String(), "\n"))
}
