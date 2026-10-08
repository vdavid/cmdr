package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunWebsiteE2E runs Playwright E2E tests on the website.
func RunWebsiteE2E(ctx *CheckContext) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")

	// Without a built site, `serve` answers 404 on every path, which Playwright never counts as ready:
	// the run sits out the full `webServer` timeout and then reports a timeout that doesn't mention the
	// build. Naming this check alone doesn't run its `website-build` dependency, so that's reachable.
	if _, err := os.Stat(filepath.Join(websiteDir, "dist", "index.html")); os.IsNotExist(err) {
		return CheckResult{}, fmt.Errorf("apps/website/dist/index.html not found: build the site first (pnpm check website-build website-e2e)")
	}

	cmd := exec.Command("pnpm", "exec", "playwright", "test", "--reporter=list")
	cmd.Dir = websiteDir
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
