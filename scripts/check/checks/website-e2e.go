package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunWebsiteE2E runs the website's Playwright suite: every spec in the `chromium` project, which
// holds only the browser demo's smoke. The website lane runs it against the RELEASED demo image,
// which can predate `main`'s frontend (`apps/website/e2e/demo-support.ts`).
func RunWebsiteE2E(ctx *CheckContext) (CheckResult, error) {
	return runWebsitePlaywright(ctx, []string{"chromium"}, false)
}

// RunWebsiteDemoE2E runs the full browser demo suite (the smoke plus `demo.spec.ts`) against the
// site's `dist/demo/`, which must exist: the release pipeline's `demo` job and the nightly CI run
// build the demo from the same sources first. Two projects, one per visitor OS: the demo answers a
// Windows visitor (`demo`) and a Mac one (`demo-mac`) with their own OS's keys and copy.
func RunWebsiteDemoE2E(ctx *CheckContext) (CheckResult, error) {
	return runWebsitePlaywright(ctx, []string{"demo", "demo-mac"}, true)
}

func runWebsitePlaywright(ctx *CheckContext, projects []string, needsDemo bool) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")

	// Without a built site, `serve` answers 404 on every path, which Playwright never counts as ready:
	// the run sits out the full `webServer` timeout and then reports a timeout that doesn't mention the
	// build. Naming this check alone doesn't run its `website-build` dependency, so that's reachable.
	if _, err := os.Stat(filepath.Join(websiteDir, "dist", "index.html")); os.IsNotExist(err) {
		return CheckResult{}, fmt.Errorf("apps/website/dist/index.html not found: build the site first (pnpm check website-build website-e2e)")
	}
	// The demo suite skips itself without a demo off CI, which would read as a pass here.
	if needsDemo {
		if _, err := os.Stat(filepath.Join(websiteDir, "dist", "demo", "index.html")); os.IsNotExist(err) {
			return CheckResult{}, fmt.Errorf("apps/website/dist/demo/index.html not found: run `pnpm build:demo` at the repo root, then `CMDR_WEBSITE_DEMO=1 pnpm build` in apps/website")
		}
	}

	args := []string{"exec", "playwright", "test", "--reporter=list"}
	for _, project := range projects {
		args = append(args, "--project="+project)
	}
	cmd := exec.Command("pnpm", args...)
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
