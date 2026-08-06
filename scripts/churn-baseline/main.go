// Churn baseline: what active build churn costs the running Cmdr app, in CPU,
// memory, index row writes, and log volume.
//
// This is the CPU half of a before/after pair; the row/disk half is
// `cargo run -p index-query --bin index-size-probe`. Use it whenever a claim is
// about what Cmdr costs while the filesystem underneath it is being rewritten:
// that cannot be settled by counting rows in the index, it needs the app watching
// a real build loop for a fixed window with an idle control beside it.
//
// # What it does
//
// Two phases against the SAME running app, so the churn number has a control:
//
//  1. idle:  nobody touches the repo; the app is just watching.
//  2. churn: `touch <file>` then `cargo build`, on repeat.
//
// For each phase it reports the app's CUMULATIVE CPU delta (see CPUTime for why
// that instrument and not `sample`), phys_footprint (see Footprint for why not
// vmmap), index rows written as the app's own reconcile summaries report them,
// and log bytes. Rows are attributed by path: inside the churned repo, inside a
// subtree named with -scope-roots, or neither. The attribution matters because
// several build trees churn on this machine at once (agents in sibling worktrees)
// and the app reconciles all of them, so a single total folds somebody else's
// build in.
//
// It also reports the index WRITER THREAD's own CPU (see WriterCPU), read off the
// app's stall-probe heartbeat. Whole-process CPU is too coarse for any claim about
// the write path: it sits in the noise of a contended machine, and macOS won't
// name a thread from outside the process.
//
// # Usage
//
//	cd scripts/churn-baseline && go run . -repo <rust-repo> [flags] > before.json
//
// Flags: -repo (the repo to churn; its `target/` is what the app re-indexes),
// -touch (file to touch, relative to -repo), -build (cargo args), -idle / -churn
// (phase durations), -interval (sample period), -exe (app binary path), -log
// (cmdr.log path), -scope-roots (a file of subtree roots, one per line, whose
// rows get counted separately), -label.
//
// # Re-running it after a change lands
//
// Same command, same -repo, same durations, app launched the same way. The JSON
// records the app's measurement-related environment so a mismatched pair is
// visible rather than silently compared. Per-phase CPU is also normalized per
// hour and per row written, which is what survives a repo whose absolute build
// cost has drifted.
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"time"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "churn-baseline:", err)
		os.Exit(1)
	}
}

func run() error {
	var (
		repo           = flag.String("repo", "", "Rust repo to churn (required); its target/ is what the app re-indexes")
		touchRel       = flag.String("touch", "crates/cmdr-fs/src/lib.rs", "file to touch each iteration, relative to -repo")
		buildArgs      = flag.String("build", "build --workspace", "cargo arguments for one churn iteration")
		settleFor      = flag.Duration("settle", 3*time.Minute, "wait before measuring, so earlier activity's indexing drains out of the idle phase")
		idleFor        = flag.Duration("idle", 5*time.Minute, "control phase with no churn")
		churnFor       = flag.Duration("churn", 20*time.Minute, "churn phase")
		interval       = flag.Duration("interval", 5*time.Second, "sample period")
		exePath        = flag.String("exe", "/Applications/Cmdr.app/Contents/MacOS/Cmdr", "app executable to watch, matched as a full path")
		logPath        = flag.String("log", defaultLogPath(), "cmdr.log to follow")
		label          = flag.String("label", "", "a label recorded in the output")
		scopeRootsFile = flag.String("scope-roots", "", "file of subtree roots, one per line; rows written inside them are counted separately")
	)
	flag.Parse()

	if *repo == "" {
		return fmt.Errorf("-repo is required")
	}
	touchAbs := filepath.Join(*repo, *touchRel)
	if _, err := os.Stat(touchAbs); err != nil {
		return fmt.Errorf("-touch file %s: %w", touchAbs, err)
	}
	pid, err := resolveApp(*exePath)
	if err != nil {
		return err
	}
	if _, err := os.Stat(*logPath); err != nil {
		return fmt.Errorf("-log %s: %w", *logPath, err)
	}

	fmt.Fprintf(os.Stderr, "Watching pid %d (%s)\n", pid, *exePath)
	fmt.Fprintf(os.Stderr, "Churning %s (touch %s, cargo %s)\n", *repo, *touchRel, *buildArgs)
	fmt.Fprintf(os.Stderr, "Phases: idle %s, churn %s. Ctrl-C aborts.\n\n", *idleFor, *churnFor)

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	scopeRoots, err := readScopeRoots(*scopeRootsFile)
	if err != nil {
		return err
	}
	tailer := &Tailer{path: *logPath, repoPrefix: *repo, scopeRoots: scopeRoots, eventWork: newEventWork(), writerRoll: newWriterRollup(), writerCPU: newWriterCPU()}
	tailDone := make(chan struct{})
	go tailer.follow(tailDone)
	defer close(tailDone)

	report := Report{
		Label:      *label,
		SettleSecs: settleFor.Seconds(),
		StartedAt:  time.Now().Format(time.RFC3339),
		PID:        pid,
		Exe:        *exePath,
		Repo:       *repo,
		TouchFile:  *touchRel,
		BuildArgs:  *buildArgs,
		LogPath:    *logPath,
		AppEnv:     readMeasurementEnv(pid),
		SampleSecs: interval.Seconds(),
	}
	report.ScopeRoots = len(scopeRoots)
	report.TargetFiles, report.TargetDirs = countTree(filepath.Join(*repo, "target"))
	fmt.Fprintf(os.Stderr, "Churned target/: %d files, %d dirs\n\n", report.TargetFiles, report.TargetDirs)

	// The writer's message counter is CUMULATIVE since the app started, so a phase
	// reads it as "end minus start". Until the first rollup line has been seen the
	// start value is zero, and the first phase would report the app's whole
	// lifetime as its own. Wait for one before measuring anything.
	report.WriterBaseline, report.WriterBaselineOK = awaitWriterBaseline(ctx, tailer, writerBaselineWait)
	if !report.WriterBaselineOK {
		fmt.Fprintf(os.Stderr,
			"⚠️  No writer rollup line in %s; writer_messages_in_phase will be 0 for every phase.\n",
			writerBaselineWait)
	}

	if *settleFor > 0 {
		fmt.Fprintf(os.Stderr, "Settling for %s before the idle phase...\n", *settleFor)
		select {
		case <-time.After(*settleFor):
		case <-ctx.Done():
			return ctx.Err()
		}
	}

	idle, err := measurePhase(ctx, "idle", *idleFor, *interval, pid, tailer, nil)
	if err != nil {
		return err
	}
	report.Phases = append(report.Phases, idle)

	driver := newChurnDriver(*repo, touchAbs, *buildArgs)
	churn, err := measurePhase(ctx, "churn", *churnFor, *interval, pid, tailer, driver)
	if err != nil {
		return err
	}
	report.Phases = append(report.Phases, churn)

	report.finish()
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	return enc.Encode(report)
}

// countTree walks a directory, returning (files, dirs). Errors are skipped rather
// than fatal: this is a comparability annotation, not a measurement, and an
// unreadable corner of a build tree shouldn't abort a 25-minute run.
func countTree(root string) (int64, int64) {
	var files, dirs int64
	_ = filepath.WalkDir(root, func(_ string, d os.DirEntry, err error) error {
		if err != nil {
			return nil //nolint:nilerr // skip what we can't read
		}
		if d.IsDir() {
			dirs++
		} else {
			files++
		}
		return nil
	})
	return files, dirs
}

// readScopeRoots loads the subtree roots to classify against, one absolute path
// per line. An empty path means the caller didn't ask for the split, which is not
// an error: the CPU and memory numbers stand on their own.
func readScopeRoots(path string) ([]string, error) {
	if path == "" {
		return nil, nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("-scope-roots %s: %w", path, err)
	}
	var roots []string
	for _, line := range strings.Split(string(data), "\n") {
		if line = strings.TrimSpace(line); line != "" {
			roots = append(roots, line)
		}
	}
	return roots, nil
}

// writerBaselineWait bounds how long to wait for the writer's first rollup line.
// The writer logs one every few seconds while it has anything to do; a fully idle
// app may log none at all, which is why this gives up rather than blocking.
const writerBaselineWait = 90 * time.Second

// awaitWriterBaseline blocks until the tailer has seen one writer rollup line, so
// the cumulative counter has a start value. Returns the value and whether one
// arrived.
func awaitWriterBaseline(ctx context.Context, tailer *Tailer, limit time.Duration) (int64, bool) {
	deadline := time.After(limit)
	tick := time.NewTicker(500 * time.Millisecond)
	defer tick.Stop()
	for {
		if total := tailer.snapshot().WriterRoll.Total; total > 0 {
			return total, true
		}
		select {
		case <-tick.C:
		case <-deadline:
			return 0, false
		case <-ctx.Done():
			return 0, false
		}
	}
}

func defaultLogPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, "Library/Logs/com.veszelovszki.cmdr/cmdr.log")
}

// ── Phases ───────────────────────────────────────────────────────────

// Phase is one measurement window. Every CPU and memory number here is the app's,
// except DriverCPUSeconds, which is the harness's own build load: it is reported
// so a re-run on a busier or quieter machine is recognizable rather than mistaken
// for a change in the app.
type Phase struct {
	Name             string  `json:"name"`
	WallSeconds      float64 `json:"wall_seconds"`
	AppCPUSeconds    float64 `json:"app_cpu_seconds"`
	AppUserSeconds   float64 `json:"app_user_cpu_seconds"`
	AppSysSeconds    float64 `json:"app_sys_cpu_seconds"`
	AppCPUPerHour    float64 `json:"app_cpu_seconds_per_hour"`
	AppCPUPercent    float64 `json:"app_cpu_percent_of_one_core"`
	DriverCPUSeconds float64 `json:"driver_cpu_seconds"`

	FootprintStart Footprint `json:"footprint_start"`
	FootprintEnd   Footprint `json:"footprint_end"`
	FootprintPeak  int64     `json:"footprint_peak_observed_bytes"`

	Builds           int     `json:"builds_completed"`
	BuildSecondsMean float64 `json:"build_seconds_mean"`

	// Writes counts every rebuild the app did; RepoWrites only those under the
	// churned repo. The gap between them is other build trees on this machine,
	// and it is the honest uncertainty on AppCPUSeconds, which cannot be split.
	//
	// ScopeWrites counts the rows that landed under -scope-roots, whatever the
	// caller pointed it at. It is the number to watch when a change is supposed to
	// move the writes in one part of the tree and leave the rest alone.
	Writes         RowWrites       `json:"row_writes_all"`
	RepoWrites     RowWrites       `json:"row_writes_in_churned_repo"`
	ScopeWrites    RowWrites       `json:"row_writes_in_scoped_subtrees"`
	RowsTotal      int64           `json:"rows_written_total"`
	RowsInRepo     int64           `json:"rows_written_in_churned_repo"`
	RowsInScope    int64           `json:"rows_written_in_scoped_subtrees"`
	RepoRowShare   float64         `json:"share_of_rows_in_churned_repo"`
	ScopeRowShare  float64         `json:"share_of_rows_in_scoped_subtrees"`
	Aggregate      AggregateWrites `json:"dir_stats_aggregate_writes"`
	EventWork      EventWork       `json:"event_work"`
	WriterRoll     WriterRollup    `json:"writer_messages"`
	WriterCPU      WriterCPU       `json:"writer_thread_cpu"`
	WriterCPUMs    int64           `json:"root_writer_cpu_ms"`
	WriterCPUShare float64         `json:"root_writer_share_of_app_cpu"`
	WriterMsgs     int64           `json:"writer_messages_in_phase"`
	CPUMsPerMsg    float64         `json:"app_cpu_ms_per_writer_message"`
	RowsPerBuild   float64         `json:"rows_written_in_repo_per_build"`
	CPUMsPerRow    float64         `json:"app_cpu_ms_per_row_written"`
	LogBytes       int64           `json:"log_bytes"`
	LogLines       int64           `json:"log_lines"`
	LogRotations   int             `json:"log_rotations"`
	LogBytesPerRow float64         `json:"log_bytes_per_row_written"`

	Samples int `json:"samples"`
}

// measurePhase runs one window, optionally with a churn driver, and returns what
// the app spent during it.
func measurePhase(
	ctx context.Context, name string, dur, interval time.Duration,
	pid int, tailer *Tailer, driver *churnDriver,
) (Phase, error) {
	fmt.Fprintf(os.Stderr, "[%s] starting, %s\n", name, dur)
	cpu0, ok := readCPU(pid)
	if !ok {
		return Phase{}, fmt.Errorf("pid %d is gone before the %s phase", pid, name)
	}
	log0 := tailer.snapshot()
	fp0 := readFootprint(pid)

	phaseCtx, cancel := context.WithTimeout(ctx, dur)
	defer cancel()
	if driver != nil {
		go driver.loop(phaseCtx)
	}

	start := time.Now()
	peak := fp0.PhysBytes
	samples := 0
	var fpEnd Footprint
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
loop:
	for {
		select {
		case <-phaseCtx.Done():
			break loop
		case <-ticker.C:
			fpEnd = readFootprint(pid)
			if fpEnd.PhysBytes > peak {
				peak = fpEnd.PhysBytes
			}
			samples++
		}
	}
	if driver != nil {
		driver.wait()
	}
	elapsed := time.Since(start)

	cpu1, ok := readCPU(pid)
	if !ok {
		return Phase{}, fmt.Errorf("pid %d exited during the %s phase", pid, name)
	}
	if fpEnd.PhysBytes == 0 {
		fpEnd = readFootprint(pid)
	}
	// Give the app a beat to flush the last rebuild's log lines before snapshotting.
	time.Sleep(500 * time.Millisecond)
	logDelta := tailer.snapshot().sub(log0)
	cpuDelta := cpu1.sub(cpu0)

	p := Phase{
		Name:           name,
		WallSeconds:    elapsed.Seconds(),
		AppCPUSeconds:  cpuDelta.Total().Seconds(),
		AppUserSeconds: cpuDelta.User.Seconds(),
		AppSysSeconds:  cpuDelta.Sys.Seconds(),
		FootprintStart: fp0,
		FootprintEnd:   fpEnd,
		FootprintPeak:  peak,
		Writes:         logDelta.Writes,
		RepoWrites:     logDelta.RepoWrite,
		ScopeWrites:    logDelta.ScopeWrite,
		RowsTotal:      logDelta.Writes.Total(),
		RowsInRepo:     logDelta.RepoWrite.Total(),
		RowsInScope:    logDelta.ScopeWrite.Total(),
		Aggregate:      logDelta.Aggregate,
		EventWork:      logDelta.EventWork,
		WriterRoll:     logDelta.WriterRoll,
		WriterMsgs:     logDelta.WriterRoll.Total,
		WriterCPU:      logDelta.WriterCPU,
		WriterCPUMs:    logDelta.WriterCPU.RootMs(),
		LogBytes:       logDelta.Bytes,
		LogLines:       logDelta.Lines,
		LogRotations:   logDelta.Rotations,
		Samples:        samples,
	}
	p.derive(elapsed, driver)
	fmt.Fprintf(os.Stderr,
		"[%s] done: %.1fs wall, app CPU %.1fs (%.1f%% of one core), %d rows (%d ours, %d in scope), "+
			"%d builds, %d writer msgs (root writer thread %.1fs CPU, %.1f%% of the app's), "+
			"%d anchors coalesced, %d events skipped, %.1f MB log\n\n",
		name, p.WallSeconds, p.AppCPUSeconds, p.AppCPUPercent, p.RowsTotal, p.RowsInRepo, p.RowsInScope,
		p.Builds, p.WriterMsgs, float64(p.WriterCPUMs)/1000, 100*p.WriterCPUShare,
		p.EventWork.CoalescedAnchors, p.EventWork.SkippedEvents,
		float64(p.LogBytes)/(1<<20))
	return p, nil
}

// derive fills in every number computed FROM the measurements rather than taken
// as one. Kept apart from measurePhase so the measuring and the arithmetic can be
// read separately; every guard here is a divide-by-zero on a phase where nothing
// of that kind happened.
//
// These normalized figures are what survives a comparison across runs: absolute
// CPU depends on how big the churned tree happened to be that day, where CPU per
// hour and CPU per row don't.
func (p *Phase) derive(elapsed time.Duration, driver *churnDriver) {
	if driver != nil {
		p.Builds = driver.builds
		p.DriverCPUSeconds = driver.cpuSeconds
		if driver.builds > 0 {
			p.BuildSecondsMean = driver.wallSeconds / float64(driver.builds)
			p.RowsPerBuild = float64(p.RowsInRepo) / float64(driver.builds)
		}
	}
	if elapsed > 0 {
		p.AppCPUPerHour = p.AppCPUSeconds / elapsed.Hours()
		p.AppCPUPercent = 100 * p.AppCPUSeconds / elapsed.Seconds()
	}
	if p.WriterMsgs > 0 {
		p.CPUMsPerMsg = 1000 * p.AppCPUSeconds / float64(p.WriterMsgs)
	}
	// What fraction of the whole app's CPU the local index WRITER thread burned.
	// Any claim about the cost of writing index rows is a claim about a slice of
	// THIS, never of the app total.
	if p.AppCPUSeconds > 0 {
		p.WriterCPUShare = float64(p.WriterCPUMs) / (1000 * p.AppCPUSeconds)
	}
	if p.RowsTotal > 0 {
		p.CPUMsPerRow = 1000 * p.AppCPUSeconds / float64(p.RowsTotal)
		p.LogBytesPerRow = float64(p.LogBytes) / float64(p.RowsTotal)
		p.RepoRowShare = float64(p.RowsInRepo) / float64(p.RowsTotal)
		p.ScopeRowShare = float64(p.RowsInScope) / float64(p.RowsTotal)
	}
}

// ── The churn driver ─────────────────────────────────────────────────

// churnDriver is the load: touch a source file, rebuild, repeat. A rebuild is
// what actually rewrites a `target/` tree, and its shape is the point: many small
// writes into deep, wide, already-indexed directories. Synthetic file creation
// would miss that.
type churnDriver struct {
	repo  string
	touch string
	args  string

	done        chan struct{}
	builds      int
	wallSeconds float64
	cpuSeconds  float64
}

func newChurnDriver(repo, touch, args string) *churnDriver {
	return &churnDriver{repo: repo, touch: touch, args: args, done: make(chan struct{})}
}

func (d *churnDriver) loop(ctx context.Context) {
	defer close(d.done)
	for ctx.Err() == nil {
		now := time.Now()
		if err := os.Chtimes(d.touch, now, now); err != nil {
			fmt.Fprintf(os.Stderr, "churn: touching %s: %v\n", d.touch, err)
			return
		}
		start := time.Now()
		cmd := exec.CommandContext(ctx, "cargo", strings.Fields(d.args)...)
		cmd.Dir = d.repo
		cmd.Stdout = nil
		cmd.Stderr = nil
		err := cmd.Run()
		if ctx.Err() != nil {
			return // the window closed mid-build; that build doesn't count
		}
		if err != nil {
			fmt.Fprintf(os.Stderr, "churn: cargo %s failed: %v\n", d.args, err)
			return
		}
		d.builds++
		d.wallSeconds += time.Since(start).Seconds()
		if st := cmd.ProcessState; st != nil {
			d.cpuSeconds += st.UserTime().Seconds() + st.SystemTime().Seconds()
		}
	}
}

// wait blocks until the driver goroutine has stopped, so its counters are stable
// before the phase reads them.
func (d *churnDriver) wait() {
	if d.done != nil {
		<-d.done
	}
}

// ── Report ───────────────────────────────────────────────────────────

// Report is the file a before-run and an after-run get diffed from. Its shape is
// the contract between the two; changing a key breaks the pairing.
type Report struct {
	Label      string            `json:"label"`
	StartedAt  string            `json:"started_at"`
	PID        int               `json:"pid"`
	Exe        string            `json:"exe"`
	Repo       string            `json:"repo"`
	TouchFile  string            `json:"touch_file"`
	BuildArgs  string            `json:"build_args"`
	LogPath    string            `json:"log_path"`
	AppEnv     map[string]string `json:"app_measurement_env"`
	SampleSecs float64           `json:"sample_interval_seconds"`
	SettleSecs float64           `json:"settle_seconds"`

	// The writer's cumulative message counter when measuring started. When
	// WriterBaselineOK is false no rollup line arrived in time and every phase's
	// writer count is meaningless rather than merely zero.
	WriterBaseline   int64 `json:"writer_messages_at_start"`
	WriterBaselineOK bool  `json:"writer_baseline_established"`

	// ScopeRoots is how many subtree roots the in-scope/out-of-scope split knew
	// about; 0 means the split was not asked for and its numbers are meaningless.
	ScopeRoots int `json:"scope_roots_supplied"`

	// The churned `target/` tree as it stood when the run started. A re-run
	// against a tree of a different size is not a comparison, and these two
	// numbers are what makes that visible instead of assumed.
	TargetFiles int64 `json:"churn_target_files"`
	TargetDirs  int64 `json:"churn_target_dirs"`

	Phases []Phase `json:"phases"`

	// ChurnCPUOverIdle is the CPU the churn actually caused: the churn phase's
	// per-hour rate minus the idle phase's. Without the subtraction, background
	// indexing of everything else on the volume would be counted as churn cost.
	ChurnCPUOverIdle float64 `json:"churn_cpu_seconds_per_hour_over_idle"`
}

func (r *Report) finish() {
	var idle, churn *Phase
	for i := range r.Phases {
		switch r.Phases[i].Name {
		case "idle":
			idle = &r.Phases[i]
		case "churn":
			churn = &r.Phases[i]
		}
	}
	if idle != nil && churn != nil {
		r.ChurnCPUOverIdle = churn.AppCPUPerHour - idle.AppCPUPerHour
	}
}
