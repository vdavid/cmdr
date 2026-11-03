package checks

import (
	"compress/gzip"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
)

// fuzzPropertyKind: every target in `fuzz/` is a property. Its catcher is one
// input, run once through the target the way `fuzz/DETAILS.md` reproduces a
// finding, so a record is deterministic where a fuzzing run would not be.
var fuzzPropertyKind = propertyKind{
	name:        "fuzz",
	properties:  fuzzTargetNames,
	checkRecord: checkFuzzMutantRecord,
	prepare: func() error {
		if err := ensureNightlyToolchain(); err != nil {
			return err
		}
		return ensureCargoFuzz()
	},
	run: runFuzzMutantCatcher,
}

// fuzzBinNamePattern reads a `[[bin]]` table's `name = "…"` line.
var fuzzBinNamePattern = regexp.MustCompile(`^name\s*=\s*"([^"]+)"`)

// fuzzTargetNames reads the target names from `fuzz/Cargo.toml`, the list
// `cargo fuzz list` prints, without needing the nightly to ask it.
func fuzzTargetNames(rootDir string) ([]string, error) {
	data, err := os.ReadFile(filepath.Join(rootDir, "fuzz", "Cargo.toml"))
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	var names []string
	inBin := false
	for line := range strings.SplitSeq(string(data), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "[") {
			inBin = line == "[[bin]]"
			continue
		}
		if m := fuzzBinNamePattern.FindStringSubmatch(line); inBin && m != nil {
			names = append(names, m[1])
			inBin = false
		}
	}
	return names, nil
}

// checkFuzzMutantRecord: a fuzz record names the input that reaches the
// property.
func checkFuzzMutantRecord(rootDir string, r mutantRecord) []string {
	if r.Input == "" {
		return []string{"`input` is empty; a fuzz record names the input its target fails on"}
	}
	if !isRepoRelative(r.Input) {
		return []string{fmt.Sprintf("`input` %q isn't a repo-relative path", r.Input)}
	}
	info, err := os.Stat(filepath.Join(rootDir, filepath.FromSlash(r.Input)))
	if err != nil || !info.Mode().IsRegular() {
		return []string{fmt.Sprintf("`input` %s isn't a file", r.Input)}
	}
	return nil
}

// runFuzzMutantCatcher builds the record's target in tree's `fuzz/` and runs
// its input through it once. A non-zero exit after a good build is the target
// going red: a panic, a sanitizer report, or `-timeout` for a hang.
func runFuzzMutantCatcher(tree string, r mutantRecord) (mutantRun, error) {
	_, target := r.kindAndName()
	fuzzDir := filepath.Join(tree, "fuzz")
	cargoFuzz := func(args ...string) *exec.Cmd {
		// `-O -a`, like the fuzz lane, so a debug assertion counts here too.
		cmd := exec.Command("cargo", append([]string{"+" + nightlyToolchain, "fuzz"}, args...)...)
		cmd.Dir = fuzzDir
		return cmd
	}
	if output, err := RunCommand(cargoFuzz("build", "-O", "-a", target), true); err != nil {
		return mutantRun{}, fmt.Errorf("the target doesn't build\n%s", indentOutput(trimBuildNoise(output)))
	}

	input, cleanup, err := materializeFuzzInput(filepath.Join(tree, filepath.FromSlash(r.Input)))
	if err != nil {
		return mutantRun{}, err
	}
	defer cleanup()
	output, err := RunCommand(cargoFuzz("run", "-O", "-a", target, input, "--", "-timeout=20"), true)
	// A red run saves its input under `artifacts/`; the record already has it.
	_ = os.RemoveAll(filepath.Join(fuzzDir, "artifacts", target))
	var exitErr *exec.ExitError
	switch {
	case err == nil:
		return mutantRun{output: output}, nil
	case errors.As(err, &exitErr):
		return mutantRun{failed: true, output: fuzzFindingReport(output)}, nil
	default:
		return mutantRun{}, fmt.Errorf("couldn't run the target: %w", err)
	}
}

// materializeFuzzInput returns a path libFuzzer can read: the input itself, or
// a decompressed copy of a `.gz` one, which is how an input too big to commit
// raw (a READ past 1 MiB, say) stays small in git.
func materializeFuzzInput(path string) (string, func(), error) {
	if !strings.HasSuffix(path, ".gz") {
		return path, func() {}, nil
	}
	compressed, err := os.Open(path)
	if err != nil {
		return "", nil, err
	}
	defer compressed.Close()
	reader, err := gzip.NewReader(compressed)
	if err != nil {
		return "", nil, fmt.Errorf("%s: %w", path, err)
	}
	out, err := os.CreateTemp("", "cmdr-mutant-input-*")
	if err != nil {
		return "", nil, err
	}
	cleanup := func() { _ = os.Remove(out.Name()) }
	if _, err := io.Copy(out, reader); err != nil {
		_ = out.Close()
		cleanup()
		return "", nil, fmt.Errorf("%s: %w", path, err)
	}
	if err := out.Close(); err != nil {
		cleanup()
		return "", nil, err
	}
	return out.Name(), cleanup, nil
}
