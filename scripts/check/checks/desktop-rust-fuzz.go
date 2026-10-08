package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

// cargoFuzzVersion is the cargo-fuzz this lane installs when the one on PATH
// differs. Pinned like every other tool install (`checks/CLAUDE.md`).
const cargoFuzzVersion = "0.13.2"

// defaultFuzzSeconds is each target's run time when `CMDR_FUZZ_SECONDS` is unset:
// a smoke that drives every target past its seeds, not a hunt. Longer hunts set it.
const defaultFuzzSeconds = 60

// fuzzTargetOverride loosens one target's limits for a documented, known upstream
// finding. Keep the map empty otherwise.
type fuzzTargetOverride struct {
	// libFuzzerArgs go after the shared flags.
	libFuzzerArgs []string
	// asanOptions is set as `ASAN_OPTIONS`; cargo-fuzz appends its own to it.
	asanOptions string
}

// fuzzTargetOverrides: `archive_index`, because `lzma-rust2` 0.21.0 reserves an
// xz index's record count before checking it, so 23 bytes reserve gigabytes of
// address space that are never touched and are freed on the error that follows.
// Lifting the per-allocation cap, and letting an impossible size come back null the
// way the system allocator does (`try_reserve_exact` then errors cleanly), stops
// counting that reservation; the default RSS limit (2 GiB) still fails any real
// memory use. Drop it once `lzma-rust2` caps the reservation (`fuzz/DETAILS.md` §
// Findings).
var fuzzTargetOverrides = map[string]fuzzTargetOverride{
	"archive_index": {
		libFuzzerArgs: []string{"-malloc_limit_mb=1048576"},
		asanOptions:   "allocator_may_return_null=1",
	},
}

// RunFuzz builds the targets in `fuzz/` on the pinned nightly, then runs each one
// for `CMDR_FUZZ_SECONDS` (default 60) over its committed seeds plus a local,
// gitignored corpus that grows run to run. A crash, OOM, or hang fails the lane
// with the fuzzer's report and the artifact path; `fuzz/DETAILS.md` has the
// reproduce-and-fix loop.
func RunFuzz(ctx *CheckContext) (CheckResult, error) {
	fuzzDir := filepath.Join(ctx.RootDir, "fuzz")
	seconds := defaultFuzzSeconds
	if raw := os.Getenv("CMDR_FUZZ_SECONDS"); raw != "" {
		parsed, err := strconv.Atoi(raw)
		if err != nil || parsed <= 0 {
			return CheckResult{}, fmt.Errorf("CMDR_FUZZ_SECONDS must be a positive whole number of seconds, got %q", raw)
		}
		seconds = parsed
	}

	if err := ensureNightlyToolchain(); err != nil {
		return CheckResult{}, err
	}
	if err := ensureCargoFuzz(); err != nil {
		return CheckResult{}, err
	}

	// `-O -a`: optimized, with debug assertions (so overflow checks) on. Every
	// command passes the same pair, or `run` rebuilds what `build` just built.
	fuzzArgs := func(args ...string) []string {
		head := []string{"+" + nightlyToolchain, "fuzz"}
		return append(head, args...)
	}
	buildCmd := exec.Command("cargo", fuzzArgs("build", "-O", "-a")...)
	buildCmd.Dir = fuzzDir
	if output, err := RunCommand(buildCmd, true); err != nil {
		return CheckResult{}, fmt.Errorf("fuzz targets don't build\n%s", indentOutput(output))
	}

	listCmd := exec.Command("cargo", fuzzArgs("list")...)
	listCmd.Dir = fuzzDir
	listed, err := RunCommand(listCmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list fuzz targets\n%s", indentOutput(listed))
	}
	targets := strings.Fields(listed)
	if len(targets) == 0 {
		return CheckResult{}, fmt.Errorf("`cargo fuzz list` found no targets in %s", fuzzDir)
	}

	var failures []string
	for _, target := range targets {
		corpus := filepath.Join("corpus", target)
		if err := os.MkdirAll(filepath.Join(fuzzDir, corpus), 0o755); err != nil {
			return CheckResult{}, fmt.Errorf("failed to create %s: %w", corpus, err)
		}
		args := fuzzArgs(
			"run", "-O", "-a", target, corpus, filepath.Join("seeds", target), "--",
			"-max_total_time="+strconv.Itoa(seconds),
			// A unit past 20 s is a hang: the slowest seed parses in milliseconds.
			"-timeout=20",
		)
		override := fuzzTargetOverrides[target]
		runCmd := exec.Command("cargo", append(args, override.libFuzzerArgs...)...)
		runCmd.Dir = fuzzDir
		if override.asanOptions != "" {
			runCmd.Env = append(os.Environ(), "ASAN_OPTIONS="+override.asanOptions)
		}
		if output, err := RunCommand(runCmd, true); err != nil {
			failures = append(failures, fmt.Sprintf("%s:\n%s", target, indentOutput(fuzzFindingReport(output))))
		}
	}
	if len(failures) > 0 {
		return CheckResult{}, fmt.Errorf("%d of %d fuzz targets found something (reproduce: `fuzz/DETAILS.md`)\n%s",
			len(failures), len(targets), strings.Join(failures, "\n"))
	}
	return Success(fmt.Sprintf("%d fuzz targets ran %d s each, no findings", len(targets), seconds)), nil
}

// fuzzFindingReport keeps the part of a failed run that says what was found: from
// the first report line (libFuzzer's or a panic's) to libFuzzer's `SUMMARY`, plus
// the line naming the saved input. The mutation log before it is noise. Without a
// report line (a build or tooling failure), the whole output passes through.
func fuzzFindingReport(output string) string {
	lines := strings.Split(output, "\n")
	start := -1
	for i, line := range lines {
		if strings.Contains(line, "==ERROR:") || strings.Contains(line, "ERROR: libFuzzer") ||
			strings.Contains(line, "panicked at") || strings.Contains(line, "stack overflow") {
			start = i
			break
		}
	}
	if start < 0 {
		return output
	}
	var kept []string
	for _, line := range lines[start:] {
		kept = append(kept, line)
		if strings.HasPrefix(line, "SUMMARY:") {
			break
		}
	}
	for _, line := range lines {
		if strings.Contains(line, "Test unit written to") {
			kept = append(kept, line)
		}
	}
	return strings.Join(kept, "\n")
}

// ensureCargoFuzz installs the pinned cargo-fuzz unless that exact version is on
// PATH. `cargo fuzz --version` prints `cargo-fuzz <version>`.
func ensureCargoFuzz() error {
	if CommandExists("cargo-fuzz") {
		output, err := RunCommand(exec.Command("cargo", "fuzz", "--version"), true)
		if err == nil && strings.TrimSpace(output) == "cargo-fuzz "+cargoFuzzVersion {
			return nil
		}
	}
	installCmd := exec.Command("cargo", "install", "cargo-fuzz", "--version", cargoFuzzVersion, "--locked")
	if output, err := RunCommand(installCmd, true); err != nil {
		return fmt.Errorf("failed to install cargo-fuzz %s\n%s", cargoFuzzVersion, indentOutput(output))
	}
	return nil
}
