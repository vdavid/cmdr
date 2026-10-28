// Land: takes a worktree branch onto `main` through CI, the only way commits reach `main` once
// its rulesets are on (`.github/rulesets/`).
//
// In order, it:
//
//  1. refuses a dirty tree, fetches, and rebases onto `origin/main` when it moved (a branch
//     carrying a merge commit stops instead: a plain rebase would flatten it);
//  2. with `-squash`, squashes the branch into one commit (`-m` for the message, else every
//     commit message, oldest first);
//  3. pushes the branch and opens its PR, or reuses the open one (the PR is plumbing: CI runs on
//     `pull_request`, and nobody reviews it);
//  4. waits for `CI OK` on the head SHA (or the first failure among the jobs it needs), and on
//     red names the failed jobs and the re-run command;
//  5. lands the green head: with `git push origin <sha>:main` while it still descends from
//     `origin/main` (the SHAs stay the same, and GitHub marks the PR merged), else with
//     `gh pr merge --admin --rebase`, which rebases it onto the `main` that moved under it and
//     leaves the clash check to CI on `main`. Either way the CI ruleset demands `CI OK` green on
//     the head; the admin only bypasses the approval ruleset;
//  6. fast-forwards local `main` (wherever it's checked out), deletes the remote branch, and
//     prints the command that removes the worktree.
//
// When GitHub can't rebase the green PR (a conflict, or a merge commit a rebase would flatten),
// it catches up here and waits for CI again, up to `maxCIRuns` runs in all.
//
// With `-merge` (a release commit, whose tag must stay on `main`'s history), it never rebases: the
// green head lands by fast-forward, or by a merge commit GitHub makes onto the `main` that moved
// (`gh pr merge --merge`). A plain land refuses to rebase a tagged commit.
//
// With `-record N`, a second land follows once the branch is on `main`: a record branch, the
// current `main` plus an `ours` merge of contributor PR N's head, through CI and fast-forwarded
// onto `main`, so GitHub marks PR N merged (`record.go`).
//
// Every run reads where the branch stands and picks up from there, so it's safe to re-run after a
// flake re-run, a rebase, or a closed laptop.
//
// # Usage
//
//	pnpm land                 push, wait for CI, land
//	pnpm land -no-wait        push and open the PR, then return (offload CI between milestones)
//	pnpm land -squash [-m M]  squash first
//	pnpm land -record N       land, then record contributor PR N as merged
//	pnpm land -merge          land without rebasing: fast-forward, or a GitHub merge commit
//	pnpm land rulesets [-apply]
//
// Docs: `scripts/land/CLAUDE.md`.
package main

import (
	"errors"
	"flag"
	"fmt"
	"net/url"
	"os"
	"time"
)

const (
	ciWorkflow = "ci.yml"
	ciCheck    = "CI OK"
	// Every PR this script opens is an agent's, and agent-made issues and PRs carry this label.
	agentLabel = "claude-created"
	// How long a CI run gets to show up after the push that starts it.
	appearWithin = 5 * time.Minute
	// How many CI runs one land waits through when each green head needs a rebase here before it
	// can land. A `main` that clashes with it this often needs a person, not a ninth run.
	maxCIRuns = 8
	// How long GitHub gets to work out whether it can rebase a PR after `main` moved.
	rebaseableWithin = 15 * time.Second
)

// errMainMoved: `origin/main` moved while CI ran, and GitHub can't land the green head on it, so
// the branch catches up here and CI runs again.
var errMainMoved = errors.New("origin/main moved while CI ran")

// errRebaseConflict: GitHub can't rebase the green PR onto the `main` that moved under it.
var errRebaseConflict = fmt.Errorf("GitHub can't rebase the PR onto main cleanly: %w", errMainMoved)

// errOriginAhead: origin's feature branch has commits this branch never had, so pushing would
// overwrite them.
var errOriginAhead = errors.New("origin's branch has commits this one doesn't")

type config struct {
	root    string
	squash  bool
	message string
	noWait  bool
	timeout time.Duration
	// record is the contributor PR to record as merged once the branch landed, or 0.
	record int
	// merge: never rebase; land by fast-forward or a merge commit GitHub makes.
	merge bool
}

// validate refuses flag combinations that don't go together.
func (c config) validate() error {
	switch {
	case c.message != "" && !c.squash:
		return errors.New("-m only works with -squash")
	case c.record < 0:
		return errors.New("-record takes the contributor's PR number")
	case c.merge && (c.squash || c.record > 0):
		return errors.New("-merge lands the branch's commits as they are, so it takes neither -squash nor -record")
	}
	return nil
}

// command is how to run this land again.
func (c config) command() string {
	if c.record > 0 {
		return fmt.Sprintf("pnpm land -record %d", c.record)
	}
	if c.merge {
		return "pnpm land -merge"
	}
	return "pnpm land"
}

func main() {
	root := flag.String("root", "", "the worktree to land (scripts/land.sh sets it)")
	squash := flag.Bool("squash", false, "squash the branch into one commit before pushing")
	message := flag.String("m", "", "with -squash: the commit message (default: every commit message, oldest first)")
	noWait := flag.Bool("no-wait", false, "push and open the PR, then return without waiting for CI or landing")
	timeout := flag.Duration("timeout", 2*time.Hour, "how long to wait for CI")
	recordPR := flag.Int("record", 0, "once the branch landed, record contributor PR `N` as merged (docs/guides/handling-prs.md)")
	merge := flag.Bool("merge", false, "never rebase: land by fast-forward or a GitHub merge commit (a release commit and its tag, docs/guides/releasing.md)")
	flag.Usage = func() {
		fmt.Fprintln(os.Stderr, "Usage: pnpm land [-no-wait] [-squash [-m MESSAGE]] [-record N] [-merge] [-timeout 2h]")
		fmt.Fprintln(os.Stderr, "       pnpm land rulesets [-apply]")
		flag.PrintDefaults()
	}
	flag.Parse()
	if *root == "" {
		flag.Usage()
		os.Exit(2)
	}
	r := runner{dir: *root, cache: responseCache{}}
	resolved, err := resolveRepo(r)
	if err != nil {
		fail(err)
	}
	repo = resolved
	if flag.NArg() > 0 {
		if flag.Arg(0) != "rulesets" {
			flag.Usage()
			os.Exit(2)
		}
		if err := rulesets(r, flag.Args()[1:]); err != nil {
			fail(err)
		}
		return
	}
	cfg := config{root: *root, squash: *squash, message: *message, noWait: *noWait, timeout: *timeout, record: *recordPR, merge: *merge}
	if err := cfg.validate(); err != nil {
		fail(err)
	}
	if err := land(r, cfg); err != nil {
		fail(err)
	}
	if cfg.noWait {
		return
	}
	if cfg.record > 0 {
		if err := record(r, cfg); err != nil {
			fail(err)
		}
	}
	if teardown := teardownCommand(r.dir); teardown != "" {
		say("Done. This worktree's branch is on main now; remove the worktree when you're finished with it:\n  %s", teardown)
	} else {
		say("Done.")
	}
}

// land runs one land, and another on top of a rebase each time GitHub can't land the green head
// on the `main` that moved under it, up to `maxCIRuns` CI runs in all.
func land(r runner, cfg config) error {
	for run := 1; ; run++ {
		err := landOnce(r, cfg)
		if !errors.Is(err, errMainMoved) {
			return err
		}
		if run >= maxCIRuns {
			return fmt.Errorf("origin/main moved during each of %d CI runs in a way GitHub couldn't rebase onto, so I stopped. Land again when it's quieter: pnpm land", maxCIRuns)
		}
		if errors.Is(err, errRebaseConflict) {
			say("GitHub can't rebase the green PR onto the new main cleanly, so rebasing here and running CI again (run %d of %d).", run+1, maxCIRuns)
		} else {
			say("origin/main moved while CI ran, so catching up here (run %d of %d).", run+1, maxCIRuns)
		}
	}
}

func landOnce(r runner, cfg config) error {
	state, err := readState(r)
	if err != nil {
		return err
	}
	if proceed, err := start(r, state, cfg.merge); !proceed {
		return err
	}
	commits, err := r.commits("origin/main..HEAD")
	if err != nil {
		return err
	}
	if cfg.squash && needsSquash(commits, cfg.message) {
		if err := squash(r, commits, cfg.message); err != nil {
			return err
		}
	}
	head, err := r.git("rev-parse", "HEAD")
	if err != nil {
		return err
	}
	if head, err = pushBranch(r, state.branch, head); err != nil {
		return err
	}
	if commits, err = r.commits("origin/main..HEAD"); err != nil {
		return err
	}
	pr, err := ensurePR(r, state.branch, commits)
	if err != nil {
		return err
	}
	if cfg.noWait {
		say("CI is on it: %s\nLand it when you're ready: %s", pr.URL, cfg.command())
		return nil
	}
	if err := waitForCI(r, head, cfg); err != nil {
		return err
	}
	if err := landGreen(r, pr, head, state.hasMerges, cfg.merge); err != nil {
		return err
	}
	return finish(r, state.branch)
}

// start acts on the start step: it refuses, finishes an earlier land, or rebases. It reports
// whether the land goes on.
func start(r runner, state branchState, merge bool) (bool, error) {
	switch nextStartStep(state, merge) {
	case stepRefuseBranch:
		return false, errors.New("check out a feature branch first: land takes a branch onto main, never main itself")
	case stepRefuseDirty:
		return false, errors.New("commit your changes first: land pushes what's committed, and the tree has uncommitted changes")
	case stepFinish:
		say("%s is already on origin/main.", state.branch)
		return false, finish(r, state.branch)
	case stepRefuseMergeRebase:
		return false, errors.New("origin/main moved, and this branch carries a merge commit that a plain rebase would flatten. Catch up by hand, then land again:\n  a release merge-back: redo the merge on top of main   (docs/guides/releasing.md)\n  a contributor's PR record: drop the merge from this branch, then land with `pnpm land -record <PR number>`,\n    which records the PR in a land of its own          (docs/guides/handling-prs.md)")
	case stepRefuseTaggedRebase:
		return false, errors.New("origin/main moved, and this branch carries a tagged commit (a release commit?) that a rebase would leave off main's history. Land it as it is: pnpm land -merge")
	case stepRebase:
		say("Rebasing onto origin/main...")
		if _, err := r.git("rebase", "origin/main"); err != nil {
			_, _ = r.git("rebase", "--abort")
			return false, fmt.Errorf("the rebase hit conflicts, so I put the branch back. Rebase by hand, then land again:\n  git rebase origin/main\n\n%w", err)
		}
	case stepProceed:
	}
	return true, nil
}

func readState(r runner) (branchState, error) {
	branch, err := r.git("branch", "--show-current")
	if err != nil {
		return branchState{}, err
	}
	if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
		return branchState{}, err
	}
	// Untracked files can't change what lands, and the main clone (where `release.sh` lands from)
	// often has some.
	status, err := r.git("status", "--porcelain", "--untracked-files=no")
	if err != nil {
		return branchState{}, err
	}
	merges, err := r.git("rev-list", "--merges", "origin/main..HEAD")
	if err != nil {
		return branchState{}, err
	}
	return branchState{
		branch:    branch,
		dirty:     status != "",
		landed:    r.isAncestor("HEAD", "origin/main") || r.allUpstream(),
		based:     r.isAncestor("origin/main", "HEAD"),
		hasMerges: merges != "",
		tagged:    r.tagged("origin/main..HEAD"),
	}, nil
}

func squash(r runner, commits []commit, message string) error {
	if message == "" {
		message = squashMessage(commits)
	}
	say("Squashing %d %s into one...", len(commits), plural(len(commits), "commit", "commits"))
	if _, err := r.git("reset", "--soft", "origin/main"); err != nil {
		return err
	}
	if err := r.gitStdin(message, "commit", "--quiet", "-F", "-"); err != nil {
		return fmt.Errorf("the squash commit didn't go through; `git reflog` has the branch as it was\n%w", err)
	}
	return nil
}

// pushBranch pushes HEAD to its feature branch unless origin already has it. It overwrites
// origin's branch only when this branch already integrated it (a rebase or squash of commits it
// had); origin commits it never had (another machine, GitHub's "Update branch") stop the land.
//
// The pre-push hook may commit a formatting fix and stop the push; then this pushes once more
// with that commit on top. It returns the SHA that's on origin.
func pushBranch(r runner, branch, head string) (string, error) {
	remote, err := r.fetchBranch(branch)
	if err != nil {
		return "", err
	}
	if remote != "" && remote != head && !r.isAncestor(remote, head) && !r.inReflog(branch, remote) {
		return "", fmt.Errorf("%w (pushed from another machine, or by GitHub's \"Update branch\"), so I didn't overwrite them. Bring them in, then land again:\n  git pull --rebase origin %s\n  or take origin's version: git reset --hard origin/%s", errOriginAhead, branch, branch)
	}
	for attempt := 0; remote != head; attempt++ {
		say("Pushing %s...", branch)
		err = pushToOrigin(r, branch)
		if err == nil {
			return head, nil
		}
		newHead, headErr := r.git("rev-parse", "HEAD")
		if headErr != nil || newHead == head || attempt > 0 {
			return "", fmt.Errorf("the branch push didn't go through; fix what it says, then land again: pnpm land\n%w", err)
		}
		say("The pre-push hook added a commit, so pushing again with it.")
		head = newHead
	}
	return head, nil
}

// pushToOrigin pushes HEAD to origin's branch. The bare lease checks origin against the
// remote-tracking ref `fetchBranch` just updated, and `--force-if-includes` refuses when that ref's
// tip isn't in the local branch's history: the backstop for a push that lands between
// `fetchBranch` and here. (An explicit lease value would make git ignore `--force-if-includes`.)
func pushToOrigin(r runner, branch string) error {
	_, err := r.git("push", "--quiet", "--force-with-lease=refs/heads/"+branch, "--force-if-includes", "origin", "HEAD:refs/heads/"+branch)
	return err
}

func ensurePR(r runner, branch string, commits []commit) (pullRequest, error) {
	var prs []pullRequest
	if err := r.ghJSON(&prs, "pr", "list", "-R", repo, "--head", branch, "--state", "all", "--json", "number,state,url"); err != nil {
		return pullRequest{}, err
	}
	if pr, ok := openPR(prs); ok {
		return pr, nil
	}
	url, err := r.gh("pr", "create", "-R", repo, "--base", "main", "--head", branch, "--title", prTitle(commits), "--body", prBody(commits), "--label", agentLabel)
	if err != nil {
		return pullRequest{}, err
	}
	say("Opened %s", url)
	number, ok := prNumber(url)
	if !ok {
		return pullRequest{}, fmt.Errorf("gh pr create printed %q, which has no PR number", url)
	}
	return pullRequest{Number: number, State: "OPEN", URL: url}, nil
}

func waitForCI(r runner, head string, cfg config) error {
	start := time.Now()
	announced := false
	// The jobs `CI OK` needs: once one of them fails, `CI OK` will too, so the wait stops there.
	required := gateJobs(r, head)
	// Whether a run on head showed up yet. Once one has, a poll that lists none is GitHub's
	// list lagging, never a push that started nothing.
	seen := false
	for {
		// One REST call per poll; the `CI OK` check runs only matter once the run finished.
		var runs struct {
			WorkflowRuns []workflowRun `json:"workflow_runs"`
		}
		if err := r.ghAPI(&runs, ciRunsPath(head, cfg.merge)); err != nil {
			return err
		}
		var checks struct {
			CheckRuns []checkRun `json:"check_runs"`
		}
		if v, _ := ciVerdict(runs.WorkflowRuns, nil); v != verdictMissing && v != verdictPending {
			if err := r.ghAPI(&checks, fmt.Sprintf("repos/%s/commits/%s/check-runs?check_name=%s&filter=all", repo, head, url.PathEscape(ciCheck))); err != nil {
				return err
			}
		}
		v, run := ciVerdict(runs.WorkflowRuns, checks.CheckRuns)
		switch v {
		case verdictGreen:
			say("%s is green.", ciCheck)
			return nil
		case verdictRed:
			jobs, _ := runJobs(r, run.ID)
			return ciFailure(run, jobs, false, cfg.command())
		case verdictMissing:
			if missingRunGivesUp(seen, time.Since(start)) {
				return fmt.Errorf("no CI run on %s after %s. Check the PR's Checks tab, then land again: %s", head[:12], appearWithin, cfg.command())
			}
		case verdictPending:
			seen = true
			if !announced {
				say("Waiting for %s: https://github.com/%s/actions/runs/%d", ciCheck, repo, run.ID)
				announced = true
			}
			if jobs, red := earlyRed(r, run, required); red {
				return ciFailure(run, jobs, true, cfg.command())
			}
		}
		if time.Since(start) > cfg.timeout {
			return fmt.Errorf("CI is still running after %s; land again to keep waiting: %s", cfg.timeout, cfg.command())
		}
		delay := pollDelay(v, time.Since(run.CreatedAt))
		time.Sleep(delay + jitter(delay/4))
	}
}

// ciRunsPath lists the CI runs on head. With `-merge` only the PR's: a release commit also has its
// release's full dispatch run, and the CI ruleset reads only the newest run on a commit, which is
// the PR's once it starts. Landing on the older green run would be refused while the PR run goes.
func ciRunsPath(head string, merge bool) string {
	path := fmt.Sprintf("repos/%s/actions/workflows/%s/runs?head_sha=%s&per_page=20", repo, ciWorkflow, head)
	if merge {
		path += "&event=pull_request"
	}
	return path
}

// gateJobs names the jobs `CI OK` needs, read from head's own `ci.yml` (its `ci-ok` job's
// `needs`, the list the gate itself checks). nil when it can't, and the wait then stops on
// `CI OK` alone.
func gateJobs(r runner, head string) []string {
	ciYml, err := r.git("show", head+":.github/workflows/"+ciWorkflow)
	if err != nil {
		say("Couldn't read %s's required jobs from %s, so waiting for %s itself: %v", ciCheck, ciWorkflow, ciCheck, err)
		return nil
	}
	required := requiredJobNames(ciYml)
	if len(required) == 0 {
		say("%s has no %s job to read required jobs from, so waiting for %s itself.", ciWorkflow, ciGateJob, ciCheck)
	}
	return required
}

// earlyRed reads a running run's jobs and reports whether one `CI OK` needs already failed, with
// the jobs for the report. Same cadence, one more call, and most polls of it are a free 304 too.
// A failed read only skips this poll's look, since `CI OK` still decides in the end.
func earlyRed(r runner, run workflowRun, required []string) ([]runJob, bool) {
	if run.Status == "completed" || len(required) == 0 {
		return nil, false
	}
	jobs, err := runJobs(r, run.ID)
	return jobs, err == nil && len(failedRequiredJobs(jobs, required, run.RunAttempt)) > 0
}

// runJobs lists the run's jobs in its latest attempt, so a `gh run rerun --failed` reads its
// re-run jobs, not the red ones they replace.
func runJobs(r runner, runID int64) ([]runJob, error) {
	var page struct {
		Jobs []runJob `json:"jobs"`
	}
	err := r.ghAPI(&page, fmt.Sprintf("repos/%s/actions/runs/%d/jobs?filter=latest&per_page=100", repo, runID))
	return page.Jobs, err
}

// ciFailure names the failed jobs and how to re-run them; flakes are expected, so that's the
// first thing to try. early: a job `CI OK` needs failed while the rest still run. The run goes on,
// so every failure still shows on the PR, and GitHub re-runs only a finished run.
func ciFailure(run workflowRun, jobs []runJob, early bool, again string) error {
	msg := fmt.Sprintf("CI is red: https://github.com/%s/actions/runs/%d\n", repo, run.ID)
	for _, job := range jobs {
		if job.Status == "completed" && job.Conclusion != "success" && job.Conclusion != "skipped" {
			msg += fmt.Sprintf("  %s: %s %s\n", job.Name, job.Conclusion, job.URL)
		}
	}
	if early {
		msg += fmt.Sprintf("%s will fail, so I stopped waiting. The other jobs keep running, so their failures show on the PR too.\nA flake? Once the run finishes, re-run the failed jobs, then land again:\n", ciCheck)
	} else {
		msg += "A flake? Re-run the failed jobs, then land again:\n"
	}
	msg += fmt.Sprintf("  gh run rerun %d --failed -R %s && %s\nA real failure? Fix it, commit, and land again.", run.ID, repo, again)
	return errors.New(msg)
}

// landGreen takes the green head onto `main` (`nextLandPath`): a fast-forward push while it still
// descends from `origin/main`, else a GitHub rebase merge (with `-merge`, a GitHub merge commit).
// It returns errMainMoved when only a rebase here and a fresh CI run can land it.
func landGreen(r runner, pr pullRequest, head string, hasMerges, merge bool) error {
	for {
		if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
			return err
		}
		path, err := choosePath(r, pr, head, hasMerges, merge)
		if err != nil {
			return err
		}
		switch path {
		case landFastForward:
			err := pushToMain(r, head)
			if err == nil {
				say("Landed %s on main; GitHub marks %s merged.", head[:12], pr.URL)
				return nil
			}
			// `main` moved between the fetch and the push: decide again.
			if _, fetchErr := r.git("fetch", "--quiet", "origin", "main"); fetchErr == nil && !r.isAncestor("origin/main", head) {
				continue
			}
			return fmt.Errorf("origin refused the land:\n%w", err)
		case landGitHubRebase:
			return mergeOnGitHub(r, pr, head)
		case landRebaseAndRerun:
			if hasMerges {
				return errMainMoved
			}
			return errRebaseConflict
		case landGitHubMerge:
			return mergeCommitOnGitHub(r, pr, head)
		case landMergeConflict:
			return errMergeConflict(head)
		}
	}
}

// choosePath reads where the head stands against `origin/main` (and, when `main` moved, what GitHub
// can do with the PR) and picks the land path.
func choosePath(r runner, pr pullRequest, head string, hasMerges, merge bool) (landPath, error) {
	based := r.isAncestor("origin/main", head)
	var rebaseable, mergeable *bool
	// A merge commit on a plain land can't go through GitHub at all, so there's nothing to ask.
	if !based && (merge || !hasMerges) {
		var err error
		if rebaseable, mergeable, err = prMergeability(r, pr.Number); err != nil {
			return 0, err
		}
	}
	if merge {
		return nextMergePath(based, mergeable), nil
	}
	return nextLandPath(based, hasMerges, rebaseable), nil
}

// pushToMain fast-forwards origin's `main` to the green SHA. `--no-verify`: CI already passed on
// this exact SHA, and a pre-push hook that adds a commit would land something CI never saw.
func pushToMain(r runner, head string) error {
	_, err := r.git("push", "--quiet", "--no-verify", "origin", head+":refs/heads/main")
	return err
}

// prMergeability asks GitHub whether it can rebase the PR onto `main`, and merge it into `main`.
// GitHub works that out in the background after `main` moves, so this gives it
// `rebaseableWithin`, then reports nil for what it still doesn't know.
func prMergeability(r runner, number int) (rebaseable, mergeable *bool, err error) {
	deadline := time.Now().Add(rebaseableWithin)
	for {
		var pull pullDetails
		if err := r.ghAPI(&pull, fmt.Sprintf("repos/%s/pulls/%d", repo, number)); err != nil {
			return nil, nil, err
		}
		if (pull.Rebaseable != nil && pull.Mergeable != nil) || time.Now().After(deadline) {
			return pull.Rebaseable, pull.Mergeable, nil
		}
		time.Sleep(3 * time.Second)
	}
}

// mergeOnGitHub has GitHub rebase the green PR onto the `main` that moved under it, and CI on
// `main` checks the result. `--admin` passes the approval ruleset; the CI one has no bypass, so
// GitHub still demands `CI OK` green on the head, and `--match-head-commit` pins that head.
func mergeOnGitHub(r runner, pr pullRequest, head string) error {
	_, err := r.gh("pr", "merge", fmt.Sprint(pr.Number), "-R", repo, "--admin", "--rebase", "--match-head-commit", head)
	if err != nil {
		// The call may have gone through before it failed, or GitHub found the rebase conflicts.
		var pull pullDetails
		if readErr := r.ghAPI(&pull, fmt.Sprintf("repos/%s/pulls/%d", repo, pr.Number)); readErr != nil {
			return fmt.Errorf("GitHub refused the merge, so nothing landed. Fix what it says, then land again: pnpm land\n%w", err)
		}
		switch {
		case pull.Merged:
		case pull.Rebaseable != nil && !*pull.Rebaseable:
			return errRebaseConflict
		default:
			return fmt.Errorf("GitHub refused the merge, so nothing landed. Fix what it says, then land again: pnpm land\n%w", err)
		}
	}
	say("main moved while CI ran, so GitHub rebased the green PR onto it: %s\nCI on main checks the result: https://github.com/%s/actions/workflows/%s?query=branch%%3Amain", pr.URL, repo, ciWorkflow)
	return nil
}

// mergeCommitOnGitHub (`-merge`) has GitHub merge the green PR into the `main` that moved under it
// with a merge commit, so the head keeps its SHA (and a release its tag) on `main`'s history.
// `--admin` passes the approval ruleset; the CI one still demands `CI OK` green on the head.
func mergeCommitOnGitHub(r runner, pr pullRequest, head string) error {
	_, err := r.gh("pr", "merge", fmt.Sprint(pr.Number), "-R", repo, "--admin", "--merge", "--match-head-commit", head)
	if err != nil {
		var pull pullDetails
		if readErr := r.ghAPI(&pull, fmt.Sprintf("repos/%s/pulls/%d", repo, pr.Number)); readErr != nil {
			return fmt.Errorf("GitHub refused the merge, so nothing landed. Fix what it says, then land again: pnpm land -merge\n%w", err)
		}
		switch {
		case pull.Merged:
		case pull.Mergeable != nil && !*pull.Mergeable:
			return errMergeConflict(head)
		default:
			return fmt.Errorf("GitHub refused the merge, so nothing landed. It takes merge commits on main (the repo setting and the ruleset's allowed merge methods, docs/guides/releasing.md). Fix what it says, then land again: pnpm land -merge\n%w", err)
		}
	}
	say("main moved while CI ran, so GitHub merged the green PR into it with a merge commit: %s\nCI on main checks the result: https://github.com/%s/actions/workflows/%s?query=branch%%3Amain", pr.URL, repo, ciWorkflow)
	return nil
}

// errMergeConflict says how to merge by hand what GitHub can't.
func errMergeConflict(head string) error {
	return fmt.Errorf("GitHub can't merge %s into main cleanly. Merge it by hand in a worktree off main (`git merge --no-ff %s`, resolving per docs/guides/releasing.md), then land that branch with `pnpm land`", head[:12], head[:12])
}

// finish syncs local `main` with origin's and deletes the landed remote branch. Both are
// best-effort: the land already happened, so trouble here is a message, not a failure.
func finish(r runner, branch string) error {
	syncMain(r)
	head, err := r.git("rev-parse", "HEAD")
	if err != nil {
		return err
	}
	remote, err := r.remoteSHA(branch)
	if err != nil {
		return err
	}
	switch {
	case remote == "":
	case remote == head:
		if _, err := r.git("push", "--quiet", "--no-verify", "origin", "--delete", branch); err != nil {
			say("Couldn't delete origin/%s: %v", branch, err)
		}
	default:
		say("Left origin/%s alone: it points at %s, which isn't what landed.", branch, remote[:12])
	}
	stopLeftoverRuns(r, branch)
	return nil
}

// stopLeftoverRuns cancels the landed branch's unfinished runs (`leftoverRuns`): an older push's
// CI, CodeQL, a land run. Best-effort, like the rest of `finish`.
func stopLeftoverRuns(r runner, branch string) {
	var runs struct {
		WorkflowRuns []workflowRun `json:"workflow_runs"`
	}
	if err := r.ghAPI(&runs, fmt.Sprintf("repos/%s/actions/runs?branch=%s&per_page=100", repo, url.QueryEscape(branch))); err != nil {
		say("Couldn't list %s's runs to stop the leftovers: %v", branch, err)
		return
	}
	for _, run := range leftoverRuns(runs.WorkflowRuns, repo) {
		// A run can finish between the list and the cancel, so a refusal is only a note.
		if _, err := r.gh("api", "-X", "POST", fmt.Sprintf("repos/%s/actions/runs/%d/cancel", repo, run.ID), "--silent"); err != nil {
			say("Couldn't stop leftover run %d (it may have just finished): %v", run.ID, err)
			continue
		}
		say("Stopped leftover run https://github.com/%s/actions/runs/%d, which nothing waits on now.", repo, run.ID)
	}
}

// syncMain fast-forwards local `main` to `origin/main`, never checking it out here: wherever it's
// checked out, that worktree moves only when it's clean.
func syncMain(r runner) {
	if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
		say("Couldn't fetch origin/main: %v", err)
		return
	}
	list, err := r.git("worktree", "list", "--porcelain")
	if err != nil {
		say("Couldn't list worktrees: %v", err)
		return
	}
	wt, checkedOut := worktreeOf(parseWorktrees(list), "main")
	if !checkedOut {
		if _, err := r.git("fetch", "--quiet", "origin", "main:main"); err != nil {
			say("Local main didn't fast-forward; see what it has that origin lacks: git log origin/main..main\n%v", err)
			return
		}
		say("Local main is at origin/main.")
		return
	}
	at := runner{dir: wt.path}
	if status, err := at.git("status", "--porcelain"); err != nil || status != "" {
		say("main is checked out at %s with uncommitted changes, so I left it. Fast-forward it when it's clean:\n  git -C %s merge --ff-only origin/main", wt.path, wt.path)
		return
	}
	if _, err := at.git("merge", "--quiet", "--ff-only", "origin/main"); err != nil {
		say("main at %s didn't fast-forward; see what it has that origin lacks: git -C %s log origin/main..main\n%v", wt.path, wt.path, err)
		return
	}
	say("Local main (%s) is at origin/main.", wt.path)
}

func say(format string, args ...any) {
	fmt.Printf(format+"\n", args...)
}

func plural(n int, singular, pluralForm string) string {
	if n == 1 {
		return singular
	}
	return pluralForm
}

func fail(err error) {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(1)
}
