package main

import (
	"fmt"
	"regexp"
	"strings"
)

// The repo's name isn't pinned: the repo moved from `vdavid/cmdr` to `getcmdr/cmdr` (#476).
// `gh repo view` answers with GitHub's current name even through a remote that still names the
// old one, so the first release after a move needs no code change here.

const pipelineWorkflowPath = ".github/workflows/release-pipeline.yml"

var repoNameRE = regexp.MustCompile(`^[A-Za-z0-9-]+/[A-Za-z0-9._-]+$`)

// parseRepoName validates `gh repo view --json nameWithOwner -q .nameWithOwner` output.
func parseRepoName(out string) (string, error) {
	name := strings.TrimSpace(out)
	if !repoNameRE.MatchString(name) {
		return "", fmt.Errorf("`gh repo view` named the repo %q, which isn't `owner/name`", name)
	}
	return name, nil
}

// pipelineSignerFor is the workflow whose provenance a release built in `repo` carries.
func pipelineSignerFor(repo string) string {
	return repo + "/" + pipelineWorkflowPath
}

// signerRepo is the `owner/name` a signer workflow (`owner/name/.github/workflows/<file>`)
// belongs to: the repo an attestation names, which stays the repo the release was BUILT in.
func signerRepo(signer string) (string, error) {
	parts := strings.SplitN(signer, "/", 3)
	if len(parts) != 3 || !strings.HasPrefix(parts[2], ".github/workflows/") || len(parts[2]) == len(".github/workflows/") {
		return "", fmt.Errorf("signer workflow %q isn't `owner/name/.github/workflows/<file>`", signer)
	}
	return parseRepoName(parts[0] + "/" + parts[1])
}

// resolveRepo asks GitHub what the repo is called now.
func resolveRepo(root string) (string, error) {
	out, err := output(root, nil, "gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner")
	if err != nil {
		return "", fmt.Errorf("couldn't ask GitHub for this repo's name: %w", err)
	}
	return parseRepoName(out)
}
