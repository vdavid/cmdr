package checks

import (
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
)

// RunRustTests runs Rust tests using cargo-nextest.
func RunRustTests(ctx *CheckContext) (CheckResult, error) {
	// Every workspace member, not just the app: a member the test lane doesn't
	// select still compiles and still looks green, so its tests silently stop
	// running the moment they move into a crate. `member-coverage` guards this.
	selection, err := HostCargoSelectionArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	// Check if cargo-nextest is installed
	if !CommandExists("cargo-nextest") {
		installCmd := exec.Command("cargo", "install", "cargo-nextest", "--version", "0.9.136", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-nextest: %w", err)
		}
	}

	// `cmdr/virtual-mtp` compiles in the virtual MTP device, which is the only way
	// ~29 MTP tests (backends/mtp_test, mtp_archive_test, mtp_read_range_test,
	// mtp_scan_oracle_tests, connection/path_cache_sync_test) can run at all.
	// Without it they're silently filtered out and protect nothing. The feature is
	// test-only and never enters a production build; it costs ~2-4 s on a ~27 s
	// suite. It MUST stay package-qualified: a bare `--features virtual-mtp`
	// changes meaning once more than one package is selected.
	baseArgs := append([]string{"--locked"}, selection...)
	baseArgs = append(baseArgs, "--features", "cmdr/virtual-mtp")
	cmd := exec.Command("cargo", append([]string{"nextest", "run"}, baseArgs...)...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	// Before the verdict branch, so a red run records WHICH tests went red. Only the
	// first run is recorded: the contention re-run below re-executes a named subset
	// under a different profile, and logging those as extra results would make a
	// starved test look like it ran twice as often as it did.
	ctx.RecordTests(ParseNextestResults(output)...)
	if err != nil {
		// Trim the per-test PASS/SKIP lines (the Linux lane already does): on a 4 800-test
		// suite they bury the diagnosis and the actual panics under thousands of lines.
		// FAIL/LEAK/TIMEOUT/SLOW and every panic body survive.
		return resolveRustFailure("rust tests failed",
			nextestContentionRunner(ctx.RootDir, baseArgs), LoadPerCore, trimRustTestProgress(output))
	}

	// Parse test count from output: "X tests run:"
	re := regexp.MustCompile(`(\d+) tests? run`)
	matches := re.FindStringSubmatch(output)
	message := "All tests passed"
	count := -1
	if len(matches) > 1 {
		count, _ = strconv.Atoi(matches[1])
		message = fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))
	}

	// A retry-rescued run still exits 0. Report it as a warning rather than a pass:
	// the retry budget in `.config/nextest.toml` is a tolerance for real-FSEvents
	// lossiness, not a licence to hide flakes.
	if flaky := ParseFlakyTests(output); len(flaky) > 0 {
		return CheckResult{
			Code:    ResultWarning,
			Message: message + "; " + FlakySummary(flaky),
			Total:   count,
			Issues:  len(flaky),
			Changes: -1,
		}, nil
	}

	result := Success(message)
	result.Total = count
	return result, nil
}

// withFailureDiagnosis prefixes nextest output with the deadline-class breakdown, so a
// reader can tell a cap kill from an in-test `wait_until` expiry without opening a file.
// Falls through to the raw output when nothing is classifiable (a build failure, say).
func withFailureDiagnosis(output string) string {
	diagnosis := DiagnoseRustFailures(ClassifyRustFailures(output))
	if diagnosis == "" {
		return output
	}
	return diagnosis + "\n" + output
}

// resolveRustFailure turns a red run into a verdict. Every failing test is re-run alone,
// first at its original deadline and then with headroom, so starvation by the rest of
// the suite is told apart from real slowness or a real defect. Only an all-contention
// outcome softens the result, and even then to a WARN, never a pass: the re-run must not
// become a silent absorber (that's how the retry budget rotted before it was surfaced).
//
// `run` and `load` are injected because the three Rust lanes re-run in different places:
// the two host lanes shell out to `cargo` here, the Docker lane execs into its still-live
// container. The verdict logic must stay ONE implementation, or a lane quietly grows its
// own idea of what a red run means.
func resolveRustFailure(label string, run ContentionRunner, load LoadSampler, trimmed string) (CheckResult, error) {
	failures := ClassifyRustFailures(trimmed)
	real := RealFailures(failures)
	diagnosis := DiagnoseRustFailures(failures)

	// Nothing classifiable (a build break, a harness problem): report as-is.
	if len(real) == 0 {
		return CheckResult{}, fmt.Errorf("%s\n%s", label, indentOutput(withFailureDiagnosis(trimmed)))
	}

	results, skipped := MaybeClassifyContention(real, run, load)
	if skipped {
		return CheckResult{}, fmt.Errorf("%s\n%s", label,
			indentOutput(diagnosis+"\n"+ContentionSkippedNote(len(real))+"\n\n"+trimmed))
	}

	summary := ContentionSummary(results, LoadAverage())
	if WarnOnly(results) {
		return CheckResult{
			Code:    ResultWarning,
			Message: contentionWarnMessage(results),
			Total:   -1,
			Issues:  len(results),
			Changes: -1,
		}, nil
	}
	return CheckResult{}, fmt.Errorf("%s\n%s", label,
		indentOutput(diagnosis+"\n"+summary+"\n"+trimmed))
}

// contentionWarnMessage is the one line that lands in the check summary, so it has to
// carry the whole story: what failed, and why it isn't being treated as a defect.
func contentionWarnMessage(results []ContentionResult) string {
	var starved, murky []string
	for _, r := range results {
		if r.Verdict == VerdictContention {
			starved = append(starved, r.Name)
		} else {
			murky = append(murky, r.Name)
		}
	}
	parts := make([]string, 0, 2)
	if len(starved) > 0 {
		parts = append(parts, fmt.Sprintf("%d passed alone at the same deadline, so the suite was starving %s (%s)",
			len(starved), Pluralize(len(starved), "it", "them"), strings.Join(starved, ", ")))
	}
	if len(murky) > 0 {
		parts = append(parts, fmt.Sprintf("%d needed headroom while the machine was still busy, so the cause is unsettled (%s)",
			len(murky), strings.Join(murky, ", ")))
	}
	return fmt.Sprintf("%d %s failed under load, none confirmed a defect: %s",
		len(results), Pluralize(len(results), "test", "tests"), strings.Join(parts, "; "))
}

// nextestRanRE matches nextest's end-of-run summary. Its presence means the tests
// actually executed, so a non-zero exit is "tests failed" (expected during a re-run),
// not "cargo couldn't run" (which is a runner error and must not be read as evidence).
var nextestRanRE = regexp.MustCompile(`(?m)^\s*Summary \[`)

// nextestContentionRunner runs a named subset under one of the contention profiles.
func nextestContentionRunner(workDir string, baseArgs []string) ContentionRunner {
	return func(profile string, names []string) (string, error) {
		args := append([]string{"nextest", "run", "--profile", profile}, baseArgs...)
		args = append(args, "-E", NextestFilterExpr(names))
		cmd := exec.Command("cargo", args...)
		cmd.Dir = workDir
		out, err := RunCommand(cmd, true)
		if err != nil && !nextestRanRE.MatchString(out) {
			return "", fmt.Errorf("contention re-run under profile %s could not run: %w", profile, err)
		}
		return out, nil
	}
}
