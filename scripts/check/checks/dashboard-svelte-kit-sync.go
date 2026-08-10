package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDashboardSvelteKitSync runs `svelte-kit sync`, which generates
// `apps/analytics-dashboard/.svelte-kit/tsconfig.json`. That file is gitignored, and the app's
// `tsconfig.json` extends it, so neither the type-aware ESLint pass nor svelte-check can build a
// TypeScript program without it. The dashboard's type-aware checks depend on this one so it always
// runs first. Same reasoning as `desktop-svelte-kit-sync`.
func RunDashboardSvelteKitSync(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "analytics-dashboard")
	cmd := exec.Command("pnpm", "exec", "svelte-kit", "sync")
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("svelte-kit sync failed\n%s", indentOutput(output))
	}
	return Success("Generated .svelte-kit"), nil
}
