package main

import (
	"bytes"
	"fmt"
	"os/exec"
	"strings"
)

// runContext is what decides whether markers count at all, and from where.
type runContext struct {
	event    string // github.event_name
	ref      string // github.ref; on a PR, `refs/pull/<n>/merge`, the scope its caches save under
	mainRef  string // the default branch's ref, `refs/heads/main`
	repo     string // github.repository
	headRepo string // the PR head's repository (github.event.pull_request.head.repo.full_name)
	runAll   bool   // a `run_all` dispatch
	// The PR author: github.event.pull_request.user.type and .login. A bot's PR (Renovate,
	// Dependabot) brings third-party code that its own lanes execute, and any process in a job
	// can save a cache entry in the PR's scope, so a bot PR trusts `main`'s markers only.
	authorType, author string
}

// botAuthored reports whether the PR's author is a bot, by type or by the `[bot]` login suffix.
func (c runContext) botAuthored() bool {
	return c.authorType == "Bot" || strings.HasSuffix(c.author, "[bot]")
}

// trustedRefs are the cache scopes whose markers this run may trust, or nil when it must ignore
// markers:
//   - Only `pull_request` runs trust markers. A push to `main`, a dispatch (the release gate's
//     `run_all`), or a schedule always runs its lanes on their filters alone, so a filter that
//     misses an input gets caught there rather than skipped forever.
//   - `main`'s markers count everywhere: only post-land runs on `main` write them.
//   - The PR's own markers count only when its head lives in this repo and a human wrote it. A
//     fork's run executes the fork's workflow, which could save a marker for a tree its next push
//     brings, and an Actions cache saved in a PR's scope is readable by that PR's later runs. A
//     bot's PR runs third-party code (an npm install script, a new crate's build script) that can
//     reach the job's cache token and do the same (the CI gate review of 2026-11-05, F3).
func (c runContext) trustedRefs() []string {
	if c.event != "pull_request" || c.runAll || c.mainRef == "" {
		return nil
	}
	refs := []string{c.mainRef}
	if c.ref != "" && c.ref != c.mainRef && c.headRepo != "" && c.headRepo == c.repo && !c.botAuthored() {
		refs = append(refs, c.ref)
	}
	return refs
}

// marker is one Actions cache entry: its key and the ref (scope) GitHub saved it under. The ref
// comes from GitHub, never from the run that saved it, so a run can't claim another scope.
type marker struct {
	key, ref string
}

// decision is a lane's verdict for this run.
type decision struct {
	Hash     string `json:"hash"`
	Key      string `json:"key"`
	Verified bool   `json:"verified"`
	Ref      string `json:"ref,omitempty"` // the scope whose marker verified it
	reason   string
}

// decide says whether a lane's exact inputs already passed: true only when a marker with exactly
// this key sits in a scope this run trusts. A lookup that failed reaches here as no markers, so
// every uncertainty means "run".
func decide(laneID, hash string, ctx runContext, markers []marker) decision {
	d := decision{Hash: hash, Key: markerKey(laneID, hash)}
	trusted := ctx.trustedRefs()
	if trusted == nil {
		d.reason = "this run ignores markers (not a pull request, or a full run)"
		return d
	}
	for _, ref := range trusted {
		for _, m := range markers {
			if m.key == d.Key && m.ref == ref {
				d.Verified, d.Ref = true, ref
				d.reason = "these exact inputs already passed (marker on " + ref + ")"
				return d
			}
		}
	}
	d.reason = "no trusted marker for these inputs"
	return d
}

// markerLister lists the lane markers saved under one ref.
type markerLister interface {
	list(ref string) ([]marker, error)
}

// ghLister reads the repo's Actions caches through `gh api` (needs `actions: read`).
type ghLister struct{ repo string }

func (g ghLister) list(ref string) ([]marker, error) {
	cmd := exec.Command("gh", "api", "-X", "GET", "repos/"+g.repo+"/actions/caches",
		"-f", "key="+markerPrefix, "-f", "ref="+ref, "-F", "per_page=100", "--paginate",
		"--jq", `.actions_caches[] | "\(.ref)\t\(.key)"`)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("list caches for %s: %v: %s", ref, err, strings.TrimSpace(stderr.String()))
	}
	return parseMarkerList(string(out), ref), nil
}

// parseMarkerList keeps only entries whose ref is exactly the one asked for: `key` is a prefix
// filter and `ref` should already narrow it, but the trust rests on this equality, not on the
// API's query semantics.
func parseMarkerList(out, ref string) []marker {
	var ms []marker
	for line := range strings.SplitSeq(out, "\n") {
		r, key, ok := strings.Cut(strings.TrimSpace(line), "\t")
		if !ok || r != ref || !strings.HasPrefix(key, markerPrefix) {
			continue
		}
		ms = append(ms, marker{key: key, ref: r})
	}
	return ms
}

// listTrusted gathers the markers of every scope this run trusts.
func listTrusted(l markerLister, ctx runContext) ([]marker, error) {
	var all []marker
	for _, ref := range ctx.trustedRefs() {
		ms, err := l.list(ref)
		if err != nil {
			return nil, err
		}
		all = append(all, ms...)
	}
	return all, nil
}
