package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"time"
)

// The fetching side of the CI ETA (`eta.go`): recent green runs' timings, cached in a file every
// worktree shares, and the runners' load. Every call goes through `ghAPI`, so it waits out a
// spent budget like the rest of `land`.

const (
	// historyTTL: how old the cached history may get before a land lists recent runs again. A
	// refresh costs one list call plus one jobs call per run it hasn't seen.
	historyTTL = 15 * time.Minute
	// How many recent green runs of each kind the history keeps: enough for a median and a 90th
	// percentile, few enough to follow CI getting faster within a day.
	historyLandRuns  = 15
	historyLightRuns = 10
	// loadEvery: how often a wait re-reads the runners' load while its jobs still queue.
	loadEvery = 2 * time.Minute
)

// historyVersion goes up whenever a sample's shape or meaning changes, which drops older caches.
const historyVersion = 1

// durationHistory is the cache file: the samples and when they were listed.
type durationHistory struct {
	Version   int         `json:"version"`
	FetchedAt time.Time   `json:"fetched_at"`
	Samples   []runSample `json:"samples"`
}

// historyPath is the cache file, in the common git dir so every worktree shares it.
func historyPath(r runner) (string, error) {
	dir, err := r.git("rev-parse", "--git-common-dir")
	if err != nil {
		return "", err
	}
	if !filepath.IsAbs(dir) {
		dir = filepath.Join(r.dir, dir)
	}
	return filepath.Join(dir, "land", "ci-durations.json"), nil
}

// pickHistoryRuns picks the newest first attempts (a re-run's untouched jobs keep the first
// attempt's times) of each kind from a newest-first list of green runs.
func pickHistoryRuns(runs []workflowRun) []workflowRun {
	var picked []workflowRun
	land, light := 0, 0
	for _, run := range runs {
		if run.RunAttempt > 1 {
			continue
		}
		switch {
		case run.isLightRun() && light < historyLightRuns:
			light++
		case !run.isLightRun() && land < historyLandRuns:
			land++
		default:
			continue
		}
		picked = append(picked, run)
	}
	return picked
}

// loadModel builds the ETA's history from the cache file, refreshing it first once it's older than
// `historyTTL`. A failed refresh keeps the older samples (or none, and the fallbacks).
func loadModel(r runner, now time.Time) etaModel {
	path, err := historyPath(r)
	if err != nil {
		return buildModel(nil)
	}
	var h durationHistory
	if raw, err := os.ReadFile(path); err == nil {
		_ = json.Unmarshal(raw, &h)
	}
	if h.Version != historyVersion {
		h = durationHistory{}
	}
	if now.Sub(h.FetchedAt) > historyTTL {
		samples, err := refreshHistory(r, h.Samples)
		if err != nil {
			say("Couldn't read recent CI runs for the ETA, so it goes by older numbers: %v", err)
			return buildModel(h.Samples)
		}
		h = durationHistory{Version: historyVersion, FetchedAt: now, Samples: samples}
		if raw, err := json.Marshal(h); err == nil && os.MkdirAll(filepath.Dir(path), 0o755) == nil {
			_ = os.WriteFile(path, raw, 0o644)
		}
	}
	return buildModel(h.Samples)
}

// refreshHistory lists recent green PR runs and samples the ones known lacks.
func refreshHistory(r runner, known []runSample) ([]runSample, error) {
	var runs struct {
		WorkflowRuns []workflowRun `json:"workflow_runs"`
	}
	if err := r.ghAPI(&runs, fmt.Sprintf("repos/%s/actions/workflows/%s/runs?event=pull_request&status=success&per_page=100", repo, ciWorkflow)); err != nil {
		return nil, err
	}
	var samples []runSample
	for _, run := range pickHistoryRuns(runs.WorkflowRuns) {
		if i := slices.IndexFunc(known, func(s runSample) bool { return s.ID == run.ID }); i >= 0 {
			samples = append(samples, known[i])
			continue
		}
		jobs, err := runJobs(r, run.ID)
		if err != nil {
			return nil, err
		}
		if s, ok := sampleRun(run, jobs); ok {
			samples = append(samples, s)
		}
	}
	return samples, nil
}

// fetchLoadJobs lists the jobs of every unfinished run in the repo (any workflow: they share the
// runners), one list call per status plus one jobs call per run.
func fetchLoadJobs(r runner) ([]runJob, error) {
	var all []runJob
	for _, status := range []string{"in_progress", "queued"} {
		var runs struct {
			WorkflowRuns []workflowRun `json:"workflow_runs"`
		}
		if err := r.ghAPI(&runs, fmt.Sprintf("repos/%s/actions/runs?status=%s&per_page=100", repo, status)); err != nil {
			return nil, err
		}
		for _, run := range runs.WorkflowRuns {
			jobs, err := runJobs(r, run.ID)
			if err != nil {
				return nil, err
			}
			all = append(all, jobs...)
		}
	}
	return all, nil
}

// etaTracker prints the wait's ETA once the run's lane plan is known, and again when it slips.
type etaTracker struct {
	loaded  bool
	model   etaModel
	wf      map[string][]string
	load    ciLoad
	loadAt  time.Time
	printed ciEstimate
}

// observe takes one poll's jobs. Nothing happens until `changes` ended; then the first call
// reads the history and `ci.yml`, and the runners' load is read once, and again every `loadEvery`
// while a lane is still to start.
func (t *etaTracker) observe(r runner, head string, run workflowRun, jobs []runJob, now time.Time) {
	listed, changed := listedLanes(jobs)
	if !changed {
		return
	}
	if !t.loaded {
		t.model = loadModel(r, now)
		ciYml, _ := r.git("show", head+":.github/workflows/"+ciWorkflow)
		t.wf = workflowJobBodies(ciYml)
		t.loaded = true
	}
	planned := plannedLanes(t.wf, listed, t.model)
	waiting := len(planned) > 0 || slices.ContainsFunc(jobs, func(j runJob) bool { return !jobStarted(j) })
	if t.loadAt.IsZero() || (waiting && now.Sub(t.loadAt) > loadEvery) {
		if all, err := fetchLoadJobs(r); err == nil {
			t.load = measureLoad(all, run.ID)
		}
		t.loadAt = now
	}
	est := estimate(run, jobs, planned, t.model, t.load, now)
	if etaSlipped(t.printed, est) {
		say("%s", etaMessage(est, now))
		t.printed = est
	}
}

// etaCommand is `pnpm land eta`: read-only, it prints the runners' load, how long recent runs
// took, and, when this commit's land run is going, its ETA.
func etaCommand(r runner) error {
	now := time.Now()
	m := loadModel(r, now)
	all, err := fetchLoadJobs(r)
	if err != nil {
		return err
	}
	var run workflowRun
	if head, err := r.git("rev-parse", "HEAD"); err == nil {
		var runs struct {
			WorkflowRuns []workflowRun `json:"workflow_runs"`
		}
		if err := r.ghAPI(&runs, ciRunsPath(head, false)); err == nil && len(landRuns(runs.WorkflowRuns)) > 0 {
			if newest := newestLandRun(landRuns(runs.WorkflowRuns)); newest.Status != "completed" {
				run = newest
			}
		}
	}
	load := measureLoad(all, run.ID)
	fmt.Print(etaReport(m, load))
	if run.ID == 0 {
		fmt.Println("No land run is going on this commit; `pnpm land` prints a live ETA once CI picks its lanes.")
		return nil
	}
	jobs := slices.DeleteFunc(slices.Clone(all), func(j runJob) bool { return j.RunID != run.ID })
	listed, changed := listedLanes(jobs)
	if !changed {
		fmt.Printf("This commit's land run is still picking its lanes: https://github.com/%s/actions/runs/%d\n", repo, run.ID)
		return nil
	}
	ciYml, _ := r.git("show", "HEAD:.github/workflows/"+ciWorkflow)
	est := estimate(run, jobs, plannedLanes(workflowJobBodies(ciYml), listed, m), m, load, now)
	fmt.Printf("This commit's land run: %s\n", etaMessage(est, now))
	return nil
}
