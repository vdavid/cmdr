package main

import (
	"errors"
	"fmt"
	"strconv"
	"time"
)

// The record phase of `pnpm land -record N`: once the branch with the contributor's cherry-picked
// commits landed, a branch of its own makes PR N's head an ancestor of `main`, so GitHub marks
// their PR merged (`docs/guides/handling-prs.md`). The record never rides the content branch: a
// merge commit can't survive the rebase a busy `main` forces on it.

// How long GitHub gets to mark the contributor's PR merged once its head is on `main`.
const mergedWithin = time.Minute

// record lands PR n's record, rebuilding it on the new `main` each time `main` moves while CI
// runs on it, up to `maxCIRuns` CI runs in all.
func record(r runner, cfg config) error {
	for run := 1; ; run++ {
		err := recordOnce(r, cfg)
		if !errors.Is(err, errMainMoved) {
			return err
		}
		if run >= maxCIRuns {
			return fmt.Errorf("origin/main moved during each of %d CI runs on the record, so I stopped. Record it again when it's quieter: %s", maxCIRuns, cfg.command())
		}
		say("origin/main moved while CI ran on the record, so rebuilding it on the new main (run %d of %d).", run+1, maxCIRuns)
	}
}

func recordOnce(r runner, cfg config) error {
	n := cfg.record
	var pr pullRequest
	if err := r.ghJSON(&pr, "pr", "view", strconv.Itoa(n), "-R", repo, "--json", "number,state,url"); err != nil {
		return err
	}
	if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
		return err
	}
	prHead, err := fetchPRHead(r, n)
	if err != nil {
		return err
	}
	switch nextRecordStep(pr.State, r.isAncestor(prHead, "origin/main")) {
	case recordAlreadyMerged:
		say("PR #%d is already merged: %s", n, pr.URL)
		tidyRecord(r, n)
		return nil
	case recordRefuseClosed:
		return fmt.Errorf("PR #%d is closed without a merge, and GitHub marks only an open PR merged. Reopen it, then record it again: %s", n, cfg.command())
	case recordConfirm:
		return confirmRecorded(r, cfg, pr)
	case recordBuild:
	}
	main, err := r.git("rev-parse", "origin/main")
	if err != nil {
		return err
	}
	sha, err := ensureRecord(r, n, main, prHead)
	if err != nil {
		return err
	}
	recordPR, err := ensurePR(r, recordBranch(n), []commit{recordMessage(n)})
	if err != nil {
		return err
	}
	if err := waitForCI(r, sha, cfg); err != nil {
		return err
	}
	if err := landRecord(r, sha); err != nil {
		return err
	}
	say("Landed the record %s on main (%s).", sha[:12], recordPR.URL)
	return confirmRecorded(r, cfg, pr)
}

// prHeadRef is where `fetchPRHead` keeps PR n's head, so it outlives the next fetch.
func prHeadRef(n int) string {
	return fmt.Sprintf("refs/land/pr-%d", n)
}

// fetchPRHead fetches PR n's head from `refs/pull/<n>/head`, where GitHub keeps it even for a fork.
func fetchPRHead(r runner, n int) (string, error) {
	ref := prHeadRef(n)
	if _, err := r.git("fetch", "--quiet", "origin", fmt.Sprintf("+refs/pull/%d/head:%s", n, ref)); err != nil {
		return "", fmt.Errorf("couldn't fetch PR #%d's head:\n%w", n, err)
	}
	return r.git("rev-parse", ref)
}

// ensureRecord puts PR n's record for `main` on origin's record branch and returns its SHA. A
// record origin already has for this exact `main` stays, since a fresh SHA would restart its CI;
// otherwise it builds one: `main`'s own tree with `main` and the PR head as parents, which is
// what `git merge -s ours --no-ff` makes, without checking anything out.
func ensureRecord(r runner, n int, main, prHead string) (string, error) {
	branch := recordBranch(n)
	remote, err := r.fetchBranch(branch)
	if err != nil {
		return "", err
	}
	if remote != "" && isRecordOf(r, remote, main, prHead) {
		return remote, nil
	}
	msg := recordMessage(n)
	sha, err := r.git("commit-tree", main+"^{tree}", "-p", main, "-p", prHead, "-m", msg.Subject, "-m", msg.Body)
	if err != nil {
		return "", err
	}
	if err := verifyRecord(r, sha, main, prHead); err != nil {
		return "", err
	}
	say("Pushing %s...", branch)
	// The branch is land's own and rebuilt whenever `main` moves, so overwriting it is the point.
	// `--no-verify`: the hooks check the worktree, and the record changes no file.
	if _, err := r.git("push", "--quiet", "--no-verify", "--force", "origin", sha+":refs/heads/"+branch); err != nil {
		return "", fmt.Errorf("the record push didn't go through:\n%w", err)
	}
	return sha, nil
}

// isRecordOf reports whether sha is exactly the record of prHead for main.
func isRecordOf(r runner, sha, main, prHead string) bool {
	parents, err := r.git("rev-list", "--parents", "-n", "1", sha)
	return err == nil && parents == sha+" "+main+" "+prHead && sameTree(r, sha, main)
}

// verifyRecord is the last look before a record goes out: the same tree as `main`, so it lands
// nothing CI has to check, with both `main` and the PR head in its history.
func verifyRecord(r runner, sha, main, prHead string) error {
	switch {
	case !sameTree(r, sha, main):
		return fmt.Errorf("the record %s changes files, so I didn't push it: a record carries only ancestry", sha[:12])
	case !r.isAncestor(main, sha):
		return fmt.Errorf("the record %s doesn't descend from origin/main, so it can't fast-forward it", sha[:12])
	case !r.isAncestor(prHead, sha):
		return fmt.Errorf("the record %s doesn't carry the PR head %s", sha[:12], prHead[:12])
	}
	return nil
}

func sameTree(r runner, a, b string) bool {
	ta, errA := r.git("rev-parse", a+"^{tree}")
	tb, errB := r.git("rev-parse", b+"^{tree}")
	return errA == nil && errB == nil && ta == tb
}

// landRecord fast-forwards `main` to the green record. It's never rebased, here or on GitHub (a
// rebase would flatten the merge), so a `main` that moved means a fresh record: errMainMoved.
func landRecord(r runner, sha string) error {
	if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
		return err
	}
	if !r.isAncestor("origin/main", sha) {
		return errMainMoved
	}
	err := pushToMain(r, sha)
	if err == nil {
		return nil
	}
	if _, fetchErr := r.git("fetch", "--quiet", "origin", "main"); fetchErr == nil && !r.isAncestor("origin/main", sha) {
		return errMainMoved
	}
	return fmt.Errorf("origin refused the record:\n%w", err)
}

// confirmRecorded waits for GitHub to mark the contributor's PR merged now that its head is on
// `main`, then tidies the record branch.
func confirmRecorded(r runner, cfg config, pr pullRequest) error {
	deadline := time.Now().Add(mergedWithin)
	for pr.State != "MERGED" && time.Now().Before(deadline) {
		time.Sleep(5 * time.Second)
		if err := r.ghJSON(&pr, "pr", "view", strconv.Itoa(cfg.record), "-R", repo, "--json", "number,state,url"); err != nil {
			return err
		}
	}
	tidyRecord(r, cfg.record)
	if pr.State != "MERGED" {
		return fmt.Errorf("PR #%d's head is on main, but GitHub still shows the PR as %s after %s: %s\nCheck again in a bit: %s", cfg.record, pr.State, mergedWithin, pr.URL, cfg.command())
	}
	say("GitHub marks PR #%d merged: %s", cfg.record, pr.URL)
	return nil
}

// tidyRecord deletes the record branch on origin, stops its leftover runs, and drops the local
// refs. Best-effort, like `finish`: the record is done either way.
func tidyRecord(r runner, n int) {
	branch := recordBranch(n)
	if remote, err := r.remoteSHA(branch); err == nil && remote != "" {
		if _, err := r.git("push", "--quiet", "--no-verify", "origin", "--delete", branch); err != nil {
			say("Couldn't delete origin/%s: %v", branch, err)
		}
	}
	stopLeftoverRuns(r, branch)
	for _, ref := range []string{prHeadRef(n), "refs/remotes/origin/" + branch} {
		_, _ = r.git("update-ref", "-d", ref)
	}
}
