package main

import (
	"errors"
	"testing"
	"time"
)

// fakeMerger answers each merge call from merges in order (the last one repeats) and each PR read
// from pulls the same way, and counts both.
type fakeMerger struct {
	merges     []fakeMerge
	pulls      []pullDetails
	mergeCalls int
	pullCalls  int
}

type fakeMerge struct {
	resp     apiResponse
	answered bool
}

func (f *fakeMerger) mergePR(int, string) (apiResponse, bool, error) {
	m := f.merges[min(f.mergeCalls, len(f.merges)-1)]
	f.mergeCalls++
	if m.answered && m.resp.status < 300 {
		return m.resp, true, nil
	}
	return m.resp, m.answered, errors.New("gh api: failed")
}

func (f *fakeMerger) readPull(int) (pullDetails, error) {
	p := f.pulls[min(f.pullCalls, len(f.pulls)-1)]
	f.pullCalls++
	return p, nil
}

// noPause makes the retries' waits instant and records them.
func noPause(t *testing.T) *[]time.Duration {
	t.Helper()
	var waits []time.Duration
	old := pause
	pause = func(d time.Duration) { waits = append(waits, d) }
	t.Cleanup(func() { pause = old })
	return &waits
}

var (
	badGateway = fakeMerge{apiResponse{status: 502, budget: apiBudget{serverError: true}, requestID: "C0FE:1"}, true}
	noAnswer   = fakeMerge{apiResponse{}, false}
	merged     = fakeMerge{apiResponse{status: 200}, true}
	open       = pullDetails{}
)

func TestMergeOnGitHubRetriesGitHubsOwnFailure(t *testing.T) {
	waits := noPause(t)
	f := &fakeMerger{merges: []fakeMerge{badGateway, noAnswer, merged}, pulls: []pullDetails{open}}
	if err := mergeOnGitHub(f, pullRequest{Number: 1004}, "0123456789abcdef", false, false); err != nil {
		t.Fatal(err)
	}
	if f.mergeCalls != 3 || len(*waits) != 2 {
		t.Errorf("got %d merge calls and %d waits, want 3 and 2", f.mergeCalls, len(*waits))
	}
	if (*waits)[0] < secondaryBackoff || (*waits)[1] < 2*secondaryBackoff {
		t.Errorf("waits %v, want a backoff from a minute, doubling", *waits)
	}
}

// The failed call may have gone through: a PR that reads merged counts as landed, and is never
// merged again.
func TestMergeOnGitHubNeverRetriesAMergeThatWentThrough(t *testing.T) {
	waits := noPause(t)
	f := &fakeMerger{merges: []fakeMerge{badGateway}, pulls: []pullDetails{{Merged: true}}}
	if err := mergeOnGitHub(f, pullRequest{Number: 1004}, "0123456789abcdef", false, false); err != nil {
		t.Fatal(err)
	}
	if f.mergeCalls != 1 || len(*waits) != 0 {
		t.Errorf("got %d merge calls and %d waits, want 1 and none", f.mergeCalls, len(*waits))
	}
}

func TestMergeOnGitHubGivesUpOnALastingOutage(t *testing.T) {
	waits := noPause(t)
	f := &fakeMerger{merges: []fakeMerge{badGateway}, pulls: []pullDetails{open}}
	err := mergeOnGitHub(f, pullRequest{Number: 1004}, "0123456789abcdef", false, false)
	var failing *errGitHubFailing
	if !errors.As(err, &failing) || failing.requestID != "C0FE:1" {
		t.Fatalf("got %v, want errGitHubFailing with the request ID", err)
	}
	if f.mergeCalls != secondaryBackoffs+1 || len(*waits) != secondaryBackoffs {
		t.Errorf("got %d merge calls and %d waits, want %d and %d", f.mergeCalls, len(*waits), secondaryBackoffs+1, secondaryBackoffs)
	}
}

func TestMergeOnGitHubStopsAtARefusal(t *testing.T) {
	waits := noPause(t)
	f := &fakeMerger{merges: []fakeMerge{{apiResponse{status: 405, body: `{"message":"Merge commits are not allowed"}`}, true}}, pulls: []pullDetails{open}}
	err := mergeOnGitHub(f, pullRequest{Number: 1004}, "0123456789abcdef", false, false)
	var refused *errMergeRefused
	if !errors.As(err, &refused) || refused.status != 405 {
		t.Fatalf("got %v, want errMergeRefused 405", err)
	}
	if f.mergeCalls != 1 || len(*waits) != 0 {
		t.Errorf("got %d merge calls and %d waits, want 1 and none", f.mergeCalls, len(*waits))
	}
}

func TestMergeOnGitHubTurnsAConflictIntoARebase(t *testing.T) {
	noPause(t)
	no := false
	f := &fakeMerger{merges: []fakeMerge{{apiResponse{status: 405}, true}}, pulls: []pullDetails{{Mergeable: &no}}}
	err := mergeOnGitHub(f, pullRequest{Number: 1004}, "0123456789abcdef", false, false)
	if !errors.Is(err, errGitHubConflict) {
		t.Fatalf("got %v, want errGitHubConflict", err)
	}
}
