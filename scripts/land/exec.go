package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math/rand/v2"
	"os/exec"
	"slices"
	"strings"
	"time"
)

// runner runs git and gh in one directory, the worktree being landed.
type runner struct {
	dir string
	// cache answers `ghAPI`'s unchanged polls from a 304; nil caches nothing.
	cache responseCache
}

func (r runner) cmd(stdin, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = r.dir
	if stdin != "" {
		cmd.Stdin = strings.NewReader(stdin)
	}
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return stdout.String(), fmt.Errorf("%s %s: %w\n%s", name, strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return stdout.String(), nil
}

// git runs a git command and returns its trimmed stdout.
func (r runner) git(args ...string) (string, error) {
	out, err := r.cmd("", "git", args...)
	return strings.TrimSpace(out), err
}

func (r runner) gitStdin(stdin string, args ...string) error {
	_, err := r.cmd(stdin, "git", args...)
	return err
}

// isAncestor reports whether commit a is reachable from b.
func (r runner) isAncestor(a, b string) bool {
	_, err := r.git("merge-base", "--is-ancestor", a, b)
	return err == nil
}

// tagged reports whether a tag points at any commit in rangeSpec: a release commit, whose tag a
// rebase would leave behind, off `main`'s history. A failed lookup reads as tagged, the safe side.
func (r runner) tagged(rangeSpec string) bool {
	commits, err := r.git("rev-list", rangeSpec)
	if err != nil {
		return true
	}
	if commits == "" {
		return false
	}
	// The peeled commit of an annotated tag, the object itself of a lightweight one.
	refs, err := r.git("for-each-ref", "--format=%(*objectname) %(objectname)", "refs/tags")
	if err != nil {
		return true
	}
	onTag := map[string]bool{}
	for line := range strings.SplitSeq(refs, "\n") {
		for sha := range strings.FieldsSeq(line) {
			onTag[sha] = true
		}
	}
	for sha := range strings.SplitSeq(commits, "\n") {
		if onTag[sha] {
			return true
		}
	}
	return false
}

// allUpstream reports whether every commit on HEAD is already on `origin/main` patch for patch, as
// after GitHub rebased the branch onto it under fresh SHAs.
func (r runner) allUpstream() bool {
	out, err := r.git("cherry", "origin/main", "HEAD")
	return err == nil && patchesUpstream(out)
}

// remoteSHA is what the branch points at on origin, or "" when origin has no such branch.
func (r runner) remoteSHA(branch string) (string, error) {
	out, err := r.git("ls-remote", "origin", "refs/heads/"+branch)
	if err != nil {
		return "", err
	}
	sha, _, _ := strings.Cut(out, "\t")
	return sha, nil
}

// fetchBranch points `refs/remotes/origin/<branch>` at origin's branch, or deletes it when origin
// has no such branch (deleted after an earlier land), and returns the SHA, or "".
func (r runner) fetchBranch(branch string) (string, error) {
	sha, err := r.remoteSHA(branch)
	if err != nil {
		return "", err
	}
	tracking := "refs/remotes/origin/" + branch
	if sha == "" {
		_, err = r.git("update-ref", "-d", tracking)
	} else {
		_, err = r.git("fetch", "--quiet", "origin", fmt.Sprintf("+refs/heads/%s:%s", branch, tracking))
	}
	return sha, err
}

// inReflog reports whether the local branch ever pointed at sha, as before a rebase or squash.
func (r runner) inReflog(branch, sha string) bool {
	out, err := r.git("log", "--walk-reflogs", "--format=%H", "refs/heads/"+branch)
	if err != nil {
		return false
	}
	return slices.Contains(strings.Split(out, "\n"), sha)
}

// commits lists the commit messages in a range, oldest first.
func (r runner) commits(rangeSpec string) ([]commit, error) {
	out, err := r.git("log", "--reverse", "--format=%s%x00%b%x1e", rangeSpec)
	if err != nil {
		return nil, err
	}
	var commits []commit
	for record := range strings.SplitSeq(out, "\x1e") {
		subject, body, _ := strings.Cut(strings.TrimSpace(record), "\x00")
		if subject != "" {
			commits = append(commits, commit{Subject: subject, Body: strings.TrimSpace(body)})
		}
	}
	return commits, nil
}

// ghAPI GETs a REST path and decodes the JSON body into out. It reads the budget from the
// response's own headers, which `gh api rate_limit` can contradict, so the polling calls use it.
// With a cache it sends the last ETag, so an unchanged answer comes back as a free 304.
func (r runner) ghAPI(out any, path string) error {
	body, err := r.ghRetrying(transientRetries, func() (string, *apiBudget, error) {
		args := []string{"api", "--include", path}
		if etag := r.cache.etag(path); etag != "" {
			args = append(args, "-H", "If-None-Match: "+etag)
		}
		// gh exits non-zero on a 304 too, so the status decides, not the exit code.
		stdout, err := r.cmd("", "gh", args...)
		resp, ok := parseAPIResponse(stdout)
		if !ok {
			return "", nil, err
		}
		if body, ok := r.cache.settle(path, resp); ok {
			return body, nil, nil
		}
		if err == nil {
			err = fmt.Errorf("gh api %s: HTTP %d\n%s", path, resp.status, resp.body)
		}
		return "", &resp.budget, err
	})
	if err != nil {
		return err
	}
	return json.Unmarshal([]byte(body), out)
}

// ghJSON runs a read-only gh command and decodes its JSON stdout into out. Reads are safe to
// repeat, so a failure also gets a few short retries.
func (r runner) ghJSON(out any, args ...string) error {
	stdout, err := r.ghRetrying(transientRetries, r.ghCall(args))
	if err != nil {
		return err
	}
	return json.Unmarshal([]byte(stdout), out)
}

// gh runs a gh command that may write (open a PR, apply a ruleset), so it only waits out a spent
// rate limit and never repeats a call that may have gone through.
func (r runner) gh(args ...string) (string, error) {
	out, err := r.ghRetrying(0, r.ghCall(args))
	return strings.TrimSpace(out), err
}

// ghCall runs gh with no budget headers to read, so a failure asks `gh api rate_limit`.
func (r runner) ghCall(args []string) func() (string, *apiBudget, error) {
	return func() (string, *apiBudget, error) {
		out, err := r.cmd("", "gh", args...)
		if err != nil {
			return out, r.rateLimit(), err
		}
		return out, nil, nil
	}
}

// ghRetrying runs call, and after a failure acts on `afterGHFailure`: it sleeps until a spent
// budget refills, backs off a secondary limit, or retries up to `retries` times.
func (r runner) ghRetrying(retries int, call func() (string, *apiBudget, error)) (string, error) {
	backedOff := 0
	for {
		out, budget, err := call()
		if err == nil {
			return out, nil
		}
		next, wait := afterGHFailure(budget, retries, backedOff, time.Now())
		switch next {
		case ghGiveUp:
			return out, err
		case ghWaitForReset:
			say("GitHub's API budget for this account is spent (many lands polling at once?), so waiting %s for it to refill.", wait.Round(time.Second))
		case ghBackOff:
			backedOff++
			say("GitHub's short-term rate limit turned a call away without saying for how long, so waiting %s before trying again.", wait.Round(time.Second))
		case ghRetryTransient:
			retries--
		}
		time.Sleep(wait + jitter(wait/10))
	}
}

// rateLimit reads the account's API budget, or nil when it can't.
func (r runner) rateLimit() *apiBudget {
	out, err := r.cmd("", "gh", "api", "rate_limit")
	if err != nil {
		return nil
	}
	var l rateLimit
	if json.Unmarshal([]byte(out), &l) != nil {
		return nil
	}
	b := l.budget()
	return &b
}

// jitter spreads parallel lands' calls apart, so they don't all wake together.
func jitter(upTo time.Duration) time.Duration {
	if upTo <= 0 {
		return 0
	}
	return rand.N(upTo)
}
