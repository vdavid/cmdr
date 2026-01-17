package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunLicenseServerTypecheck runs TypeScript checking on the license server.
func RunLicenseServerTypecheck(ctx *CheckContext) (CheckResult, error) {
	serverDir := filepath.Join(ctx.RootDir, "apps", "license-server")

	cmd := exec.Command("pnpm", "typecheck")
	cmd.Dir = serverDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("typecheck failed\n%s", indentOutput(output))
	}
	return Success("No type errors"), nil
}
