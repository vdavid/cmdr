package main

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
)

// contributorPR pushes a commit that changes a file to `refs/pull/<n>/head` on origin, where
// GitHub exposes a PR's head, and returns its SHA.
func contributorPR(t *testing.T, origin string, n int) string {
	t.Helper()
	c := testClone(t, origin, "main")
	commitFileOn(t, c, "theirs.txt")
	mustGit(t, c, "push", "--quiet", "origin", fmt.Sprintf("HEAD:refs/pull/%d/head", n))
	return mustGit(t, c, "rev-parse", "HEAD")
}

// moveMain lands an unrelated commit on origin's `main`.
func moveMain(t *testing.T, origin, file string) {
	t.Helper()
	o := testClone(t, origin, "main")
	commitFileOn(t, o, file)
	mustGit(t, o, "push", "--quiet", "origin", "main")
}

func commitFileOn(t *testing.T, r runner, name string) {
	t.Helper()
	if err := os.WriteFile(filepath.Join(r.dir, name), []byte(name), 0o644); err != nil {
		t.Fatal(err)
	}
	mustGit(t, r, "add", name)
	mustGit(t, r, "commit", "--quiet", "-m", "feat: "+name)
}

func originMain(t *testing.T, r runner) string {
	t.Helper()
	mustGit(t, r, "fetch", "--quiet", "origin", "main")
	return mustGit(t, r, "rev-parse", "origin/main")
}

func mustPRHead(t *testing.T, r runner, n int) string {
	t.Helper()
	sha, err := fetchPRHead(r, n)
	if err != nil {
		t.Fatal(err)
	}
	return sha
}

func TestEnsureRecordIsAnOursMergeOfThePRHead(t *testing.T) {
	origin := testOrigin(t)
	theirs := contributorPR(t, origin, 7)
	moveMain(t, origin, "ours.txt")
	a := testClone(t, origin, "worktree-pr7")

	prHead := mustPRHead(t, a, 7)
	if prHead != theirs {
		t.Fatalf("fetched %s, want the PR head %s", prHead, theirs)
	}
	main := originMain(t, a)
	sha, err := ensureRecord(a, 7, main, prHead)
	if err != nil {
		t.Fatal(err)
	}
	if got := originSHA(t, a, recordBranch(7)); got != sha {
		t.Errorf("origin/%s is %q, want the record %s", recordBranch(7), got, sha)
	}
	if parents := mustGit(t, a, "rev-list", "--parents", "-n", "1", sha); parents != sha+" "+main+" "+theirs {
		t.Errorf("parents are %q, want main %s, then the PR head %s", parents, main, theirs)
	}
	if mustGit(t, a, "rev-parse", sha+"^{tree}") != mustGit(t, a, "rev-parse", main+"^{tree}") {
		t.Error("the record changes the tree; it must carry only ancestry")
	}
	if subject := mustGit(t, a, "log", "-1", "--format=%s", sha); subject != "chore: record PR #7 as merged" {
		t.Errorf("subject is %q", subject)
	}
}

// A re-run while CI tests the record must not push a fresh SHA, which would restart CI.
func TestEnsureRecordReusesOriginsRecordOnTheSameMain(t *testing.T) {
	origin := testOrigin(t)
	contributorPR(t, origin, 7)
	a := testClone(t, origin, "worktree-pr7")
	first, err := ensureRecord(a, 7, originMain(t, a), mustPRHead(t, a, 7))
	if err != nil {
		t.Fatal(err)
	}

	// A rebuild within the same second would come out byte-identical; this date makes it differ.
	t.Setenv("GIT_COMMITTER_DATE", "2030-01-01T00:00:00Z")
	b := testClone(t, origin, "worktree-pr7")
	again, err := ensureRecord(b, 7, originMain(t, b), mustPRHead(t, b, 7))
	if err != nil {
		t.Fatal(err)
	}
	if again != first {
		t.Errorf("the re-run built %s, want origin's %s reused", again, first)
	}
}

// When `main` moves, the record is rebuilt on it (a fresh `ours` merge), never rebased.
func TestEnsureRecordRebuildsOnAMovedMain(t *testing.T) {
	origin := testOrigin(t)
	theirs := contributorPR(t, origin, 7)
	a := testClone(t, origin, "worktree-pr7")
	prHead := mustPRHead(t, a, 7)
	first, err := ensureRecord(a, 7, originMain(t, a), prHead)
	if err != nil {
		t.Fatal(err)
	}

	moveMain(t, origin, "later.txt")
	main := originMain(t, a)
	second, err := ensureRecord(a, 7, main, prHead)
	if err != nil {
		t.Fatal(err)
	}
	if second == first {
		t.Fatal("kept the record built on the old main")
	}
	if parents := mustGit(t, a, "rev-list", "--parents", "-n", "1", second); parents != second+" "+main+" "+theirs {
		t.Errorf("parents are %q, want the new main %s, then the PR head %s", parents, main, theirs)
	}
	if got := originSHA(t, a, recordBranch(7)); got != second {
		t.Errorf("origin/%s is %q, want the rebuilt record %s", recordBranch(7), got, second)
	}
}

func TestLandRecordFastForwardsMain(t *testing.T) {
	origin := testOrigin(t)
	theirs := contributorPR(t, origin, 7)
	a := testClone(t, origin, "worktree-pr7")
	sha, err := ensureRecord(a, 7, originMain(t, a), mustPRHead(t, a, 7))
	if err != nil {
		t.Fatal(err)
	}
	if err := landRecord(a, sha); err != nil {
		t.Fatal(err)
	}
	if got := originSHA(t, a, "main"); got != sha {
		t.Errorf("origin/main is %s, want the record %s", got, sha)
	}
	if !a.isAncestor(theirs, originMain(t, a)) {
		t.Error("the PR head isn't an ancestor of main, so GitHub wouldn't mark it merged")
	}
}

// A record is never rebased: when `main` moved, land builds a fresh one.
func TestLandRecordRefusesAMovedMain(t *testing.T) {
	origin := testOrigin(t)
	contributorPR(t, origin, 7)
	a := testClone(t, origin, "worktree-pr7")
	sha, err := ensureRecord(a, 7, originMain(t, a), mustPRHead(t, a, 7))
	if err != nil {
		t.Fatal(err)
	}
	moveMain(t, origin, "later.txt")
	if err := landRecord(a, sha); !errors.Is(err, errMainMoved) {
		t.Errorf("got %v, want errMainMoved", err)
	}
	if got := originSHA(t, a, "main"); got == sha {
		t.Error("the stale record landed")
	}
}

func TestVerifyRecordRefusesATreeChange(t *testing.T) {
	origin := testOrigin(t)
	contributorPR(t, origin, 7)
	a := testClone(t, origin, "worktree-pr7")
	prHead := mustPRHead(t, a, 7)
	main := originMain(t, a)
	// A real merge takes the PR's file into the tree: exactly what a record must never do.
	bad := mustGit(t, a, "commit-tree", prHead+"^{tree}", "-p", main, "-p", prHead, "-m", "chore: record PR #7 as merged")
	if err := verifyRecord(a, bad, main, prHead); err == nil {
		t.Error("verifyRecord accepted a record that changes the tree")
	}
	good := mustGit(t, a, "commit-tree", main+"^{tree}", "-p", main, "-p", prHead, "-m", "chore: record PR #7 as merged")
	if err := verifyRecord(a, good, main, prHead); err != nil {
		t.Errorf("verifyRecord refused a proper record: %v", err)
	}
}
