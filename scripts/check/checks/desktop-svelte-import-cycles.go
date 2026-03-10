package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunImportCycles uses oxlint's import plugin to detect circular imports in TypeScript/Svelte code.
func RunImportCycles(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	cmd := exec.Command("pnpm", "exec", "oxlint",
		"--import-plugin",
		"--allow", "all",
		"--deny", "import/no-cycle",
		"src",
	)
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("circular imports detected\n%s", indentOutput(output))
	}

	return Success("No circular imports"), nil
}
