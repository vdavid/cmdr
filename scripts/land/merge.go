package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// The GitHub merge of a green PR onto a `main` that moved under it, through the REST API, whose
// HTTP status tells GitHub's side failing (no answer, a 5xx, a rate limit) from GitHub saying no
// (405, 409, 422): typed, so no message is ever matched. `gh pr merge` gave neither apart, and
// on PR #1004 (2026-11-06) a 502, a bare EOF, and a GraphQL "Something went wrong" all read as a
// refusal that sent the reader to repo settings that were fine.

// githubMerger is the two GitHub calls a merge makes; a fake in tests.
type githubMerger interface {
	// mergePR makes one merge call. answered is false when no HTTP response came back.
	mergePR(number int, head string) (resp apiResponse, answered bool, err error)
	readPull(number int) (pullDetails, error)
}

// mergePR has GitHub merge the PR with a merge commit, pinned to head (`sha`, so a newer push
// can't land unchecked). The admin's ruleset bypass (`bypass_mode: always`) passes the approval
// rule; the CI ruleset has no bypass, so GitHub still demands `CI OK` green on head.
func (r runner) mergePR(number int, head string) (apiResponse, bool, error) {
	stdout, err := r.cmd("", "gh", "api", "--include", "-X", "PUT", fmt.Sprintf("repos/%s/pulls/%d/merge", repo, number),
		"-f", "merge_method=merge", "-f", "sha="+head)
	resp, answered := parseAPIResponse(stdout)
	return resp, answered, err
}

func (r runner) readPull(number int) (pullDetails, error) {
	var pull pullDetails
	err := r.ghAPI(&pull, fmt.Sprintf("repos/%s/pulls/%d", repo, number))
	return pull, err
}

// pause is time.Sleep, swapped out in tests.
var pause = time.Sleep

// mergeOnGitHub has GitHub merge the green PR into the `main` that moved under it with a merge
// commit, and CI on `main` checks the result. The branch's commits keep their SHAs and signatures
// (and a release its tag on `main`'s history); GitHub signs the merge commit itself.
//
// After any failed call it reads the PR first: the call may have gone through before it failed
// (then it counts as landed and is never repeated), or GitHub found a conflict. GitHub's side
// failing backs off like a server error on a read (`afterGHFailure`) and tries again; a refusal
// stops.
func mergeOnGitHub(m githubMerger, pr pullRequest, head string, hasMerges, merge bool) error {
	again := config{merge: merge}.command()
	backedOff := 0
	for {
		resp, answered, err := m.mergePR(pr.Number, head)
		outcome := classifyMerge(resp, answered)
		if outcome != mergeDone {
			pull, readErr := m.readPull(pr.Number)
			switch {
			case readErr == nil && pull.Merged:
				outcome = mergeDone
			case readErr == nil && pull.Mergeable != nil && !*pull.Mergeable:
				return conflictError(head, hasMerges, merge)
			}
		}
		if outcome == mergeDone {
			say("main moved while CI ran, so GitHub merged the green PR into it with a merge commit: %s\nCI on main checks the result: https://github.com/%s/actions/workflows/%s?query=branch%%3Amain", pr.URL, repo, ciWorkflow)
			return nil
		}
		if outcome == mergeRefused {
			return mergeFailure(outcome, resp, answered, head, again, err)
		}
		next, wait := afterGHFailure(mergeRetryBudget(resp, answered), 0, backedOff, time.Now())
		switch next {
		case ghWaitForReset:
			say("GitHub's API budget for this account is spent, so the merge waits %s for it to refill. Nothing landed yet.", wait.Round(time.Second))
		case ghBackOff:
			backedOff++
			say("GitHub's side failed the merge (%s), and nothing landed, so trying again in %s.", mergeAnswer(resp, answered), wait.Round(time.Second))
		case ghGiveUp, ghRetryTransient:
			return mergeFailure(outcome, resp, answered, head, again, err)
		}
		pause(wait + jitter(wait/10))
	}
}

// mergeOutcome is what one merge call came back with.
type mergeOutcome int

const (
	mergeDone mergeOutcome = iota
	// mergeGitHubFailing: no answer, a 5xx, or a rate limit. GitHub's side; trying later is the fix.
	mergeGitHubFailing
	// mergeRefused: GitHub read the request and said no (405, 409, 422, a plain 403).
	mergeRefused
)

// classifyMerge reads the merge call's HTTP status, never its wording.
func classifyMerge(resp apiResponse, answered bool) mergeOutcome {
	b := resp.budget
	switch {
	case !answered:
		return mergeGitHubFailing
	case resp.status >= 200 && resp.status < 300:
		return mergeDone
	case b.serverError || b.spent || b.retryAfter > 0 || resp.status == http.StatusTooManyRequests:
		return mergeGitHubFailing
	default:
		return mergeRefused
	}
}

// mergeRetryBudget is what the retry decision goes on: the response's own budget headers, and no
// answer at all counts as a server error, so both back off from a minute.
func mergeRetryBudget(resp apiResponse, answered bool) *apiBudget {
	if !answered {
		return &apiBudget{serverError: true}
	}
	return &resp.budget
}

// mergeAnswer names what came back, for a person: the HTTP status, or no answer.
func mergeAnswer(resp apiResponse, answered bool) string {
	if !answered {
		return "no answer"
	}
	return fmt.Sprintf("HTTP %d", resp.status)
}

// errGitHubFailing: GitHub's side kept failing the merge, and nothing landed.
type errGitHubFailing struct {
	answer    string
	requestID string
	again     string
	// cause is gh's own error, kept only when GitHub sent no answer (a 5xx body is an HTML page).
	cause error
}

func (e *errGitHubFailing) Error() string {
	msg := fmt.Sprintf("GitHub's side kept failing the merge (%s), so nothing landed. CI is green and the repo's settings are fine; run it again in a few minutes: %s", e.answer, e.again)
	if e.requestID != "" {
		msg += "\nGitHub's request ID, if you ask GitHub support: " + e.requestID
	}
	if e.cause != nil {
		msg += "\n" + e.cause.Error()
	}
	return msg
}

func (e *errGitHubFailing) Unwrap() error { return e.cause }

// errMergeRefused: GitHub read the merge and said no.
type errMergeRefused struct {
	status    int
	message   string
	requestID string
	head      string
	again     string
}

func (e *errMergeRefused) Error() string {
	var msg string
	switch e.status {
	case http.StatusConflict:
		msg = fmt.Sprintf("GitHub refused the merge because the PR's head is no longer %s (HTTP 409: %s), so nothing landed. Land again: %s", e.head[:12], e.message, e.again)
	case http.StatusMethodNotAllowed:
		// GitHub's "not mergeable": a merge method the repo or the ruleset doesn't allow, or a rule
		// the head doesn't meet.
		msg = fmt.Sprintf("GitHub refused the merge (HTTP 405: %s), so nothing landed. It takes merge commits on main (the repo setting and the ruleset's allowed merge methods, scripts/land/DETAILS.md). Fix what it says, then land again: %s", e.message, e.again)
	default:
		msg = fmt.Sprintf("GitHub refused the merge (HTTP %d: %s), so nothing landed. Fix what it says, then land again: %s", e.status, e.message, e.again)
	}
	if e.requestID != "" {
		msg += "\nGitHub's request ID: " + e.requestID
	}
	return msg
}

// mergeFailure is the error a merge that didn't land ends with.
func mergeFailure(outcome mergeOutcome, resp apiResponse, answered bool, head, again string, cause error) error {
	if outcome == mergeRefused {
		return &errMergeRefused{status: resp.status, message: githubMessage(resp.body), requestID: resp.requestID, head: head, again: again}
	}
	failing := &errGitHubFailing{answer: mergeAnswer(resp, answered), requestID: resp.requestID, again: again}
	if !answered {
		failing.cause = cause
	}
	return failing
}

// githubMessage is the `message` of GitHub's JSON error body, or a placeholder when there's none.
func githubMessage(body string) string {
	var e struct {
		Message string `json:"message"`
	}
	if json.Unmarshal([]byte(body), &e) != nil || e.Message == "" {
		return "no message"
	}
	return e.Message
}
