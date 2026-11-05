package main

import (
	"cmp"
	"fmt"
	"maps"
	"slices"
	"strings"
	"time"
)

// The pure side of the CI ETA: how long recent green runs took, what CI's runners are busy with,
// and when this run's `CI OK` should report. `etagh.go` fetches the inputs and caches the history.
//
// A lane is a top-level `ci.yml` job: a reusable workflow's jobs show as "<lane> / <job>"
// (`laneOf`). A lane's jobs start one after another (Website's demo build, then its checks), and
// a lane that needs another (Linux E2E needs the base image) isn't listed until that one ends, so
// the model works per lane, never per job: how long after the run started a lane typically ends
// (`end`, which carries its queueing and the lanes it waits on), and how long it runs once started
// (`took`).

const (
	// changesJob is the `changes` job's display name: the lane plan is known once it ends.
	changesJob = "Detect changes"
	// lightGate is the gate's name on a light run (`ci.yml`'s `ci-ok` job).
	lightGate = "CI light"
	// laneSettle: a lane whose listed jobs all finished this recently may still list its next job
	// (GitHub creates it seconds after its needs end), so history decides whether it's done.
	laneSettle = 30 * time.Second
	// etaSlip is how far a new estimate may drift from the printed one before it's printed again.
	etaSlip = 3 * time.Minute
)

// Fallbacks for when GitHub has no green run to learn from (a new lane, a fresh repo). The only
// fixed durations here: everything else comes from recent runs.
var (
	fallbackLane = laneSpread{end: spread{Typical: 20 * time.Minute, High: 30 * time.Minute}, took: spread{Typical: 20 * time.Minute, High: 30 * time.Minute}}
	fallbackGate = spread{Typical: 20 * time.Second, High: 40 * time.Second}
)

// laneOf is the lane a job belongs to: the caller's name before " / ", or the job's own name.
func laneOf(job string) string {
	lane, _, _ := strings.Cut(job, " / ")
	return lane
}

// laneSample is one lane's timing in one green run.
type laneSample struct {
	// End: from the run's start to the lane's last job ending.
	End time.Duration `json:"end"`
	// Took: from the lane's first job starting to its last ending.
	Took time.Duration `json:"took"`
}

// runSample is what one green run teaches: its total, its gate's own time, and each lane's.
type runSample struct {
	ID    int64 `json:"id"`
	Light bool  `json:"light"`
	// Total: from the run's start to its gate (`CI OK` or `CI light`) reporting.
	Total time.Duration         `json:"total"`
	Gate  time.Duration         `json:"gate"`
	Lanes map[string]laneSample `json:"lanes"`
}

// sampleRun reads a finished run's jobs into a sample. ok is false unless its gate ended green.
func sampleRun(run workflowRun, jobs []runJob) (runSample, bool) {
	gateName := ciCheck
	if run.isLightRun() {
		gateName = lightGate
	}
	gate := slices.IndexFunc(jobs, func(j runJob) bool { return j.Name == gateName })
	if gate < 0 || jobs[gate].Conclusion != "success" || jobs[gate].CompletedAt.IsZero() {
		return runSample{}, false
	}
	start := run.attemptStart()
	s := runSample{
		ID:    run.ID,
		Light: run.isLightRun(),
		Total: jobs[gate].CompletedAt.Sub(start),
		Gate:  jobs[gate].CompletedAt.Sub(jobs[gate].CreatedAt),
		Lanes: map[string]laneSample{},
	}
	for _, span := range laneSpans(jobs) {
		s.Lanes[laneKey(span.lane, span.class)] = laneSample{End: span.last.Sub(start), Took: span.last.Sub(span.first)}
	}
	return s, true
}

// laneSpan is when a lane's finished jobs ran, and on which runners.
type laneSpan struct {
	lane        string
	class       runnerClass
	first, last time.Time
}

// laneSpans gathers the finished, not skipped jobs other than `changes` and the gate by lane.
func laneSpans(jobs []runJob) map[string]laneSpan {
	spans := map[string]laneSpan{}
	for _, job := range jobs {
		if job.Name == changesJob || job.Name == ciCheck || job.Name == lightGate || job.Conclusion == "skipped" || job.StartedAt.IsZero() || job.CompletedAt.IsZero() {
			continue
		}
		lane := laneOf(job.Name)
		span, ok := spans[lane]
		if !ok {
			span = laneSpan{lane: lane, first: job.StartedAt, last: job.CompletedAt}
		}
		span.class = max(span.class, classOf(job.Labels))
		if job.StartedAt.Before(span.first) {
			span.first = job.StartedAt
		}
		if job.CompletedAt.After(span.last) {
			span.last = job.CompletedAt
		}
		spans[lane] = span
	}
	return spans
}

// laneKey names a lane's history by the runners it ran on, since one lane can move between pools
// (macOS from hosted runners to the M1) and its timing with it. A lane's runners are its jobs'
// most specific pool: a lane marker runs on Linux beside the M1 job it marks.
func laneKey(lane string, class runnerClass) string {
	if class == linuxRunners {
		return lane
	}
	return lane + " on " + runnerNames[class]
}

// spread is a duration's typical value (the median) and its high one (about the 90th
// percentile), over N samples.
type spread struct {
	Typical time.Duration
	High    time.Duration
	N       int
}

func spreadOf(ds []time.Duration) spread {
	if len(ds) == 0 {
		return spread{}
	}
	sorted := slices.Sorted(slices.Values(ds))
	// Nearest rank: the 90th percentile of ten samples is the ninth.
	high := (len(sorted)*9 + 9) / 10
	return spread{Typical: sorted[(len(sorted)-1)/2], High: sorted[high-1], N: len(sorted)}
}

type laneSpread struct{ end, took spread }

// etaModel is the history the ETA reads, built from recent green runs.
type etaModel struct {
	land, light spread
	gate        spread
	lanes       map[string]laneSpread
}

func buildModel(samples []runSample) etaModel {
	var land, light, gate []time.Duration
	ends, tooks := map[string][]time.Duration{}, map[string][]time.Duration{}
	for _, s := range samples {
		if s.Light {
			light = append(light, s.Total)
			continue
		}
		land = append(land, s.Total)
		gate = append(gate, s.Gate)
		for lane, ls := range s.Lanes {
			ends[lane] = append(ends[lane], ls.End)
			tooks[lane] = append(tooks[lane], ls.Took)
		}
	}
	m := etaModel{land: spreadOf(land), light: spreadOf(light), gate: spreadOf(gate), lanes: map[string]laneSpread{}}
	for lane := range ends {
		m.lanes[lane] = laneSpread{end: spreadOf(ends[lane]), took: spreadOf(tooks[lane])}
	}
	return m
}

// lane is a lane's history on its runners (`laneKey`), else on any runners, else the fallback.
func (m etaModel) lane(name string, class runnerClass) laneSpread {
	for c := class; c >= linuxRunners; c-- {
		if ls, ok := m.lanes[laneKey(name, c)]; ok {
			return ls
		}
	}
	for c := class + 1; c < runnerClasses; c++ {
		if ls, ok := m.lanes[laneKey(name, c)]; ok {
			return ls
		}
	}
	return fallbackLane
}

// ranLane says whether recent land runs ran the lane, on any runners.
func (m etaModel) ranLane(name string) bool {
	return m.lane(name, linuxRunners) != fallbackLane
}

func (m etaModel) gateSpread() spread {
	if m.gate.N == 0 {
		return fallbackGate
	}
	return m.gate
}

// typicalTook is the median lane's typical run time, what a queued job waits on per slot.
func (m etaModel) typicalTook() time.Duration {
	var ds []time.Duration
	for _, ls := range m.lanes {
		ds = append(ds, ls.took.Typical)
	}
	if len(ds) == 0 {
		return fallbackLane.took.Typical
	}
	return spreadOf(ds).Typical
}

// runnerClass is a pool of runners with its own concurrency limit.
type runnerClass int

const (
	linuxRunners runnerClass = iota
	macosRunners
	m1Runners
	runnerClasses
)

// m1Label routes a job to David's M1 (`docs/tooling/ci.md`).
const m1Label = "cmdr-m1"

// runnerSlots: the free plan's 20 concurrent jobs (five of them hosted macOS), and the M1's four
// self-hosted runners.
var runnerSlots = [runnerClasses]int{linuxRunners: 20, macosRunners: 5, m1Runners: 4}

var runnerNames = [runnerClasses]string{linuxRunners: "Linux", macosRunners: "hosted macOS", m1Runners: "M1"}

func classOf(labels []string) runnerClass {
	switch {
	case slices.Contains(labels, m1Label):
		return m1Runners
	case slices.ContainsFunc(labels, func(l string) bool { return strings.HasPrefix(l, "macos") }):
		return macosRunners
	default:
		return linuxRunners
	}
}

func jobStarted(j runJob) bool { return j.Status == "in_progress" || j.Status == "completed" }

// ciLoad is what the repo's runners are busy with, from every unfinished run's jobs.
type ciLoad struct {
	busy, queued [runnerClasses]int
	// ahead: other runs' jobs queued before this run's first queued job of the class.
	ahead [runnerClasses]int
}

// measureLoad counts running and queued jobs per runner class across jobs (every unfinished
// run's), and how many of the queued ones are ahead of run ours's (0 for none).
func measureLoad(jobs []runJob, ours int64) ciLoad {
	var l ciLoad
	var oursFirst [runnerClasses]time.Time
	for _, j := range jobs {
		c := classOf(j.Labels)
		switch {
		case j.Status == "in_progress":
			l.busy[c]++
		case j.Status != "completed":
			l.queued[c]++
			if j.RunID == ours && (oursFirst[c].IsZero() || j.CreatedAt.Before(oursFirst[c])) {
				oursFirst[c] = j.CreatedAt
			}
		}
	}
	for _, j := range jobs {
		c := classOf(j.Labels)
		if j.RunID != ours && !jobStarted(j) && !oursFirst[c].IsZero() && j.CreatedAt.Before(oursFirst[c]) {
			l.ahead[c]++
		}
	}
	return l
}

// queueWait is about how long a job queued now waits in class c: nothing while a slot is free;
// else one slot frees about every half a typical job per slot-full of jobs ahead of it (a running
// job is halfway through, on average).
func (l ciLoad) queueWait(c runnerClass, typicalTook time.Duration) time.Duration {
	if l.busy[c] < runnerSlots[c] {
		return 0
	}
	rounds := (l.ahead[c] + runnerSlots[c]) / runnerSlots[c]
	return time.Duration(rounds) * typicalTook / 2
}

// summary is one line on the runners, like "Linux 7 of 20 busy, 3 queued; M1 2 of 4 busy".
func (l ciLoad) summary() string {
	var parts []string
	for c := range runnerClasses {
		part := fmt.Sprintf("%s %d of %d busy", runnerNames[c], l.busy[c], runnerSlots[c])
		if l.queued[c] > 0 {
			part += fmt.Sprintf(", %d queued", l.queued[c])
		}
		parts = append(parts, part)
	}
	return strings.Join(parts, "; ")
}

// listedLanes maps every lane in jobs to whether it runs (any job not skipped), and reports
// whether the `changes` job ended, which is when the lanes that need only it get listed.
func listedLanes(jobs []runJob) (map[string]bool, bool) {
	listed := map[string]bool{}
	changed := false
	for _, j := range jobs {
		switch j.Name {
		case changesJob:
			changed = j.Status == "completed"
		case ciCheck, lightGate:
		default:
			lane := laneOf(j.Name)
			listed[lane] = listed[lane] || j.Conclusion != "skipped"
		}
	}
	return listed, changed
}

// plannedLanes are lanes not listed yet that will run: their needs beyond `changes` are all
// running lanes (directly or through another planned one), and recent land runs ran them, which
// leaves out what a PR never runs (the deploys). wf is `ci.yml`'s jobs (`workflowJobBodies`);
// listed has every lane in the run's jobs so far, true for one that runs, false for a skipped one.
func plannedLanes(wf map[string][]string, listed map[string]bool, m etaModel) []string {
	names := map[string]string{}
	for id, body := range wf {
		names[id] = jobDisplayName(id, body)
	}
	runs := map[string]bool{}
	maps.Copy(runs, listed)
	var planned []string
	for grew := true; grew; {
		grew = false
		for _, id := range slices.Sorted(maps.Keys(wf)) {
			name := names[id]
			if _, seen := runs[name]; seen || id == ciGateJob || !m.ranLane(name) {
				continue
			}
			needs := gateNeeds(wf[id])
			ready := len(needs) > 0 && !slices.ContainsFunc(needs, func(n string) bool {
				return n != "changes" && !runs[names[n]]
			})
			if ready && slices.ContainsFunc(needs, func(n string) bool { return n != "changes" }) {
				runs[name] = true
				planned = append(planned, name)
				grew = true
			}
		}
	}
	return planned
}

// laneEstimate is when one lane should end: typically, and at the high end.
type laneEstimate struct {
	name          string
	class         runnerClass
	typical, high time.Time
}

// ciEstimate is when the run's `CI OK` should report, and the lanes it waits on, latest first.
type ciEstimate struct {
	lanes         []laneEstimate
	typical, high time.Time
	// queuedAhead and queuedClass: the queue this run's jobs wait in, when they do.
	queuedAhead int
	queuedClass runnerClass
	queueWait   time.Duration
}

// estimate works out when the run's gate reports, from its jobs so far, the lanes still to be
// listed (`plannedLanes`), the history, and the runners' load. A lane that started runs its
// typical time from its start, one not started yet waits its queue first; and none ends before
// the history's usual offset from the run's start, which carries the lanes it waits on.
func estimate(run workflowRun, jobs []runJob, planned []string, m etaModel, load ciLoad, now time.Time) ciEstimate {
	start := run.attemptStart()
	byLane := map[string][]runJob{}
	for _, j := range jobs {
		if j.Name == changesJob || j.Name == ciCheck || j.Name == lightGate || j.Conclusion == "skipped" {
			continue
		}
		lane := laneOf(j.Name)
		byLane[lane] = append(byLane[lane], j)
	}
	for _, lane := range planned {
		if _, ok := byLane[lane]; !ok {
			byLane[lane] = nil
		}
	}
	var est ciEstimate
	for lane, laneJobs := range byLane {
		le := estimateLane(lane, laneJobs, start, m, load, now)
		if wait := le.waitFor(laneJobs, load, m); wait > est.queueWait {
			est.queueWait, est.queuedClass, est.queuedAhead = wait, le.class, load.ahead[le.class]
		}
		est.lanes = append(est.lanes, le)
	}
	slices.SortFunc(est.lanes, func(a, b laneEstimate) int {
		return cmp.Or(b.typical.Compare(a.typical), cmp.Compare(a.name, b.name))
	})
	gate := m.gateSpread()
	est.typical, est.high = now.Add(gate.Typical), now.Add(gate.High)
	if len(est.lanes) > 0 {
		est.typical = latest(est.typical, est.lanes[0].typical.Add(gate.Typical))
		for _, le := range est.lanes {
			est.high = latest(est.high, le.high.Add(gate.High))
		}
	}
	return est
}

func estimateLane(lane string, jobs []runJob, start time.Time, m etaModel, load ciLoad, now time.Time) laneEstimate {
	le := laneEstimate{name: lane}
	for _, j := range jobs {
		le.class = max(le.class, classOf(j.Labels))
	}
	ls := m.lane(lane, le.class)
	var first, last time.Time
	done := len(jobs) > 0
	for _, j := range jobs {
		if jobStarted(j) && (first.IsZero() || j.StartedAt.Before(first)) {
			first = j.StartedAt
		}
		if j.Status != "completed" {
			done = false
		} else if j.CompletedAt.After(last) {
			last = j.CompletedAt
		}
	}
	base := first
	if base.IsZero() {
		base = now.Add(load.queueWait(le.class, m.typicalTook()))
	}
	if done && (now.Sub(last) > laneSettle || !base.Add(ls.took.Typical).After(now)) {
		le.typical, le.high = last, last
		return le
	}
	le.typical = latest(start.Add(ls.end.Typical), base.Add(ls.took.Typical), now)
	le.high = latest(start.Add(ls.end.High), base.Add(ls.took.High), now)
	return le
}

// waitFor is the queue wait counted in for a lane none of whose jobs started yet.
func (le laneEstimate) waitFor(jobs []runJob, load ciLoad, m etaModel) time.Duration {
	if len(jobs) == 0 || slices.ContainsFunc(jobs, jobStarted) {
		return 0
	}
	return load.queueWait(le.class, m.typicalTook())
}

func latest(t time.Time, more ...time.Time) time.Time {
	for _, u := range more {
		if u.After(t) {
			t = u
		}
	}
	return t
}

// laneShortNames are the lanes' names in the ETA line. A lane missing here shows its full name.
var laneShortNames = map[string]string{
	"Desktop (Rust)":                                  "Rust",
	"Desktop (Rust, fixtures)":                        "Rust fixtures",
	"Desktop (Rust, macOS)":                           "macOS",
	"Desktop (Svelte)":                                "Svelte",
	"E2E tests (Playwright, Linux)":                   "E2E",
	"E2E base image":                                  "",
	"Light lane (hygiene, Go, API server, dashboard)": "the light lane",
	"Website": "Website",
}

func shortLane(le laneEstimate) string {
	name, ok := laneShortNames[le.name]
	if !ok {
		name = le.name
	}
	if name != "" && le.class == m1Runners {
		name += " (on the M1)"
	}
	return name
}

// etaMessage is the line `land` prints, like "CI runs E2E, Rust, and macOS (on the M1). Expect
// CI OK around 00:31, in about 19 min (up to 24)."
func etaMessage(est ciEstimate, now time.Time) string {
	var names []string
	for _, le := range est.lanes {
		if name := shortLane(le); name != "" {
			names = append(names, name)
		}
	}
	msg := "CI runs no lanes."
	if len(names) > 0 {
		msg = fmt.Sprintf("CI runs %s.", joinList(names))
	}
	msg += fmt.Sprintf(" Expect %s around %s, in about %s (up to %s).", ciCheck, est.typical.Local().Format("15:04"), mins(est.typical.Sub(now)), mins(est.high.Sub(now)))
	if est.queueWait > 0 {
		msg += fmt.Sprintf(" That counts about %s in the %s queue: %d %s ahead of ours, every runner busy.", mins(est.queueWait), runnerNames[est.queuedClass], est.queuedAhead, plural(est.queuedAhead, "job", "jobs"))
	}
	return msg
}

// joinList joins names with commas and a final "and" (Oxford comma from three on).
func joinList(names []string) string {
	switch len(names) {
	case 1:
		return names[0]
	case 2:
		return names[0] + " and " + names[1]
	default:
		return strings.Join(names[:len(names)-1], ", ") + ", and " + names[len(names)-1]
	}
}

// etaSlipped says whether next moved far enough from the printed estimate to print again.
func etaSlipped(printed, next ciEstimate) bool {
	return printed.typical.IsZero() || (next.typical.Sub(printed.typical)).Abs() > etaSlip
}

// etaReport is what `pnpm land eta` prints: the runners' load and the typical run lengths.
func etaReport(m etaModel, load ciLoad) string {
	var b strings.Builder
	fmt.Fprintf(&b, "CI runners now: %s.\n", load.summary())
	if wait := load.queueWait(linuxRunners, m.typicalTook()); wait > 0 {
		fmt.Fprintf(&b, "Every Linux runner is busy, so a run starting now waits about %s for one.\n", mins(wait))
	}
	b.WriteString(runLengthLine("A land run", ciCheck, m.land))
	b.WriteString(runLengthLine("A light run", lightGate, m.light))
	if len(m.lanes) > 0 {
		lanes := slices.SortedFunc(maps.Keys(m.lanes), func(a, b string) int {
			return cmp.Or(cmp.Compare(m.lanes[b].end.Typical, m.lanes[a].end.Typical), cmp.Compare(a, b))
		})
		var parts []string
		for _, lane := range lanes {
			ls := m.lanes[lane]
			parts = append(parts, fmt.Sprintf("%s %s (up to %s, in %d of %d runs)", lane, mins(ls.end.Typical), mins(ls.end.High), ls.end.N, m.land.N))
		}
		fmt.Fprintf(&b, "Each lane in a land run ends, after the run starts: %s. The slowest lane that runs sets the time.\n", strings.Join(parts, "; "))
	}
	return b.String()
}

func runLengthLine(what, gate string, s spread) string {
	if s.N == 0 {
		return fmt.Sprintf("%s: no recent green run to learn from.\n", what)
	}
	return fmt.Sprintf("%s reports %s about %s after it starts, up to %s (the last %d green ones).\n", what, gate, mins(s.Typical), mins(s.High), s.N)
}

// mins rounds d up to whole minutes, like "19 min".
func mins(d time.Duration) string {
	return fmt.Sprintf("%d min", max(int((d+time.Minute-1)/time.Minute), 1))
}
