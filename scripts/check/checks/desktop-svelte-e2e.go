package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunDesktopE2E runs end-to-end tests with Playwright.
func RunDesktopE2E(ctx *CheckContext) (CheckResult, error) {
	cmd := exec.Command("pnpm", "test:e2e")
	cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("e2e tests failed\n%s", indentOutput(output))
	}

	// Extract test count
	re := regexp.MustCompile(`(\d+) passed`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("%d %s passed", count, Pluralize(count, "test", "tests"))), nil
	}
	return Success("All E2E tests passed"), nil
}
