package checks

import (
	"path/filepath"
)

// RunDashboardStylelint validates the dashboard's CSS and Svelte `<style>` blocks.
func RunDashboardStylelint(ctx *CheckContext) (CheckResult, error) {
	return runStylelintCheck(ctx, filepath.Join(ctx.RootDir, "apps", "analytics-dashboard"))
}
