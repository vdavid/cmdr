package main

import (
	"slices"
	"testing"
)

func TestNormalizeRepoRel(t *testing.T) {
	cases := []struct {
		in      string
		want    string
		wantOK  bool
		comment string
	}{
		{"/Users/x/projects-git/vdavid/cmdr/apps/desktop/CLAUDE.md", "apps/desktop/CLAUDE.md", true, "main clone"},
		{"/Users/x/projects-git/vdavid/cmdr/.claude/worktrees/feat/apps/desktop/CLAUDE.md", "apps/desktop/CLAUDE.md", true, "worktree copy folds to canonical"},
		{"/private/tmp/ab/cmdr/scripts/check/DETAILS.md", "scripts/check/DETAILS.md", true, "target clone"},
		{"/Users/x/projects-git/vdavid/cmdr/AGENTS.md", "AGENTS.md", true, "repo-root doc"},
		{"/Users/x/.claude/projects/foo/session.jsonl", "", false, "outside repo"},
		{"/Users/x/projects-git/vdavid/mtp-rs/src/lib.rs", "", false, "sibling repo, no /cmdr/"},
		{"/Users/x/projects-git/vdavid/cmdr-docs/notes.md", "", false, "cmdr-prefixed dir is not the repo"},
		{"/Users/x/projects-git/vdavid/cmdr/.claude/worktrees/feat", "", false, "worktree root with no file under it"},
	}
	for _, c := range cases {
		got, ok := normalizeRepoRel(c.in)
		if ok != c.wantOK || got != c.want {
			t.Errorf("normalizeRepoRel(%q) = (%q, %v), want (%q, %v) [%s]", c.in, got, ok, c.want, c.wantOK, c.comment)
		}
	}
}

func TestAncestorDirs(t *testing.T) {
	cases := []struct {
		in   string
		want []string
	}{
		{"apps/desktop/src/foo.rs", []string{"apps/desktop/src", "apps/desktop", "apps", "."}},
		{"AGENTS.md", []string{"."}},
		{"scripts/check/checks/x.go", []string{"scripts/check/checks", "scripts/check", "scripts", "."}},
	}
	for _, c := range cases {
		if got := ancestorDirs(c.in); !slices.Equal(got, c.want) {
			t.Errorf("ancestorDirs(%q) = %v, want %v", c.in, got, c.want)
		}
	}
	// "." is an ancestor of every path: this is what forces the repo-root CLAUDE.md
	// (and its @-imported AGENTS.md) to load in every session, i.e. the 100% invariant.
	for _, p := range []string{"a.md", "a/b.md", "a/b/c/d.rs"} {
		if !slices.Contains(ancestorDirs(p), ".") {
			t.Errorf("ancestorDirs(%q) must contain \".\"", p)
		}
	}
}

func TestPercentStr(t *testing.T) {
	cases := []struct {
		n, total int
		want     string
	}{
		{773, 773, "100%"},
		{0, 773, "0%"},
		{0, 0, "0%"},
		{2, 773, "<1%"},   // 0.26% rounds below 1
		{20, 773, "3%"},   // 2.6% rounds to 3
		{16, 773, "2%"},   // 2.07%
		{386, 773, "50%"}, // exact half rounds up
	}
	for _, c := range cases {
		if got := percentStr(c.n, c.total); got != c.want {
			t.Errorf("percentStr(%d, %d) = %q, want %q", c.n, c.total, got, c.want)
		}
	}
}

// TestLoadedDocsForSession_AncestorAutoloadAndImports covers the core rule: a
// single touch anywhere loads every ancestor CLAUDE.md plus the root's @-import.
func TestLoadedDocsForSession_AncestorAutoloadAndImports(t *testing.T) {
	claudeDirToNode := map[string]string{
		".":                "CLAUDE.md",
		"apps/desktop":     "apps/desktop/CLAUDE.md",
		"apps/desktop/src": "apps/desktop/src/CLAUDE.md",
	}
	importEdges := map[string][]string{"CLAUDE.md": {"AGENTS.md"}}

	touched := map[string]bool{"apps/desktop/src/foo.rs": true}
	loaded := loadedDocsForSession(touched, map[string]bool{}, claudeDirToNode, importEdges)

	for _, want := range []string{"CLAUDE.md", "AGENTS.md", "apps/desktop/CLAUDE.md", "apps/desktop/src/CLAUDE.md"} {
		if !loaded[want] {
			t.Errorf("expected %q loaded from touching apps/desktop/src/foo.rs; loaded=%v", want, loaded)
		}
	}
	// A DETAILS.md is never autoloaded: only an explicit Read counts it.
	if loaded["apps/desktop/DETAILS.md"] {
		t.Error("DETAILS.md must not autoload via ancestor touch")
	}
}

func TestLoadedDocsForSession_ExplicitReadCountsNonAutoloadDoc(t *testing.T) {
	// Touching only AGENTS.md loads root CLAUDE.md + AGENTS.md; a docs/ file is
	// counted only because it was explicitly Read (it never autoloads).
	loaded := loadedDocsForSession(
		map[string]bool{"AGENTS.md": true},
		map[string]bool{"docs/style-guide.md": true},
		map[string]string{".": "CLAUDE.md"},
		map[string][]string{"CLAUDE.md": {"AGENTS.md"}},
	)
	for _, want := range []string{"CLAUDE.md", "AGENTS.md", "docs/style-guide.md"} {
		if !loaded[want] {
			t.Errorf("expected %q loaded; loaded=%v", want, loaded)
		}
	}
}

func TestReadColorFor(t *testing.T) {
	const total = 100
	cases := []struct {
		count int
		want  string
		note  string
	}{
		{0, colorRed, "never read is dead (red)"},
		{1, colorYellow, "1% is domain-specific (yellow)"},
		{20, colorYellow, "exactly 20% is still yellow (boundary inclusive)"},
		{21, colorGreen, "just over 20% is broadly loaded (green)"},
		{100, colorGreen, "always loaded (green)"},
	}
	for _, c := range cases {
		if got := readColorFor(c.count, total); got != c.want {
			t.Errorf("readColorFor(%d, %d) = %q, want %q [%s]", c.count, total, got, c.want, c.note)
		}
	}
	// A zero total never divides by zero and never yields green.
	if got := readColorFor(0, 0); got != colorRed {
		t.Errorf("readColorFor(0, 0) = %q, want red", got)
	}
}

func TestReadColorBuckets(t *testing.T) {
	nodes := []string{"dead", "niche", "hot"}
	r := usageReport{usage: map[string]*docUsage{
		"dead": {}, "niche": {readSessions: 10}, "hot": {readSessions: 90},
	}, totalSessions: 100, available: true}
	colors := readColorBuckets(r, nodes)
	want := map[string]string{"dead": colorRed, "niche": colorYellow, "hot": colorGreen}
	for n, w := range want {
		if colors[n] != w {
			t.Errorf("node %s color = %q, want %q", n, colors[n], w)
		}
	}
}
