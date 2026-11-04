package main

import (
	"encoding/json"
	"fmt"
)

// jobNeed is one entry of `toJSON(needs)` in the `CI OK` job.
type jobNeed struct {
	Result  string            `json:"result"`
	Outputs map[string]string `json:"outputs"`
}

// verifyInput is everything `CI OK` hands the check: the lanes and keys it recomputed from its
// own checkout, the jobs' results, and the trusted markers it listed itself.
type verifyInput struct {
	lanes   []lane
	hashes  map[string]string // lane id → lane hash recomputed in this job
	needs   map[string]jobNeed
	ctx     runContext
	markers []marker
	listErr error // a failed marker lookup: every verified skip then counts as red
}

// verifyLanes is `CI OK`'s lane rule, on top of its existing "every job passed or skipped" one.
// A lane that ran is judged by its own result there. A lane that skipped passes only when:
//   - its filters said nothing it reads changed (the ordinary path-filter skip), or
//   - it's a verified skip: this is a run that trusts markers, `changes`' plan said verified
//     with the very key this job recomputed from its own checkout, and that key's marker exists
//     in a scope this run trusts, looked up again here.
//
// A full (`run_all`) run never skips a lane. Anything else that skipped a lane whose inputs
// changed (a wiring slip, a skipped `needs` job, a plan that lied) is red: that's a lane
// reported green for inputs it never tested.
func verifyLanes(in verifyInput) []string {
	changes, ok := in.needs["changes"]
	if !ok {
		return []string{"`CI OK` doesn't need `changes`, so it can't read the lane plan"}
	}
	var plan map[string]decision
	if raw := changes.Outputs[laneOutput]; raw != "" {
		if err := json.Unmarshal([]byte(raw), &plan); err != nil {
			return []string{fmt.Sprintf("the `%s` output of `changes` isn't valid JSON: %v", laneOutput, err)}
		}
	}
	var bad []string
	for _, l := range in.lanes {
		need, ok := in.needs[l.id]
		if !ok {
			bad = append(bad, fmt.Sprintf("%s: a lane, but `CI OK` doesn't need it", l.id))
			continue
		}
		if need.Result != "skipped" {
			continue
		}
		if in.ctx.runAll {
			bad = append(bad, fmt.Sprintf("%s: skipped on a full run", l.id))
			continue
		}
		if !anyFilterTrue(changes.Outputs, l.gate) {
			continue // a path-filter skip: nothing it reads changed
		}
		if msg := verifiedSkipProblem(l.id, plan, in); msg != "" {
			bad = append(bad, fmt.Sprintf("%s: skipped although its inputs changed, and %s", l.id, msg))
		}
	}
	return bad
}

func verifiedSkipProblem(laneID string, plan map[string]decision, in verifyInput) string {
	d, ok := plan[laneID]
	switch {
	case !ok || !d.Verified:
		return "the plan didn't verify it"
	case in.ctx.trustedRefs() == nil:
		return "this run doesn't trust markers"
	case d.Key != markerKey(laneID, in.hashes[laneID]):
		return fmt.Sprintf("the plan's key %s isn't the %s this job computes", d.Key, markerKey(laneID, in.hashes[laneID]))
	case in.listErr != nil:
		return fmt.Sprintf("the marker lookup failed (%v); re-run the whole workflow", in.listErr)
	}
	if !decide(laneID, in.hashes[laneID], in.ctx, in.markers).Verified {
		return "its marker isn't in a trusted scope (evicted? re-run the whole workflow)"
	}
	return ""
}

func anyFilterTrue(outputs map[string]string, filters []string) bool {
	for _, f := range filters {
		if outputs[f] == "true" {
			return true
		}
	}
	return false
}
