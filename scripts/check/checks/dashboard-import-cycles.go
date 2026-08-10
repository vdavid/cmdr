package checks

import (
	"path/filepath"
)

// RunDashboardImportCycles detects circular imports in the dashboard.
func RunDashboardImportCycles(ctx *CheckContext) (CheckResult, error) {
	return runImportCyclesCheck(ctx, filepath.Join(ctx.RootDir, "apps", "analytics-dashboard"))
}
