package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunWebsitePrettier runs Prettier on the website.
func RunWebsitePrettier(ctx *CheckContext) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")

	// Count files
	findCmd := exec.Command("find", "src", "-type", "f", "(", "-name", "*.ts", "-o", "-name", "*.astro", "-o", "-name", "*.css", "-o", "-name", "*.js", ")")
	findCmd.Dir = websiteDir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "format:check")
	} else {
		cmd = exec.Command("pnpm", "format")
	}
	cmd.Dir = websiteDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("prettier failed\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("All files already formatted"), nil
}
