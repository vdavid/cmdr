package checks

import (
	"path/filepath"
)

// RunDashboardKnip finds unused code, exports, and dependencies in the dashboard.
func RunDashboardKnip(ctx *CheckContext) (CheckResult, error) {
	return runKnipCheck(ctx, filepath.Join(ctx.RootDir, "apps", "analytics-dashboard"))
}
