package main

import (
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"
)

const (
	repo   = "vdavid/cmdr"
	prRef  = "refs/pull/12/merge"
	main_  = "refs/heads/main"
	hashA  = "aaaa"
	laneID = "desktop-rust"
)

func prCtx() runContext {
	return runContext{event: "pull_request", ref: prRef, mainRef: main_, repo: repo, headRepo: repo}
}

func TestTrustedRefs(t *testing.T) {
	fork := prCtx()
	fork.headRepo = "someone/cmdr"
	runAll := prCtx()
	runAll.runAll = true
	cases := []struct {
		name string
		ctx  runContext
		want []string
	}{
		{"same-repo PR trusts main and itself", prCtx(), []string{main_, prRef}},
		{"fork PR trusts main only", fork, []string{main_}},
		{"run_all ignores markers", runAll, nil},
		{"push to main ignores markers", runContext{event: "push", ref: main_, mainRef: main_, repo: repo}, nil},
		{"dispatch ignores markers", runContext{event: "workflow_dispatch", ref: "refs/heads/release", mainRef: main_, repo: repo}, nil},
		{"schedule ignores markers", runContext{event: "schedule", ref: main_, mainRef: main_, repo: repo}, nil},
	}
	for _, tc := range cases {
		if got := tc.ctx.trustedRefs(); !reflect.DeepEqual(got, tc.want) {
			t.Errorf("%s: %v, want %v", tc.name, got, tc.want)
		}
	}
}

func TestDecide(t *testing.T) {
	key := markerKey(laneID, hashA)
	fork := prCtx()
	fork.headRepo = "someone/cmdr"
	push := runContext{event: "push", ref: main_, mainRef: main_, repo: repo}
	cases := []struct {
		name    string
		ctx     runContext
		markers []marker
		want    bool
	}{
		{"own scope", prCtx(), []marker{{key, prRef}}, true},
		{"main's scope", prCtx(), []marker{{key, main_}}, true},
		{"another PR's scope", prCtx(), []marker{{key, "refs/pull/13/merge"}}, false},
		{"a release branch's scope", prCtx(), []marker{{key, "refs/heads/release-0.60"}}, false},
		{"a fork's own scope", fork, []marker{{key, prRef}}, false},
		{"a fork, main's scope", fork, []marker{{key, main_}}, true},
		{"other inputs", prCtx(), []marker{{markerKey(laneID, "bbbb"), prRef}}, false},
		{"a lane whose name extends this one", prCtx(), []marker{{markerKey(laneID+"-macos", hashA), prRef}}, false},
		{"no markers", prCtx(), nil, false},
		{"push to main", push, []marker{{key, main_}}, false},
	}
	for _, tc := range cases {
		d := decide(laneID, hashA, tc.ctx, tc.markers)
		if d.Verified != tc.want || d.Key != key {
			t.Errorf("%s: verified=%v key=%s, want %v", tc.name, d.Verified, d.Key, tc.want)
		}
	}
}

func TestParseMarkerListKeepsOnlyTheAskedRef(t *testing.T) {
	out := prRef + "\tlane-green-a-1\n" + "refs/pull/13/merge\tlane-green-a-2\n" + prRef + "\tv0-rust-x\n\n" + prRef + "\tlane-green-b-3\n"
	got := parseMarkerList(out, prRef)
	want := []marker{{"lane-green-a-1", prRef}, {"lane-green-b-3", prRef}}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %v, want %v", got, want)
	}
}

type fakeLister struct {
	byRef map[string][]marker
	err   error
	asked []string
}

func (f *fakeLister) list(ref string) ([]marker, error) {
	f.asked = append(f.asked, ref)
	return f.byRef[ref], f.err
}

func TestListTrustedAsksOnlyTrustedScopes(t *testing.T) {
	f := &fakeLister{byRef: map[string][]marker{main_: {{"lane-green-x-1", main_}}, prRef: {{"lane-green-x-2", prRef}}}}
	ms, err := listTrusted(f, prCtx())
	if err != nil || len(ms) != 2 || !reflect.DeepEqual(f.asked, []string{main_, prRef}) {
		t.Errorf("markers=%v err=%v asked=%v", ms, err, f.asked)
	}
	f.asked = nil
	if ms, _ := listTrusted(f, runContext{event: "push", mainRef: main_}); ms != nil || f.asked != nil {
		t.Errorf("a push listed markers: %v %v", ms, f.asked)
	}
}

// --- CI OK ---

func planJSON(t *testing.T, plan map[string]decision) string {
	t.Helper()
	b, err := json.Marshal(plan)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

type verifyCase struct {
	result      string
	rustChanged bool
	planned     *decision
	ctx         runContext
	markers     []marker
	listErr     error
	hash        string // this job's recomputed hash
}

func runVerifyCase(t *testing.T, vc verifyCase) []string {
	t.Helper()
	// A sub-gate filter that's on never makes a skip suspicious: only the `if:` gate does.
	outputs := map[string]string{"rust": "false", "finder-extension": "true"}
	if vc.rustChanged {
		outputs["rust"] = "true"
	}
	if vc.planned != nil {
		outputs[laneOutput] = planJSON(t, map[string]decision{laneID: *vc.planned})
	}
	if vc.hash == "" {
		vc.hash = hashA
	}
	return verifyLanes(verifyInput{
		lanes:   []lane{{id: laneID, gate: []string{"rust"}, filters: []string{"finder-extension", "rust"}}},
		hashes:  map[string]string{laneID: vc.hash},
		needs:   map[string]jobNeed{"changes": {Result: "success", Outputs: outputs}, laneID: {Result: vc.result}},
		ctx:     vc.ctx,
		markers: vc.markers,
		listErr: vc.listErr,
	})
}

func TestVerifyLanes(t *testing.T) {
	key := markerKey(laneID, hashA)
	verified := &decision{Hash: hashA, Key: key, Verified: true, Ref: prRef}
	notVerified := &decision{Hash: hashA, Key: key}
	runAll := prCtx()
	runAll.runAll = true
	fork := prCtx()
	fork.headRepo = "someone/cmdr"
	cases := []struct {
		name   string
		vc     verifyCase
		wantOK bool
	}{
		{"the lane ran and passed", verifyCase{result: "success", rustChanged: true, planned: notVerified, ctx: prCtx()}, true},
		{"the lane ran and failed: the existing gate's call", verifyCase{result: "failure", rustChanged: true, planned: notVerified, ctx: prCtx()}, true},
		{"path-filter skip", verifyCase{result: "skipped", planned: notVerified, ctx: prCtx()}, true},
		{"path-filter skip, no plan at all", verifyCase{result: "skipped", ctx: prCtx()}, true},
		{"verified skip with its marker", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: prCtx(), markers: []marker{{key, prRef}}}, true},
		{"verified skip on main's marker", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: prCtx(), markers: []marker{{key, main_}}}, true},

		{"skipped although changed, plan said run (a needs cascade)", verifyCase{result: "skipped", rustChanged: true, planned: notVerified, ctx: prCtx(), markers: []marker{{key, prRef}}}, false},
		{"skipped although changed, no plan", verifyCase{result: "skipped", rustChanged: true, ctx: prCtx()}, false},
		{"plan said verified, marker gone", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: prCtx()}, false},
		{"plan said verified, marker in another PR", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: prCtx(), markers: []marker{{key, "refs/pull/13/merge"}}}, false},
		{"plan's key isn't this job's hash", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: prCtx(), markers: []marker{{key, prRef}}, hash: "bbbb"}, false},
		{"marker lookup failed", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: prCtx(), listErr: errors.New("502")}, false},
		{"a fork's own marker", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: fork, markers: []marker{{key, prRef}}}, false},
		{"a push never trusts markers", verifyCase{result: "skipped", rustChanged: true, planned: verified, ctx: runContext{event: "push", ref: main_, mainRef: main_, repo: repo}, markers: []marker{{key, main_}}}, false},
		{"a full run never skips", verifyCase{result: "skipped", planned: notVerified, ctx: runAll}, false},
	}
	for _, tc := range cases {
		bad := runVerifyCase(t, tc.vc)
		if ok := len(bad) == 0; ok != tc.wantOK {
			t.Errorf("%s: ok=%v, want %v (%v)", tc.name, ok, tc.wantOK, bad)
		}
	}
}

func TestVerifyLanesWantsEveryLaneInNeeds(t *testing.T) {
	bad := verifyLanes(verifyInput{
		lanes: []lane{{id: laneID, gate: []string{"rust"}, filters: []string{"rust"}}},
		needs: map[string]jobNeed{"changes": {Result: "success"}},
		ctx:   prCtx(),
	})
	if len(bad) != 1 || !strings.Contains(bad[0], "doesn't need it") {
		t.Errorf("got %v", bad)
	}
	if bad := verifyLanes(verifyInput{needs: map[string]jobNeed{}}); len(bad) != 1 {
		t.Errorf("no changes job: %v", bad)
	}
}
