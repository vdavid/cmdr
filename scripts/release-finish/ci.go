package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"time"
)

// The release's CI gate: nothing ships unless a full (`run_all`) `ci.yml` run on the tagged
// commit went green, and CodeQL with it. `release.sh` dispatches that run on the release branch
// right after it pushes the tag, so it runs beside the build; this waits for it (failing fast on
// the first red job), and the `ci-gate` job in `release-pipeline.yml` checks it again before
// `publish`. `docs/guides/releasing.md` § The CI gate.

const (
	ciWorkflow = "ci.yml"
	// The `ci.yml` job that runs only when `run_all` is on: the runs API carries no inputs, so this
	// job's success is how a gate tells a full run from a dispatch with `run_all` off. Matched by
	// name, which `ci.yml` owns.
	fullRunMarker = "Full run (run_all)"
	// The emergency bypass, for a hotfix while CI is red for a reason outside the repo. The
	// pipeline sets it from the repository variable `RELEASE_SKIP_CI_GATE_TAG` naming the tag.
	skipCIGateEnv = "RELEASE_SKIP_CI_GATE"
	// How often a wait looks at CI and the build. Both take most of an hour, so a minute keeps the
	// fail-fast prompt without spending the API budget other sessions share.
	ciPollEvery = time.Minute
)

type ciJob struct {
	ID         int64  `json:"id"`
	Name       string `json:"name"`
	Status     string `json:"status"`
	Conclusion string `json:"conclusion"`
	URL        string `json:"html_url"`
}

// state is the job's conclusion once it has one, else its status.
func (j ciJob) state() string {
	if j.Conclusion != "" {
		return j.Conclusion
	}
	return j.Status
}

// red: a job that finished any way but green or skipped.
func (j ciJob) red() bool {
	return j.Status == "completed" && j.Conclusion != "success" && j.Conclusion != "skipped" && j.Conclusion != "neutral"
}

// ciRun is one `ci.yml` run with the jobs of its latest attempt.
type ciRun struct {
	ID         int64     `json:"id"`
	Status     string    `json:"status"`
	Conclusion string    `json:"conclusion"`
	CreatedAt  time.Time `json:"created_at"`
	Jobs       []ciJob   `json:"-"`
}

type ciState int

const (
	ciPending ciState = iota
	ciGreen
	ciRed
)

// judgeCI reads a run: red as soon as any job is (the gate fails fast instead of waiting out a
// run that can't go green), green once the run completed green, pending otherwise. A re-run of
// the failed jobs queues them again, so it reads pending, never the old attempt's red.
func judgeCI(run ciRun) (ciState, []ciJob) {
	var failed []ciJob
	for _, j := range run.Jobs {
		if j.red() {
			failed = append(failed, j)
		}
	}
	switch {
	case len(failed) > 0:
		return ciRed, failed
	case run.Status != "completed":
		return ciPending, nil
	case run.Conclusion == "success":
		return ciGreen, nil
	default:
		return ciRed, nil
	}
}

// isFullOrUnknown: the marker job succeeded or hasn't finished (it needs no other job, so it
// settles within seconds of the run starting). A run whose marker skipped had `run_all` off, and a
// finished run without one predates the marker; neither proves anything.
func isFullOrUnknown(run ciRun) bool {
	i := slices.IndexFunc(run.Jobs, func(j ciJob) bool { return j.Name == fullRunMarker })
	if i < 0 {
		return run.Status != "completed"
	}
	m := run.Jobs[i]
	return m.Status != "completed" || m.Conclusion == "success"
}

// newestFullRun picks the newest dispatch run that is (or may still turn out to be) a full one.
func newestFullRun(runs []ciRun) (ciRun, bool) {
	var full []ciRun
	for _, r := range runs {
		if isFullOrUnknown(r) {
			full = append(full, r)
		}
	}
	if len(full) == 0 {
		return ciRun{}, false
	}
	return slices.MaxFunc(full, func(a, b ciRun) int { return a.CreatedAt.Compare(b.CreatedAt) }), true
}

type gateTarget int

const (
	gateOwn gateTarget = iota
	gateParent
)

// gateCommit decides whose full run counts. The tagged commit's own run, whenever it has one: a
// red or running run on the release commit holds the release however green its parent is. The
// parent's only when the tagged commit is the `chore(release)` commit and has no full run at all:
// the security-only patch release gates the fix commit, then bumps the version on top of it by
// hand (`docs/guides/releasing.md` § Security-only patch release).
func gateCommit(ownHasFullRun, isReleaseCommit bool) gateTarget {
	if !ownHasFullRun && isReleaseCommit {
		return gateParent
	}
	return gateOwn
}

// jobChanges answers one line per job whose state moved since the last look, and records it.
func jobChanges(seen map[string]string, jobs []ciJob) []string {
	var lines []string
	for _, j := range jobs {
		if seen[j.Name] != j.state() {
			seen[j.Name] = j.state()
			lines = append(lines, j.Name+": "+j.state())
		}
	}
	return lines
}

// ciRedMessage names the red jobs, how to read why, and the two ways on: re-run a flake, or abort
// the release (nothing was public yet) and start over once the fix landed.
func ciRedMessage(repo string, run ciRun, failed []ciJob, version string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "the release's full CI run is red: https://github.com/%s/actions/runs/%d\n", repo, run.ID)
	for _, j := range failed {
		fmt.Fprintf(&b, "  %s: %s %s\n", j.Name, j.Conclusion, j.URL)
	}
	if len(failed) > 0 {
		fmt.Fprintf(&b, "See why: gh run view %d -R %s --job %d --log-failed\n", run.ID, repo, failed[0].ID)
	} else {
		fmt.Fprintf(&b, "The run ended %s. See why: gh run view %d -R %s\n", run.Conclusion, run.ID, repo)
	}
	fmt.Fprintf(&b, "A flake? Re-run the failed jobs, then run this again (the wait picks up the new attempt):\n  gh run rerun %d --failed -R %s\n", run.ID, repo)
	fmt.Fprintf(&b, "A real failure? Abort the release (it deletes the draft, the tag, and the release branch; nothing was public), land the fix on main, and release again:\n  ./scripts/release.sh -abort %s", version)
	return b.String()
}

// ciGate is where the release's CI stands: which commit's run counts, and that run.
type ciGate struct {
	commit string
	run    ciRun
	found  bool
}

// findCIGate looks up the full runs on the tagged commit, and on its parent when `gateCommit`
// says so.
func findCIGate(cfg config, commit string) (ciGate, error) {
	own, err := fullRunsOn(commit)
	if err != nil {
		return ciGate{}, err
	}
	run, ok := newestFullRun(own)
	subject, err := output(cfg.root, nil, "git", "log", "-1", "--format=%s", commit)
	if err != nil {
		return ciGate{}, err
	}
	if gateCommit(ok, subject == "chore(release): "+cfg.tag) == gateOwn {
		return ciGate{commit: commit, run: run, found: ok}, nil
	}
	parent, err := output(cfg.root, nil, "git", "rev-parse", commit+"^")
	if err != nil {
		return ciGate{}, err
	}
	runs, err := fullRunsOn(parent)
	if err != nil {
		return ciGate{}, err
	}
	run, ok = newestFullRun(runs)
	return ciGate{commit: parent, run: run, found: ok}, nil
}

// fullRunsOn lists the `workflow_dispatch` runs of `ci.yml` on commit, each with its jobs.
func fullRunsOn(commit string) ([]ciRun, error) {
	out, err := output("", nil, "gh", "api", fmt.Sprintf("repos/%s/actions/workflows/%s/runs?head_sha=%s&event=workflow_dispatch&per_page=10", repo, ciWorkflow, commit),
		"--jq", ".workflow_runs[]")
	if err != nil {
		return nil, err
	}
	runs, err := decodeStream[ciRun](out)
	if err != nil {
		return nil, fmt.Errorf("reading the CI runs on %.9s: %w", commit, err)
	}
	for i := range runs {
		if runs[i].Jobs, err = runJobs(runs[i].ID); err != nil {
			return nil, err
		}
	}
	return runs, nil
}

// runJobs lists the jobs of a run's latest attempt.
func runJobs(id int64) ([]ciJob, error) {
	out, err := output("", nil, "gh", "api", "--paginate", fmt.Sprintf("repos/%s/actions/runs/%d/jobs?per_page=100", repo, id), "--jq", ".jobs[]")
	if err != nil {
		return nil, err
	}
	return decodeStream[ciJob](out)
}

// refreshRun reads the run and its jobs again.
func refreshRun(id int64) (ciRun, error) {
	out, err := output("", nil, "gh", "api", fmt.Sprintf("repos/%s/actions/runs/%d", repo, id))
	if err != nil {
		return ciRun{}, err
	}
	var run ciRun
	if err := json.Unmarshal([]byte(out), &run); err != nil {
		return ciRun{}, fmt.Errorf("reading run %d: %w", id, err)
	}
	run.Jobs, err = runJobs(id)
	return run, err
}

// decodeStream reads `gh api --jq '.x[]'` output: one JSON value after another.
func decodeStream[T any](out string) ([]T, error) {
	var items []T
	dec := json.NewDecoder(strings.NewReader(out))
	for {
		var item T
		if err := dec.Decode(&item); errors.Is(err, io.EOF) {
			return items, nil
		} else if err != nil {
			return nil, err
		}
		items = append(items, item)
	}
}

func skipCIGate() bool {
	if os.Getenv(skipCIGateEnv) != "1" {
		return false
	}
	step("⚠️  %s=1: going on WITHOUT a green full CI run or the CodeQL gate. Clear it once this release is out.", skipCIGateEnv)
	return true
}

// checkCI is the gate with no waiting: the dry run's and the pipeline's `ci-gate` job's. It
// refuses unless the full run is green, then runs the CodeQL gate on the same commit.
func checkCI(cfg config, commit string) error {
	if skipCIGate() {
		return nil
	}
	g, err := findCIGate(cfg, commit)
	if err != nil {
		return err
	}
	if !g.found {
		return noCIRunError(cfg, g.commit)
	}
	state, failed := judgeCI(g.run)
	switch state {
	case ciRed:
		return errors.New(ciRedMessage(repo, g.run, failed, cfg.version))
	case ciPending:
		return fmt.Errorf("the full CI run on %.9s is still going (%s), so %s can't ship yet. Wait for it: ./scripts/release-ci-gate.sh %s", g.commit, runURL(g.run.ID), cfg.tag, cfg.version)
	}
	step("Full CI run on %.9s is green (%s)", g.commit, runURL(g.run.ID))
	return codeQLGate(cfg, g.commit, false)
}

func noCIRunError(cfg config, commit string) error {
	return fmt.Errorf("no full CI run (ci.yml, workflow_dispatch with run_all) on %.9s, so %s can't ship. `./scripts/release.sh %s` starts it on the release branch", commit, cfg.tag, cfg.version)
}

// codeQLGate runs `release-codeql-gate.sh` on the commit whose CI run counted.
func codeQLGate(cfg config, commit string, wait bool) error {
	args := []string{commit}
	if wait {
		args = []string{"--wait", commit}
	}
	cmd := exec.Command(filepath.Join(cfg.root, "scripts", "release-codeql-gate.sh"), args...)
	cmd.Dir = cfg.root
	cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("the CodeQL gate refused %s (above)", cfg.tag)
	}
	return nil
}

// ciWatch follows the release's CI run for a wait, printing each job as it moves.
type ciWatch struct {
	cfg  config
	gate ciGate
	seen map[string]string
	done bool
}

func newCIWatch(cfg config, commit string) (*ciWatch, error) {
	if skipCIGate() {
		return &ciWatch{cfg: cfg, done: true}, nil
	}
	g, err := findCIGate(cfg, commit)
	if err != nil {
		return nil, err
	}
	w := &ciWatch{cfg: cfg, gate: g, seen: map[string]string{}}
	if !g.found {
		return nil, noCIRunError(cfg, g.commit)
	}
	step("Watching the full CI run on %.9s (%s)", g.commit, runURL(g.run.ID))
	return w, nil
}

// poll looks once: an error on red (or a lookup failure), done once green.
func (w *ciWatch) poll() error {
	if w.done {
		return nil
	}
	run, err := refreshRun(w.gate.run.ID)
	if err != nil {
		return err
	}
	for _, line := range jobChanges(w.seen, run.Jobs) {
		fmt.Println("    CI: " + line)
	}
	state, failed := judgeCI(run)
	switch state {
	case ciRed:
		return errors.New(ciRedMessage(repo, run, failed, w.cfg.version))
	case ciGreen:
		step("The full CI run is green (%s)", runURL(run.ID))
		w.done = true
		return codeQLGate(w.cfg, w.gate.commit, true)
	}
	return nil
}

// waitForCI waits for the release's CI and CodeQL, failing on the first red job.
func waitForCI(cfg config, commit string) error {
	w, err := newCIWatch(cfg, commit)
	if err != nil {
		return err
	}
	for {
		if err := w.poll(); err != nil || w.done {
			return err
		}
		time.Sleep(ciPollEvery)
	}
}
