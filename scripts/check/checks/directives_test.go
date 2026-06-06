package checks

import (
	"strings"
	"testing"
)

func TestDirectiveTracker_PureCommentSiteIsRecorded(t *testing.T) {
	tr := newDirectiveTracker("// allowed-bare-poll:", "//")
	tr.observe(3, "    // allowed-bare-poll: best-effort cleanup")
	orphans := tr.orphans("a.ts")
	if len(orphans) != 1 {
		t.Fatalf("expected 1 orphan, got %d", len(orphans))
	}
	if orphans[0].line != 3 || orphans[0].relPath != "a.ts" {
		t.Errorf("unexpected orphan: %+v", orphans[0])
	}
}

func TestDirectiveTracker_TrailingSiteIsRecorded(t *testing.T) {
	tr := newDirectiveTracker("// allowed-lock-poison:", "//")
	tr.observe(7, `let g = state.lock().unwrap(); // allowed-lock-poison: proven unpoisonable`)
	if len(tr.orphans("x.rs")) != 1 {
		t.Fatal("expected trailing directive to be recorded as a site")
	}
}

func TestDirectiveTracker_MetaMentionIsIgnored(t *testing.T) {
	tr := newDirectiveTracker("// allowed-bare-poll:", "//")
	tr.observe(1, "// Opt out with `// allowed-bare-poll: <reason>` on the line above")
	tr.observe(2, " * see `// allowed-bare-poll:` in the docs")
	if got := tr.orphans("a.ts"); len(got) != 0 {
		t.Fatalf("expected meta-mentions to be ignored, got %d orphans", len(got))
	}
}

func TestDirectiveTracker_MarkUsedClearsSite(t *testing.T) {
	tr := newDirectiveTracker("// allowed-bare-poll:", "//")
	tr.observe(3, "    // allowed-bare-poll: cleanup")
	tr.observe(4, "    await pollUntil(page, () => check(), 3000)")
	// Line 4 is the violation; the directive sits on the previous line.
	tr.markUsed(4, "    await pollUntil(page, () => check(), 3000)", "    // allowed-bare-poll: cleanup")
	if got := tr.orphans("a.ts"); len(got) != 0 {
		t.Fatalf("expected no orphans after markUsed, got %d", len(got))
	}
}

func TestDirectiveTracker_MarkUsedSameLine(t *testing.T) {
	tr := newDirectiveTracker("// allowed-bare-poll:", "//")
	line := "    await pollUntil(page, () => check(), 3000) // allowed-bare-poll: cleanup"
	tr.observe(9, line)
	tr.markUsed(9, line, "function dismiss() {")
	if got := tr.orphans("a.ts"); len(got) != 0 {
		t.Fatalf("expected no orphans after same-line markUsed, got %d", len(got))
	}
}

func TestDirectiveTracker_HashCommentPrefix(t *testing.T) {
	tr := newDirectiveTracker("# allowed-rustup-add:", "#")
	tr.observe(2, "  # allowed-rustup-add: floating directive that excuses nothing")
	tr.observe(3, "  # add `# allowed-rustup-add: <reason>` to opt out")
	orphans := tr.orphans("ci.yml")
	if len(orphans) != 1 {
		t.Fatalf("expected exactly the pure directive line as orphan, got %d", len(orphans))
	}
	if orphans[0].line != 2 {
		t.Errorf("expected line 2, got %d", orphans[0].line)
	}
}

func TestFormatOrphanDirectives_ListsAllSites(t *testing.T) {
	msg := formatOrphanDirectives("// allowed-bare-poll:", []orphanDirective{
		{relPath: "a.ts", line: 3, text: "// allowed-bare-poll: stale"},
		{relPath: "b.ts", line: 9, text: "// allowed-bare-poll: also stale"},
	})
	for _, want := range []string{"a.ts:3", "b.ts:9", "// allowed-bare-poll:", "unused"} {
		if !strings.Contains(msg, want) {
			t.Errorf("expected message to contain %q, got: %s", want, msg)
		}
	}
}
