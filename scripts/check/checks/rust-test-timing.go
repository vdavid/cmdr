package checks

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
	"time"
)

// rustTimingTopN is how many of the slowest tests a CI run's timing profile names.
// The CI macOS runner has three cores, so the tail that sets the step's length is a
// handful of tests, not hundreds.
const rustTimingTopN = 25

// nextestSummaryRE captures the run's wall clock from nextest's end-of-run summary.
var nextestSummaryRE = regexp.MustCompile(`(?m)^\s*Summary \[([^\]]+)\]`)

// RustTimingProfile says where a nextest lane's wall clock went: how much was building
// and listing (everything before nextest's own clock starts) against running tests, and
// which tests ran longest. CI prints it on every run, which is the only place the
// macOS lane's per-test timings survive: the runner's CSV log dies with the machine.
func RustTimingProfile(output string, wall time.Duration, records []TestRecord, top int) string {
	var sb strings.Builder
	if m := nextestSummaryRE.FindStringSubmatch(output); m != nil {
		run := time.Duration(parseNextestDuration(m[1]) * float64(time.Second))
		fmt.Fprintf(&sb, "Timing: %s in all: %s building and listing, %s running tests\n",
			minutesSeconds(wall), minutesSeconds(wall-run), minutesSeconds(run))
	} else {
		fmt.Fprintf(&sb, "Timing: %s in all (nextest printed no summary, so no split)\n", minutesSeconds(wall))
	}

	timed := make([]TestRecord, 0, len(records))
	for _, rec := range records {
		if rec.Seconds >= 0 {
			timed = append(timed, rec)
		}
	}
	var summed float64
	for _, rec := range timed {
		summed += rec.Seconds
	}
	// The bulk a parallel run divides among its threads: set against the run time above,
	// it says whether the step is long because of its tail or because of its volume.
	fmt.Fprintf(&sb, "Tests summed: %s over %s %s\n", minutesSeconds(time.Duration(summed*float64(time.Second))),
		FormatThousands(len(timed)), Pluralize(len(timed), "test", "tests"))

	sort.SliceStable(timed, func(i, j int) bool { return timed[i].Seconds > timed[j].Seconds })
	if len(timed) > top {
		timed = timed[:top]
	}
	fmt.Fprintf(&sb, "Slowest %d %s:", len(timed), Pluralize(len(timed), "test", "tests"))
	for _, rec := range timed {
		fmt.Fprintf(&sb, "\n  %5.2f s  %s", rec.Seconds, rec.ID)
	}
	return sb.String()
}

// minutesSeconds renders a duration as `11m4s`, rounded to the second.
func minutesSeconds(d time.Duration) string {
	d = d.Round(time.Second)
	return fmt.Sprintf("%dm%ds", int(d.Minutes()), int(d.Seconds())%60)
}
