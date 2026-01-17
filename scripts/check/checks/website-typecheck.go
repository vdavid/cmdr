package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunWebsiteTypecheck runs TypeScript/Astro checking on the website.
func RunWebsiteTypecheck(ctx *CheckContext) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")

	cmd := exec.Command("pnpm", "typecheck")
	cmd.Dir = websiteDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("typecheck failed\n%s", indentOutput(output))
	}
	return Success("No type errors"), nil
}
