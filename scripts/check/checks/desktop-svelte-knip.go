package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunKnip finds unused code, dependencies, and exports.
func RunKnip(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")

	// Count source files for context
	findCmd := exec.Command("find", "src", "-type", "f", "(", "-name", "*.ts", "-o", "-name", "*.svelte", ")")
	findCmd.Dir = desktopDir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	cmd := exec.Command("pnpm", "knip")
	cmd.Dir = desktopDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("knip found unused code or dependencies\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, no unused code", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("No unused code"), nil
}
