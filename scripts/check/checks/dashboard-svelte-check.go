package checks

import (
	"path/filepath"
)

// RunDashboardSvelteCheck runs svelte-check for type and a11y validation on the dashboard,
// on TypeScript 7 (`--tsgo`); `runSvelteCheckLane` in `desktop-svelte-check.go` does the work.
//
// Uses `typecheck:no-sync`, not the `typecheck` humans run standalone: the
// `dashboard-svelte-kit-sync` dependency already generated `.svelte-kit/`, and re-syncing here
// would rewrite the generated tsconfig while the parallel `dashboard-eslint` pass reads it.
func RunDashboardSvelteCheck(ctx *CheckContext) (CheckResult, error) {
	return runSvelteCheckLane(filepath.Join(ctx.RootDir, "apps", "analytics-dashboard"), true)
}

// RunDashboardSvelteCheckTS6 is the dashboard's TypeScript 6 backstop, as RunSvelteCheckTS6.
func RunDashboardSvelteCheckTS6(ctx *CheckContext) (CheckResult, error) {
	return runSvelteCheckLane(filepath.Join(ctx.RootDir, "apps", "analytics-dashboard"), false)
}
