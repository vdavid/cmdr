package checks

import (
	"fmt"
	"sort"
	"strings"
)

// orphanDirective is an opt-out comment with no matching violation: the
// directive excuses nothing, so it should be removed. Orphans are the
// comment-allowlist equivalent of ESLint's "unused eslint-disable" report.
type orphanDirective struct {
	relPath string
	line    int
	text    string
}

// directiveTracker records the opt-out directive sites in one file and which
// of them actually excused a violation, so unused (orphaned) directives can be
// reported. Create one tracker per scanned file.
//
// A "site" is either a pure-comment line that starts with the directive, or a
// code line carrying the directive as a trailing comment. Comment lines that
// merely mention the directive in prose ("opt out with `// allowed-x: ...`")
// are not sites and never get reported.
type directiveTracker struct {
	directive     string         // for example "// allowed-bare-poll:"
	commentPrefix string         // the line-comment marker: "//" or "#"
	sites         map[int]string // line number → trimmed line text
	used          map[int]bool
}

func newDirectiveTracker(directive, commentPrefix string) *directiveTracker {
	return &directiveTracker{
		directive:     directive,
		commentPrefix: commentPrefix,
		sites:         make(map[int]string),
		used:          make(map[int]bool),
	}
}

// observe records the line if it carries a directive site. Call it for every
// line the check actually scans (skip lines the check itself skips, like
// `#[cfg(test)]` mod bodies, so directives there don't get flagged).
func (t *directiveTracker) observe(lineNum int, line string) {
	if !strings.Contains(line, t.directive) {
		return
	}
	trimmed := strings.TrimLeft(line, " \t")
	if strings.HasPrefix(trimmed, t.commentPrefix) || strings.HasPrefix(trimmed, "*") {
		// Pure comment line: a site only when the whole comment IS the
		// directive; anything else is prose mentioning it.
		if strings.HasPrefix(trimmed, t.directive) {
			t.sites[lineNum] = strings.TrimSpace(line)
		}
		return
	}
	// Code line with a trailing directive.
	t.sites[lineNum] = strings.TrimSpace(line)
}

// markUsed flags the directive site(s) that excused a violation on lineNum:
// the directive may sit on the same line (trailing) or on the line above.
func (t *directiveTracker) markUsed(lineNum int, line, prev string) {
	if strings.Contains(prev, t.directive) {
		t.used[lineNum-1] = true
	}
	if strings.Contains(line, t.directive) {
		t.used[lineNum] = true
	}
}

// orphans returns the recorded-but-never-used directive sites, sorted by line.
func (t *directiveTracker) orphans(relPath string) []orphanDirective {
	var out []orphanDirective
	for line, text := range t.sites {
		if t.used[line] {
			continue
		}
		out = append(out, orphanDirective{relPath: relPath, line: line, text: text})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].line < out[j].line })
	return out
}

// formatOrphanDirectives builds the failure message for unused opt-out
// comments. Checks append this to their error (or fail on it alone).
func formatOrphanDirectives(directive string, orphans []orphanDirective) string {
	sort.Slice(orphans, func(i, j int) bool {
		if orphans[i].relPath == orphans[j].relPath {
			return orphans[i].line < orphans[j].line
		}
		return orphans[i].relPath < orphans[j].relPath
	})
	var sb strings.Builder
	sb.WriteString(fmt.Sprintf(
		"found %d unused `%s` opt-out %s (no matching violation on the same or next line) — remove them:\n",
		len(orphans), directive, Pluralize(len(orphans), "comment", "comments"),
	))
	for _, o := range orphans {
		sb.WriteString(fmt.Sprintf("  %s:%d: %s\n", o.relPath, o.line, o.text))
	}
	return strings.TrimRight(sb.String(), "\n")
}
