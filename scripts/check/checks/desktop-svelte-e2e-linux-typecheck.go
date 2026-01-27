package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunDesktopE2ELinuxTypecheck runs TypeScript checking on the e2e-linux test files.
func RunDesktopE2ELinuxTypecheck(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	tsconfigPath := filepath.Join("test", "e2e-linux", "tsconfig.json")

	cmd := exec.Command("pnpm", "tsc", "--noEmit", "-p", tsconfigPath)
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("typecheck failed\n%s", indentOutput(output))
	}
	return Success("No type errors"), nil
}
