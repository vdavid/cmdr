package main

import (
	"fmt"
	"regexp"
	"strings"
)

// repo is GitHub's current name for this repo, set once in `main` before anything talks to
// GitHub. Not a constant: the repo moved from `vdavid/cmdr` to `getcmdr/cmdr` (#476), and
// `gh repo view` answers with the current name even through a remote that still names the old one.
var repo string

var repoNameRE = regexp.MustCompile(`^[A-Za-z0-9-]+/[A-Za-z0-9._-]+$`)

// parseRepoName validates `gh repo view --json nameWithOwner -q .nameWithOwner` output.
func parseRepoName(out string) (string, error) {
	name := strings.TrimSpace(out)
	if !repoNameRE.MatchString(name) {
		return "", fmt.Errorf("`gh repo view` named the repo %q, which isn't `owner/name`", name)
	}
	return name, nil
}

// resolveRepo asks GitHub what the repo is called now.
func resolveRepo(r runner) (string, error) {
	out, err := r.gh("repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner")
	if err != nil {
		return "", fmt.Errorf("couldn't ask GitHub for this repo's name: %w", err)
	}
	return parseRepoName(out)
}
