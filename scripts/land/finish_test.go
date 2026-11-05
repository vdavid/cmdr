package main

import "testing"

// Regression: a land whose worktree got a new local commit while CI ran (the agent kept working)
// left the landed remote branch behind, saying it "isn't what landed", because the cleanup
// compared origin's branch with the local HEAD rather than with `main`.
func TestDeleteLandedBranchDeletesItAfterANewLocalCommit(t *testing.T) {
	origin := testOrigin(t)
	r := testClone(t, origin, "feature")
	head := commitAndPush(t, r, "feature", "the landed change")
	mustGit(t, r, "push", "--quiet", "origin", head+":refs/heads/main")
	mustGit(t, r, "commit", "--quiet", "--allow-empty", "-m", "work after the land started")
	mustGit(t, r, "fetch", "--quiet", "origin", "main")

	deleteLandedBranch(r, "feature")

	if sha := originSHA(t, r, "feature"); sha != "" {
		t.Fatalf("origin/feature should be gone, still at %s", sha)
	}
}

// GitHub's merge commit lands a head that `main` then only has as a parent.
func TestDeleteLandedBranchDeletesItAfterAMergeCommitLand(t *testing.T) {
	origin := testOrigin(t)
	r := testClone(t, origin, "feature")
	head := commitAndPush(t, r, "feature", "the landed change")
	other := testClone(t, origin, "main")
	mustGit(t, other, "commit", "--quiet", "--allow-empty", "-m", "main moved")
	mustGit(t, other, "fetch", "--quiet", "origin", "feature")
	mustGit(t, other, "merge", "--quiet", "--no-ff", "-m", "merge", head)
	mustGit(t, other, "push", "--quiet", "origin", "main")
	mustGit(t, r, "fetch", "--quiet", "origin", "main")

	deleteLandedBranch(r, "feature")

	if sha := originSHA(t, r, "feature"); sha != "" {
		t.Fatalf("origin/feature should be gone, still at %s", sha)
	}
}

// A branch with commits `main` lacks (pushed from elsewhere after the land) stays.
func TestDeleteLandedBranchKeepsABranchWithUnlandedCommits(t *testing.T) {
	origin := testOrigin(t)
	r := testClone(t, origin, "feature")
	head := commitAndPush(t, r, "feature", "the landed change")
	mustGit(t, r, "push", "--quiet", "origin", head+":refs/heads/main")
	elsewhere := testClone(t, origin, "feature")
	newer := commitAndPush(t, elsewhere, "feature", "pushed from another machine")
	mustGit(t, r, "fetch", "--quiet", "origin", "main")

	deleteLandedBranch(r, "feature")

	if sha := originSHA(t, r, "feature"); sha != newer {
		t.Fatalf("origin/feature should stay at %s, got %q", newer, sha)
	}
}
