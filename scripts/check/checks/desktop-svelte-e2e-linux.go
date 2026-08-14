package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"time"
)

// linuxE2EReportPath is where this run's Playwright JSON report lands on the host.
// `apps/desktop/scripts/e2e-linux.sh` takes the path from CMDR_E2E_HOST_JSON_REPORT,
// truncates it, and bind-mounts it into the container as CMDR_E2E_JSON_REPORT.
//
// Run-scoped for the same reason the macOS shards' reports are: the report is what
// the per-test log, the duration flagger, and the flake warning all read back, and a
// fixed path lets a suite in another worktree answer those questions about a run it
// never took part in.
func linuxE2EReportPath(pid int) string {
	return fmt.Sprintf("/tmp/cmdr-e2e-report-linux-%d.json", pid)
}

// RunDesktopE2ELinux runs Playwright E2E tests against the real Tauri app in Docker.
func RunDesktopE2ELinux(ctx *CheckContext) (CheckResult, error) {
	// Check if Docker is available
	if !CommandExists("docker") {
		return Skipped("Docker not installed"), nil
	}

	// Check if Docker daemon is running
	checkCmd := exec.Command("docker", "info")
	if _, err := RunCommand(checkCmd, true); err != nil {
		return Skipped("Docker daemon not running"), nil
	}

	// OrbStack mounts Linux VM files at ~/OrbStack via NFS. Under heavy Docker I/O
	// the NFS server lags, causing noisy kernel warnings on the terminal. Unmount it
	// before running; OrbStack will remount it automatically when next needed.
	unmountOrbStackNFS()

	sweepStaleE2EArtifacts(time.Now())

	timestamp := time.Now().Unix()
	logFile := fmt.Sprintf("/tmp/cmdr-e2e-linux-%d.log", timestamp)
	reportPath := linuxE2EReportPath(os.Getpid())

	cmd := exec.Command("pnpm", "test:e2e:linux")
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	cmd.Env = append(os.Environ(), "CMDR_E2E_HOST_JSON_REPORT="+reportPath)
	runStart := time.Now()
	output, err := RunCommand(cmd, true)

	// Save full output for post-mortem debugging
	appendToLogFile(logFile, output)

	// Before the verdict, so a red run records WHICH specs went red (`test-log.go`).
	recordPlaywrightTests(ctx, []string{reportPath}, runStart)

	if err != nil {
		summary := extractE2ETestOutput(output)
		return CheckResult{}, fmt.Errorf("linux E2E tests failed (full log: %s)\n%s", logFile, indentOutput(summary))
	}

	// Extract test count from Playwright output (like "48 passed")
	re := regexp.MustCompile(`(\d+) passed`)
	matches := re.FindStringSubmatch(output)
	result := Success("All Linux E2E tests passed")
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		result = Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests")))
	}
	// Warn-only duration flagging from the JSON report the run just wrote.
	result = applyE2EDurationWarnings(ctx, result, []string{reportPath}, "linux")
	// This lane runs with `CI=true`, so `retries: 1` is live here: a rescued spec would
	// otherwise be reported as a clean pass. See `e2e-flaky.go`.
	return applyE2EFlakyWarning(result, []string{reportPath}), nil
}

// unmountOrbStackNFS unmounts OrbStack's reverse NFS mount (~/OrbStack) if present.
func unmountOrbStackNFS() {
	home, err := os.UserHomeDir()
	if err != nil {
		return
	}
	mountPoint := filepath.Join(home, "OrbStack")
	if fi, err := os.Stat(mountPoint); err != nil || !fi.IsDir() {
		return
	}
	_ = exec.Command("umount", mountPoint).Run()
}
