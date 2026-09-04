package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunApiServerWorkerTypes runs `wrangler types`, which generates
// `apps/api-server/worker-configuration.d.ts` from `wrangler.toml`: every Worker runtime global at
// the Worker's `compatibility_date`, plus the `Env` shape its bindings and vars imply. That file is
// gitignored and the app's `tsconfig.json` includes it, so without it neither the type-aware ESLint
// pass nor `tsc` knows what a `KVNamespace` is. The api-server's type-aware checks depend on this
// one so it always runs first. Same shape as `dashboard-svelte-kit-sync`.
//
// It regenerates rather than verifying freshness, which is why nothing here can go stale: the
// output is derived, never reviewed, and rebuilding it costs a few seconds. The api-server
// `prepare` script runs the same command, so a plain `pnpm install` (CI included) already leaves
// the file in place.
func RunApiServerWorkerTypes(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "api-server")
	cmd := exec.Command("pnpm", "types:gen")
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("`wrangler types` failed\n%s", indentOutput(output))
	}
	return Success("Generated worker-configuration.d.ts"), nil
}
