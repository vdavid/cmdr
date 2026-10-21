package checks

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"time"
)

// A host Rust test lane asks cargo exactly once. `buildNextestLane` builds the tests with
// `cargo nextest list --list-type binaries-only`, which records the binaries it produced in
// a binaries-metadata file. The main run and every contention re-run then execute those
// binaries with `--binaries-metadata` and no cargo options at all, so a re-run can't
// rebuild, whatever changed on disk after the build.
//
// Why it matters: a re-run that asks cargo again rebuilds whatever it finds dirty. On CI's
// Linux runner it found `cmdr` dirty every time, so re-running one 1-second test cost 1m37s
// of compile; from the build it takes 2 s (#619). It's also the honest re-run: it executes
// the very binaries that failed, not a fresh build of whatever the tree holds now.
//
// nextest refuses cargo options next to `--binaries-metadata` (they conflict at the CLI), so
// a cargo flag smuggled into a run fails loudly instead of quietly rebuilding.

// nextestRunOptions are the run-time choices a lane makes. Deliberately not a []string:
// nothing that reaches cargo is representable here. The cargo question every host lane asks
// is fixed in `buildNextestLane`.
type nextestRunOptions struct {
	// RunIgnoredOnly selects the `#[ignore]`-gated tests (the fixture and disk-image
	// lanes). A re-run carries it too, or it would select nothing and read as "everything
	// passed alone".
	RunIgnoredOnly bool
}

// nextestLane is one lane's built test suite.
type nextestLane struct {
	rootDir          string
	binariesMetadata string
	opts             nextestRunOptions
	// exec runs `cargo <argv>` in rootDir; injected so the argv contract is testable.
	exec func(argv []string) (string, error)
}

// buildNextestLane compiles the lane's tests with the same cargo question every host Rust
// lane asks (`--locked` plus `HostCargoLaneArgs`), so the lanes share one warm `target/`.
// It returns cargo's build output, which the caller reports on a broken build. The caller
// owns the lane and must `remove` it.
func buildNextestLane(rootDir string, opts nextestRunOptions) (lane *nextestLane, buildOutput string, err error) {
	laneArgs, err := HostCargoLaneArgs(rootDir)
	if err != nil {
		return nil, "", err
	}
	if err := EnsureCargoNextest(); err != nil {
		return nil, "", err
	}

	metadata, err := os.CreateTemp("", "cmdr-nextest-binaries-*.json")
	if err != nil {
		return nil, "", fmt.Errorf("couldn't create the binaries-metadata file: %w", err)
	}
	lane = &nextestLane{rootDir: rootDir, binariesMetadata: metadata.Name(), opts: opts, exec: cargoExec(rootDir)}

	// stdout is the binaries-metadata JSON, straight into the file; stderr is cargo's build.
	var stderr bytes.Buffer
	cmd := exec.Command("cargo", nextestBuildArgv(laneArgs)...)
	cmd.Dir = rootDir
	err = RunCommandTo(cmd, metadata, &stderr)
	closeErr := metadata.Close()
	buildOutput = StripANSI(stderr.String())
	if err == nil {
		err = closeErr
	}
	if err != nil {
		lane.remove()
		return nil, buildOutput, err
	}
	return lane, buildOutput, nil
}

// nextestBuildArgv is the one cargo-facing invocation of a host test lane.
func nextestBuildArgv(laneArgs []string) []string {
	argv := []string{"nextest", "list", "--list-type", "binaries-only", "--message-format", "json", "--locked"}
	return append(argv, laneArgs...)
}

// runArgv builds a `nextest run` over the lane's built binaries. `profile` is empty for the
// main run (nextest's default profile); `filter` is empty for no `-E`.
func (l *nextestLane) runArgv(profile, filter string) []string {
	argv := []string{"nextest", "run"}
	if profile != "" {
		argv = append(argv, "--profile", profile)
	}
	argv = append(argv, "--binaries-metadata", l.binariesMetadata)
	if l.opts.RunIgnoredOnly {
		argv = append(argv, "--run-ignored", "only")
	}
	if filter != "" {
		argv = append(argv, "-E", filter)
	}
	return argv
}

// run executes the lane's tests under nextest's default profile.
func (l *nextestLane) run(filter string) (string, error) {
	out, err := l.exec(l.runArgv("", filter))
	return StripANSI(out), err
}

// contentionRunner re-runs named tests from the same binaries under a contention profile.
func (l *nextestLane) contentionRunner() ContentionRunner {
	return func(profile string, names []string) (string, error) {
		out, err := l.exec(l.runArgv(profile, NextestFilterExpr(names)))
		out = StripANSI(out)
		if err != nil && !nextestRanRE.MatchString(out) {
			return "", fmt.Errorf("contention re-run under profile %s could not run: %w", profile, err)
		}
		return out, nil
	}
}

// remove deletes the binaries-metadata file. The binaries themselves stay in `target/`.
// A no-op on a nil lane, so callers can defer it before checking the build.
func (l *nextestLane) remove() {
	if l == nil {
		return
	}
	_ = os.Remove(l.binariesMetadata)
}

func cargoExec(rootDir string) func(argv []string) (string, error) {
	return func(argv []string) (string, error) {
		cmd := exec.Command("cargo", argv...)
		cmd.Dir = rootDir
		return RunCommand(cmd, true)
	}
}

// runNextestLane builds a lane and runs it, returning the build and run output together
// (in that order, so the timing profile's "building and listing" split still holds) and how
// long both took. A lane that couldn't be built comes back as a ready-to-return error with
// a nil lane; otherwise the caller owns the lane and must `remove` it, and `err` is the
// test run's own.
func runNextestLane(rootDir string, opts nextestRunOptions, filter string) (lane *nextestLane, output string, wall time.Duration, err error) {
	started := time.Now()
	lane, buildOutput, err := buildNextestLane(rootDir, opts)
	if err != nil {
		return nil, "", time.Since(started), fmt.Errorf("couldn't build the Rust tests (%w)\n%s",
			err, indentOutput(trimRustTestProgress(buildOutput)))
	}
	runOutput, err := lane.run(filter)
	return lane, buildOutput + runOutput, time.Since(started), err
}
