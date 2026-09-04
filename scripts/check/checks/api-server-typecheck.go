package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunApiServerTypecheck runs TypeScript checking on the API server.
//
// Uses `typecheck:no-gen`, not the `typecheck` humans run standalone: the
// `api-server-worker-types` dependency already generated `worker-configuration.d.ts`, and
// regenerating here would rewrite it while the parallel `api-server-eslint` pass reads it.
func RunApiServerTypecheck(ctx *CheckContext) (CheckResult, error) {
	serverDir := filepath.Join(ctx.RootDir, "apps", "api-server")

	cmd := exec.Command("pnpm", "typecheck:no-gen")
	cmd.Dir = serverDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("typecheck failed\n%s", indentOutput(output))
	}
	return Success("No type errors"), nil
}
