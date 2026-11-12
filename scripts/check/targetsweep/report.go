package targetsweep

import (
	"fmt"
	"strconv"
	"strings"
	"time"
)

// Summary is the one line the handoff hook prints. A skipped unit sweep says why in the
// same line, so it shows in the warming log or the teardown's output, where someone
// reading about a slow first build will look.
func (r Report) Summary() string {
	p := r.Plan
	var b strings.Builder
	switch {
	case p.Reset:
		fmt.Fprintf(&b, "reset the Linux cache: %s was still over the %s budget after the sweep, so the next build is cold",
			gibs(p.BytesBefore), gibs(Budget))
	default:
		fmt.Fprintf(&b, "swept the Linux cache from %s to %s in %s (%s build %s, %s incremental %s)",
			gibs(p.BytesBefore), gibs(p.BytesAfter), r.Took.Round(100*time.Millisecond),
			thousands(p.DeletedUnits), plural(p.DeletedUnits, "unit", "units"),
			thousands(p.DeletedIncremental), plural(p.DeletedIncremental, "dir", "dirs"))
	}
	if p.Repruned && !p.Reset {
		fmt.Fprintf(&b, "; over the %s budget, so incremental dirs older than the seed went too", gibs(Budget))
	}
	if p.UnitsSkipped != "" {
		fmt.Fprintf(&b, "; kept every build unit, since %s", p.UnitsSkipped)
	}
	if r.Failures > 0 {
		fmt.Fprintf(&b, "; %s %s couldn't be deleted", thousands(r.Failures), plural(r.Failures, "entry", "entries"))
	}
	return b.String()
}

func gibs(n int64) string {
	return strconv.FormatFloat(float64(n)/float64(1<<30), 'f', 1, 64) + " GiB"
}

func plural(n int, one, many string) string {
	if n == 1 {
		return one
	}
	return many
}

func thousands(n int) string {
	s := strconv.Itoa(n)
	for i := len(s) - 3; i > 0; i -= 3 {
		s = s[:i] + "," + s[i:]
	}
	return s
}
