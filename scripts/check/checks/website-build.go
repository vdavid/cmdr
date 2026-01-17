package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunWebsiteBuild runs the build to verify it works.
func RunWebsiteBuild(ctx *CheckContext) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")

	cmd := exec.Command("pnpm", "build")
	cmd.Dir = websiteDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("build failed\n%s", indentOutput(output))
	}
	return Success("Build completed"), nil
}
