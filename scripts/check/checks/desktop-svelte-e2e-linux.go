package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunDesktopE2ELinux runs end-to-end tests against the real Tauri app in Docker.
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

	cmd := exec.Command("pnpm", "test:e2e:linux")
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("linux E2E tests failed\n%s", indentOutput(output))
	}

	// Extract test count from WebDriverIO output (e.g., "11 passing")
	re := regexp.MustCompile(`(\d+) passing`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))), nil
	}
	return Success("All Linux E2E tests passed"), nil
}
