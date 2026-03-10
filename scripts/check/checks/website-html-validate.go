package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunWebsiteHTMLValidate runs html-validate on the built website HTML files.
func RunWebsiteHTMLValidate(ctx *CheckContext) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")
	distDir := filepath.Join(websiteDir, "dist")

	if _, err := os.Stat(distDir); os.IsNotExist(err) {
		return Skipped("dist/ not found (run website-build first)"), nil
	}

	cmd := exec.Command("pnpm", "exec", "html-validate", "dist/**/*.html")
	cmd.Dir = websiteDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("HTML validation failed\n%s", indentOutput(output))
	}
	return Success("All HTML files valid"), nil
}
