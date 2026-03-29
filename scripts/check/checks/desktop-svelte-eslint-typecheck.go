package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopESLintTypecheck runs the full ESLint config (all rules including type-aware).
// This is the slow counterpart to RunDesktopESLint (which skips type-aware rules).
// Running all rules here also catches stale eslint-disable comments that the fast
// check can't detect (since it suppresses reportUnusedDisableDirectives).
func RunDesktopESLintTypecheck(ctx *CheckContext) (CheckResult, error) {
	dir := filepath.Join(ctx.RootDir, "apps", "desktop")

	// Count lintable files
	findArgs := buildFindArgs("src", []string{"*.ts", "*.svelte", "*.js"})
	findCmd := exec.Command("find", findArgs...)
	findCmd.Dir = dir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "lint")
	} else {
		cmd = exec.Command("pnpm", "lint:fix")
	}
	cmd.Dir = dir
	// No env var override — runs the full config with all rules + projectService.

	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("type-aware lint errors found, run pnpm lint:fix locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("eslint found unfixable errors\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		result := Success(fmt.Sprintf("%d %s passed", fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		return result, nil
	}
	return Success("All files passed"), nil
}
