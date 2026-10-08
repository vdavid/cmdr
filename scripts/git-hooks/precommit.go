package main

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

// restageFile, inside the worktree's git directory, carries the files pre-commit
// formatted over to post-commit when the commit runs from a temporary index.
const restageFile = "cmdr-format-restage"

// preCommit formats the staged files and re-stages them, so the formatting lands
// inside the commit being made.
func preCommit(r repo, fs []formatter, stderr io.Writer) error {
	restagePath, err := r.gitPath(restageFile)
	if err != nil {
		return err
	}
	// A commit that died after an earlier pre-commit leaves its list behind.
	_ = os.Remove(restagePath)

	candidates, err := fullyStagedFiles(r)
	if err != nil || len(candidates) == 0 {
		return err
	}
	for _, f := range fs {
		if err := f.format(candidates); err != nil {
			// One unparseable file mustn't cost the others their formatting.
			fmt.Fprintf(stderr, "format hook: %v\n", err)
		}
	}

	// The candidates matched the index before formatting, so whichever of them
	// differ from it now are exactly the files a formatter rewrote.
	modified, err := r.paths("diff", "--name-only", "-z")
	if err != nil {
		return err
	}
	formatted := intersect(candidates, toSet(modified))
	if len(formatted) == 0 {
		return nil
	}
	if err := r.add(formatted); err != nil {
		return err
	}
	if commitsFromTemporaryIndex() {
		if err := os.WriteFile(restagePath, []byte(joinNul(formatted)), 0o644); err != nil {
			return err
		}
	}
	fmt.Fprintf(stderr, "Formatted %d staged %s before committing.\n", len(formatted), pluralize(len(formatted), "file", "files"))
	return nil
}

// fullyStagedFiles lists the files this commit adds or changes whose working-tree
// copy matches what's staged. A file with further unstaged edits is skipped:
// formatting and re-staging it would pull those edits into the commit.
func fullyStagedFiles(r repo) ([]string, error) {
	staged, err := r.paths("diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR")
	if err != nil || len(staged) == 0 {
		return nil, err
	}
	unstaged, err := r.paths("diff", "--name-only", "-z")
	if err != nil {
		return nil, err
	}
	hasUnstagedEdits := toSet(unstaged)

	var files []string
	for _, p := range staged {
		if hasUnstagedEdits[p] {
			continue
		}
		// Symlinks and submodules are staged entries too, and neither is a file to
		// rewrite.
		if !isRegularFile(r.abs(p)) {
			continue
		}
		files = append(files, p)
	}
	return files, nil
}

// isRegularFile reports whether the path itself is a plain file, without following a
// symlink.
func isRegularFile(path string) bool {
	info, err := os.Lstat(path)
	if err != nil {
		return false
	}
	return info.Mode().IsRegular()
}

// commitsFromTemporaryIndex reports whether git is running `git commit <paths>`.
// That form commits from a throwaway `next-index-<pid>.lock` and afterwards
// installs a copy of the real index it prepared before the hook ran, so the
// re-staging done here never reaches the real index.
func commitsFromTemporaryIndex() bool {
	return strings.HasPrefix(filepath.Base(os.Getenv("GIT_INDEX_FILE")), "next-index-")
}

// postCommit finishes what preCommit couldn't do for a `git commit <paths>`: it
// stages the formatted files in the real index. Without it, a file shows as both
// staged and modified right after a commit that contains it.
func postCommit(r repo) error {
	restagePath, err := r.gitPath(restageFile)
	if err != nil {
		return err
	}
	data, err := os.ReadFile(restagePath)
	if err != nil {
		return nil
	}
	_ = os.Remove(restagePath)

	// Only a file whose working-tree copy is what the commit holds is safe to
	// stage: anything else has been edited since, and staging it isn't this hook's
	// call.
	differsFromHead, err := r.paths("diff", "--name-only", "-z", "HEAD")
	if err != nil {
		return err
	}
	skip := toSet(differsFromHead)
	var restage []string
	for _, p := range splitNul(string(data)) {
		if !skip[p] {
			restage = append(restage, p)
		}
	}
	if len(restage) == 0 {
		return nil
	}
	return r.add(restage)
}
