package main

import (
	"slices"
	"strings"
	"testing"
	"time"
)

var etaT0 = time.Date(2026, 11, 5, 22, 37, 46, 0, time.UTC)

func at(min int) time.Time { return etaT0.Add(time.Duration(min) * time.Minute) }

func doneJob(name string, from, to int) runJob {
	return runJob{Name: name, Status: "completed", Conclusion: "success", CreatedAt: at(from), StartedAt: at(from), CompletedAt: at(to)}
}

func TestLaneOf(t *testing.T) {
	if got := laneOf("Desktop (Rust) / Kani proofs"); got != "Desktop (Rust)" {
		t.Errorf("a reusable workflow's job: got %q", got)
	}
	if got := laneOf("E2E base image"); got != "E2E base image" {
		t.Errorf("a plain job: got %q", got)
	}
}

// Shaped after run 42123098315: the lane's jobs chain, and the gate's own time is from its
// creation (its needs ending) to its end.
func TestSampleRun(t *testing.T) {
	run := workflowRun{ID: 1, RunAttempt: 1, RunStartedAt: etaT0}
	jobs := []runJob{
		doneJob(changesJob, 0, 1),
		{Name: "Full run (run_all)", Status: "completed", Conclusion: "skipped"},
		doneJob("Website / Demo build", 1, 4),
		doneJob("Website / Checks, E2E, and Lighthouse", 4, 10),
		doneJob("Website / Lane marker", 10, 11),
		doneJob("Desktop (Rust) / Tests", 1, 17),
		{Name: ciCheck, Status: "completed", Conclusion: "success", CreatedAt: at(17), StartedAt: at(17), CompletedAt: etaT0.Add(17*time.Minute + 20*time.Second)},
	}
	s, ok := sampleRun(run, jobs)
	if !ok {
		t.Fatal("a green run gave no sample")
	}
	if s.Total != 17*time.Minute+20*time.Second || s.Gate != 20*time.Second {
		t.Errorf("total %s, gate %s", s.Total, s.Gate)
	}
	if got := s.Lanes["Website"]; got != (laneSample{End: 11 * time.Minute, Took: 10 * time.Minute}) {
		t.Errorf("Website: %+v", got)
	}
	if _, ok := s.Lanes["Full run (run_all)"]; ok {
		t.Error("a skipped job made a lane")
	}
	jobs[len(jobs)-1].Conclusion = "failure"
	if _, ok := sampleRun(run, jobs); ok {
		t.Error("a red gate gave a sample")
	}
	light := workflowRun{ID: 2, DisplayTitle: lightRunPrefix + "x", RunStartedAt: etaT0}
	if s, ok := sampleRun(light, []runJob{doneJob(lightGate, 0, 1)}); !ok || !s.Light || s.Total != time.Minute {
		t.Errorf("a light run: %+v, %v", s, ok)
	}
}

func TestSpreadOf(t *testing.T) {
	var ds []time.Duration
	for i := 1; i <= 10; i++ {
		ds = append(ds, time.Duration(i)*time.Minute)
	}
	if got := spreadOf(ds); got != (spread{Typical: 5 * time.Minute, High: 9 * time.Minute, N: 10}) {
		t.Errorf("one to ten: %+v", got)
	}
	if got := spreadOf([]time.Duration{time.Minute}); got.Typical != time.Minute || got.High != time.Minute {
		t.Errorf("one sample: %+v", got)
	}
	if got := spreadOf(nil); got.N != 0 {
		t.Errorf("none: %+v", got)
	}
}

func TestPickHistoryRunsSkipsReRuns(t *testing.T) {
	runs := []workflowRun{{ID: 1, RunAttempt: 2}, {ID: 2, RunAttempt: 1}, {ID: 3, RunAttempt: 1, DisplayTitle: lightRunPrefix + "x"}}
	for i := range 30 {
		runs = append(runs, workflowRun{ID: int64(10 + i), RunAttempt: 1})
	}
	picked := pickHistoryRuns(runs)
	if len(picked) != historyLandRuns+1 {
		t.Fatalf("picked %d runs", len(picked))
	}
	if picked[0].ID != 2 || picked[1].ID != 3 {
		t.Errorf("picked %d runs, first %d and %d", len(picked), picked[0].ID, picked[1].ID)
	}
}

func TestMeasureLoad(t *testing.T) {
	var jobs []runJob
	for range 20 {
		jobs = append(jobs, runJob{RunID: 9, Status: "in_progress", Labels: []string{"ubuntu-latest"}})
	}
	jobs = append(jobs,
		runJob{RunID: 9, Status: "queued", CreatedAt: at(0), Labels: []string{"ubuntu-latest"}},
		runJob{RunID: 9, Status: "queued", CreatedAt: at(1), Labels: []string{"ubuntu-latest"}},
		runJob{RunID: 1, Status: "queued", CreatedAt: at(2), Labels: []string{"ubuntu-latest"}},
		runJob{RunID: 9, Status: "completed", Labels: []string{"ubuntu-latest"}},
		runJob{RunID: 9, Status: "in_progress", Labels: []string{"self-hosted", m1Label}},
		runJob{RunID: 9, Status: "queued", Labels: []string{"macos-26"}},
	)
	l := measureLoad(jobs, 1)
	if l.busy[linuxRunners] != 20 || l.queued[linuxRunners] != 3 || l.ahead[linuxRunners] != 2 {
		t.Errorf("Linux: %+v", l)
	}
	if l.busy[m1Runners] != 1 || l.queued[macosRunners] != 1 || l.ahead[macosRunners] != 0 {
		t.Errorf("macOS: %+v", l)
	}
	// Two ahead and ours fit in the first slot-full: about half a typical job.
	if got := l.queueWait(linuxRunners, 10*time.Minute); got != 5*time.Minute {
		t.Errorf("Linux wait: %s", got)
	}
	if got := l.queueWait(m1Runners, 10*time.Minute); got != 0 {
		t.Errorf("a free M1 slot waited %s", got)
	}
	if got := l.summary(); !strings.HasPrefix(got, "Linux 20 of 20 busy, 3 queued; hosted macOS 0 of 5 busy, 1 queued; M1 1 of 4 busy") {
		t.Errorf("summary: %q", got)
	}
}

const etaWorkflow = `jobs:
  changes:
    name: Detect changes
  e2e-base-image:
    name: E2E base image
    needs: changes
  desktop-e2e-linux:
    name: E2E tests (Playwright, Linux)
    needs: [changes, e2e-base-image]
  website:
    name: Website
    needs: changes
  deploy-website:
    name: Deploy website
    needs: [changes, website]
  ci-ok:
    name: ${{ 'CI OK' }}
    needs: [changes, website, desktop-e2e-linux]
`

func etaTestModel() etaModel {
	lane := func(end, took int) laneSpread {
		return laneSpread{
			end:  spread{Typical: time.Duration(end) * time.Minute, High: time.Duration(end+3) * time.Minute, N: 10},
			took: spread{Typical: time.Duration(took) * time.Minute, High: time.Duration(took+3) * time.Minute, N: 10},
		}
	}
	return etaModel{
		gate: spread{Typical: 20 * time.Second, High: 30 * time.Second, N: 10},
		lanes: map[string]laneSpread{
			"E2E base image":                lane(2, 1),
			"E2E tests (Playwright, Linux)": lane(20, 18),
			"Website":                       lane(10, 9),
		},
	}
}

func TestPlannedLanes(t *testing.T) {
	wf := workflowJobBodies(etaWorkflow)
	m := etaTestModel()
	// The base image runs, so E2E (not listed until it ends) will; the deploy never ran on a PR.
	got := plannedLanes(wf, map[string]bool{"E2E base image": true, "Website": true}, m)
	if !slices.Equal(got, []string{"E2E tests (Playwright, Linux)"}) {
		t.Errorf("base image running: %v", got)
	}
	if got := plannedLanes(wf, map[string]bool{"E2E base image": false, "Website": true}, m); len(got) != 0 {
		t.Errorf("base image skipped: %v", got)
	}
	if got := plannedLanes(wf, map[string]bool{"E2E base image": true, "E2E tests (Playwright, Linux)": true}, m); len(got) != 0 {
		t.Errorf("E2E already listed: %v", got)
	}
}

func TestEstimate(t *testing.T) {
	run := workflowRun{ID: 9, RunAttempt: 1, RunStartedAt: etaT0}
	m := etaTestModel()
	now := at(3)
	jobs := []runJob{
		doneJob(changesJob, 0, 1),
		doneJob("E2E base image", 1, 2),
		{Name: "Website / Demo build", Status: "in_progress", StartedAt: at(1)},
		{Name: "Desktop (Rust)", Status: "completed", Conclusion: "skipped"},
	}
	listed, changed := listedLanes(jobs)
	if !changed || !listed["Website"] || listed["Desktop (Rust)"] {
		t.Fatalf("listed %v, changed %v", listed, changed)
	}
	planned := plannedLanes(workflowJobBodies(etaWorkflow), listed, m)
	est := estimate(run, jobs, planned, m, ciLoad{}, now)
	// E2E isn't listed yet: it ends no sooner than its usual 20 min, nor 18 after now.
	if want := at(21).Add(20 * time.Second); !est.typical.Equal(want) {
		t.Errorf("typical %s, want %s", est.typical, want)
	}
	if want := at(24).Add(30 * time.Second); !est.high.Equal(want) {
		t.Errorf("high %s, want %s", est.high, want)
	}
	if est.lanes[0].name != "E2E tests (Playwright, Linux)" {
		t.Errorf("lanes, latest first: %+v", est.lanes)
	}
	// The base image ended two minutes ago, so it stays where it ended.
	for _, le := range est.lanes {
		if le.name == "E2E base image" && !le.typical.Equal(at(2)) {
			t.Errorf("the base image: %s", le.typical)
		}
	}
	msg := etaMessage(est, now)
	if !strings.HasPrefix(msg, "CI runs E2E and Website. Expect CI OK around ") || !strings.HasSuffix(msg, ", in about 19 min (up to 22 min).") {
		t.Errorf("message: %q", msg)
	}
}

func TestEstimateCountsTheQueue(t *testing.T) {
	run := workflowRun{ID: 9, RunAttempt: 1, RunStartedAt: etaT0}
	m := etaTestModel()
	jobs := []runJob{doneJob(changesJob, 0, 1), {RunID: 9, Name: "Website / Demo build", Status: "queued", CreatedAt: at(1)}}
	var load ciLoad
	load.busy[linuxRunners], load.ahead[linuxRunners] = 20, 12
	est := estimate(run, jobs, nil, m, load, at(2))
	if est.queuedAhead != 12 || est.queueWait <= 0 {
		t.Fatalf("queue: %+v", est)
	}
	if msg := etaMessage(est, at(2)); !strings.Contains(msg, "in the Linux queue: 12 jobs ahead of ours, every runner busy.") {
		t.Errorf("message: %q", msg)
	}
}

func TestEtaSlipped(t *testing.T) {
	printed := ciEstimate{typical: at(20)}
	if etaSlipped(printed, ciEstimate{typical: at(22)}) {
		t.Error("two minutes reprinted")
	}
	if !etaSlipped(printed, ciEstimate{typical: at(24)}) || !etaSlipped(printed, ciEstimate{typical: at(16)}) {
		t.Error("four minutes either way didn't reprint")
	}
	if !etaSlipped(ciEstimate{}, printed) {
		t.Error("the first estimate didn't print")
	}
}

func TestJoinList(t *testing.T) {
	if got := joinList([]string{"Rust", "Svelte", "E2E"}); got != "Rust, Svelte, and E2E" {
		t.Errorf("three: %q", got)
	}
	if got := joinList([]string{"Rust", "E2E"}); got != "Rust and E2E" {
		t.Errorf("two: %q", got)
	}
}

func TestShortLaneMarksTheM1(t *testing.T) {
	if got := shortLane(laneEstimate{name: "Desktop (Rust, macOS)", class: m1Runners}); got != "macOS (on the M1)" {
		t.Errorf("got %q", got)
	}
	if got := shortLane(laneEstimate{name: "Some new lane"}); got != "Some new lane" {
		t.Errorf("an unknown lane: %q", got)
	}
}
