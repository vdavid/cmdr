package main

import (
	"cmp"
	"fmt"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"time"
)

// The decisions that make `land` idempotent: each run reads where the branch stands locally
// and on GitHub and picks up from there, so it holds no state of its own between runs.

// branchState is what the local clone says about the branch, right after a fetch.
type branchState struct {
	// branch is the checked-out branch, empty on a detached HEAD.
	branch string
	dirty  bool
	// landed: HEAD is already on origin/main (a finished land, or a branch with no commits).
	landed bool
	// based: origin/main is an ancestor of HEAD, so the branch fast-forwards onto it.
	based bool
	// hasMerges: the branch carries a merge commit, which a plain rebase would flatten. A
	// contributor's PR record never rides a branch: `-record N` lands it alone.
	hasMerges bool
	// tagged: a tag points at one of the branch's commits (a release commit), so a rebase would
	// leave the tag off `main`'s history. `-merge` lands it as it is.
	tagged bool
}

type startStep int

const (
	// stepRefuseBranch: on `main` or a detached HEAD, there's no feature branch to land.
	stepRefuseBranch startStep = iota
	stepRefuseDirty
	// stepFinish: nothing left to land, so only sync local `main` and tidy the remote branch.
	stepFinish
	// stepRebase: `origin/main` moved, so catch up before pushing.
	stepRebase
	// stepRefuseMergeRebase: `origin/main` moved under a merge commit; say how to catch up by hand
	// and stop.
	stepRefuseMergeRebase
	// stepRefuseTaggedRebase: `origin/main` moved under a tagged commit, which only `-merge` lands.
	stepRefuseTaggedRebase
	stepProceed
)

// nextStartStep decides from the local state what a run does first. With `-merge` the branch is
// never rebased: it lands as it is, by fast-forward or a merge commit GitHub makes.
func nextStartStep(s branchState, merge bool) startStep {
	switch {
	case s.branch == "" || s.branch == "main":
		return stepRefuseBranch
	case s.dirty:
		return stepRefuseDirty
	case s.landed:
		return stepFinish
	case merge || s.based:
		return stepProceed
	case s.tagged:
		return stepRefuseTaggedRebase
	case !s.based && s.hasMerges:
		return stepRefuseMergeRebase
	case !s.based:
		return stepRebase
	default:
		return stepProceed
	}
}

// patchesUpstream reports whether `git cherry <upstream> <branch>` found every commit already
// upstream ("-" lines only), as after GitHub rebased the branch onto `main` under fresh SHAs.
func patchesUpstream(cherry string) bool {
	for line := range strings.SplitSeq(cherry, "\n") {
		if line != "" && !strings.HasPrefix(line, "- ") {
			return false
		}
	}
	return true
}

// landPath is how a branch whose head is green on CI gets onto `main`.
type landPath int

const (
	// landFastForward: the head still descends from `origin/main`, so push it there as is.
	landFastForward landPath = iota
	// landGitHubRebase: `origin/main` moved, so GitHub rebases the green PR onto it, and CI on
	// `main` checks the result.
	landGitHubRebase
	// landRebaseAndRerun: GitHub can't rebase it (a conflict, or a merge commit a rebase would
	// flatten), so the branch catches up here and CI runs again.
	landRebaseAndRerun
	// landGitHubMerge (`-merge`): `origin/main` moved, so GitHub merges the green PR into it with a
	// merge commit, keeping the head's SHA (and its tag) on `main`'s history.
	landGitHubMerge
	// landMergeConflict (`-merge`): GitHub can't merge it cleanly; a person merges by hand.
	landMergeConflict
)

// nextLandPath picks the land path. rebaseable is GitHub's view of the PR, nil while it's still
// computing it.
func nextLandPath(based, hasMerges bool, rebaseable *bool) landPath {
	switch {
	case based:
		return landFastForward
	case hasMerges, rebaseable != nil && !*rebaseable:
		return landRebaseAndRerun
	default:
		// Still computing: try, and GitHub refuses the merge if it can't rebase.
		return landGitHubRebase
	}
}

// nextMergePath picks the land path of `-merge`. mergeable is GitHub's view of the PR, nil while
// it's still computing it.
func nextMergePath(based bool, mergeable *bool) landPath {
	switch {
	case based:
		return landFastForward
	case mergeable != nil && !*mergeable:
		return landMergeConflict
	default:
		// Still computing: try, and GitHub refuses the merge if it can't make it.
		return landGitHubMerge
	}
}

// recordStep is what `-record N` does once the branch itself landed.
type recordStep int

const (
	// recordRefuseClosed: the PR is closed unmerged, and GitHub marks only an open PR merged.
	recordRefuseClosed recordStep = iota
	recordAlreadyMerged
	// recordConfirm: the PR head is already on `main` (a record landed), so only wait for GitHub
	// to mark the PR merged, then tidy up.
	recordConfirm
	// recordBuild: build the record, put it through CI, and land it.
	recordBuild
)

// nextRecordStep decides from the contributor PR's state (`gh pr view --json state`) and whether
// its head is already an ancestor of `origin/main`.
func nextRecordStep(prState string, headOnMain bool) recordStep {
	switch {
	case prState == "MERGED":
		return recordAlreadyMerged
	case prState != "OPEN":
		return recordRefuseClosed
	case headOnMain:
		return recordConfirm
	default:
		return recordBuild
	}
}

// recordBranch is the branch that carries PR n's record through CI.
func recordBranch(n int) string {
	return "record-pr-" + strconv.Itoa(n)
}

// recordMessage is the record commit's message, which its PR also takes as title and body.
func recordMessage(n int) commit {
	return commit{
		Subject: fmt.Sprintf("chore: record PR #%d as merged", n),
		Body:    fmt.Sprintf("An `ours` merge of PR #%d's head: it changes no file, only makes that head an ancestor of `main`, so GitHub marks the PR merged. Its commits landed cherry-picked, in their own land. See `docs/guides/handling-prs.md`.", n),
	}
}

// teardownCommand is the command that removes the worktree at root, or "" when root isn't one of
// the `.claude/worktrees/<slug>` worktrees (the main clone, where `release.sh` lands from).
func teardownCommand(root string) string {
	dir, slug := filepath.Split(filepath.Clean(root))
	if filepath.Base(filepath.Clean(dir)) != "worktrees" || filepath.Base(filepath.Dir(filepath.Clean(dir))) != ".claude" {
		return ""
	}
	return "~/.claude/scripts/remove-worktree.sh " + slug
}

// commit is one commit message, split the way `git log` gives it.
type commit struct {
	Subject string
	Body    string
}

func (c commit) message() string {
	if c.Body == "" {
		return c.Subject + "\n"
	}
	return c.Subject + "\n\n" + c.Body + "\n"
}

// prTitle names the PR after the branch's oldest commit, the one that says what the branch is for.
func prTitle(commits []commit) string {
	if len(commits) == 0 {
		return ""
	}
	return commits[0].Subject
}

// prBody lists every commit message, oldest first, one heading each.
func prBody(commits []commit) string {
	parts := make([]string, 0, len(commits))
	for _, c := range commits {
		parts = append(parts, "### "+c.message())
	}
	return strings.Join(parts, "\n")
}

// squashMessage keeps every commit message, oldest first, so the oldest subject becomes the title.
func squashMessage(commits []commit) string {
	parts := make([]string, 0, len(commits))
	for _, c := range commits {
		parts = append(parts, c.message())
	}
	return strings.Join(parts, "\n")
}

// needsSquash reports whether `-squash` would change anything, so a re-run after a squash is a
// no-op and doesn't force-push a fresh SHA that restarts CI.
func needsSquash(commits []commit, msg string) bool {
	if len(commits) != 1 {
		return len(commits) > 1
	}
	return msg != "" && strings.TrimSpace(msg) != strings.TrimSpace(commits[0].message())
}

// workflowRun is one of `repos/{repo}/actions/workflows/{file}/runs`' `workflow_runs`.
type workflowRun struct {
	ID             int64     `json:"id"`
	Status         string    `json:"status"`
	Conclusion     string    `json:"conclusion"`
	CreatedAt      time.Time `json:"created_at"`
	HeadRepository struct {
		FullName string `json:"full_name"`
	} `json:"head_repository"`
}

// leftoverRuns picks the runs a landed branch leaves behind: unfinished, and on this repo's
// branch, not a fork's namesake. GitHub keeps them going after the PR merges and the branch is
// deleted, holding runners (the five macOS ones above all) that nobody waits on anymore. `repo`
// is this repo's current name (`resolveRepo`).
func leftoverRuns(runs []workflowRun, repo string) []workflowRun {
	var out []workflowRun
	for _, run := range runs {
		if run.Status != "completed" && run.HeadRepository.FullName == repo {
			out = append(out, run)
		}
	}
	return out
}

const (
	// A run shows up seconds after the push.
	appearPoll = 20 * time.Second
	// CI takes 25–35 min (a warm re-run about 18), so a run younger than `earlyFor` is checked
	// rarely, and an older one every minute, since it may finish any time.
	earlyPoll = 3 * time.Minute
	earlyFor  = 15 * time.Minute
	latePoll  = time.Minute
)

// missingRunGivesUp says whether a poll that found no CI run on the head ends the wait. Only a
// run that never showed up does: one that was seen and then dropped out of the list (it
// happened 2026-10-20, with the run still queued) is still on its way, and the wait's own
// timeout bounds it.
func missingRunGivesUp(seen bool, waited time.Duration) bool {
	return !seen && waited > appearWithin
}

// pollDelay is how long to wait before the next CI poll, before jitter.
func pollDelay(v verdict, runAge time.Duration) time.Duration {
	switch {
	case v == verdictMissing:
		return appearPoll
	case runAge < earlyFor:
		return earlyPoll
	default:
		return latePoll
	}
}

// checkRun is one `CI OK` check run on the head SHA; a re-run adds a new one with a higher id.
type checkRun struct {
	ID         int64  `json:"id"`
	Status     string `json:"status"`
	Conclusion string `json:"conclusion"`
}

type verdict int

const (
	// verdictMissing: no CI run on the SHA yet (just pushed, or the PR isn't open).
	verdictMissing verdict = iota
	verdictPending
	verdictGreen
	verdictRed
)

// ciVerdict reads the CI runs and `CI OK` check runs on one SHA. The run says whether CI is
// still going (a re-run reopens it), and the newest `CI OK` says how it ended, because that's
// the exact check `main`'s ruleset demands. Only `success` is green: GitHub would also accept a
// skipped check, which is never what a finished CI run should produce.
func ciVerdict(runs []workflowRun, checks []checkRun) (verdict, workflowRun) {
	if len(runs) == 0 {
		return verdictMissing, workflowRun{}
	}
	run := slices.MaxFunc(runs, func(a, b workflowRun) int { return a.CreatedAt.Compare(b.CreatedAt) })
	if run.Status != "completed" {
		return verdictPending, run
	}
	if len(checks) == 0 {
		return verdictRed, run
	}
	check := slices.MaxFunc(checks, func(a, b checkRun) int { return cmp.Compare(a.ID, b.ID) })
	if check.Status == "completed" && check.Conclusion == "success" {
		return verdictGreen, run
	}
	return verdictRed, run
}

// pullRequest is one row of `gh pr list --json number,state,url`.
type pullRequest struct {
	Number int    `json:"number"`
	State  string `json:"state"`
	URL    string `json:"url"`
}

// pullDetails is the part of `repos/{repo}/pulls/{n}` a GitHub merge decides on. Rebaseable and
// Mergeable are null while GitHub is still working them out.
type pullDetails struct {
	Merged     bool  `json:"merged"`
	Rebaseable *bool `json:"rebaseable"`
	Mergeable  *bool `json:"mergeable"`
}

// openPR finds the branch's open PR; a closed one is never revived.
func openPR(prs []pullRequest) (pullRequest, bool) {
	for _, pr := range prs {
		if pr.State == "OPEN" {
			return pr, true
		}
	}
	return pullRequest{}, false
}

// prNumber reads the number off a PR URL, the one thing `gh pr create` prints.
func prNumber(url string) (int, bool) {
	n, err := strconv.Atoi(url[strings.LastIndex(url, "/")+1:])
	return n, err == nil && n > 0
}

// worktree is one entry of `git worktree list --porcelain`.
type worktree struct {
	path   string
	branch string
}

func parseWorktrees(porcelain string) []worktree {
	var out []worktree
	for block := range strings.SplitSeq(porcelain, "\n\n") {
		var wt worktree
		for line := range strings.SplitSeq(block, "\n") {
			if p, ok := strings.CutPrefix(line, "worktree "); ok {
				wt.path = p
			} else if b, ok := strings.CutPrefix(line, "branch refs/heads/"); ok {
				wt.branch = b
			}
		}
		if wt.path != "" {
			out = append(out, wt)
		}
	}
	return out
}

// worktreeOf finds the worktree that has a branch checked out.
func worktreeOf(wts []worktree, branch string) (worktree, bool) {
	for _, wt := range wts {
		if wt.branch == branch {
			return wt, true
		}
	}
	return worktree{}, false
}
