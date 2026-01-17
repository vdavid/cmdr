package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunStylelint validates CSS and catches undefined custom properties.
func RunStylelint(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	// Count CSS files
	findCmd := exec.Command("find", "src", "-type", "f", "-name", "*.css")
	findCmd.Dir = desktopDir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "stylelint")
	} else {
		cmd = exec.Command("pnpm", "stylelint:fix")
	}
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("CSS lint errors found, run pnpm stylelint:fix locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("stylelint found unfixable errors\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d CSS %s valid", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("All CSS valid"), nil
}
