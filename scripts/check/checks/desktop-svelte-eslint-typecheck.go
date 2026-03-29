package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDesktopESLintTypecheck runs only the type-aware ESLint rules.
// This is the slow half of the ESLint split — it loads TypeScript's project service
// to power rules like no-floating-promises, no-unsafe-*, and no-misused-promises.
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
	cmd.Env = append(os.Environ(), "ESLINT_TYPECHECK_ONLY=1")

	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("type-aware lint errors found, run pnpm lint:fix locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("type-aware eslint found unfixable errors\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		result := Success(fmt.Sprintf("%d %s passed", fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		return result, nil
	}
	return Success("All files passed"), nil
}
