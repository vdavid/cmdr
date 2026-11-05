package main

import (
	"cmp"
	"fmt"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"
)

// The decisions that make `land` idempotent: each run reads where the branch stands locally
// and on GitHub and picks up from there, so it holds no state of its own between runs.

// branchState is what the local clone says about the branch, right after a fetch.
type branchState struct {
	// branch is the checked-out branch, empty on a detached HEAD.
	branch string
	dirty  bool
	// landed: HEAD is already on origin/main (a finished land, or a branch with no commits).
	landed bool
	// based: origin/main is an ancestor of HEAD, so the branch fast-forwards onto it.
	based bool
	// hasMerges: the branch carries a merge commit, which a plain rebase would flatten. A
	// contributor's PR record never rides a branch: `-record N` lands it alone.
	hasMerges bool
	// tagged: a tag points at one of the branch's commits (a release commit), so a rebase would
	// leave the tag off `main`'s history. `-merge` lands it as it is.
	tagged bool
	// pushedMergeable: `main` moved, but HEAD is already origin's branch and GitHub can merge its
	// open PR, so a re-run keeps that PR's CI (and lands with a merge commit) rather than
	// rebasing, which would push a new head and restart CI. Never set under `-squash`.
	pushedMergeable bool
}

type startStep int

const (
	// stepRefuseBranch: on `main` or a detached HEAD, there's no feature branch to land.
	stepRefuseBranch startStep = iota
	stepRefuseDirty
	// stepFinish: nothing left to land, so only sync local `main` and tidy the remote branch.
	stepFinish
	// stepRebase: `origin/main` moved, so catch up before pushing.
	stepRebase
	// stepRefuseMergeRebase: `origin/main` moved under a merge commit; say how to catch up by hand
	// and stop.
	stepRefuseMergeRebase
	// stepRefuseTaggedRebase: `origin/main` moved under a tagged commit, which only `-merge` lands.
	stepRefuseTaggedRebase
	stepProceed
)

// nextStartStep decides from the local state what a run does first. With `-merge` the branch is
// never rebased: it lands as it is, by fast-forward or a merge commit GitHub makes.
func nextStartStep(s branchState, merge bool) startStep {
	switch {
	case s.branch == "" || s.branch == "main":
		return stepRefuseBranch
	case s.dirty:
		return stepRefuseDirty
	case s.landed:
		return stepFinish
	case merge || s.based || s.pushedMergeable:
		return stepProceed
	case s.tagged:
		return stepRefuseTaggedRebase
	case !s.based && s.hasMerges:
		return stepRefuseMergeRebase
	case !s.based:
		return stepRebase
	default:
		return stepProceed
	}
}

// patchesUpstream reports whether `git cherry <upstream> <branch>` found every commit already
// upstream ("-" lines only): the branch's patches reached `main` under other SHAs (a cherry-pick,
// or an older land through GitHub's rebase merge).
func patchesUpstream(cherry string) bool {
	for line := range strings.SplitSeq(cherry, "\n") {
		if line != "" && !strings.HasPrefix(line, "- ") {
			return false
		}
	}
	return true
}

// landPath is how a branch whose head is green on CI gets onto `main`.
type landPath int

const (
	// landFastForward: the head still descends from `origin/main`, so push it there as is.
	landFastForward landPath = iota
	// landGitHubMerge: `origin/main` moved, so GitHub merges the green PR into it with a merge
	// commit. The branch's commits keep their SHAs and David's signatures, GitHub signs the merge
	// commit with its own key, and CI on `main` checks the result. Never a GitHub rebase merge: it
	// rewrites every commit unsigned, which a required-signatures rule refuses.
	landGitHubMerge
	// landRebaseAndRerun: GitHub can't merge it cleanly, so the branch catches up here (a local,
	// signed rebase) and CI runs again.
	landRebaseAndRerun
	// landMergeConflict (`-merge`): GitHub can't merge it cleanly, and the branch must not be
	// rebased (a release commit and its tag); a person merges by hand.
	landMergeConflict
)

// nextLandPath picks the land path. merge: `-merge`, which never rebases. mergeable is GitHub's
// view of the PR, nil while it's still computing it.
func nextLandPath(based, merge bool, mergeable *bool) landPath {
	switch {
	case based:
		return landFastForward
	case mergeable != nil && !*mergeable && merge:
		return landMergeConflict
	case mergeable != nil && !*mergeable:
		return landRebaseAndRerun
	default:
		// Still computing: try, and GitHub refuses the merge if it can't make it.
		return landGitHubMerge
	}
}

// recordStep is what `-record N` does once the branch itself landed.
type recordStep int

const (
	// recordRefuseClosed: the PR is closed unmerged, and GitHub marks only an open PR merged.
	recordRefuseClosed recordStep = iota
	recordAlreadyMerged
	// recordConfirm: the PR head is already on `main` (a record landed), so only wait for GitHub
	// to mark the PR merged, then tidy up.
	recordConfirm
	// recordBuild: build the record, put it through CI, and land it.
	recordBuild
)

// nextRecordStep decides from the contributor PR's state (`gh pr view --json state`) and whether
// its head is already an ancestor of `origin/main`.
func nextRecordStep(prState string, headOnMain bool) recordStep {
	switch {
	case prState == "MERGED":
		return recordAlreadyMerged
	case prState != "OPEN":
		return recordRefuseClosed
	case headOnMain:
		return recordConfirm
	default:
		return recordBuild
	}
}

// recordBranch is the branch that carries PR n's record through CI.
func recordBranch(n int) string {
	return "record-pr-" + strconv.Itoa(n)
}

// recordMessage is the record commit's message, which its PR also takes as title and body.
func recordMessage(n int) commit {
	return commit{
		Subject: fmt.Sprintf("chore: record PR #%d as merged", n),
		Body:    fmt.Sprintf("An `ours` merge of PR #%d's head: it changes no file, only makes that head an ancestor of `main`, so GitHub marks the PR merged. Its commits landed cherry-picked, in their own land. See `docs/guides/handling-prs.md`.", n),
	}
}

// teardownCommand is the command that removes the worktree at root, or "" when root isn't one of
// the `.claude/worktrees/<slug>` worktrees (the main clone, where `release.sh` lands from).
func teardownCommand(root string) string {
	dir, slug := filepath.Split(filepath.Clean(root))
	if filepath.Base(filepath.Clean(dir)) != "worktrees" || filepath.Base(filepath.Dir(filepath.Clean(dir))) != ".claude" {
		return ""
	}
	return "~/.claude/scripts/remove-worktree.sh " + slug
}

// commit is one commit message, split the way `git log` gives it.
type commit struct {
	Subject string
	Body    string
}

func (c commit) message() string {
	if c.Body == "" {
		return c.Subject + "\n"
	}
	return c.Subject + "\n\n" + c.Body + "\n"
}

// prTitle names the PR after the branch's oldest commit, the one that says what the branch is for.
func prTitle(commits []commit) string {
	if len(commits) == 0 {
		return ""
	}
	return commits[0].Subject
}

// prCreateArgs opens the branch's PR with only the `claude-created` label. Never `land` too: two
// labels on `gh pr create` came back as four `labeled` events, each starting a run (§ Gotcha:
// labels on `gh pr create` in DETAILS.md); `askToLand` adds `land` in a call of its own.
func prCreateArgs(repo, branch string, commits []commit) []string {
	return []string{"pr", "create", "-R", repo, "--base", "main", "--head", branch, "--title", prTitle(commits), "--body", prBody(commits), "--label", agentLabel}
}

// maxPRBodyBytes is GitHub's PR body limit: 65,536 characters, past which `gh pr create`
// fails. Measured in bytes, which are never fewer than characters however GitHub counts
// them, so a body under it is under GitHub's limit too.
const maxPRBodyBytes = 65_536

// prBodyTooLongNote heads a body that had to drop the commit messages to fit.
const prBodyTooLongNote = "The commit messages run past GitHub's 65,536-character PR body limit, so this lists their subjects, oldest first. The full messages are in the commits."

// prBody lists every commit message, oldest first, one heading each. A branch whose messages
// don't fit GitHub's limit gets its subjects under a note instead, cut on a rune boundary if
// even those don't fit.
func prBody(commits []commit) string {
	parts := make([]string, 0, len(commits))
	for _, c := range commits {
		parts = append(parts, "### "+c.message())
	}
	if full := strings.Join(parts, "\n"); len(full) <= maxPRBodyBytes {
		return full
	}
	var subjects strings.Builder
	for _, c := range commits {
		subjects.WriteString("- " + c.Subject + "\n")
	}
	head := prBodyTooLongNote + "\n\n"
	if len(head)+subjects.Len() <= maxPRBodyBytes {
		return head + subjects.String()
	}
	const ellipsis = "\n…\n"
	return head + truncateOnRune(subjects.String(), maxPRBodyBytes-len(head)-len(ellipsis)) + ellipsis
}

// truncateOnRune cuts s to at most n bytes without splitting a UTF-8 sequence.
func truncateOnRune(s string, n int) string {
	if len(s) <= n {
		return s
	}
	for n > 0 && !utf8.RuneStart(s[n]) {
		n--
	}
	return s[:n]
}

// squashMessage keeps every commit message, oldest first, so the oldest subject becomes the title.
func squashMessage(commits []commit) string {
	parts := make([]string, 0, len(commits))
	for _, c := range commits {
		parts = append(parts, c.message())
	}
	return strings.Join(parts, "\n")
}

// needsSquash reports whether `-squash` would change anything, so a re-run after a squash is a
// no-op and doesn't force-push a fresh SHA that restarts CI.
func needsSquash(commits []commit, msg string) bool {
	if len(commits) != 1 {
		return len(commits) > 1
	}
	return msg != "" && strings.TrimSpace(msg) != strings.TrimSpace(commits[0].message())
}

// workflowRun is one of `repos/{repo}/actions/workflows/{file}/runs`' `workflow_runs`.
type workflowRun struct {
	ID         int64     `json:"id"`
	Status     string    `json:"status"`
	Conclusion string    `json:"conclusion"`
	CreatedAt  time.Time `json:"created_at"`
	// DisplayTitle is the run's name: `ci.yml`'s `run-name` starts a light run's with
	// `lightRunPrefix`.
	DisplayTitle string `json:"display_title"`
	// RunAttempt goes up with each `gh run rerun`, and RunStartedAt moves to that attempt's start.
	RunAttempt   int       `json:"run_attempt"`
	RunStartedAt time.Time `json:"run_started_at"`
	// CheckSuiteID is shared by every attempt of the run, and by every job's check run.
	CheckSuiteID   int64 `json:"check_suite_id"`
	HeadRepository struct {
		FullName string `json:"full_name"`
	} `json:"head_repository"`
}

// leftoverRuns picks the runs a landed branch leaves behind: unfinished, and on this repo's
// branch, not a fork's namesake. GitHub keeps them going after the PR merges and the branch is
// deleted, holding runners (the five macOS ones above all) that nobody waits on anymore. `repo`
// is this repo's current name (`resolveRepo`).
func leftoverRuns(runs []workflowRun, repo string) []workflowRun {
	var out []workflowRun
	for _, run := range runs {
		if run.Status != "completed" && run.HeadRepository.FullName == repo {
			out = append(out, run)
		}
	}
	return out
}

// attemptStart is when the run's current attempt started: a re-run starts the clock over.
func (r workflowRun) attemptStart() time.Time {
	if r.RunStartedAt.IsZero() {
		return r.CreatedAt
	}
	return r.RunStartedAt
}

const (
	// A run shows up seconds after the push, and a CI that skips every lane is green about a
	// minute later, so a young run is polled every `fastPoll`.
	fastPoll = 5 * time.Second
	// The delay grows with the attempt's age (a tenth of it), up to `slowPoll` from minute five:
	// a 30-minute CI is found green within half a minute, and costs about 90 polls.
	pollAgeShare = 10
	slowPoll     = 30 * time.Second
)

// missingRunGivesUp says whether a poll that found no CI run on the head ends the wait. Only a
// run that never showed up does: one that was seen and then dropped out of the list (it
// happened 2026-10-20, with the run still queued) is still on its way, and the wait's own
// timeout bounds it.
func missingRunGivesUp(seen bool, waited time.Duration) bool {
	return !seen && waited > appearWithin
}

// cancelledRunGivesUp says whether a newest run that was cancelled ends the wait, given how long
// polls have seen it cancelled with nothing newer listed. A run a newer one replaced (a push and
// the `land` label's run start together, and the later cancels the earlier) has a successor that
// GitHub's list can lag behind, so it waits `successorWithin` first; only a run still alone after
// that was stopped by a person.
func cancelledRunGivesUp(cancelledFor time.Duration) bool {
	return cancelledFor > successorWithin
}

// cancelWait remembers when polls first saw the newest run cancelled with nothing newer listed.
type cancelWait struct{ since time.Time }

// verdict turns a red verdict on a cancelled run into pending while its successor may still
// show up (`cancelledRunGivesUp`), and passes every other verdict through, resetting the clock.
func (c *cancelWait) verdict(v verdict, run workflowRun, now time.Time) verdict {
	if v != verdictRed || !run.cancelled() {
		c.since = time.Time{}
		return v
	}
	if c.since.IsZero() {
		c.since = now
	}
	if cancelledRunGivesUp(now.Sub(c.since)) {
		return v
	}
	return verdictPending
}

// pollDelay is how long to wait before the next CI poll, before jitter. attemptAge is the age of
// the run's current attempt. Most polls are free 304s (§ Decision: cheap polling in DETAILS.md),
// so the fast start costs little budget; the cap keeps the request rate low with many lands.
func pollDelay(v verdict, attemptAge time.Duration) time.Duration {
	if v == verdictMissing {
		return fastPoll
	}
	return min(max(attemptAge/pollAgeShare, fastPoll), slowPoll)
}

// checkRun is one `CI OK` check run on the head SHA; a re-run adds a new one with a higher id.
type checkRun struct {
	ID         int64     `json:"id"`
	Status     string    `json:"status"`
	Conclusion string    `json:"conclusion"`
	StartedAt  time.Time `json:"started_at"`
	CheckSuite struct {
		ID int64 `json:"id"`
	} `json:"check_suite"`
}

// ofAttempt says whether the check belongs to the run's current attempt. Every attempt shares
// the run's check suite, so an earlier attempt's `CI OK` (it stays in the list) is told apart by
// starting before this attempt did; one still going can't be an earlier attempt's, since a
// re-run starts only once the run finished (verified on run 41854674280's three attempts,
// `gh api .../check-runs`, 2026-11-05). Another run on the same SHA (a release's dispatch run
// beside its PR run) has a suite of its own.
func (c checkRun) ofAttempt(run workflowRun) bool {
	if run.CheckSuiteID != 0 && c.CheckSuite.ID != run.CheckSuiteID {
		return false
	}
	return c.Status != "completed" || !c.StartedAt.Before(run.attemptStart())
}

type verdict int

const (
	// verdictMissing: no CI run on the SHA yet (just pushed, or the PR isn't open).
	verdictMissing verdict = iota
	verdictPending
	verdictGreen
	verdictRed
)

// lightRunPrefix starts a light run's title (`ci.yml`'s `run-name`): a PR run nobody asked to land,
// which skips the heavy lanes and reports `CI light`, never `CI OK`.
const lightRunPrefix = "Light: "

// isLightRun tells a light run from a land run before either finishes. A title that lies only
// makes the wait follow the wrong run: a light run never reports `CI OK`, so it reads red, never
// green.
func (r workflowRun) isLightRun() bool {
	return strings.HasPrefix(r.DisplayTitle, lightRunPrefix)
}

// landRuns drops the light runs: only a land run can report the `CI OK` that lands a head.
func landRuns(runs []workflowRun) []workflowRun {
	return slices.DeleteFunc(slices.Clone(runs), workflowRun.isLightRun)
}

// cancelled: the run ended cancelled (a newer run in its concurrency group, or a person).
func (r workflowRun) cancelled() bool {
	return r.Status == "completed" && r.Conclusion == "cancelled"
}

// byAge orders runs oldest first: `created_at` has one-second precision, so a tie goes to the
// higher ID, the one GitHub made later.
func byAge(a, b workflowRun) int {
	return cmp.Or(a.CreatedAt.Compare(b.CreatedAt), cmp.Compare(a.ID, b.ID))
}

// newestLandRun picks the land run the wait follows: the newest, except that a cancelled one
// gives way to a live run created in the same second. One PR event burst can start several land
// runs in one second, and their concurrency group cancels all but one (PR #982, 2026-11-05: three
// land runs at 11:39:48Z, the first listed a cancelled one). A cancelled run with nothing as new
// beside it was cancelled by a person, and stays the verdict's run: not green.
func newestLandRun(runs []workflowRun) workflowRun {
	newest := slices.MaxFunc(runs, byAge)
	if !newest.cancelled() {
		return newest
	}
	live := slices.DeleteFunc(slices.Clone(runs), func(r workflowRun) bool {
		return r.cancelled() || !r.CreatedAt.Equal(newest.CreatedAt)
	})
	if len(live) == 0 {
		return newest
	}
	return slices.MaxFunc(live, byAge)
}

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

// ciVerdict reads the CI runs and `CI OK` check runs on one SHA. Light runs don't count: they never
// report `CI OK`, and `land` asks for a land run before it waits (`nextLandAsk`). The newest land
// run's (`newestLandRun`) current attempt's `CI OK` decides as soon as it completes, whatever the run's status says:
// it's the exact check `main`'s ruleset demands, and GitHub can mark the run completed many minutes
// later (PR #944, 2026-11-04: `CI OK` done at 16:19:55Z, the run at 16:35:58Z). An earlier
// attempt's or another run's `CI OK` never counts (`ofAttempt`), so an old green can't beat a
// re-run in flight. With no such check, a finished run is red (`CI OK` never ran) and an unfinished
// one pending. Only `success` is green: GitHub would also accept a skipped check, which is never
// what a finished CI run should produce.
func ciVerdict(runs []workflowRun, checks []checkRun) (verdict, workflowRun) {
	runs = landRuns(runs)
	if len(runs) == 0 {
		return verdictMissing, workflowRun{}
	}
	run := newestLandRun(runs)
	current := slices.DeleteFunc(slices.Clone(checks), func(c checkRun) bool { return !c.ofAttempt(run) })
	if len(current) == 0 {
		if run.Status == "completed" {
			return verdictRed, run
		}
		return verdictPending, run
	}
	check := slices.MaxFunc(current, func(a, b checkRun) int { return cmp.Compare(a.ID, b.ID) })
	switch {
	// GitHub can report the run completed a moment before its `CI OK` job runs.
	case check.Status != "completed":
		return verdictPending, run
	case check.Conclusion == "success":
		return verdictGreen, run
	default:
		return verdictRed, run
	}
}

// runJob is one of `repos/{repo}/actions/runs/{id}/jobs`' `jobs`.
type runJob struct {
	ID         int64  `json:"id"`
	Name       string `json:"name"`
	Status     string `json:"status"`
	Conclusion string `json:"conclusion"`
	URL        string `json:"html_url"`
	RunAttempt int    `json:"run_attempt"`
}

// failed: the job ended red. Cancelled and skipped aren't failures (a lane change detection
// skipped, a run a newer push replaced); the gate's own rules decide those.
func (j runJob) failed() bool {
	return j.Status == "completed" && (j.Conclusion == "failure" || j.Conclusion == "timed_out")
}

// ciGateJob is the job ID of `CI OK` in `ci.yml`.
const ciGateJob = "ci-ok"

var (
	workflowJobKeyRe = regexp.MustCompile(`^  ([A-Za-z0-9_-]+):\s*(#.*)?$`)
	jobIDRe          = regexp.MustCompile(`[A-Za-z0-9_][A-Za-z0-9_-]*`)
)

// requiredJobNames reads `ci.yml` and returns the display names of the jobs `CI OK` needs, the
// names GitHub's jobs API reports (a job's `name:`, else its ID). nil when there's no `ci-ok`
// job. Line-based, like `scripts/check/checks/ci-ok-gate.go`: ci.yml keeps the standard
// two-space layout. A name with an expression in it can't be matched, so it's left out.
func requiredJobNames(ciYml string) []string {
	jobs := map[string][]string{}
	inJobs, current := false, ""
	for line := range strings.SplitSeq(ciYml, "\n") {
		if strings.TrimSpace(line) == "" {
			continue
		}
		if !strings.HasPrefix(line, " ") && !strings.HasPrefix(line, "#") {
			inJobs, current = strings.HasPrefix(line, "jobs:"), ""
			continue
		}
		if !inJobs {
			continue
		}
		if m := workflowJobKeyRe.FindStringSubmatch(line); m != nil {
			current = m[1]
			jobs[current] = []string{}
			continue
		}
		if current != "" && strings.HasPrefix(line, "    ") {
			jobs[current] = append(jobs[current], line)
		}
	}
	gate, ok := jobs[ciGateJob]
	if !ok {
		return nil
	}
	var names []string
	for _, id := range gateNeeds(gate) {
		body, ok := jobs[id]
		if !ok {
			continue
		}
		name := id
		for _, line := range body {
			if v, ok := strings.CutPrefix(line, "    name:"); ok {
				name = strings.Trim(strings.TrimSpace(v), `'"`)
				break
			}
		}
		if !strings.Contains(name, "${{") {
			names = append(names, name)
		}
	}
	return names
}

// gateNeeds returns the job IDs in a job's `needs`: a scalar, a flow sequence on one or several
// lines, or a block sequence.
func gateNeeds(body []string) []string {
	var ids []string
	collecting := false
	for _, line := range body {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "#") {
			continue
		}
		if i := strings.Index(trimmed, "#"); i >= 0 {
			trimmed = trimmed[:i]
		}
		if !collecting {
			if v, ok := strings.CutPrefix(line, "    needs:"); ok {
				collecting = true
				ids = append(ids, jobIDRe.FindAllString(v, -1)...)
			}
			continue
		}
		// A sibling key at the job's own indent ends the value.
		if !strings.HasPrefix(line, "     ") && !strings.HasPrefix(trimmed, "-") && !strings.HasPrefix(trimmed, "]") {
			break
		}
		ids = append(ids, jobIDRe.FindAllString(trimmed, -1)...)
	}
	return ids
}

// failedRequiredJobs picks the jobs `CI OK` requires that already ended red (`failure` or
// `timed_out`), so the wait can stop before the rest finish: `CI OK` is bound to fail. Cancelled
// and skipped don't count (the gate has its own rules for those, and a cancelled job is usually a
// newer push). Only jobs of the run's current attempt count: right after a
// `gh run rerun --failed` the list can still hold the red jobs that re-run replaces. A reusable
// workflow's jobs show as "<caller name> / <job name>" (verified on run 39684718106,
// `gh api .../jobs`, 2026-10-28), so they match by that prefix; matrix legs don't, since a job
// outside the gate could share the shape.
func failedRequiredJobs(jobs []runJob, required []string, attempt int) []runJob {
	var out []runJob
	for _, job := range jobs {
		if job.RunAttempt != attempt || !job.failed() {
			continue
		}
		if slices.ContainsFunc(required, func(name string) bool {
			return job.Name == name || strings.HasPrefix(job.Name, name+" / ")
		}) {
			out = append(out, job)
		}
	}
	return out
}

// pullRequest is one row of `gh pr list --json number,state,url,labels`.
type pullRequest struct {
	Number int       `json:"number"`
	State  string    `json:"state"`
	URL    string    `json:"url"`
	Labels []prLabel `json:"labels"`
}

type prLabel struct {
	Name string `json:"name"`
}

// hasLabel says whether the PR carries the label.
func (p pullRequest) hasLabel(name string) bool {
	return slices.ContainsFunc(p.Labels, func(l prLabel) bool { return l.Name == name })
}

// pullDetails is the part of `repos/{repo}/pulls/{n}` a GitHub merge decides on. Mergeable is
// null while GitHub is still working it out.
type pullDetails struct {
	Merged    bool  `json:"merged"`
	Mergeable *bool `json:"mergeable"`
}

// openPR finds the branch's open PR; a closed one is never revived.
func openPR(prs []pullRequest) (pullRequest, bool) {
	for _, pr := range prs {
		if pr.State == "OPEN" {
			return pr, true
		}
	}
	return pullRequest{}, false
}

// prNumber reads the number off a PR URL, the one thing `gh pr create` prints.
func prNumber(url string) (int, bool) {
	n, err := strconv.Atoi(url[strings.LastIndex(url, "/")+1:])
	return n, err == nil && n > 0
}

// worktree is one entry of `git worktree list --porcelain`.
type worktree struct {
	path   string
	branch string
}

func parseWorktrees(porcelain string) []worktree {
	var out []worktree
	for block := range strings.SplitSeq(porcelain, "\n\n") {
		var wt worktree
		for line := range strings.SplitSeq(block, "\n") {
			if p, ok := strings.CutPrefix(line, "worktree "); ok {
				wt.path = p
			} else if b, ok := strings.CutPrefix(line, "branch refs/heads/"); ok {
				wt.branch = b
			}
		}
		if wt.path != "" {
			out = append(out, wt)
		}
	}
	return out
}

// worktreeOf finds the worktree that has a branch checked out.
func worktreeOf(wts []worktree, branch string) (worktree, bool) {
	for _, wt := range wts {
		if wt.branch == branch {
			return wt, true
		}
	}
	return worktree{}, false
}
