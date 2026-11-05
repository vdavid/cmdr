package main

import (
	"fmt"
	"net/url"
)

// The tidy-up after a land: sync local `main`, delete the landed remote branch, and stop the
// branch's leftover runs.

// finish syncs local `main` with origin's and deletes the landed remote branch. Both are
// best-effort: the land already happened, so trouble here is a message, not a failure.
func finish(r runner, branch string) error {
	syncMain(r)
	deleteLandedBranch(r, branch)
	stopLeftoverRuns(r, branch)
	return nil
}

// deleteLandedBranch deletes origin's branch once `origin/main` holds all of it, whichever way it
// landed (a fast-forward, or a merge commit with the head as a parent). It asks `main`, never the
// local HEAD: a worktree that committed more while CI ran still has a fully landed remote branch.
// Best-effort, like the rest of `finish`.
func deleteLandedBranch(r runner, branch string) {
	remote, err := r.fetchBranch(branch)
	if err != nil {
		say("Couldn't read origin/%s, so I left it: %v", branch, err)
		return
	}
	if remote == "" {
		return
	}
	if !r.isAncestor(remote, "origin/main") {
		say("Left origin/%s alone: it points at %s, which has commits main doesn't.", branch, remote[:12])
		return
	}
	if _, err := r.git("push", "--quiet", "--no-verify", "origin", "--delete", branch); err != nil {
		say("Couldn't delete origin/%s: %v", branch, err)
	}
}

// stopLeftoverRuns cancels the landed branch's unfinished runs (`leftoverRuns`): an older push's
// CI, CodeQL, a land run. Best-effort, like the rest of `finish`.
func stopLeftoverRuns(r runner, branch string) {
	var runs struct {
		WorkflowRuns []workflowRun `json:"workflow_runs"`
	}
	if err := r.ghAPI(&runs, fmt.Sprintf("repos/%s/actions/runs?branch=%s&per_page=100", repo, url.QueryEscape(branch))); err != nil {
		say("Couldn't list %s's runs to stop the leftovers: %v", branch, err)
		return
	}
	for _, run := range leftoverRuns(runs.WorkflowRuns, repo) {
		// A run can finish between the list and the cancel, so a refusal is only a note.
		if _, err := r.gh("api", "-X", "POST", fmt.Sprintf("repos/%s/actions/runs/%d/cancel", repo, run.ID), "--silent"); err != nil {
			say("Couldn't stop leftover run %d (it may have just finished): %v", run.ID, err)
			continue
		}
		say("Stopped leftover run https://github.com/%s/actions/runs/%d, which nothing waits on now.", repo, run.ID)
	}
}

// syncMain fast-forwards local `main` to `origin/main`, never checking it out here: wherever it's
// checked out, that worktree moves only when it's clean.
func syncMain(r runner) {
	if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
		say("Couldn't fetch origin/main: %v", err)
		return
	}
	list, err := r.git("worktree", "list", "--porcelain")
	if err != nil {
		say("Couldn't list worktrees: %v", err)
		return
	}
	wt, checkedOut := worktreeOf(parseWorktrees(list), "main")
	if !checkedOut {
		if _, err := r.git("fetch", "--quiet", "origin", "main:main"); err != nil {
			say("Local main didn't fast-forward; see what it has that origin lacks: git log origin/main..main\n%v", err)
			return
		}
		say("Local main is at origin/main.")
		return
	}
	at := runner{dir: wt.path}
	if status, err := at.git("status", "--porcelain"); err != nil || status != "" {
		say("main is checked out at %s with uncommitted changes, so I left it. Fast-forward it when it's clean:\n  git -C %s merge --ff-only origin/main", wt.path, wt.path)
		return
	}
	if _, err := at.git("merge", "--quiet", "--ff-only", "origin/main"); err != nil {
		say("main at %s didn't fast-forward; see what it has that origin lacks: git -C %s log origin/main..main\n%v", wt.path, wt.path, err)
		return
	}
	say("Local main (%s) is at origin/main.", wt.path)
}
