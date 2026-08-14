package checks

import (
	"os"
	"path/filepath"
	"regexp"
	"time"
)

// Housekeeping for the run-scoped artifacts the E2E lanes leave in /tmp.
//
// Reports, logs, and Playwright's recordings deliberately OUTLIVE their run: they're
// what a post-mortem reads, and a lane that deleted them on the way out would be
// deleting the only picture of the failure it just reported. The cost is that /tmp
// grows without bound (2.8 GB of `cmdr-e2e-*` and 120 shard logs on this laptop before
// this sweep existed), so age is what collects them instead.
//
// The rule is deliberately narrow: a name must match one of the run-scoped shapes AND
// be older than [e2eArtifactMaxAge]. Both halves matter. Matching alone would delete a
// concurrent suite's live report; age alone would delete `cmdr-e2e-fixtures-cache`,
// which is shared on purpose, rebuilt only when the fixture shape changes, and older
// than any cutoff worth having.

// e2eArtifactMaxAge is how long a run's leftovers stay readable. A week covers "the
// nightly went red on Friday and I'm looking on Monday", and no E2E run comes close to
// living that long, so nothing in flight can be caught by it.
const e2eArtifactMaxAge = 7 * 24 * time.Hour

// e2eRunScopedArtifacts matches the /tmp entries an E2E run creates and names after
// itself. Each pattern is anchored and ends in the run's own number (a pid, or the
// launch timestamp for the Linux lane's), which is exactly what distinguishes a
// leftover from a hand-made path like `cmdr-e2e-data` that no run owns and no sweep
// should touch.
var e2eRunScopedArtifacts = []*regexp.Regexp{
	// Playwright JSON reports: cmdr-e2e-report-<shard>-<pid>.json
	regexp.MustCompile(`^cmdr-e2e-report-[a-z0-9]+-\d+\.json$`),
	// Shard and build logs: cmdr-e2e-playwright-<name>-<ts>-<pid>.log
	regexp.MustCompile(`^cmdr-e2e-playwright-[a-z0-9-]+-\d+-\d+\.log$`),
	// Linux lane log: cmdr-e2e-linux-<ts>.log
	regexp.MustCompile(`^cmdr-e2e-linux-\d+\.log$`),
	// Playwright output dirs: cmdr-e2e-results-<shard>-<pid>
	regexp.MustCompile(`^cmdr-e2e-results-[a-z0-9-]+-\d+$`),
	// Per-shard data dirs: cmdr-e2e-data-<shard>-<pid>
	regexp.MustCompile(`^cmdr-e2e-data-(?:mtp|nonmtp\d+)-\d+$`),
	// Per-shard fixture trees: cmdr-e2e-fixtures-e2e-<shard>-<pid>-<ts>
	regexp.MustCompile(`^cmdr-e2e-fixtures-e2e-[a-z0-9]+-\d+-\d+$`),
	// The run's virtual MTP backing dir: cmdr-mtp-e2e-fixtures-<pid>
	regexp.MustCompile(`^cmdr-mtp-e2e-fixtures-\d+$`),
	// Playwright control sockets: tauri-playwright-<shard>-<pid>.sock
	regexp.MustCompile(`^tauri-playwright-[a-z0-9]+-\d+\.sock$`),
	// The Linux lane's fixture root: cmdr-e2e-<ms timestamp>
	regexp.MustCompile(`^cmdr-e2e-\d+$`),
}

// e2eArtifactIsSweepable reports whether a bare /tmp entry name is a run-scoped E2E
// artifact. Name only: age is the caller's half of the decision.
func e2eArtifactIsSweepable(name string) bool {
	for _, re := range e2eRunScopedArtifacts {
		if re.MatchString(name) {
			return true
		}
	}
	return false
}

// sweepStaleE2EArtifacts collects the E2E leftovers in /tmp that are older than
// [e2eArtifactMaxAge]. Called at the start of a lane, where a failure to tidy up is
// never worth reporting: every error is swallowed, exactly as the per-test
// instrumentation swallows its own.
func sweepStaleE2EArtifacts(now time.Time) int {
	return sweepStaleE2EArtifactsIn(os.TempDir(), now)
}

// sweepStaleE2EArtifactsIn is [sweepStaleE2EArtifacts] against a named directory, so a
// test can age real files without writing into the machine's /tmp.
func sweepStaleE2EArtifactsIn(dir string, now time.Time) int {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return 0
	}
	cutoff := now.Add(-e2eArtifactMaxAge)
	removed := 0
	for _, entry := range entries {
		if !e2eArtifactIsSweepable(entry.Name()) {
			continue
		}
		info, err := entry.Info()
		// A modification time we can't read means we can't tell a leftover from a
		// live run's file, and keeping something too long is the cheap mistake.
		if err != nil || !info.ModTime().Before(cutoff) {
			continue
		}
		if os.RemoveAll(filepath.Join(dir, entry.Name())) == nil {
			removed++
		}
	}
	return removed
}
