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

// fuzzTargetOverrides: empty. Every target runs with the default per-allocation
// and RSS limits.
var fuzzTargetOverrides = map[string]fuzzTargetOverride{}

// fuzzTargetMaxLen sets `-max_len` for the targets whose deep code needs inputs
// bigger than their seeds. Without it, libFuzzer caps inputs at its largest
// starting input (4 KiB at least), which no bug that needs megabytes gets past.
// Each target here has large seeds shaped to reach that code (`fuzz/DETAILS.md`
// § "Large inputs"). libFuzzer still starts small and grows lengths as coverage
// pays (`-len_control`), so a large cap costs little until the corpus earns it.
var fuzzTargetMaxLen = map[string]int{
	// A stored block at the format's 4 MiB maximum, plus its frame.
	"adb_lz4": 4<<20 + 64<<10,
	// A file past the target's 256 KiB read in each format, and trees of
	// hundreds of entries, uncompressed.
	"archive_index": 4<<20 + 64<<10,
	// A READ answering past the client's 1 MiB `MAX_PAYLOAD`, plus its headers.
	"nfs_reply": 1<<20 + 4<<10,
}

// fuzzCorpusBudget is the size past which a target's grown corpus is
// minimized after its run (`cargo fuzz cmin`), so the CI cache that carries it
// from run to run stays small under GitHub's 10 GB ceiling.
const fuzzCorpusBudget = 32 << 20

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

	buildCmd := cargoFuzzCmd(fuzzDir, "build", "-O", "-a")
	if output, err := RunCommand(buildCmd, true); err != nil {
		return CheckResult{}, fmt.Errorf("fuzz targets don't build\n%s", indentOutput(output))
	}

	listed, err := RunCommand(cargoFuzzCmd(fuzzDir, "list"), true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to list fuzz targets\n%s", indentOutput(listed))
	}
	targets := strings.Fields(listed)
	if len(targets) == 0 {
		return CheckResult{}, fmt.Errorf("`cargo fuzz list` found no targets in %s", fuzzDir)
	}

	var failures []string
	for _, target := range targets {
		finding, err := runFuzzTarget(fuzzDir, target, seconds)
		if err != nil {
			return CheckResult{}, err
		}
		if finding != "" {
			failures = append(failures, fmt.Sprintf("%s:\n%s", target, indentOutput(finding)))
		}
	}
	if len(failures) > 0 {
		return CheckResult{}, fmt.Errorf("%d of %d fuzz targets found something (reproduce: `fuzz/DETAILS.md`)\n%s",
			len(failures), len(targets), strings.Join(failures, "\n"))
	}
	return Success(fmt.Sprintf("%d fuzz targets ran %d s each, no findings", len(targets), seconds)), nil
}

// cargoFuzzCmd is `cargo +<nightly> fuzz <args>` in fuzzDir. Pass `-O -a`
// (optimized, with debug assertions, so overflow checks, on) to every command
// that builds, or `run` rebuilds what `build` just built.
func cargoFuzzCmd(fuzzDir string, args ...string) *exec.Cmd {
	cmd := exec.Command("cargo", append([]string{"+" + nightlyToolchain, "fuzz"}, args...)...)
	cmd.Dir = fuzzDir
	return cmd
}

// runFuzzTarget runs one target for `seconds` over its grown corpus, its seeds,
// and its unpacked large seeds, then minimizes the corpus once it's past
// fuzzCorpusBudget. finding is the fuzzer's report when it found something.
func runFuzzTarget(fuzzDir, target string, seconds int) (finding string, err error) {
	corpus := filepath.Join("corpus", target)
	if err := os.MkdirAll(filepath.Join(fuzzDir, corpus), 0o755); err != nil {
		return "", fmt.Errorf("failed to create %s: %w", corpus, err)
	}
	// New finds go to the first dir; the committed seeds are read only.
	corpusDirs := []string{corpus, filepath.Join("seeds", target)}
	largeSeeds, err := unpackLargeSeeds(fuzzDir, target)
	if err != nil {
		return "", err
	}
	if largeSeeds != "" {
		corpusDirs = append(corpusDirs, largeSeeds)
	}
	var lengthArgs []string
	if maxLen, ok := fuzzTargetMaxLen[target]; ok {
		lengthArgs = []string{"-max_len=" + strconv.Itoa(maxLen)}
	}
	override := fuzzTargetOverrides[target]
	args := append([]string{"run", "-O", "-a", target}, corpusDirs...)
	args = append(args, "--",
		"-max_total_time="+strconv.Itoa(seconds),
		// A unit past 20 s is a hang: the slowest seed parses in milliseconds.
		"-timeout=20",
	)
	args = append(append(args, lengthArgs...), override.libFuzzerArgs...)
	runCmd := cargoFuzzCmd(fuzzDir, args...)
	if override.asanOptions != "" {
		runCmd.Env = append(os.Environ(), "ASAN_OPTIONS="+override.asanOptions)
	}
	if output, err := RunCommand(runCmd, true); err != nil {
		if report := fuzzFindingReport(output); strings.TrimSpace(report) != "" {
			return report, nil
		}
		return err.Error(), nil
	}
	if dirSize(filepath.Join(fuzzDir, corpus)) > fuzzCorpusBudget {
		cminCmd := cargoFuzzCmd(fuzzDir, append([]string{"cmin", "-O", "-a", target, corpus, "--"}, lengthArgs...)...)
		if output, err := RunCommand(cminCmd, true); err != nil {
			return "", fmt.Errorf("couldn't minimize the %s corpus\n%s", target, indentOutput(output))
		}
	}
	return "", nil
}

// unpackLargeSeeds writes each `fuzz/large-seeds/<target>/*.gz`, decompressed,
// to `fuzz/target/large-seeds/<target>/` and returns that dir relative to
// `fuzzDir`, or "" when the target has no large seeds. They live gzipped so a
// multi-megabyte seed costs git kilobytes; unpacked under `target/`, they're
// rebuilt from the committed copies on every run and never committed.
func unpackLargeSeeds(fuzzDir, target string) (string, error) {
	sources, err := filepath.Glob(filepath.Join(fuzzDir, "large-seeds", target, "*.gz"))
	if err != nil || len(sources) == 0 {
		return "", err
	}
	rel := filepath.Join("target", "large-seeds", target)
	dir := filepath.Join(fuzzDir, rel)
	if err := os.RemoveAll(dir); err != nil {
		return "", err
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	for _, source := range sources {
		unpacked, cleanup, err := materializeFuzzInput(source)
		if err != nil {
			return "", err
		}
		data, err := os.ReadFile(unpacked)
		cleanup()
		if err != nil {
			return "", err
		}
		name := strings.TrimSuffix(filepath.Base(source), ".gz")
		if err := os.WriteFile(filepath.Join(dir, name), data, 0o644); err != nil {
			return "", err
		}
	}
	return rel, nil
}

// dirSize is the total size of the regular files directly in dir (a libFuzzer
// corpus is flat); 0 when it can't be read.
func dirSize(dir string) int64 {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return 0
	}
	var total int64
	for _, entry := range entries {
		if info, err := entry.Info(); err == nil && info.Mode().IsRegular() {
			total += info.Size()
		}
	}
	return total
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
