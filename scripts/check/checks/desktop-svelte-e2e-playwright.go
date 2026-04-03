package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunDesktopE2EPlaywright runs Playwright E2E tests against the real Tauri app
// via tauri-playwright. Requires the app to be running with the playwright-e2e
// Cargo feature and a Unix socket at /tmp/tauri-playwright.sock.
func RunDesktopE2EPlaywright(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("pnpm", "test:e2e:playwright")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("playwright E2E tests failed\n%s", indentOutput(output))
	}

	// Extract test count from Playwright output (like "42 passed")
	re := regexp.MustCompile(`(\d+) passed`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))), nil
	}
	return Success("All Playwright E2E tests passed"), nil
}
