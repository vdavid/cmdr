package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDashboardBuild builds the dashboard for Cloudflare Pages.
//
// This is NOT redundant with `dashboard-svelte-check`. The `$lib/server` boundary guard
// (`vite-plugin-sveltekit-guard`) only trips at build time, and svelte-check does not catch it, so
// this is the only check standing between a stray runtime import and shipping the admin token or an
// API key into the browser bundle.
func RunDashboardBuild(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "analytics-dashboard")

	cmd := exec.Command("pnpm", "build")
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("build failed\n%s", indentOutput(output))
	}

	return Success("Builds clean"), nil
}
