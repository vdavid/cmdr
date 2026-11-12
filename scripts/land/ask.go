package main

import (
	"errors"
	"fmt"
	"slices"
	"time"
)

// Asking CI for a land run: the PR's `land` label, when it goes on, and the fresh run a land run
// gets when a newer run on the commit hides it. GitHub's ruleset reads `CI OK` off the newest run
// of the workflow on a commit, so the land run must be the newest one there (DETAILS.md § Gotcha:
// GitHub reads only the newest run on a commit).

// landAsk is how `land` asks CI to land the head (`ci.yml`'s `mode` step reads the PR's labels).
type landAsk int

const (
	// askAddLabel: the PR lacks the `land` label; adding it starts a land run on the head.
	askAddLabel landAsk = iota
	// askNothing: the label is on, so the head's land run exists or is on its way (a push while
	// the label is on runs as a land run).
	askNothing
	// askRelabel: the label is on, yet every run on the head is light (it went on before this
	// head, or its run never started), so taking it off and on again starts one.
	askRelabel
)

// nextLandAsk decides how to get a land run on the head, from whether the PR has the label and
// the CI runs on the head so far. No run at all with the label on is a push whose land run hasn't
// shown up yet.
func nextLandAsk(labeled bool, runs []workflowRun) landAsk {
	switch {
	case !labeled:
		return askAddLabel
	case len(runs) > 0 && len(landRuns(runs)) == 0:
		return askRelabel
	default:
		return askNothing
	}
}

// burstWithin: how long a new PR's second run (the agent label's, beside the `opened` one) gets to
// be listed once the first is. Both come within seconds; past this the label goes on anyway.
const burstWithin = 30 * time.Second

// runsToAwait is how many CI runs this land's own push or new PR starts on the head, which must
// be listed before the `land` label goes on: a run made after the label's land run reads a
// payload without the label, runs light, cancels the land run, and hides its `CI OK`. A push to
// an open PR starts one (`synchronize`); a new PR starts two (`opened`, and `labeled` for the
// agent label), and its branch push none, since no PR existed yet. A head origin already had
// starts nothing now.
func runsToAwait(pushed, opened bool) int {
	switch {
	case opened:
		return 2
	case pushed:
		return 1
	default:
		return 0
	}
}

type awaitStep int

const (
	awaitMore awaitStep = iota
	// awaitDone: the runs are listed (or the last of a burst never came), so the label can go on.
	awaitDone
	// awaitNoRun: the push started no run within `appearWithin`, so the label would start none
	// either: GitHub runs no `pull_request` workflow on a PR that conflicts with its base.
	awaitNoRun
)

// nextAwait decides whether the runs this land started are all listed. listed counts the CI runs
// on the head, want is `runsToAwait`, waited is the time since the wait began, and sinceFirst the
// time since a run was first listed.
func nextAwait(listed, want int, waited, sinceFirst time.Duration) awaitStep {
	switch {
	case listed >= want:
		return awaitDone
	case listed == 0 && waited > appearWithin:
		return awaitNoRun
	case listed > 0 && sinceFirst > burstWithin:
		return awaitDone
	default:
		return awaitMore
	}
}

// hiddenBy finds the newest CI run on the head made after run: the one GitHub's ruleset reads
// instead of it.
func hiddenBy(runs []workflowRun, run workflowRun) (workflowRun, bool) {
	if len(runs) == 0 {
		return workflowRun{}, false
	}
	if newest := slices.MaxFunc(runs, byAge); byAge(newest, run) > 0 {
		return newest, true
	}
	return workflowRun{}, false
}

// needsFreshRun says whether the land run the wait follows (`ciVerdict`'s) can no longer land the
// head and a fresh land run must replace it, and names the run in its way. Two cases, both a run
// made after it:
//   - it was cancelled: a light run took its slot in the PR's concurrency group (the push's run,
//     made after the label's; PR #1010, 2026-11-05). With nothing newer, a person cancelled it,
//     which stays red.
//   - it's green: GitHub won't read its `CI OK` (PR #1116, 2026-11-12, issue #1127).
//
// A land run still going isn't replaced yet: the lanes it passes let the fresh run skip them. A
// re-run never helps here: it stays in the old run's check suite, behind the newer one.
func needsFreshRun(v verdict, run workflowRun, runs []workflowRun) (workflowRun, bool) {
	if v != verdictGreen && (v != verdictRed || !run.cancelled()) {
		return workflowRun{}, false
	}
	return hiddenBy(runs, run)
}

// slotStep is what the wait does about a land run that needs replacing (`needsFreshRun`).
type slotStep int

const (
	// slotHeld: nothing hides the land run.
	slotHeld slotStep = iota
	// slotAsk: ask CI for a fresh land run (the label off and on again), which is the newest then.
	slotAsk
	// slotAwait: asked, and the fresh run isn't listed yet.
	slotAwait
	// slotLost: asked, and no fresh run showed up within `successorWithin`.
	slotLost
)

// slotRetaker asks for a fresh land run once per hiding run: GitHub's list takes a poll or two to
// show the fresh run, and a second ask would start another.
type slotRetaker struct{ asked map[int64]time.Time }

// next decides the step for this poll. hiding is the run in the land run's way, when hidden.
func (s *slotRetaker) next(hiding workflowRun, hidden bool, now time.Time) slotStep {
	if !hidden {
		return slotHeld
	}
	since, ok := s.asked[hiding.ID]
	switch {
	case !ok:
		if s.asked == nil {
			s.asked = map[int64]time.Time{}
		}
		s.asked[hiding.ID] = now
		return slotAsk
	case now.Sub(since) > successorWithin:
		return slotLost
	default:
		return slotAwait
	}
}

// retake acts on this poll's verdict when the land run needs replacing: it asks for the fresh run
// (once), and reports whether the wait should poll again instead of reading the verdict, which
// must never reach a push while a newer run hides the green.
func (s *slotRetaker) retake(r runner, pr pullRequest, head string, v verdict, run workflowRun, runs []workflowRun, cfg config) (bool, error) {
	hiding, hidden := needsFreshRun(v, run, runs)
	switch s.next(hiding, hidden, time.Now()) {
	case slotAsk:
		if run.cancelled() {
			say("A light run took the land run's slot (GitHub started the push's run after the label's), so asking CI for a fresh land run.")
		} else {
			say("%s is green, but a newer CI run on the commit hides it from GitHub's ruleset, which reads only the newest (https://github.com/%s/actions/runs/%d). Asking CI for a fresh land run: the lanes that passed skip.", ciCheck, repo, hiding.ID)
		}
		return true, freshLandRun(r, pr)
	case slotAwait:
		return true, nil
	case slotLost:
		return false, errSlotLost(head, hiding, cfg)
	case slotHeld:
	}
	return false, nil
}

// errSlotLost: the fresh land run `land` asked for never showed up, so nothing on the head can
// land it.
func errSlotLost(head string, hiding workflowRun, cfg config) error {
	msg := fmt.Sprintf("A newer CI run on %s hides its land run from GitHub's ruleset (https://github.com/%s/actions/runs/%d), and the fresh land run I asked for didn't start within %s. Nothing landed. Ask again: %s", head[:12], repo, hiding.ID, successorWithin, cfg.command())
	if cfg.keepsCommitID() {
		return errors.New(msg)
	}
	return errors.New(msg + "\nStill stuck? A new commit ID gets one clean run:\n  git commit --amend --no-edit && " + cfg.command())
}

// dropLandLabel takes the `land` label off the branch's open PR, so the push that follows runs
// light. No PR, or no label, is nothing to do.
func dropLandLabel(r runner, branch string) error {
	pr, ok, err := findOpenPR(r, branch)
	if err != nil || !ok || !pr.hasLabel(landLabel) {
		return err
	}
	if _, err := r.gh("pr", "edit", fmt.Sprint(pr.Number), "-R", repo, "--remove-label", landLabel); err != nil {
		return fmt.Errorf("couldn't take the %q label off %s, so I didn't push (the push would run every lane):\n%w", landLabel, pr.URL, err)
	}
	return nil
}

// askToLand makes sure a land run is on its way for head (`nextLandAsk`): it adds the PR's `land`
// label, or takes it off and on again when the label is on but every run on head is light. A PR
// this land just opened lacks the label, so it gets it here: one `labeled` event, one land run.
//
// The label goes on only once the runs this land itself started on head are listed (started, from
// `runsToAwait`), so the label's land run is the newest run on the commit.
func askToLand(r runner, pr pullRequest, head string, started int, cfg config) error {
	var runs struct {
		WorkflowRuns []workflowRun `json:"workflow_runs"`
	}
	// Without the label, the runs don't change the answer.
	if pr.hasLabel(landLabel) {
		if err := r.ghAPI(&runs, ciRunsPath(head, cfg.merge)); err != nil {
			return err
		}
	}
	switch nextLandAsk(pr.hasLabel(landLabel), runs.WorkflowRuns) {
	case askNothing:
		return nil
	case askRelabel:
		if err := freshLandRun(r, pr); err != nil {
			return err
		}
	case askAddLabel:
		if err := awaitStartedRuns(r, head, started, cfg); err != nil {
			return err
		}
		if err := addLandLabel(r, pr); err != nil {
			return err
		}
	}
	say("Asked CI to land %s: every lane it needs runs now.", head[:12])
	return nil
}

func addLandLabel(r runner, pr pullRequest) error {
	if _, err := r.gh("pr", "edit", fmt.Sprint(pr.Number), "-R", repo, "--add-label", landLabel); err != nil {
		return fmt.Errorf("couldn't add the %q label to %s, which asks CI for the land run:\n%w", landLabel, pr.URL, err)
	}
	return nil
}

// freshLandRun takes the `land` label off and on again: one `labeled` event, one new land run on
// the PR's head, newer than every run there.
func freshLandRun(r runner, pr pullRequest) error {
	if _, err := r.gh("pr", "edit", fmt.Sprint(pr.Number), "-R", repo, "--remove-label", landLabel); err != nil {
		return fmt.Errorf("couldn't take the %q label off %s to put it back on:\n%w", landLabel, pr.URL, err)
	}
	return addLandLabel(r, pr)
}

// listPoll is how often `awaitStartedRuns` looks for runs that show up within seconds.
const listPoll = 2 * time.Second

// awaitStartedRuns waits until the CI runs this land's own push or new PR started on head are
// listed (`nextAwait`), so none of them can be made after the label's land run.
func awaitStartedRuns(r runner, head string, want int, cfg config) error {
	if want == 0 {
		return nil
	}
	start := time.Now()
	var first time.Time
	for {
		var runs struct {
			WorkflowRuns []workflowRun `json:"workflow_runs"`
		}
		if err := r.ghAPI(&runs, ciRunsPath(head, cfg.merge)); err != nil {
			return err
		}
		listed := len(runs.WorkflowRuns)
		if listed > 0 && first.IsZero() {
			first = time.Now()
		}
		var sinceFirst time.Duration
		if !first.IsZero() {
			sinceFirst = time.Since(first)
		}
		switch nextAwait(listed, want, time.Since(start), sinceFirst) {
		case awaitDone:
			return nil
		case awaitNoRun:
			return errNoRun(head, cfg.command())
		case awaitMore:
		}
		time.Sleep(listPoll)
	}
}
