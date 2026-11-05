package main

import (
	"errors"
	"slices"
	"strings"
	"testing"
	"time"
	"unicode/utf8"
)

func TestNextStartStep(t *testing.T) {
	ready := branchState{branch: "worktree-x", based: true}
	cases := []struct {
		name  string
		state branchState
		want  startStep
	}{
		{"ready to go", ready, stepProceed},
		{"main itself", branchState{branch: "main", based: true}, stepRefuseBranch},
		{"detached HEAD", branchState{branch: "", based: true}, stepRefuseBranch},
		{"dirty tree", branchState{branch: "worktree-x", dirty: true, based: true}, stepRefuseDirty},
		{"dirty beats landed", branchState{branch: "worktree-x", dirty: true, landed: true, based: true}, stepRefuseDirty},
		{"already landed", branchState{branch: "worktree-x", landed: true, based: true}, stepFinish},
		{"main moved", branchState{branch: "worktree-x"}, stepRebase},
		{"main moved under a merge commit", branchState{branch: "worktree-x", hasMerges: true}, stepRefuseMergeRebase},
		{"a merge commit on a current branch", branchState{branch: "worktree-x", based: true, hasMerges: true}, stepProceed},
	}
	for _, c := range cases {
		if got := nextStartStep(c.state, false); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

// A release commit carries its tag, so a rebase would take the tagged commit off `main`'s history.
func TestNextStartStepKeepsATaggedCommit(t *testing.T) {
	moved := branchState{branch: "release/v1.2.3", tagged: true}
	if got := nextStartStep(moved, false); got != stepRefuseTaggedRebase {
		t.Errorf("a plain land rebasing a tagged commit: got %d, want %d", got, stepRefuseTaggedRebase)
	}
	if got := nextStartStep(moved, true); got != stepProceed {
		t.Errorf("-merge never rebases: got %d, want %d", got, stepProceed)
	}
	if got := nextStartStep(branchState{branch: "release/v1.2.3"}, true); got != stepProceed {
		t.Errorf("-merge on any branch never rebases: got %d, want %d", got, stepProceed)
	}
	if got := nextStartStep(branchState{branch: "release/v1.2.3", tagged: true, based: true}, false); got != stepProceed {
		t.Errorf("a tagged commit main hasn't moved under fast-forwards: got %d, want %d", got, stepProceed)
	}
	if got := nextStartStep(branchState{branch: "release/v1.2.3", tagged: true, landed: true}, true); got != stepFinish {
		t.Errorf("a merged-back release: got %d, want %d", got, stepFinish)
	}
}

// When `main` moved, every land goes through a GitHub merge commit, never a GitHub rebase merge:
// that rewrites the branch's commits unsigned, which the required-signatures rule refuses.
func TestNextLandPath(t *testing.T) {
	yes, no := true, false
	cases := []struct {
		name      string
		based     bool
		merge     bool
		mergeable *bool
		want      landPath
	}{
		{"main didn't move", true, false, nil, landFastForward},
		{"main didn't move, -merge", true, true, &no, landFastForward},
		{"main moved", false, false, &yes, landGitHubMerge},
		{"main moved, GitHub still computing", false, false, nil, landGitHubMerge},
		{"main moved into a conflict", false, false, &no, landRebaseAndRerun},
		{"main moved, -merge", false, true, &yes, landGitHubMerge},
		{"main moved, -merge, GitHub still computing", false, true, nil, landGitHubMerge},
		{"main moved into a conflict, -merge", false, true, &no, landMergeConflict},
	}
	for _, c := range cases {
		if got := nextLandPath(c.based, c.merge, c.mergeable); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

// A conflict GitHub can't merge: a plain branch rebases here and runs CI again; a branch carrying
// a merge commit goes back to the start, which refuses to flatten it; `-merge` stops for a hand merge.
func TestConflictError(t *testing.T) {
	if err := conflictError("0123456789abcdef", false, false); !errors.Is(err, errGitHubConflict) {
		t.Errorf("plain branch: got %v, want errGitHubConflict", err)
	}
	if err := conflictError("0123456789abcdef", true, false); !errors.Is(err, errMainMoved) || errors.Is(err, errGitHubConflict) {
		t.Errorf("merge-carrying branch: got %v, want bare errMainMoved", err)
	}
	if err := conflictError("0123456789abcdef", false, true); errors.Is(err, errMainMoved) {
		t.Errorf("-merge: got %v, want a stop, not another run", err)
	}
}

func TestNextRecordStep(t *testing.T) {
	cases := []struct {
		name       string
		prState    string
		headOnMain bool
		want       recordStep
	}{
		{"open, not recorded yet", "OPEN", false, recordBuild},
		{"open, record landed, GitHub not caught up", "OPEN", true, recordConfirm},
		{"already merged", "MERGED", true, recordAlreadyMerged},
		{"merged beats everything", "MERGED", false, recordAlreadyMerged},
		{"closed without a merge", "CLOSED", false, recordRefuseClosed},
		{"closed, head on main anyway", "CLOSED", true, recordRefuseClosed},
	}
	for _, c := range cases {
		if got := nextRecordStep(c.prState, c.headOnMain); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

func TestRecordMessageNamesThePR(t *testing.T) {
	if got := recordMessage(393).Subject; got != "chore: record PR #393 as merged" {
		t.Errorf("got %q", got)
	}
	if got := recordBranch(393); got != "record-pr-393" {
		t.Errorf("got %q", got)
	}
}

func TestPatchesUpstream(t *testing.T) {
	cases := []struct {
		name, cherry string
		want         bool
	}{
		{"every commit rebased onto main", "- 1111\n- 2222", true},
		{"one commit not upstream", "- 1111\n+ 2222", false},
		{"nothing on the branch", "", true},
	}
	for _, c := range cases {
		if got := patchesUpstream(c.cherry); got != c.want {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

var (
	first  = commit{Subject: "feat: search finds files by size", Body: "- `SizeFilter` parses `>10mb`"}
	second = commit{Subject: "fix: the size filter rounds like Finder"}
	third  = commit{Subject: "docs: the search guide covers size", Body: "Two lines\nof body"}
)

func TestPRTitleIsTheOldestSubject(t *testing.T) {
	if got := prTitle([]commit{first, second}); got != first.Subject {
		t.Errorf("got %q", got)
	}
}

func TestPRBodyListsEveryCommit(t *testing.T) {
	want := "### feat: search finds files by size\n\n- `SizeFilter` parses `>10mb`\n\n" +
		"### fix: the size filter rounds like Finder\n\n" +
		"### docs: the search guide covers size\n\nTwo lines\nof body\n"
	if got := prBody([]commit{first, second, third}); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestPRBodyPastGitHubsLimitListsSubjectsAndSaysWhereTheMessagesAre(t *testing.T) {
	long := commit{Subject: "feat: a huge change", Body: strings.Repeat("x", maxPRBodyBytes)}
	got := prBody([]commit{long, second})
	want := prBodyTooLongNote + "\n\n- feat: a huge change\n- fix: the size filter rounds like Finder\n"
	if got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestPRBodyWithTooManySubjectsTruncatesOnARuneBoundary(t *testing.T) {
	// Two-byte runes, offset by one byte, so a cut at the byte limit would split one.
	commits := make([]commit, 0, 40_000)
	for range 40_000 {
		commits = append(commits, commit{Subject: "fix: é"})
	}
	got := prBody(commits)
	if len(got) > maxPRBodyBytes {
		t.Fatalf("the body is %d bytes, past the %d GitHub takes", len(got), maxPRBodyBytes)
	}
	if !utf8.ValidString(got) {
		t.Fatal("the body was cut inside a rune")
	}
	if !strings.HasPrefix(got, prBodyTooLongNote) || !strings.HasSuffix(got, "\n…\n") {
		t.Errorf("want the note first and an ellipsis last, got the tail %q", got[len(got)-40:])
	}
}

func TestPRBodyAtExactlyTheLimitIsKeptWhole(t *testing.T) {
	// "### " + subject + "\n" is 5 bytes around the subject.
	exact := commit{Subject: strings.Repeat("y", maxPRBodyBytes-5)}
	if got := prBody([]commit{exact}); got != "### "+exact.Subject+"\n" {
		t.Errorf("a body of exactly %d bytes must stay whole, got %d bytes", maxPRBodyBytes, len(got))
	}
}

func TestSquashMessage(t *testing.T) {
	want := "feat: search finds files by size\n\n- `SizeFilter` parses `>10mb`\n\n" +
		"fix: the size filter rounds like Finder\n\n" +
		"docs: the search guide covers size\n\nTwo lines\nof body\n"
	if got := squashMessage([]commit{first, second, third}); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
	if got, want := squashMessage([]commit{second}), "fix: the size filter rounds like Finder\n"; got != want {
		t.Errorf("one commit: got %q, want %q", got, want)
	}
}

func TestNeedsSquash(t *testing.T) {
	if needsSquash([]commit{first}, "") {
		t.Error("one commit and no message: nothing to squash, so a re-run is a no-op")
	}
	if !needsSquash([]commit{first, second}, "") {
		t.Error("two commits squash")
	}
	if !needsSquash([]commit{first}, "feat: a better title") {
		t.Error("a new message rewrites even one commit")
	}
	if needsSquash([]commit{first}, first.Subject+"\n\n"+first.Body) {
		t.Error("the same message again is a no-op")
	}
}

func TestCIVerdict(t *testing.T) {
	at := func(m int) time.Time { return time.Date(2026, 10, 18, 12, m, 0, 0, time.UTC) }
	run := func(id int64, status string, created int) workflowRun {
		return workflowRun{ID: id, Status: status, CreatedAt: at(created), RunStartedAt: at(created), CheckSuiteID: 100 + id}
	}
	check := func(id int64, status, conclusion string, suite int64, started int) checkRun {
		c := checkRun{ID: id, Status: status, Conclusion: conclusion, StartedAt: at(started)}
		c.CheckSuite.ID = suite
		return c
	}
	running := run(2, "in_progress", 5)
	red := run(1, "completed", 1)
	done := run(2, "completed", 5)
	ok := check(20, "completed", "success", 102, 30)
	failed := check(10, "completed", "failure", 101, 20)
	skipped := check(30, "completed", "skipped", 102, 30)

	cases := []struct {
		name   string
		runs   []workflowRun
		checks []checkRun
		want   verdict
		wantID int64
	}{
		{"no run yet", nil, nil, verdictMissing, 0},
		{"running", []workflowRun{running}, nil, verdictPending, 2},
		{"a re-run in flight outranks the old red check", []workflowRun{red, running}, []checkRun{failed}, verdictPending, 2},
		{"green", []workflowRun{done}, []checkRun{failed, ok}, verdictGreen, 2},
		{"the newest check decides", []workflowRun{done}, []checkRun{ok, check(40, "completed", "failure", 102, 31)}, verdictRed, 2},
		{"red run", []workflowRun{red}, []checkRun{failed}, verdictRed, 1},
		{"finished with no CI OK at all", []workflowRun{red}, nil, verdictRed, 1},
		{"a skipped CI OK is not green", []workflowRun{done}, []checkRun{skipped}, verdictRed, 2},
		// GitHub can mark the run completed while its `CI OK` is still queued (seen 2026-10-22).
		{"a finished run whose CI OK has not run yet", []workflowRun{done}, []checkRun{check(50, "queued", "", 102, 0)}, verdictPending, 2},
		// PR #944 (2026-11-04): `CI OK` went green 16 minutes before GitHub marked the run done.
		{"a green CI OK beats a run still in progress", []workflowRun{running}, []checkRun{ok}, verdictGreen, 2},
		{"a red CI OK beats a run still in progress", []workflowRun{running}, []checkRun{check(20, "completed", "failure", 102, 30)}, verdictRed, 2},
		{"a running CI OK is pending", []workflowRun{running}, []checkRun{check(20, "in_progress", "", 102, 30)}, verdictPending, 2},
		// The same SHA's other run (a release's dispatch run beside its PR run) has its own suite.
		{"another run's green CI OK doesn't count", []workflowRun{red, running}, []checkRun{check(15, "completed", "success", 101, 3)}, verdictPending, 2},
	}
	for _, c := range cases {
		got, run := ciVerdict(c.runs, c.checks)
		if got != c.want || run.ID != c.wantID {
			t.Errorf("%s: got %d (run %d), want %d (run %d)", c.name, got, run.ID, c.want, c.wantID)
		}
	}
}

// A light run (a `-no-wait` push nobody asked to land) never reports `CI OK`, so the wait follows
// the newest land run and ignores light ones, however new or green.
func TestCIVerdictIgnoresLightRuns(t *testing.T) {
	at := func(m int) time.Time { return time.Date(2026, 11, 5, 9, m, 0, 0, time.UTC) }
	run := func(id int64, status, title string, created int) workflowRun {
		return workflowRun{ID: id, Status: status, DisplayTitle: title, CreatedAt: at(created), RunStartedAt: at(created), CheckSuiteID: 100 + id}
	}
	green := func(suite int64) checkRun {
		c := checkRun{ID: suite, Status: "completed", Conclusion: "success", StartedAt: at(30)}
		c.CheckSuite.ID = suite
		return c
	}
	light := run(1, "completed", "Light: feat: a thing", 1)
	newerLight := run(3, "in_progress", "Light: feat: a thing", 9)
	landing := run(2, "in_progress", "feat: a thing", 5)

	cases := []struct {
		name   string
		runs   []workflowRun
		checks []checkRun
		want   verdict
		wantID int64
	}{
		{"a finished light run alone is no run yet, never red", []workflowRun{light}, nil, verdictMissing, 0},
		{"the land run decides over an older light run", []workflowRun{light, landing}, nil, verdictPending, 2},
		{"a newer light run (another label) doesn't shadow the land run", []workflowRun{light, landing, newerLight}, []checkRun{green(102)}, verdictGreen, 2},
	}
	for _, c := range cases {
		got, run := ciVerdict(c.runs, c.checks)
		if got != c.want || run.ID != c.wantID {
			t.Errorf("%s: got %d (run %d), want %d (run %d)", c.name, got, run.ID, c.want, c.wantID)
		}
	}
}

// PR #982 (2026-11-05): one PR open set off three land runs in the same second (`created_at` has
// one-second precision), and the concurrency group cancelled all but the last. The wait has to
// follow the survivor, never a cancelled sibling that tied with it.
func TestCIVerdictFollowsTheNewestOfABurst(t *testing.T) {
	second := time.Date(2026, 11, 5, 11, 39, 48, 0, time.UTC)
	burst := func(id int64, status, conclusion string) workflowRun {
		return workflowRun{ID: id, Status: status, Conclusion: conclusion, DisplayTitle: "feat: a thing", CreatedAt: second, RunStartedAt: second, CheckSuiteID: id}
	}
	cancelledA := burst(42020776853, "completed", "cancelled")
	cancelledB := burst(42020777514, "completed", "cancelled")
	alive := burst(42020777999, "in_progress", "")
	// If the IDs ever disagree with the order the group cancelled them in, the live run still wins.
	cancelledHigh := burst(42020778500, "completed", "cancelled")
	later := workflowRun{ID: 1, Status: "completed", Conclusion: "cancelled", DisplayTitle: "feat: a thing", CreatedAt: second.Add(time.Minute), CheckSuiteID: 1}

	cases := []struct {
		name   string
		runs   []workflowRun
		want   verdict
		wantID int64
	}{
		{"a cancelled sibling listed first doesn't win the tie", []workflowRun{cancelledA, cancelledB, alive}, verdictPending, alive.ID},
		{"order in the list doesn't matter", []workflowRun{alive, cancelledB, cancelledA}, verdictPending, alive.ID},
		{"a cancelled tie with a higher ID doesn't hide the live run", []workflowRun{cancelledHigh, alive}, verdictPending, alive.ID},
		// Nothing newer runs, so someone cancelled it: not green.
		{"a cancelled run that is genuinely the newest reads red", []workflowRun{alive, later}, verdictRed, later.ID},
		{"a whole cancelled burst reads red, on the newest", []workflowRun{cancelledA, cancelledB}, verdictRed, cancelledB.ID},
	}
	for _, c := range cases {
		got, run := ciVerdict(c.runs, nil)
		if got != c.want || run.ID != c.wantID {
			t.Errorf("%s: got %d (run %d), want %d (run %d)", c.name, got, run.ID, c.want, c.wantID)
		}
	}
}

// A new PR opens with only `claude-created`; `askToLand` adds `land` in a call of its own, so one
// event starts the land run (§ Gotcha: labels on `gh pr create` in DETAILS.md).
func TestPRCreateArgsCarryOnlyTheAgentLabel(t *testing.T) {
	args := prCreateArgs("vdavid/cmdr", "worktree-x", []commit{first})
	var labels []string
	for i, a := range args {
		if a == "--label" && i+1 < len(args) {
			labels = append(labels, args[i+1])
		}
	}
	if !slices.Equal(labels, []string{agentLabel}) {
		t.Errorf("labels: got %q, want only %q", labels, agentLabel)
	}
	if !slices.Contains(args, "worktree-x") || !slices.Contains(args, "vdavid/cmdr") || !slices.Contains(args, first.Subject) {
		t.Errorf("args miss the branch, repo, or title: %q", args)
	}
}

func TestCIFailureForACancelledRunSaysSo(t *testing.T) {
	run := workflowRun{ID: 42, Status: "completed", Conclusion: "cancelled"}
	msg := ciFailure(run, nil, nil, false, "pnpm land").Error()
	if !strings.Contains(msg, "cancelled") || !strings.Contains(msg, "gh run rerun 42 -R") || strings.Contains(msg, "--failed") {
		t.Errorf("a cancelled run should say so and re-run the whole run:\n%s", msg)
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

func TestHasLabel(t *testing.T) {
	pr := pullRequest{Labels: []prLabel{{Name: agentLabel}, {Name: landLabel}}}
	if !pr.hasLabel(landLabel) || (pullRequest{Labels: []prLabel{{Name: agentLabel}}}).hasLabel(landLabel) {
		t.Error("hasLabel should find exactly the labels the PR carries")
	}
}

// A re-run reopens the run as a new attempt in the same check suite, and the earlier attempt's
// `CI OK` stays in the list (run 41854674280, 2026-11-04). Only the current attempt's counts, so
// an old green can't land a SHA whose re-run is still going, and an old red can't stop one.
func TestCIVerdictAcrossReRuns(t *testing.T) {
	at := func(m int) time.Time { return time.Date(2026, 11, 4, 15, m, 0, 0, time.UTC) }
	attempt := func(n int, status string, started int) workflowRun {
		return workflowRun{ID: 7, Status: status, CreatedAt: at(0), RunAttempt: n, RunStartedAt: at(started), CheckSuiteID: 70}
	}
	check := func(id int64, status, conclusion string, started int) checkRun {
		c := checkRun{ID: id, Status: status, Conclusion: conclusion, StartedAt: at(started)}
		c.CheckSuite.ID = 70
		return c
	}
	oldGreen := check(1, "completed", "success", 30)
	oldRed := check(1, "completed", "failure", 30)
	reRun := attempt(2, "in_progress", 40)

	cases := []struct {
		name   string
		run    workflowRun
		checks []checkRun
		want   verdict
	}{
		{"the first attempt's green", attempt(1, "in_progress", 0), []checkRun{oldGreen}, verdictGreen},
		{"a re-run in flight outranks the old green", reRun, []checkRun{oldGreen}, verdictPending},
		{"a re-run in flight outranks the old red", reRun, []checkRun{oldRed}, verdictPending},
		{"the re-run's CI OK is queued", reRun, []checkRun{oldGreen, check(2, "queued", "", 0)}, verdictPending},
		{"the re-run's red beats the old green", reRun, []checkRun{oldGreen, check(2, "completed", "failure", 55)}, verdictRed},
		{"the re-run's green beats the old red", reRun, []checkRun{oldRed, check(2, "completed", "success", 55)}, verdictGreen},
		{"a finished re-run that never reached CI OK", attempt(2, "completed", 40), []checkRun{oldGreen}, verdictRed},
	}
	for _, c := range cases {
		if got, _ := ciVerdict([]workflowRun{c.run}, c.checks); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

func TestLeftoverRuns(t *testing.T) {
	ours := func(id int64, status string) workflowRun {
		run := workflowRun{ID: id, Status: status}
		// The org the repo may move to (#476): the match follows the resolved name, not a pin.
		run.HeadRepository.FullName = "getcmdr/cmdr"
		return run
	}
	fork := workflowRun{ID: 4, Status: "in_progress"}
	fork.HeadRepository.FullName = "someone/cmdr"
	runs := []workflowRun{ours(1, "completed"), ours(2, "in_progress"), ours(3, "queued"), fork}

	var got []int64
	for _, run := range leftoverRuns(runs, "getcmdr/cmdr") {
		got = append(got, run.ID)
	}
	if want := []int64{2, 3}; !slices.Equal(got, want) {
		t.Errorf("got %v, want %v: only this repo's unfinished runs", got, want)
	}
}

func TestPollDelay(t *testing.T) {
	cases := []struct {
		name   string
		v      verdict
		runAge time.Duration
		want   time.Duration
	}{
		{"waiting for the run to show up", verdictMissing, 0, fastPoll},
		{"a just-started run may skip every lane", verdictPending, 10 * time.Second, fastPoll},
		{"still fast at the minute a skipping CI takes", verdictPending, 50 * time.Second, fastPoll},
		{"backing off with age", verdictPending, 2 * time.Minute, 12 * time.Second},
		{"a long run stays at the cap", verdictPending, 20 * time.Minute, slowPoll},
	}
	for _, c := range cases {
		if got := pollDelay(c.v, c.runAge); got != c.want {
			t.Errorf("%s: got %s, want %s", c.name, got, c.want)
		}
	}
}

// A re-run restarts the clock, so its first minute is polled fast too.
func TestAttemptStart(t *testing.T) {
	created := time.Date(2026, 11, 4, 15, 0, 0, 0, time.UTC)
	if got := (workflowRun{CreatedAt: created}).attemptStart(); !got.Equal(created) {
		t.Errorf("no run_started_at: got %s, want the creation time", got)
	}
	reRun := workflowRun{CreatedAt: created, RunStartedAt: created.Add(time.Hour)}
	if got := reRun.attemptStart(); !got.Equal(reRun.RunStartedAt) {
		t.Errorf("a re-run: got %s, want its attempt's start", got)
	}
}

func TestOpenPR(t *testing.T) {
	prs := []pullRequest{{Number: 1, State: "CLOSED"}, {Number: 2, State: "OPEN"}, {Number: 3, State: "MERGED"}}
	if pr, ok := openPR(prs); !ok || pr.Number != 2 {
		t.Errorf("got %+v %v", pr, ok)
	}
	if _, ok := openPR(prs[:1]); ok {
		t.Error("a closed PR isn't reused")
	}
}

func TestPRNumber(t *testing.T) {
	if n, ok := prNumber("https://github.com/vdavid/cmdr/pull/512"); !ok || n != 512 {
		t.Errorf("got %d %v", n, ok)
	}
	if _, ok := prNumber("https://github.com/vdavid/cmdr/pull/"); ok {
		t.Error("no number: got one")
	}
}

func TestTeardownCommand(t *testing.T) {
	cases := []struct{ root, want string }{
		{"/repo/.claude/worktrees/cursor-fix", "~/.claude/scripts/remove-worktree.sh cursor-fix"},
		{"/repo/.claude/worktrees/cursor-fix/", "~/.claude/scripts/remove-worktree.sh cursor-fix"},
		{"/repo", ""},
		{"/elsewhere/cursor-fix", ""},
	}
	for _, c := range cases {
		if got := teardownCommand(c.root); got != c.want {
			t.Errorf("%s: got %q, want %q", c.root, got, c.want)
		}
	}
}

func TestWorktrees(t *testing.T) {
	porcelain := "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\n" +
		"worktree /repo/.claude/worktrees/x\nHEAD def\nbranch refs/heads/worktree-x\n\n" +
		"worktree /repo/.claude/worktrees/y\nHEAD 123\ndetached\n"
	wts := parseWorktrees(porcelain)
	if len(wts) != 3 {
		t.Fatalf("got %+v", wts)
	}
	if wt, ok := worktreeOf(wts, "main"); !ok || wt.path != "/repo" {
		t.Errorf("main: got %+v %v", wt, ok)
	}
	if wt, ok := worktreeOf(wts, "worktree-x"); !ok || wt.path != "/repo/.claude/worktrees/x" {
		t.Errorf("worktree-x: got %+v %v", wt, ok)
	}
	if _, ok := worktreeOf(wts, "nope"); ok {
		t.Error("nope: found")
	}
}

// A run that showed up once and then dropped out of GitHub's list (2026-10-20: "no CI run after
// 5m0s" right after "Waiting for CI OK: …/runs/39442510363") is still on its way, so only a run
// that never appeared ends the wait.
func TestMissingRunGivesUp(t *testing.T) {
	cases := []struct {
		name   string
		seen   bool
		waited time.Duration
		want   bool
	}{
		{"just pushed", false, time.Minute, false},
		{"never appeared", false, appearWithin + time.Second, true},
		{"seen, then missing", true, appearWithin + time.Hour, false},
	}
	for _, c := range cases {
		if got := missingRunGivesUp(c.seen, c.waited); got != c.want {
			t.Errorf("%s: got %v, want %v", c.name, got, c.want)
		}
	}
}

// The shape of ci.yml's gate: a multi-line flow `needs`, a job with no `name:` (GitHub shows its
// ID), a quoted name, a commented-out need, and a post-land job the gate leaves out.
const gateYML = `name: CI
on:
  pull_request:
jobs:
  changes:
    name: Detect changes
    runs-on: ubuntu-latest
    outputs:
      name: not-a-job-name
  lint:
    name: "Desktop (Svelte, type-aware ESLint)"
    needs: changes
  macos:
    name: Desktop (Rust, macOS)
    needs: changes
    uses: ./.github/workflows/macos-lane.yml
  bare-job:
    needs: [changes]
  warm-cache:
    name: Warm Rust build cache (Linux)
    needs: changes
  ci-ok:
    name: CI OK
    needs:
      [
        changes,
        lint, # the slow one
        # retired,
        macos,
        bare-job,
      ]
    if: always()
`

func TestRequiredJobNames(t *testing.T) {
	got := requiredJobNames(gateYML)
	want := []string{"Detect changes", "Desktop (Svelte, type-aware ESLint)", "Desktop (Rust, macOS)", "bare-job"}
	if !slices.Equal(got, want) {
		t.Errorf("got %q, want %q", got, want)
	}
	flow := "    needs:\n      [\n        changes,\n        lint, # the slow one\n        # retired,\n        macos,\n        bare-job,\n      ]\n"
	block := "    needs:\n      - changes\n      - lint\n"
	if got := requiredJobNames(strings.Replace(gateYML, flow, block, 1)); !slices.Equal(got, []string{"Detect changes", "Desktop (Svelte, type-aware ESLint)"}) {
		t.Errorf("block sequence: got %q", got)
	}
	if got := requiredJobNames("jobs:\n  build:\n    name: Build\n"); got != nil {
		t.Errorf("no ci-ok job: got %q, want nil", got)
	}
}

func TestFailedRequiredJobs(t *testing.T) {
	required := []string{"Detect changes", "Desktop (Svelte, type-aware ESLint)", "Desktop (Rust, macOS)"}
	job := func(name, status, conclusion string) runJob {
		return runJob{Name: name, Status: status, Conclusion: conclusion, RunAttempt: 2}
	}
	stale := runJob{Name: "Detect changes", Status: "completed", Conclusion: "failure", RunAttempt: 1}
	cases := []struct {
		name string
		jobs []runJob
		want []string
	}{
		{"all running", []runJob{job("Detect changes", "completed", "success"), job("Desktop (Svelte, type-aware ESLint)", "in_progress", "")}, nil},
		// Run 40670839400 (2026-10-28): this lane failed at 4 min, and `CI OK` reported 16 min later.
		{"a required lane failed", []runJob{job("Desktop (Svelte, type-aware ESLint)", "completed", "failure"), job("E2E tests (Playwright, Linux)", "in_progress", "")}, []string{"Desktop (Svelte, type-aware ESLint)"}},
		{"timed out counts", []runJob{job("Detect changes", "completed", "timed_out")}, []string{"Detect changes"}},
		{"cancelled and skipped don't", []runJob{job("Detect changes", "completed", "cancelled"), job("Desktop (Rust, macOS)", "completed", "skipped")}, nil},
		{"a job outside the gate doesn't", []runJob{job("Warm Rust build cache (Linux)", "completed", "failure")}, nil},
		{"a reusable workflow's inner job counts", []runJob{job("Desktop (Rust, macOS) / Tests", "completed", "failure")}, []string{"Desktop (Rust, macOS) / Tests"}},
		// Matrix legs ("Name (ubuntu)") don't match either: ci.yml has none, and a job outside the
		// gate named like one ("Desktop (Rust) (deploy)") would stop a land CI OK passes.
		{"a name that only shares a prefix doesn't", []runJob{job("Detect changes (deploy)", "completed", "failure"), job("Detect changes again", "completed", "failure")}, nil},
		{"CI OK itself isn't a lane", []runJob{job("CI OK", "completed", "failure")}, nil},
		{"the red job a re-run replaces doesn't", []runJob{stale, job("Detect changes", "queued", "")}, nil},
	}
	for _, c := range cases {
		var got []string
		for _, j := range failedRequiredJobs(c.jobs, required, 2) {
			got = append(got, j.Name)
		}
		if !slices.Equal(got, c.want) {
			t.Errorf("%s: got %q, want %q", c.name, got, c.want)
		}
	}
}

// Regression: on 2026-11-05 a re-run of `pnpm land` (after a poll error) found `main` moved and
// rebased the batch PR #992, though its land run was already going. A head that's already origin's
// branch, with a PR GitHub can merge, keeps that CI; the land then makes a merge commit.
func TestNextStartStepKeepsAPushedMergeableHead(t *testing.T) {
	pushed := branchState{branch: "worktree-x", pushedMergeable: true}
	if got := nextStartStep(pushed, false); got != stepProceed {
		t.Errorf("a pushed, mergeable head: got %d, want %d", got, stepProceed)
	}
	if got := nextStartStep(branchState{branch: "worktree-x"}, false); got != stepRebase {
		t.Errorf("not pushed, or not mergeable: got %d, want %d", got, stepRebase)
	}
	if got := nextStartStep(branchState{branch: "worktree-x", pushedMergeable: true, dirty: true}, false); got != stepRefuseDirty {
		t.Errorf("a dirty tree still refuses: got %d, want %d", got, stepRefuseDirty)
	}
}
