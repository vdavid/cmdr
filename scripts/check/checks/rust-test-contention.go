package checks

import (
	"fmt"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
)

// Contention-aware re-run for the Rust suite.
//
// The problem: on a saturated machine the global 8 s nextest cap kills CPU-bound tests
// that would finish in milliseconds on an idle one. Measured 2026-07-29 on an M3 Max
// (16 cores) at load ~198, a full `rust-tests` run produced 13 failures, 9 of them cap
// kills of pure-compute tests (`find_newlines_utf8_matches_memchr`,
// `walk_memory_tests::*`, `tar_each_codec_round_trips_a_file`, …). Those tests are not
// wrong and their deadlines are not wrong; they simply could not get 8 s of wall-clock
// while 200 threads fought over 16 cores.
//
// Loosening the cap globally is the wrong fix: it costs every idle run its hang
// detector, and the cap encodes a real incident (see `.config/nextest.toml`). Instead,
// a red run re-runs ONLY the failing tests, alone, and lets the outcome classify them:
//
//   - Passes alone at the UNCHANGED deadline     → the suite was starving it. Contention.
//   - Needs headroom, machine quiet              → it genuinely got slower. Not absorbed.
//   - Needs headroom, machine still busy         → inconclusive; neither claim is made.
//   - Fails alone even with headroom             → a real failure, whatever the load.
//
// Load is NOT the gate. The isolated re-run is. Load enters at exactly one point: the
// "needed headroom" verdict is the only one whose meaning depends on the machine being
// quiet, so when the re-run itself ran hot that verdict is demoted to "inconclusive"
// rather than reported as real slowness. A test that passes alone despite load is still
// contention, and a test that fails alone with headroom is still broken; neither
// conclusion needs a threshold.

// MaxContentionRerun bounds the re-run. Past it the machine was too loaded for the
// result to mean anything, and re-running hundreds of tests serially is its own problem.
// Real saturated runs have produced up to 13 failures at once (measured at load ~198),
// so 15 clears observed reality with headroom while still refusing a runaway.
const MaxContentionRerun = 15

// nextest profiles the two re-run stages use. Defined in `.config/nextest.toml`.
const (
	// ContentionProbeProfile keeps every deadline exactly as the failing run had them
	// and only removes the parallelism. That's what makes a pass here mean "starved".
	ContentionProbeProfile = "contention-probe"
	// ContentionRetryProfile grants headroom. It deliberately does NOT `inherit` the
	// default profile: an inherited per-test override BEATS a profile-level
	// `slow-timeout` (verified against nextest 0.9.136, 2026-07-29), so inheriting
	// would silently keep the tight per-test caps this stage exists to lift.
	ContentionRetryProfile = "contention-retry"
)

// ContentionVerdict is what the isolated re-runs concluded about one failing test.
type ContentionVerdict string

const (
	// VerdictContention: passed alone at the same deadline. The suite starved it.
	VerdictContention ContentionVerdict = "contention"
	// VerdictTooSlow: needed headroom even alone, on a quiet machine. Wants tweaking or
	// an explicit, documented per-test override, not silent absorption.
	VerdictTooSlow ContentionVerdict = "too-slow"
	// VerdictInconclusive: needed headroom, but the re-run itself ran on a busy machine,
	// so "it got slower" can't be told from "it was starved again". Reported, never
	// dressed up as either.
	VerdictInconclusive ContentionVerdict = "inconclusive"
	// VerdictReal: failed even alone with headroom.
	VerdictReal ContentionVerdict = "real"
)

// BusyLoadPerCore is where a machine is considered too busy for the "needed headroom"
// verdict to mean anything. Normal interactive work sits well under 1 runnable thread
// per core; the saturated runs this exists for measured ~12 per core.
const BusyLoadPerCore = 1.5

// ContentionResult is one failing test plus what the re-runs concluded.
type ContentionResult struct {
	Binary  string
	Name    string
	Class   FailureClass
	Verdict ContentionVerdict
}

// ContentionRunner runs the named tests under one nextest profile and returns the raw
// output. Injected so the classification logic is testable without a cargo build.
type ContentionRunner func(profile string, names []string) (string, error)

// LoadSampler reports the current load average per core. Injected for testability.
type LoadSampler func() float64

// RealFailures drops leaks. nextest counts a leaky test as PASSED (it appears in the
// "N passed (M leaky)" tally), so re-running one is meaningless and counting one as a
// failure overstates a red run.
func RealFailures(failures []RustFailure) []RustFailure {
	real := make([]RustFailure, 0, len(failures))
	for _, f := range failures {
		if f.Class != ClassLeak {
			real = append(real, f)
		}
	}
	return real
}

// MaybeClassifyContention runs the two-stage classification unless the failure count is
// past MaxContentionRerun, in which case it reports `skipped` so the caller can disclose
// the cap instead of silently examining a subset.
func MaybeClassifyContention(failures []RustFailure, run ContentionRunner, load LoadSampler) (results []ContentionResult, skipped bool) {
	if len(failures) > MaxContentionRerun {
		return nil, true
	}
	return ClassifyContention(failures, run, load), false
}

// ClassifyContention re-runs the failing tests alone, first at their original deadlines
// and then (only for those still failing) with headroom.
func ClassifyContention(failures []RustFailure, run ContentionRunner, load LoadSampler) []ContentionResult {
	if len(failures) == 0 {
		return nil
	}

	results := make([]ContentionResult, 0, len(failures))
	index := map[string]int{}
	names := make([]string, 0, len(failures))
	for _, f := range failures {
		index[f.Name] = len(results)
		results = append(results, ContentionResult{
			Binary:  f.Binary,
			Name:    f.Name,
			Class:   f.Class,
			Verdict: VerdictContention, // upgraded below if it keeps failing
		})
		names = append(names, f.Name)
	}

	probeOut, err := run(ContentionProbeProfile, names)
	if err != nil {
		// A runner-level problem (couldn't launch cargo) isn't evidence about any test.
		// Leave every verdict real rather than inventing an excuse for a red run.
		return markAll(results, VerdictReal)
	}
	stillFailing := failedNames(probeOut)
	if len(stillFailing) == 0 {
		return results // everything passed alone at the unchanged deadline
	}

	// Sample load around the escalation run: that verdict is the only one whose meaning
	// depends on the machine being quiet.
	headroomVerdict := VerdictTooSlow
	if load() > BusyLoadPerCore {
		headroomVerdict = VerdictInconclusive
	}
	for _, n := range stillFailing {
		if i, ok := index[n]; ok {
			results[i].Verdict = headroomVerdict
		}
	}

	retryOut, err := run(ContentionRetryProfile, stillFailing)
	if err != nil {
		return markNames(results, index, stillFailing, VerdictReal)
	}
	return markNames(results, index, failedNames(retryOut), VerdictReal)
}

func markAll(results []ContentionResult, v ContentionVerdict) []ContentionResult {
	for i := range results {
		results[i].Verdict = v
	}
	return results
}

func markNames(results []ContentionResult, index map[string]int, names []string, v ContentionVerdict) []ContentionResult {
	for _, n := range names {
		if i, ok := index[n]; ok {
			results[i].Verdict = v
		}
	}
	return results
}

// failedNames reuses the shared classifier so a re-run's failures are recognised exactly
// as the main run's are, leaks included (and therefore excluded).
func failedNames(output string) []string {
	failures := RealFailures(ClassifyRustFailures(output))
	names := make([]string, 0, len(failures))
	for _, f := range failures {
		names = append(names, f.Name)
	}
	return names
}

// WarnOnly decides whether a red run may be softened to a warn. Contention is proven
// harmless, and inconclusive means the machine was too busy to prove anything, so
// failing on it would just punish the user for running the suite while busy: exactly the
// case this whole mechanism exists to stop mislabelling. A too-slow or real verdict
// keeps the run red.
func WarnOnly(results []ContentionResult) bool {
	if len(results) == 0 {
		return false
	}
	for _, r := range results {
		if r.Verdict != VerdictContention && r.Verdict != VerdictInconclusive {
			return false
		}
	}
	return true
}

// ContentionSummary renders the verdicts for a human or agent reading the check output.
func ContentionSummary(results []ContentionResult, loadAvg float64) string {
	var contention, tooSlow, inconclusive, real []string
	for _, r := range results {
		switch r.Verdict {
		case VerdictContention:
			contention = append(contention, r.Name)
		case VerdictTooSlow:
			tooSlow = append(tooSlow, r.Name)
		case VerdictInconclusive:
			inconclusive = append(inconclusive, r.Name)
		default:
			real = append(real, r.Name)
		}
	}

	var b strings.Builder
	fmt.Fprintf(&b, "%d %s re-run alone (load was %.1f):\n",
		len(results), Pluralize(len(results), "test", "tests"), loadAvg)
	section := func(heading string, names []string) {
		if len(names) == 0 {
			return
		}
		b.WriteString("  • " + heading + "\n")
		for _, n := range names {
			b.WriteString("      - " + n + "\n")
		}
	}
	section(fmt.Sprintf("%d passed alone at the same deadline, so the suite was starving %s: contention, not a defect",
		len(contention), Pluralize(len(contention), "it", "them")), contention)
	section("Needed extra headroom even alone on a quiet machine, so this is real slowness: tweak the test or give it an explicit per-test override",
		tooSlow)
	section("Needed extra headroom, but the machine was still busy during the re-run, so starvation and real slowness can't be told apart. Re-run on a quiet machine to settle it",
		inconclusive)
	section("Still failing alone with headroom: a genuine failure", real)
	return b.String()
}

// ContentionSkippedNote explains why no re-run happened, disclosing the cap so a reader
// never mistakes a bounded look for a full one.
func ContentionSkippedNote(failed int) string {
	return fmt.Sprintf(
		"%d tests failed, past the %d-test contention re-run cap, so no isolated re-run was attempted. "+
			"That many failures at once usually means the machine was too loaded for the run to mean anything; "+
			"re-run the suite on a quieter machine.",
		failed, MaxContentionRerun)
}

// LoadPerCore is the 1-minute load average divided by the core count, the shape the
// busy/quiet judgement is expressed in.
func LoadPerCore() float64 {
	cores := runtime.NumCPU()
	if cores <= 0 {
		return 0
	}
	return LoadAverage() / float64(cores)
}

// LoadAverage returns the 1-minute load average, or 0 when it can't be read.
func LoadAverage() float64 {
	if raw, err := os.ReadFile("/proc/loadavg"); err == nil { // Linux
		if fields := strings.Fields(string(raw)); len(fields) > 0 {
			if v, err := strconv.ParseFloat(fields[0], 64); err == nil {
				return v
			}
		}
	}
	out, err := exec.Command("sysctl", "-n", "vm.loadavg").Output() // macOS: "{ 1.83 2.05 2.11 }"
	if err != nil {
		return 0
	}
	fields := strings.Fields(strings.Trim(strings.TrimSpace(string(out)), "{}"))
	if len(fields) == 0 {
		return 0
	}
	v, err := strconv.ParseFloat(fields[0], 64)
	if err != nil {
		return 0
	}
	return v
}

// NextestFilterExpr builds an exact-match filter for the named tests.
func NextestFilterExpr(names []string) string {
	parts := make([]string, 0, len(names))
	for _, n := range names {
		parts = append(parts, fmt.Sprintf("test(=%s)", n))
	}
	return strings.Join(parts, " + ")
}
