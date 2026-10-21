package main

import (
	"slices"
	"testing"
	"time"
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
		if got := nextStartStep(c.state); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

func TestNextLandPath(t *testing.T) {
	yes, no := true, false
	cases := []struct {
		name       string
		based      bool
		hasMerges  bool
		rebaseable *bool
		want       landPath
	}{
		{"main didn't move", true, false, nil, landFastForward},
		{"main didn't move under a merge commit", true, true, nil, landFastForward},
		{"main moved", false, false, &yes, landGitHubRebase},
		{"main moved, GitHub still computing", false, false, nil, landGitHubRebase},
		{"main moved into a conflict", false, false, &no, landRebaseAndRerun},
		{"main moved under a merge commit", false, true, &yes, landRebaseAndRerun},
	}
	for _, c := range cases {
		if got := nextLandPath(c.based, c.hasMerges, c.rebaseable); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
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
	running := workflowRun{ID: 2, Status: "in_progress", CreatedAt: at(5)}
	red := workflowRun{ID: 1, Status: "completed", Conclusion: "failure", CreatedAt: at(1)}
	done := workflowRun{ID: 2, Status: "completed", Conclusion: "success", CreatedAt: at(5)}
	ok := checkRun{ID: 20, Status: "completed", Conclusion: "success"}
	failed := checkRun{ID: 10, Status: "completed", Conclusion: "failure"}
	skipped := checkRun{ID: 30, Status: "completed", Conclusion: "skipped"}

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
		{"the newest check decides", []workflowRun{done}, []checkRun{ok, {ID: 40, Status: "completed", Conclusion: "failure"}}, verdictRed, 2},
		{"red run", []workflowRun{red}, []checkRun{failed}, verdictRed, 1},
		{"finished with no CI OK at all", []workflowRun{red}, nil, verdictRed, 1},
		{"a skipped CI OK is not green", []workflowRun{done}, []checkRun{skipped}, verdictRed, 2},
	}
	for _, c := range cases {
		got, run := ciVerdict(c.runs, c.checks)
		if got != c.want || run.ID != c.wantID {
			t.Errorf("%s: got %d (run %d), want %d (run %d)", c.name, got, run.ID, c.want, c.wantID)
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
		{"waiting for the run to show up", verdictMissing, 0, appearPoll},
		{"a young run can't be done yet", verdictPending, 2 * time.Minute, earlyPoll},
		{"an older run may finish any time", verdictPending, 20 * time.Minute, latePoll},
	}
	for _, c := range cases {
		if got := pollDelay(c.v, c.runAge); got != c.want {
			t.Errorf("%s: got %s, want %s", c.name, got, c.want)
		}
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
