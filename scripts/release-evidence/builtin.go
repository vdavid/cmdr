package main

import (
	"encoding/json"
	"fmt"
	"regexp"
	"slices"
	"strings"
)

// formerRepos are the names this repo had before it moved to the `getcmdr` org (#476). A
// release's attestations name the repo that built it, for good, so one built under a former name
// verifies under that name.
var formerRepos = []string{"vdavid/cmdr"}

var releaseURLRepoRE = regexp.MustCompile(`^https://github\.com/([A-Za-z0-9-]+/[A-Za-z0-9._-]+)/releases/download/`)

// mirrorHost is where the EU mirror serves release files (`scripts/release-mirror`). Every
// release from the mirror on names it in `latest.json`; earlier ones name GitHub.
const mirrorHost = "https://download.getcmdr.com/"

// builtInRepo is the repo that built a release, read from the `latest.json` on its own GitHub
// release: up to the EU mirror the pipeline wrote `github.repository` into every URL there, and
// only our pipeline and maintainers upload to our releases. It must be the current name or a
// former one, so a lookalike repo can never become the one an attestation is checked against.
// A mirror URL carries no repo name, so it reads as the current repo: right until the repo
// moves (#476), and evidence is collected right after each release, before any move.
func builtInRepo(manifest []byte, version, current string) (string, error) {
	var m struct {
		Version   string `json:"version"`
		Platforms map[string]struct {
			URL string `json:"url"`
		} `json:"platforms"`
	}
	if err := json.Unmarshal(manifest, &m); err != nil {
		return "", fmt.Errorf("the release's latest.json isn't JSON: %w", err)
	}
	if m.Version != version {
		return "", fmt.Errorf("the release's latest.json says version %q, not %q", m.Version, version)
	}
	var repos []string
	for _, p := range m.Platforms {
		if strings.HasPrefix(p.URL, mirrorHost) {
			repos = append(repos, current)
			continue
		}
		match := releaseURLRepoRE.FindStringSubmatch(p.URL)
		if match == nil {
			return "", fmt.Errorf("the release's latest.json has a URL outside GitHub releases and the EU mirror: %s", p.URL)
		}
		repos = append(repos, match[1])
	}
	slices.Sort(repos)
	repos = slices.Compact(repos)
	switch {
	case len(repos) == 0:
		return "", fmt.Errorf("the release's latest.json names no platforms")
	case len(repos) > 1:
		return "", fmt.Errorf("the release's latest.json names more than one repo: %s", strings.Join(repos, ", "))
	}
	built := repos[0]
	if built != current && !slices.Contains(formerRepos, built) {
		return "", fmt.Errorf("the release's latest.json names %s, which is neither this repo (%s) nor a former name of it", built, current)
	}
	return built, nil
}
