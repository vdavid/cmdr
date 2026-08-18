package main

import (
	"strings"
	"testing"
)

func TestParseDirtyFilesReadsPorcelainAndSkipsUntracked(t *testing.T) {
	porcelain := " M apps/desktop/src/lib/foo.ts\x00" +
		"?? scripts/check/checks/new-allowlist.json\x00" +
		// `-z` puts the NEW path in the entry and the original in the next field
		// (verified against git 2.x: `R  new.txt\0old.txt\0`).
		"R  new/path.rs\x00old/path.rs\x00" +
		"MM crates/cmdr-fs/src/lib.rs\x00"

	dirty := parseDirtyFiles(porcelain)

	if !dirty["apps/desktop/src/lib/foo.ts"] || !dirty["crates/cmdr-fs/src/lib.rs"] {
		t.Fatalf("modified tracked files missing: %v", dirty)
	}
	// A file the fixer CREATED can't break CI's check-only run of a formatter, and
	// untracked noise (build output, scratch files) would drown the signal.
	if dirty["scripts/check/checks/new-allowlist.json"] {
		t.Fatal("untracked files must not count as dirty")
	}
	// A rename's `-> new` half is the path that exists now.
	if !dirty["new/path.rs"] || dirty["old/path.rs"] {
		t.Fatalf("rename should track the new path: %v", dirty)
	}
}

func TestNewlyDirtyNamesOnlyFilesTheRunItselfChanged(t *testing.T) {
	before := parseDirtyFiles(" M already-editing.ts\x00")
	after := parseDirtyFiles(" M already-editing.ts\x00 M was-committed.ts\x00 M also-committed.rs\x00")

	got := newlyDirty(before, after)

	if len(got) != 2 || got[0] != "also-committed.rs" || got[1] != "was-committed.ts" {
		t.Fatalf("newlyDirty = %v, want the two committed files, sorted", got)
	}
}

func TestNewlyDirtyIsQuietWhenTheRunChangedNothing(t *testing.T) {
	before := parseDirtyFiles(" M a.ts\x00 M b.ts\x00")
	if got := newlyDirty(before, before); len(got) != 0 {
		t.Fatalf("newlyDirty = %v, want none", got)
	}
}

func TestFormatAutoFixNoticeNamesEveryFileAndSaysWhyItMatters(t *testing.T) {
	notice := formatAutoFixNotice([]string{"a.ts", "b.rs"})

	for _, want := range []string{"a.ts", "b.rs", "commit"} {
		if !strings.Contains(strings.ToLower(notice), want) {
			t.Fatalf("notice is missing %q:\n%s", want, notice)
		}
	}
}
