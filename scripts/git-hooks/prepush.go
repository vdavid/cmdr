package main

import (
	"bufio"
	"fmt"
	"io"
	"slices"
	"strings"
	"sync"
)

const formatCommitSubject = "style: apply formatter output"

// prePush is the backstop for what pre-commit never sees: a rebase, a conflict
// resolution, a `--no-verify` commit, a generated file. It runs the same
// whole-repo checks as CI's formatting lanes, and when they'd fail it formats,
// commits, and stops the push. Then it does the same for the license notices a
// dependency push makes stale, and runs CI's size limits (`prepush_checks.go`).
//
// Stopping is not a choice. Git settles which commits a push sends before it runs
// this hook, so a commit made here can't join the push in flight; letting the push
// continue would send the unformatted tip and leave the fix behind.
func prePush(r repo, fs []formatter, check checkRunner, stdin io.Reader, stderr io.Writer) (stopPush bool, err error) {
	pushed, err := pushedBranches(stdin)
	if err != nil {
		return false, err
	}
	if len(pushed) == 0 {
		return false, nil
	}
	ref, skipped := describesPush(r, pushed)
	if skipped != "" {
		fmt.Fprintf(stderr, "pre-push: skipped formatting, license notices, and size limits, %s. CI still checks them.\n", skipped)
		return false, nil
	}

	if stop, err := commitFormatting(r, fs, ref, stderr); stop || err != nil {
		return stop, err
	}
	if stop, err := regenerateNotices(r, check, ref, stderr); stop || err != nil {
		return stop, err
	}
	return checkBudgets(r, check, stderr)
}

// commitFormatting formats what CI's formatting lanes would flag, commits it, and
// reports whether it did (which stops the push).
func commitFormatting(r repo, fs []formatter, pushed pushedRef, stderr io.Writer) (stopPush bool, err error) {
	fixable, err := fixableFiles(r, unformattedFiles(fs, stderr))
	if err != nil || len(fixable) == 0 {
		return false, err
	}
	for _, f := range fs {
		if err := f.format(fixable); err != nil {
			fmt.Fprintf(stderr, "format hook: %v\n", err)
		}
	}
	// The fixable files matched the index, so the ones that differ now are the ones
	// a formatter rewrote. If none did, a check and its formatter disagree, and
	// stopping the push would stop every retry as well.
	modified, err := r.paths("diff", "--name-only", "-z")
	if err != nil {
		return false, err
	}
	formatted := intersect(fixable, toSet(modified))
	if len(formatted) == 0 {
		return false, nil
	}

	sha, err := commitOnly(r, formatted, formatCommitSubject)
	if err != nil {
		return false, err
	}
	fmt.Fprintf(stderr, "Formatted %d %s and committed the result as %s (%q).\n"+
		"Git can't add a commit to a push that's already running, so this push stopped. %s\n",
		len(formatted), pluralize(len(formatted), "file", "files"), sha, formatCommitSubject, pushAgain(r, pushed))
	return true, nil
}

// commitOnly commits these paths and nothing else, whatever is staged (`--only`),
// and returns the new commit's short sha. The pre-commit hook has nothing left to
// do for files a hook just rewrote, so it's skipped.
func commitOnly(r repo, paths []string, subject string) (string, error) {
	if _, err := r.gitWithStdin(joinNul(paths), "commit", "--quiet", "--no-verify", "--only",
		"-m", subject, "--pathspec-from-file=-", "--pathspec-file-nul"); err != nil {
		return "", err
	}
	sha, err := r.git("rev-parse", "--short", "HEAD")
	return strings.TrimSpace(sha), err
}

// pushedRef is one branch in a push: the local tip being sent and the remote tip
// it replaces (all zeros for a new branch).
type pushedRef struct {
	localRef, localSha, remoteRef, remoteSha string
}

// pushedBranches reads what git feeds a pre-push hook, one line per ref
// (`<local ref> <local sha> <remote ref> <remote sha>`), and returns the pushes
// that update a remote branch. Tags and deletions are left out: neither puts new
// branch content on the remote.
func pushedBranches(stdin io.Reader) ([]pushedRef, error) {
	var branches []pushedRef
	scanner := bufio.NewScanner(stdin)
	for scanner.Scan() {
		fields := strings.Fields(scanner.Text())
		if len(fields) != 4 {
			continue
		}
		p := pushedRef{localRef: fields[0], localSha: fields[1], remoteRef: fields[2], remoteSha: fields[3]}
		if strings.HasPrefix(p.remoteRef, "refs/heads/") && !isZeroSha(p.localSha) {
			branches = append(branches, p)
		}
	}
	return branches, scanner.Err()
}

// isZeroSha reports whether git means "no commit": a new remote branch's tip, or
// a deletion's local one.
func isZeroSha(sha string) bool {
	return strings.Trim(sha, "0") == ""
}

// describesPush picks the push the working tree is a fair stand-in for, or says
// why there's none. The checks read files on disk, so their verdict only applies to
// a pushed commit that IS the checked-out one, whatever the refs are called: agents
// push a worktree branch as `HEAD:main` or `push-staging:main`, and matching the
// checked-out branch's name let those reach `main` unchecked. A commit can only be
// added while no merge, rebase, or cherry-pick is underway.
func describesPush(r repo, pushed []pushedRef) (ref pushedRef, skipped string) {
	head, err := r.git("rev-parse", "--verify", "--quiet", "HEAD")
	if err != nil {
		return pushedRef{}, "there's no commit checked out here"
	}
	var candidates []pushedRef
	for _, p := range pushed {
		if p.localSha == strings.TrimSpace(head) {
			candidates = append(candidates, p)
		}
	}
	if len(candidates) == 0 {
		return pushedRef{}, "the pushed commit isn't checked out here"
	}
	inProgress := []string{"MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply"}
	if slices.ContainsFunc(inProgress, r.gitPathExists) {
		return pushedRef{}, "a merge, rebase, cherry-pick, or revert is underway"
	}
	// A commit the hook adds lands on HEAD, so prefer a push that'll pick it up.
	if i := slices.IndexFunc(candidates, func(p pushedRef) bool { return followsHead(r, p) }); i >= 0 {
		return candidates[i], ""
	}
	return candidates[0], ""
}

// followsHead reports whether a push of this local ref picks up a commit added on
// top of HEAD: it's `HEAD` itself, or the branch HEAD points at.
func followsHead(r repo, p pushedRef) bool {
	if p.localRef == "HEAD" {
		return true
	}
	branch, err := r.git("symbolic-ref", "--quiet", "HEAD")
	return err == nil && strings.TrimSpace(branch) == p.localRef
}

// pushAgain tells someone whose push stopped for a new commit how to send it. When
// the pushed ref doesn't follow HEAD, pushing it again would resend the old commit.
func pushAgain(r repo, p pushedRef) string {
	if followsHead(r, p) {
		return "Push again to send everything."
	}
	return fmt.Sprintf("The commit is on HEAD, which %s doesn't follow, so push HEAD this time (for example `git push origin HEAD:%s`).",
		p.localRef, strings.TrimPrefix(p.remoteRef, "refs/heads/"))
}

// unformattedFiles asks every formatter, in parallel, what its CI lane would flag.
// A formatter that can't answer is reported and skipped.
func unformattedFiles(fs []formatter, stderr io.Writer) []string {
	results := make([][]string, len(fs))
	errs := make([]error, len(fs))
	var wg sync.WaitGroup
	for i, f := range fs {
		wg.Go(func() {
			results[i], errs[i] = f.unformatted()
		})
	}
	wg.Wait()

	var files []string
	for i, f := range fs {
		if errs[i] != nil {
			fmt.Fprintf(stderr, "format hook: skipped %s, %v\n", f.name(), errs[i])
			continue
		}
		files = append(files, results[i]...)
	}
	return files
}

// fixableFiles narrows unformatted files to those the hook may rewrite and commit:
// tracked, and identical to the commit being pushed. A file with uncommitted
// changes is someone's work in progress; formatting it says nothing about the
// pushed commit, and committing it would sweep that work along. Untracked files
// aren't part of the push at all.
func fixableFiles(r repo, unformatted []string) ([]string, error) {
	if len(unformatted) == 0 {
		return nil, nil
	}
	tracked, err := r.paths("ls-files", "-z")
	if err != nil {
		return nil, err
	}
	changedInWorktree, err := r.paths("diff", "--name-only", "-z", "HEAD")
	if err != nil {
		return nil, err
	}
	changedInIndex, err := r.paths("diff", "--cached", "--name-only", "-z")
	if err != nil {
		return nil, err
	}
	isTracked, inWorktree, inIndex := toSet(tracked), toSet(changedInWorktree), toSet(changedInIndex)

	var fixable []string
	for _, p := range unformatted {
		if isTracked[p] && !inWorktree[p] && !inIndex[p] {
			fixable = append(fixable, p)
		}
	}
	return fixable, nil
}
