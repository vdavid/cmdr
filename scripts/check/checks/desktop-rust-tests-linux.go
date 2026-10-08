package checks

import (
	"context"
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
)

// containerNextestVersion pins the container's cargo-nextest to the same version the host
// lanes install. Two reasons it can't be `latest`: the "pin every tool install" rule, and
// the contention re-run, whose profile semantics (a per-test override beating a
// profile-level `slow-timeout`) were verified against this exact version. A container
// silently drifting to a newer nextest would classify starvation differently from the host
// lanes with nothing to say it had.
const containerNextestVersion = NextestVersion

// containerNextestScript builds a `cargo nextest run` for the container, so the main run
// and the contention re-run can't drift apart in selection, `--locked`, or quoting.
func containerNextestScript(args ...string) string {
	return containerCargoScript(append([]string{"nextest", "run"}, args...)...)
}

// RunRustTestsLinux runs Rust tests in a Linux Docker container.
// This catches platform-specific issues before CI.
//
// The container runs from the provisioned image and builds into this checkout's target
// volume (`desktop-rust-linux-container.go`), so a warm run compiles only what changed.
// It's started detached and each phase (test run, contention re-run) is a `docker exec`
// into it, rather than one `docker run sh -c <everything>`. That's what lets a red run
// re-run its failures alone the way the host lanes do: the re-run lands in the same
// container, so it costs seconds. The container is removed on every exit path.
func RunRustTestsLinux(ctx *CheckContext) (CheckResult, error) {
	if skip, unavailable := dockerUnavailable(); unavailable {
		return skip, nil
	}

	selection, err := linuxSelectionArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	env, err := prepareLinuxContainer(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	container := linuxContainerName("tests")
	if err := startLinuxContainer(container, ctx.RootDir, env); err != nil {
		return CheckResult{}, err
	}
	defer removeLinuxContainer(container)

	testArgs := append([]string{"--locked"}, selection...)
	testArgs = append(testArgs, "--no-fail-fast")
	output, err := dockerExec(container, containerNextestScript(testArgs...))
	// Before the verdict branch, so a red run records WHICH tests went red
	// (`test-log.go`); the contention re-run below is deliberately not recorded.
	ctx.RecordTests(ParseNextestResults(output)...)
	if err != nil {
		// Same verdict machinery as the host lanes: a failure is re-run alone before it's
		// believed. This lane needs it MORE, not less. The container's cores are a slice of
		// a host that may be running three Playwright shards and four cargo processes, so a
		// starved test here looks exactly like a hung one. Measured 2026-07-30: three
		// consecutive runs of an unchanged tree each timed out on a DIFFERENT small set,
		// and one of them (`sqlite_util::tests::cached_pages_come_from_the_shared_slab`)
		// runs in 0.07 s natively.
		summary := trimRustTestProgress(trimBuildNoise(output))
		return resolveRustFailure(
			"rust tests failed on Linux"+env.buildNote(),
			dockerContentionRunner(container, selection),
			dockerLoadSampler(container),
			summary)
	}

	// Same retry-rescued case as the macOS lane: nextest exits 0, so without this the
	// Linux lane reports green while hiding the flake.
	if flaky := ParseFlakyTests(output); len(flaky) > 0 {
		return CheckResult{
			Code:    ResultWarning,
			Message: fmt.Sprintf("All tests passed on Linux; %s%s", FlakySummary(flaky), env.buildNote()),
			Total:   -1,
			Issues:  len(flaky),
			Changes: -1,
		}, nil
	}
	return Success("All tests passed on Linux" + env.buildNote()), nil
}

// dockerContentionRunner re-runs the named tests inside the SAME container the failing run
// used, under one of the contention profiles. Reusing the container is what makes the
// re-run affordable: nothing recompiles and only the named tests execute.
//
// The package selection is carried over verbatim. A re-run that selected differently could
// find no tests at all and read as "everything passed alone", which is the failure mode
// that would turn every real Linux failure into a warn.
func dockerContentionRunner(container string, selection []string) ContentionRunner {
	return func(profile string, names []string) (string, error) {
		out, err := dockerExec(container, containerNextestScript(containerRerunArgs(profile, selection, names)...))
		out = StripANSI(out)
		if err != nil && !nextestRanRE.MatchString(out) {
			return "", fmt.Errorf("contention re-run under profile %s could not run in the container: %w", profile, err)
		}
		return out, nil
	}
}

// containerRerunArgs builds one contention stage's `cargo nextest run` arguments. Pure, so
// the selection-parity and profile contract is testable without a container.
func containerRerunArgs(profile string, selection, names []string) []string {
	args := append([]string{"--locked", "--profile", profile}, selection...)
	return append(args, "-E", NextestFilterExpr(names))
}

// dockerLoadSampler answers "was the machine quiet during the re-run?" from both sides of
// the VM boundary, and takes the worse of the two.
//
// Neither number alone is enough. On macOS the host's load average sees the Linux VM as a
// handful of vCPU threads, so a container saturated from the inside (this suite, plus the
// second container the `--include-slow` lane runs) barely moves it. The container's own
// `/proc/loadavg` sees that, but is blind to the Playwright shards and cargo processes
// outside the VM that are actually competing for the same cores. Either side being busy
// means the re-run wasn't quiet.
//
// This only ever decides whether a "needed headroom" verdict is reported as real slowness
// or as inconclusive. An unreadable load reads as 0 (quiet), which is the safe direction:
// it keeps the run red rather than softening it.
func dockerLoadSampler(container string) LoadSampler {
	return func() float64 {
		return max(LoadPerCore(), containerLoadPerCore(container))
	}
}

func containerLoadPerCore(container string) float64 {
	loadCtx, cancel := context.WithTimeout(context.Background(), dockerControlTimeout)
	defer cancel()
	cmd := exec.CommandContext(loadCtx, "docker", "exec", container, "sh", "-c", "cat /proc/loadavg; nproc")
	out, err := RunCommand(cmd, true)
	if err != nil {
		return 0
	}
	return parseContainerLoadPerCore(out)
}

// parseContainerLoadPerCore reads `cat /proc/loadavg; nproc` output: the 1-minute load
// average from the first line's first field, the core count from the second line. Anything
// unparseable reports 0 rather than a guess.
func parseContainerLoadPerCore(out string) float64 {
	lines := strings.Split(strings.TrimSpace(out), "\n")
	if len(lines) < 2 {
		return 0
	}
	fields := strings.Fields(lines[0])
	if len(fields) == 0 {
		return 0
	}
	load, err := strconv.ParseFloat(fields[0], 64)
	if err != nil {
		return 0
	}
	cores, err := strconv.Atoi(strings.TrimSpace(lines[len(lines)-1]))
	if err != nil || cores <= 0 {
		return 0
	}
	return load / float64(cores)
}

var compilingLineRe = regexp.MustCompile(`(?m)^\s*Compiling \w+ v`)

// trimBuildNoise drops cargo's pre-test build chatter by keeping everything
// after the last `Compiling …` line. If no Compiling line exists (nothing
// needed rebuilding, or the failure came before cargo got that far), the
// output is returned as-is. Provisioning happens at image build time, so no
// apt chatter ever reaches a test exec.
//
// Nothing is ever truncated by length: if the test run produces 500 lines of
// real failures, all 500 survive.
func trimBuildNoise(output string) string {
	if locs := compilingLineRe.FindAllStringIndex(output, -1); len(locs) > 0 {
		lastEnd := locs[len(locs)-1][1]
		if nl := strings.IndexByte(output[lastEnd:], '\n'); nl >= 0 {
			if trimmed := strings.TrimLeft(output[lastEnd+nl+1:], "\n"); trimmed != "" {
				return trimmed
			}
		}
	}
	return output
}

// testProgressNoiseRE matches per-test pass/skip lines that are pure noise on
// a failure. Two formats are recognised:
//
//	cargo test    `test foo::bar ... ok`
//	              `test foo::bar ... ignored, <reason>`
//	cargo nextest `        PASS [   0.001s] cmdr_lib foo::bar`
//	              `        SKIP [   0.001s] cmdr_lib foo::bar`
//	              `        PASS [   0.001s] cmdr_lib foo::bar (reason)`
//	              `        PASS [   0.094s] (  42/4802) cmdr_lib foo::bar`
//
// The progress counter is its own optional group because nextest right-aligns the
// index to the total's width, so it can hold spaces (`(  42/4802)`). Folding it
// into the binary field instead let exactly the 1-99 range slip through the filter.
//
// Anchored to the start of the line (with optional leading whitespace for the
// nextest form) so panic-message bodies that quote these phrases can't be
// misclassified. FAIL/LEAK/TIMEOUT/SLOW/bench results and every non-test line
// fall through unchanged.
var testProgressNoiseRE = regexp.MustCompile(
	`^(?:test .+ \.\.\. (?:ok|ignored(?:, .*)?)|\s+(?:PASS|SKIP) \[[^\]]*\]\s+(?:\([^)]*\)\s+)?\S+ \S.*)$`,
)

// trimRustTestProgress drops `test … ok` / `test … ignored…` / nextest
// `PASS [...]` and `SKIP [...]` lines from cargo test or cargo nextest
// output. Everything else is kept verbatim: `running N tests` headers,
// FAIL/FAILED markers, the `failures:` block (panic stdout + listing), the
// `test result:` / `Summary` tally, `error:` lines, and any other text.
//
// The filter is single-pass and per-line, so it survives weird interleaving
// (multiple test binaries, multi-line panic messages, debconf noise after the
// suite exits) and can only ever keep too much, never drop a real signal.
func trimRustTestProgress(output string) string {
	// Normalise first: nextest colours its output under a forced-colour environment, and
	// every pattern here is line-anchored, so an unnormalised buffer keeps all ~6 000
	// progress lines and buries the diagnosis under them.
	lines := strings.Split(StripANSI(output), "\n")
	kept := make([]string, 0, len(lines))
	for _, line := range lines {
		if testProgressNoiseRE.MatchString(line) {
			continue
		}
		kept = append(kept, line)
	}
	return strings.Join(kept, "\n")
}
