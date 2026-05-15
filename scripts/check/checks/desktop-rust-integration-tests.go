package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// RunRustIntegrationTests runs the Docker-backed SMB Rust integration tests.
//
// Container lifecycle: managed by the runner-level SMB orchestrator (see
// scripts/check/smb_orchestrator.go). This check is marked `NeedsSmb: SmbModeCore`
// in the registry, so the containers are guaranteed up by the time this
// function runs and they survive past it (no per-check `defer ./stop.sh`).
// The old defer broke parallel runs by tearing down containers
// `desktop-e2e-linux` was still using.
//
// Flow:
//  1. Containers are already up (orchestrator started them at runner init).
//     We still wait until the expected services report `running` as a
//     cheap guard against mid-run zombies; smb2 reconnects if the server
//     isn't ready on the first write.
//  2. Invoke `cargo nextest run --release --run-ignored only -E 'test(smb_integration_)'`
//     in apps/desktop/src-tauri. The expression filter matches every
//     `smb_integration_*` test and skips other `#[ignore]` tests.
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

	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

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
	}
	if err := waitForSmbContainers(expected, 120*time.Second); err != nil {
		return CheckResult{}, err
	}

	// Make sure cargo-nextest is available (mirrors desktop-rust-tests.go).
	if !CommandExists("cargo-nextest") {
		installCmd := exec.Command("cargo", "install", "cargo-nextest", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-nextest: %w", err)
		}
	}

	// Use --release to match the perf profile of shipped code; compound reads
	// and writes are sensitive to -O settings. nextest's expression filter
	// matches only our `smb_integration_*` tests, so unrelated `#[ignore]`
	// tests are still skipped.
	cmd := exec.Command(
		"cargo", "nextest", "run",
		"--release",
		"--run-ignored", "only",
		"-E", "test(smb_integration_)",
	)
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("SMB integration tests failed\n%s", indentOutput(output))
	}

	re := regexp.MustCompile(`(\d+) tests? run`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		result := Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests")))
		result.Total = count
		return result, nil
	}
	return Success("All SMB integration tests passed"), nil
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
