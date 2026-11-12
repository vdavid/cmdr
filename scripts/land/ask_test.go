package main

import (
	"strings"
	"testing"
	"time"
)

// A fresh run that never starts ends the wait with the next step, and never tells a release or a
// record to change its commit ID.
func TestErrSlotLostSaysWhatToRun(t *testing.T) {
	head, hiding := "30ac144c45d47dedcb7e919c2027402fe533929e", workflowRun{ID: 43160663212}
	plain := errSlotLost(head, hiding, config{}).Error()
	for _, want := range []string{"30ac144c45d4", "actions/runs/43160663212", "pnpm land", "git commit --amend --no-edit"} {
		if !strings.Contains(plain, want) {
			t.Errorf("the message misses %q:\n%s", want, plain)
		}
	}
	if msg := errSlotLost(head, hiding, config{merge: true}).Error(); strings.Contains(msg, "--amend") || !strings.Contains(msg, "pnpm land -merge") {
		t.Errorf("-merge keeps the commit's ID:\n%s", msg)
	}
}

// GitHub's ruleset reads `CI OK` off the newest CI run on a commit, so a land run with a newer run
// beside it can't land the head, however green. PR #1010 (2026-11-05) and PR #1116 (2026-11-12,
// issue #1127): the push's light run was made after the label's land run and cancelled it; the
// land run was re-run to green, and GitHub refused the push: `Required status check "CI OK" is
// expected`.
func TestNeedsFreshRun(t *testing.T) {
	at := func(s int) time.Time { return time.Date(2026, 11, 12, 5, 19, s, 0, time.UTC) }
	run := func(id int64, status, conclusion, title string, created int) workflowRun {
		return workflowRun{ID: id, Status: status, Conclusion: conclusion, DisplayTitle: title, CreatedAt: at(created)}
	}
	cancelledLand := run(10, "completed", "cancelled", "docs: a thing", 53)
	greenLand := run(10, "completed", "success", "docs: a thing", 53)
	liveLand := run(10, "in_progress", "", "docs: a thing", 53)
	redLand := run(10, "completed", "failure", "docs: a thing", 53)
	// The incident's shape: the same second, the higher ID.
	laterLight := run(11, "completed", "cancelled", "Light: docs: a thing", 53)
	olderLight := run(4, "completed", "success", "Light: docs: a thing", 10)
	// One event burst can start several land runs in a second; `newestLandRun` follows the live one.
	cancelledTwin := run(12, "completed", "cancelled", "docs: a thing", 53)

	cases := []struct {
		name string
		v    verdict
		run  workflowRun
		runs []workflowRun
		want int64
	}{
		{"a light run made after the land run cancelled it", verdictRed, cancelledLand, []workflowRun{cancelledLand, laterLight}, 11},
		{"a green land run with a newer light run beside it (the incident, after the re-run)", verdictGreen, greenLand, []workflowRun{laterLight, greenLand}, 11},
		{"a green land run behind a cancelled twin of the same burst", verdictGreen, greenLand, []workflowRun{greenLand, cancelledTwin}, 12},
		{"the usual land: the light run is the older one", verdictGreen, greenLand, []workflowRun{olderLight, greenLand}, 0},
		{"a land run alone", verdictGreen, greenLand, []workflowRun{greenLand}, 0},
		{"a person cancelled the land run: nothing newer", verdictRed, cancelledLand, []workflowRun{olderLight, cancelledLand}, 0},
		// The lanes it ran still count: wait for green, then ask (the fresh run skips them).
		{"a land run still going, with a newer run beside it", verdictPending, liveLand, []workflowRun{liveLand, laterLight}, 0},
		{"a red land run stays red", verdictRed, redLand, []workflowRun{redLand, laterLight}, 0},
		{"no land run at all", verdictMissing, workflowRun{}, []workflowRun{laterLight}, 0},
	}
	for _, c := range cases {
		got, ok := needsFreshRun(c.v, c.run, c.runs)
		if ok != (c.want != 0) || got.ID != c.want {
			t.Errorf("%s: got run %d, %v; want run %d", c.name, got.ID, ok, c.want)
		}
	}
}

// One ask per hiding run: the fresh run takes a poll or two to be listed, and asking again would
// start another. A fresh run that never shows up ends the wait with what to do, never a push.
func TestSlotRetakerAsksOncePerHidingRun(t *testing.T) {
	t0 := time.Date(2026, 11, 12, 5, 20, 0, 0, time.UTC)
	light, another := workflowRun{ID: 11}, workflowRun{ID: 15}
	var s slotRetaker
	steps := []struct {
		name   string
		hiding workflowRun
		hidden bool
		at     time.Duration
		want   slotStep
	}{
		{"nothing hides the land run", workflowRun{}, false, 0, slotHeld},
		{"a newer run does: ask", light, true, 0, slotAsk},
		{"the fresh run isn't listed yet", light, true, 10 * time.Second, slotAwait},
		{"it's listed, and is the newest now", workflowRun{}, false, 20 * time.Second, slotHeld},
		{"another run hides that one: ask again", another, true, time.Minute, slotAsk},
		{"still not listed just inside the grace", another, true, time.Minute + successorWithin, slotAwait},
		{"no fresh run after the grace", another, true, time.Minute + successorWithin + time.Second, slotLost},
	}
	for _, c := range steps {
		if got := s.next(c.hiding, c.hidden, t0.Add(c.at)); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

// The label goes on only once the runs this land's own push or new PR starts are listed: a run
// made after the label's land run would cancel it and hide its `CI OK` (issue #1127).
func TestRunsToAwait(t *testing.T) {
	cases := []struct {
		name           string
		pushed, opened bool
		want           int
	}{
		{"a head origin already had: its runs are long listed", false, false, 0},
		{"a push to an open PR: its `synchronize` run", true, false, 1},
		{"a new PR: its `opened` run and the agent label's", true, true, 2},
		{"a new PR for a branch origin already had", false, true, 2},
	}
	for _, c := range cases {
		if got := runsToAwait(c.pushed, c.opened); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

func TestNextAwait(t *testing.T) {
	cases := []struct {
		name               string
		listed, want       int
		waited, sinceFirst time.Duration
		wantStep           awaitStep
	}{
		{"nothing to wait for", 0, 0, 0, 0, awaitDone},
		{"the push's run isn't listed yet", 0, 1, 4 * time.Second, 0, awaitMore},
		{"it's listed", 1, 1, 6 * time.Second, 0, awaitDone},
		{"runs from an earlier push are listed too", 3, 1, 0, 0, awaitDone},
		// GitHub runs no `pull_request` workflow on a PR that conflicts with its base.
		{"no run at all: the label would start none either", 0, 1, appearWithin + time.Second, 0, awaitNoRun},
		{"a new PR's second run isn't listed yet", 1, 2, 8 * time.Second, 2 * time.Second, awaitMore},
		{"a second run that never comes doesn't hold the land up", 1, 2, time.Minute, burstWithin + time.Second, awaitDone},
	}
	for _, c := range cases {
		if got := nextAwait(c.listed, c.want, c.waited, c.sinceFirst); got != c.wantStep {
			t.Errorf("%s: got %d, want %d", c.name, got, c.wantStep)
		}
	}
}

func TestNextLandAsk(t *testing.T) {
	light := workflowRun{ID: 1, DisplayTitle: "Light: fix: x"}
	landing := workflowRun{ID: 2, DisplayTitle: "fix: x"}
	cases := []struct {
		name    string
		labeled bool
		runs    []workflowRun
		want    landAsk
	}{
		{"no label: add it", false, nil, askAddLabel},
		{"no label, a light run on head: add it", false, []workflowRun{light}, askAddLabel},
		// A push while the label is on starts a land run; it may not be listed yet.
		{"label on, no run yet: wait for the push's land run", true, nil, askNothing},
		{"label on, a land run on head: wait for it", true, []workflowRun{light, landing}, askNothing},
		{"label on, only light runs on head: off and on again", true, []workflowRun{light}, askRelabel},
	}
	for _, c := range cases {
		if got := nextLandAsk(c.labeled, c.runs); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}
