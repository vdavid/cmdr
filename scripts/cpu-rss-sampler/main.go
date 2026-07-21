// CPU/RSS sampler: watch the main Cmdr app process during a benchmark and report
// its CPU and resident memory over time.
//
// It exists because ad-hoc `pgrep -f cmdr` sampling matched the WRONG process.
// Dev tooling (node LSPs, the codegraph MCP server) carries the repo path
// `…/vdavid/cmdr/…` in its arguments, so a substring match on "cmdr" catches
// those and, being case-sensitive, misses the real `Cmdr` app; the sampler then
// reported a node process's tiny RSS as if it were the app's. This tool matches
// the app's EXECUTABLE (argv[0] basename), prints the PID and path it picked so a
// wrong match is visible, and fails loudly when two instances are live rather
// than silently guessing. See selectTarget in match.go.
//
// Usage:
//
//	go run ./scripts/cpu-rss-sampler [flags]
//
// Flags: -interval (sample period, default 2s), -name (executable basename to
// match, default Cmdr), -pid (watch this PID directly, skipping discovery),
// -path-contains (disambiguate two instances by a path substring, e.g.
// /Applications/), -duration (stop after this long; default 0 runs until the
// process exits or you press Ctrl-C), -label (a run label for the report), and
// -md (markdown output).
//
// What it reports: sample count, elapsed time, CPU average and peak (percent of
// one core, so >100% on the multithreaded scan is expected), and RSS average and
// peak in MB. RSS is the MAIN process only, and on macOS it over-counts vs the
// app's own `phys_footprint` because it includes reclaimable SQLite pages and GPU
// mappings; `phys_footprint` (logged with CMDR_LOG_RAM_USE=1) is the honest
// memory number. See docs/notes/indexing-benchmarks-2026-07-21.md § "CPU and
// memory" and docs/notes/high-memory-gpu-compositor-investigation-2026-07.md.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "cpu-rss-sampler:", err)
		os.Exit(1)
	}
}

func run() error {
	var (
		interval     = flag.Duration("interval", 2*time.Second, "sample period")
		maxDuration  = flag.Duration("duration", 0, "stop after this long (0 = until the process exits or Ctrl-C)")
		name         = flag.String("name", "Cmdr", "executable basename to match (case-insensitive)")
		pathContains = flag.String("path-contains", "", "require the executable path to contain this substring (disambiguates two instances)")
		label        = flag.String("label", "", "a label for this run, printed in the report")
		pid          = flag.Int("pid", 0, "watch this PID directly, skipping discovery")
		md           = flag.Bool("md", false, "print a markdown table instead of aligned text")
	)
	flag.Parse()

	target, err := resolveTarget(*pid, *name, *pathContains)
	if err != nil {
		return err
	}
	fmt.Fprintf(os.Stderr, "Watching pid %d: %s\n", target.PID, target.Exe)
	if *label != "" {
		fmt.Fprintf(os.Stderr, "Run: %s\n", *label)
	}
	fmt.Fprintf(os.Stderr, "Sampling every %s; Ctrl-C to stop.\n\n", *interval)

	samples, elapsed := sampleLoop(target.PID, *interval, *maxDuration)
	if len(samples) == 0 {
		return fmt.Errorf("collected no samples; pid %d was already gone", target.PID)
	}
	report(os.Stdout, *label, target, samples, elapsed, *md)
	return nil
}

// resolveTarget finds the process to watch: an explicit -pid is read straight
// from ps (so its path is still reported), otherwise the app is discovered by
// executable name.
func resolveTarget(pid int, name, pathContains string) (Process, error) {
	procs, err := snapshotAll()
	if err != nil {
		return Process{}, err
	}
	if pid != 0 {
		for _, p := range procs {
			if p.PID == pid {
				return p, nil
			}
		}
		return Process{}, fmt.Errorf("no process with pid %d", pid)
	}
	target, _, err := selectTarget(procs, name, pathContains, os.Getpid())
	return target, err
}

// sample is one reading of the watched process.
type sample struct {
	rssKiB int64
	cpuPct float64
}

// sampleLoop reads the process every interval until it exits, the max duration
// elapses, or an interrupt arrives. It takes one reading immediately so a short
// run still reports something.
func sampleLoop(pid int, interval, maxDuration time.Duration) ([]sample, time.Duration) {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if maxDuration > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, maxDuration)
		defer cancel()
	}

	start := time.Now()
	var samples []sample
	take := func() bool {
		s, alive := sampleOne(pid)
		if !alive {
			return false
		}
		samples = append(samples, s)
		return true
	}

	if !take() {
		return samples, time.Since(start)
	}
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return samples, time.Since(start)
		case <-ticker.C:
			if !take() {
				return samples, time.Since(start)
			}
		}
	}
}

func report(w *os.File, label string, target Process, samples []sample, elapsed time.Duration, md bool) {
	var cpuSum, cpuPeak, rssSum float64
	var rssPeakKiB int64
	for _, s := range samples {
		cpuSum += s.cpuPct
		if s.cpuPct > cpuPeak {
			cpuPeak = s.cpuPct
		}
		rssSum += float64(s.rssKiB)
		if s.rssKiB > rssPeakKiB {
			rssPeakKiB = s.rssKiB
		}
	}
	n := float64(len(samples))
	cpuAvg := cpuSum / n
	rssAvgMB := rssSum / n / 1024
	rssPeakMB := float64(rssPeakKiB) / 1024

	if md {
		fmt.Fprintf(w, "| run | pid | samples | wall | CPU avg | CPU peak | RSS avg | RSS peak |\n")
		fmt.Fprintf(w, "| --- | --- | ------- | ---- | ------- | -------- | ------- | -------- |\n")
		runLabel := label
		if runLabel == "" {
			runLabel = "-"
		}
		fmt.Fprintf(w, "| %s | %d | %d | %s | %.1f%% | %.1f%% | %.1f MB | %.1f MB |\n",
			runLabel, target.PID, len(samples), roundDur(elapsed), cpuAvg, cpuPeak, rssAvgMB, rssPeakMB)
		return
	}

	if label != "" {
		fmt.Fprintf(w, "%s\n", label)
	}
	fmt.Fprintf(w, "pid %d  %s\n", target.PID, target.Exe)
	fmt.Fprintf(w, "%d samples over %s\n", len(samples), roundDur(elapsed))
	fmt.Fprintf(w, "CPU: %.1f%% average of one core, %.1f%% peak\n", cpuAvg, cpuPeak)
	fmt.Fprintf(w, "RSS: %.1f MB average, %.1f MB peak\n", rssAvgMB, rssPeakMB)
	fmt.Fprintf(w, "(RSS over-counts vs the app's phys_footprint; see the tool's doc comment.)\n")
}

func roundDur(d time.Duration) time.Duration { return d.Round(time.Second) }

// snapshotAll lists every process with the fields the matcher needs.
func snapshotAll() ([]Process, error) {
	out, err := exec.Command("ps", "-Ao", "pid=,rss=,%cpu=,command=").Output()
	if err != nil {
		return nil, fmt.Errorf("running ps: %w", err)
	}
	var procs []Process
	for line := range strings.SplitSeq(string(out), "\n") {
		if p, ok := parsePSLine(line); ok {
			procs = append(procs, p)
		}
	}
	return procs, nil
}

// sampleOne reads one process's rss and %cpu. alive is false when ps reports no
// such pid, which is how the loop learns the app exited.
func sampleOne(pid int) (sample, bool) {
	out, err := exec.Command("ps", "-o", "rss=,%cpu=", "-p", strconv.Itoa(pid)).Output()
	if err != nil {
		return sample{}, false // ps exits non-zero when the pid is gone
	}
	fields := strings.Fields(string(out))
	if len(fields) < 2 {
		return sample{}, false
	}
	rss, err1 := strconv.ParseInt(fields[0], 10, 64)
	cpu, err2 := strconv.ParseFloat(fields[1], 64)
	if err1 != nil || err2 != nil {
		return sample{}, false
	}
	return sample{rssKiB: rss, cpuPct: cpu}, true
}
