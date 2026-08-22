package checks

import (
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// RunRustIntegrationTests runs the Docker-backed Rust integration tests for
// every network fixture. The lane's selection expression is built by
// `fixtureIntegrationFilter` (see `fixture-lane-coverage.go`), which also holds
// the fixture table `desktop-fixture-lane-coverage` guards.
//
// Container lifecycle: managed by the runner-level SMB orchestrator (see
// scripts/check/stack_orchestrator.go). This check declares its fixture stacks in
// `NeedsContainers` in the registry, so the containers are guaranteed up by the
// time this function runs and they survive past it (no per-check `defer ./stop.sh`).
// The old defer broke parallel runs by tearing down containers
// `desktop-e2e-linux` was still using.
//
// Flow:
//  1. Containers are already up (orchestrator started them at runner init).
//     We still wait until the expected services report `running` as a
//     cheap guard against mid-run zombies; smb2 reconnects if the server
//     isn't ready on the first write.
//  2. Invoke `cargo nextest run --workspace --run-ignored only -E <fixture filter>`
//     (debug, reusing desktop-rust-tests' build) from the repo root. The expression
//     filter matches every fixture cell and skips other `#[ignore]` tests.
func RunRustIntegrationTests(ctx *CheckContext) (CheckResult, error) {
	// Docker is a hard requirement. Surface a clear message instead of a cryptic error.
	if !CommandExists("docker") {
		return CheckResult{}, fmt.Errorf(
			"docker is required for SMB integration tests; install Docker or run without this check",
		)
	}
	if _, err := RunCommand(exec.Command("docker", "info"), true); err != nil {
		return CheckResult{}, fmt.Errorf(
			"docker daemon is not running; start Docker or run without this check",
		)
	}

	// Workspace-wide; the filter expression is what narrows the run, not the
	// package selection. The features come along for the ride even though no SMB
	// test needs them: asking cargo a different question than `desktop-rust-tests`
	// does would make the two lanes rebuild `cmdr` for each other, every run.
	laneArgs, err := HostCargoLaneArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	// Wait for the core services to be running (the orchestrator started them,
	// but they may still be transitioning to `running` when this check
	// kicks off). We don't require `healthy` here because these images don't
	// all ship healthchecks.
	expected := []string{
		"smb-consumer-guest",
		"smb-consumer-auth",
		"smb-consumer-both",
		"smb-consumer-readonly",
		"smb-consumer-flaky",
		"smb-consumer-slow",
		"smb-consumer-50shares",
		// The only fixture with non-ASCII share names, and so the only one that can
		// catch a regression in the escaping macOS's SMB stack requires of every
		// mount URL (`network/mount.rs::build_smb_mount_url`).
		"smb-consumer-unicode",
	}
	if err := waitForSmbContainers(expected, 120*time.Second); err != nil {
		return CheckResult{}, err
	}

	// Make sure cargo-nextest is available (mirrors desktop-rust-tests.go).
	if !CommandExists("cargo-nextest") {
		installCmd := exec.Command("cargo", "install", "cargo-nextest", "--version", "0.9.136", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-nextest: %w", err)
		}
	}

	// Run in debug (the default profile) so this reuses the warm test build from
	// `desktop-rust-tests` instead of paying a separate full release compile.
	// Measured on the 35 cells the lane held in 2026-08: ~4s in debug vs ~1m52s in
	// release, where ~all the release time was the disjoint compile, not the SMB
	// execution. The tests are correctness checks (pass/fail), not benchmarks, so
	// `-O` doesn't change their outcome — every one of them passed in debug.
	// `--run-ignored only` rides in baseArgs so the contention re-run inherits it: these
	// tests are all `#[ignore]`-gated, so a re-run without it would select nothing and
	// read as "everything passed alone".
	baseArgs := append([]string{"--locked", "--run-ignored", "only"}, laneArgs...)
	cmd := exec.Command("cargo", append(append([]string{"nextest", "run"}, baseArgs...),
		"-E", fixtureIntegrationFilter(ctx.RootDir))...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	// See `desktop-rust-tests.go`: captured nextest output is not plain text by default.
	output = StripANSI(output)
	// Before the verdict branch, so a red run records WHICH tests went red
	// (`test-log.go`); the contention re-run below is deliberately not recorded.
	ctx.RecordTests(ParseNextestResults(output)...)
	if err != nil {
		// This lane contends on a SHARED Docker Samba stack as well as on CPU, so a red run
		// here is even likelier to be starvation than in the default lane. Slowest healthy
		// test measured 2.8s (whole 53-test suite: 5.3s wall-clock), well inside the
		// contention-retry profile's 40s headroom.
		return resolveRustFailure("fixture integration tests failed",
			nextestContentionRunner(ctx.RootDir, baseArgs), LoadPerCore, trimRustTestProgress(output))
	}

	re := regexp.MustCompile(`(\d+) tests? run`)
	matches := re.FindStringSubmatch(output)
	message := "All fixture integration tests passed"
	count := -1
	if len(matches) > 1 {
		count, _ = strconv.Atoi(matches[1])
		message = fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))
	}

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

// waitForSmbContainers polls `docker compose -p smb-consumer ps` until every
// expected service appears in the running set, or the timeout expires.
func waitForSmbContainers(expected []string, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	interval := 1 * time.Second

	for {
		psCmd := exec.Command(
			"docker", "compose", "-p", "smb-consumer",
			"ps", "--services", "--filter", "status=running",
		)
		output, _ := RunCommand(psCmd, true)

		running := make(map[string]struct{})
		for _, line := range strings.Split(strings.TrimSpace(output), "\n") {
			if line = strings.TrimSpace(line); line != "" {
				running[line] = struct{}{}
			}
		}

		missing := []string{}
		for _, svc := range expected {
			if _, ok := running[svc]; !ok {
				missing = append(missing, svc)
			}
		}
		if len(missing) == 0 {
			return nil
		}

		if time.Now().After(deadline) {
			return fmt.Errorf(
				"SMB containers didn't reach running state within %s: still waiting for %s",
				timeout, strings.Join(missing, ", "),
			)
		}
		time.Sleep(interval)
	}
}
