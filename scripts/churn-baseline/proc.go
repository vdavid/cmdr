package main

import (
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// resolveApp finds the single process whose executable is exactly exePath.
//
// The anchored full-path match is deliberate and load-bearing: a release build
// under a repo `target/` is also called `Cmdr`, and a loose `pgrep -x Cmdr`
// matched it instead of the installed app, corrupting several measurement
// windows. Two matches is an error, never a guess.
func resolveApp(exePath string) (int, error) {
	pattern := "^" + regexp.QuoteMeta(exePath) + "$"
	out, err := exec.Command("pgrep", "-f", pattern).Output()
	if err != nil {
		return 0, fmt.Errorf("no process matching %s; start the app first", exePath)
	}
	var pids []int
	for _, f := range strings.Fields(string(out)) {
		if pid, convErr := strconv.Atoi(f); convErr == nil {
			pids = append(pids, pid)
		}
	}
	switch len(pids) {
	case 1:
		return pids[0], nil
	case 0:
		return 0, fmt.Errorf("no process matching %s; start the app first", exePath)
	default:
		return 0, fmt.Errorf("%d processes match %s: %v; stop all but one", len(pids), exePath, pids)
	}
}

// CPUTime is a process's CUMULATIVE CPU since it started.
//
// Cumulative time integrated over a window is the instrument here, not a
// `sample`/`spindump` snapshot. Snapshots have twice produced wrong conclusions
// about this codebase: a 20-second `sample` put one thread at 45% of CPU, and a
// 180-second three-bucket sample refuted it (3.4% of busy, 0.2% of userspace,
// nearly all of it `stat` wait). A delta between two readings cannot lie that way.
type CPUTime struct {
	User time.Duration
	Sys  time.Duration
}

func (c CPUTime) Total() time.Duration { return c.User + c.Sys }

func (c CPUTime) sub(o CPUTime) CPUTime {
	return CPUTime{User: c.User - o.User, Sys: c.Sys - o.Sys}
}

// readCPU reads the process's cumulative user and system CPU. alive is false once
// `ps` no longer knows the pid, which is how a phase learns the app exited.
func readCPU(pid int) (CPUTime, bool) {
	out, err := exec.Command("ps", "-o", "utime=,stime=", "-p", strconv.Itoa(pid)).Output()
	if err != nil {
		return CPUTime{}, false
	}
	fields := strings.Fields(string(out))
	if len(fields) != 2 {
		return CPUTime{}, false
	}
	user, ok1 := parseCPUTime(fields[0])
	sys, ok2 := parseCPUTime(fields[1])
	if !ok1 || !ok2 {
		return CPUTime{}, false
	}
	return CPUTime{User: user, Sys: sys}, true
}

// readFootprint runs `footprint -p`, the only reading that attributes memory
// honestly on this app (see Footprint).
func readFootprint(pid int) Footprint {
	out, err := exec.Command("footprint", "-p", strconv.Itoa(pid)).Output()
	if err != nil {
		return Footprint{}
	}
	return parseFootprint(string(out))
}

// readMeasurementEnv reports which measurement-related environment variables the
// target was launched with. It exists so a re-run can be verified rather than
// assumed: the app has to be started the same way for the numbers to pair up.
func readMeasurementEnv(pid int) map[string]string {
	found := map[string]string{}
	out, err := exec.Command("ps", "-Eww", "-o", "command=", "-p", strconv.Itoa(pid)).Output()
	if err != nil {
		return found
	}
	for _, tok := range strings.Fields(string(out)) {
		name, value, ok := strings.Cut(tok, "=")
		if !ok {
			continue
		}
		if strings.HasPrefix(name, "CMDR_") || name == "RUST_LOG" {
			found[name] = value
		}
	}
	return found
}
