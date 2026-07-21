package main

import (
	"fmt"
	"path"
	"strconv"
	"strings"
)

// Process is one row of a `ps` snapshot: enough to identify the app and read its
// resource use.
type Process struct {
	PID     int
	RSSKiB  int64   // resident set size in KiB (ps -o rss)
	CPUPct  float64 // ps -o %cpu; on macOS this can exceed 100 across cores
	Exe     string  // argv[0]: the program actually running, NOT the argument string
	Command string  // the full command line, for display and diagnosis only
}

// parsePSLine reads one line of `ps -Ao pid=,rss=,%cpu=,command=`. It returns
// ok=false for the header, blanks, or anything whose leading fields aren't
// numbers, so a stray line never becomes a phantom process.
func parsePSLine(line string) (Process, bool) {
	fields := strings.Fields(line)
	if len(fields) < 4 {
		return Process{}, false
	}
	pid, err := strconv.Atoi(fields[0])
	if err != nil {
		return Process{}, false
	}
	rss, err := strconv.ParseInt(fields[1], 10, 64)
	if err != nil {
		return Process{}, false
	}
	cpu, err := strconv.ParseFloat(fields[2], 64)
	if err != nil {
		return Process{}, false
	}
	// The command starts at field 3. argv[0] is its first token; the app's
	// binary path never contains spaces (neither `/Applications/Cmdr.app/…`
	// nor a cargo `target/<profile>/cmdr` under the repo), so the first token
	// is the executable we match on.
	return Process{
		PID:     pid,
		RSSKiB:  rss,
		CPUPct:  cpu,
		Exe:     fields[3],
		Command: strings.Join(fields[3:], " "),
	}, true
}

// selectTarget picks the single main Cmdr app process out of a ps snapshot.
//
// It matches on the EXECUTABLE's basename (argv[0]), case-insensitively, never on
// the argument string. That is the whole fix: dev tooling (node LSPs, the
// codegraph MCP server, ripgrep) carries the repo path `…/cmdr/…` in its
// arguments, so any substring match on "cmdr" catches those and, being
// case-sensitive, misses the real `Cmdr` app entirely. Matching argv[0]'s
// basename catches exactly the app binary, bundled (`Cmdr`) or bare cargo build
// (`cmdr`), and nothing else.
//
// pathContains, when set, further requires the executable path to contain that
// substring, to disambiguate deliberately (e.g. `/Applications/` vs a worktree
// `target/`). selfPID is excluded so the sampler never watches itself.
//
// It returns the chosen process, the full candidate list (for reporting), and an
// error when the result is anything other than exactly one candidate: zero means
// the app isn't running (or isn't matched), and more than one means two live
// instances, which must fail loudly rather than silently pick one.
func selectTarget(procs []Process, nameWant, pathContains string, selfPID int) (Process, []Process, error) {
	wantLower := strings.ToLower(nameWant)
	var candidates []Process
	for _, p := range procs {
		if p.PID == selfPID {
			continue
		}
		if strings.ToLower(path.Base(p.Exe)) != wantLower {
			continue
		}
		if pathContains != "" && !strings.Contains(p.Exe, pathContains) {
			continue
		}
		candidates = append(candidates, p)
	}

	switch len(candidates) {
	case 1:
		return candidates[0], candidates, nil
	case 0:
		hint := ""
		if pathContains != "" {
			hint = fmt.Sprintf(" whose path contains %q", pathContains)
		}
		return Process{}, nil, fmt.Errorf("no running process named %q%s; is the app running?", nameWant, hint)
	default:
		var b strings.Builder
		fmt.Fprintf(&b, "%d processes match %q; narrow with -pid or -path-contains:", len(candidates), nameWant)
		for _, c := range candidates {
			fmt.Fprintf(&b, "\n  pid %d  %s", c.PID, c.Exe)
		}
		return Process{}, candidates, fmt.Errorf("%s", b.String())
	}
}
