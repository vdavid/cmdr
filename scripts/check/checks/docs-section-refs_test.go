package checks

import (
	"strings"
	"testing"
)

func TestRunDocsSectionRefs_HeadingExists(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Depth in `lib/DETAILS.md` § The writer.")
	writeDeadLinkFile(t, tmp, "lib/DETAILS.md", "# Lib\n\n## The writer\n\nText.\n")

	if _, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("the heading exists, expected success: %v", err)
	}
}

func TestRunDocsSectionRefs_HeadingMissing(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Depth in `lib/DETAILS.md` § Generation semantics.")
	writeDeadLinkFile(t, tmp, "lib/DETAILS.md", "# Lib\n\n## The writer\n\nText.\n")

	_, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp})
	if err == nil {
		t.Fatal("expected an error for a § pointer at a heading that doesn't exist")
	}
	for _, want := range []string{"lib/DETAILS.md", "Generation semantics"} {
		if !strings.Contains(err.Error(), want) {
			t.Errorf("expected %q in the error, got: %v", want, err)
		}
	}
}

func TestRunDocsSectionRefs_ClaimIsHeadingPrefix(t *testing.T) {
	tmp := t.TempDir()
	// House style names the heading's distinctive opening, not its full text.
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "See `lib/DETAILS.md` § Platform constraints, then carry on.")
	writeDeadLinkFile(t, tmp, "lib/DETAILS.md", "## Platform constraints (filesystem and IPC)\n\nText.\n")

	if _, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("a heading-prefix claim must resolve: %v", err)
	}
}

func TestRunDocsSectionRefs_BoldPseudoHeading(t *testing.T) {
	tmp := t.TempDir()
	// Docs here use a bold lead-in as a subsection marker; § pointers target them.
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "See `lib/DETAILS.md` § The source contract.")
	writeDeadLinkFile(t, tmp, "lib/DETAILS.md", "## Writer\n\n**The source contract (`source: Maps|Sql`).** Text.\n")

	if _, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("a bold pseudo-heading is a valid § target: %v", err)
	}
}

func TestRunDocsSectionRefs_QuotedAndBacktickedHeading(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "See `lib/DETAILS.md` § \"The `walk` pass\" for depth.")
	writeDeadLinkFile(t, tmp, "lib/DETAILS.md", "## The `walk` pass\n\nText.\n")

	if _, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("quotes and backticks must normalize away: %v", err)
	}
}

func TestRunDocsSectionRefs_UnresolvableTargetIsDeadLinksJob(t *testing.T) {
	tmp := t.TempDir()
	// The path doesn't exist; docs-dead-links reports that. Reporting it twice, once
	// per check, would make one fix look like two failures.
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "See `lib/GONE.md` § Whatever.")

	if _, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("a missing target belongs to docs-dead-links, not here: %v", err)
	}
}

func TestRunDocsSectionRefs_SkipsSpecsAndNotes(t *testing.T) {
	tmp := t.TempDir()
	// Same carve-out as docs-dead-links: plans describe headings they intend to
	// write, and dated notes cite a doc as it read then.
	writeDeadLinkFile(t, tmp, "docs/specs/plan.md", "Lands in `lib/DETAILS.md` § A future section.")
	writeDeadLinkFile(t, tmp, "docs/notes/bench.md", "Measured per `lib/DETAILS.md` § An old section.")
	writeDeadLinkFile(t, tmp, "lib/DETAILS.md", "## The writer\n\nText.\n")

	if _, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("specs and notes are exempt: %v", err)
	}
}

func TestRunDocsSectionRefs_IgnoresFencedExample(t *testing.T) {
	tmp := t.TempDir()
	writeDeadLinkFile(t, tmp, "CLAUDE.md", "Write it like:\n\n```md\nSee `lib/DETAILS.md` § Some section.\n```\n")
	writeDeadLinkFile(t, tmp, "lib/DETAILS.md", "## The writer\n\nText.\n")

	if _, err := RunDocsSectionRefs(&CheckContext{RootDir: tmp}); err != nil {
		t.Fatalf("a § pointer inside a fence is an example: %v", err)
	}
}
