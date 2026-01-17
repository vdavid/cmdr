package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunLicenseServerESLint runs ESLint on the license server.
func RunLicenseServerESLint(ctx *CheckContext) (CheckResult, error) {
	serverDir := filepath.Join(ctx.RootDir, "apps", "license-server")

	// Check if eslint.config.js exists
	if _, err := os.Stat(filepath.Join(serverDir, "eslint.config.js")); os.IsNotExist(err) {
		return Skipped("no eslint.config.js"), nil
	}

	// Count lintable files
	findCmd := exec.Command("find", "src", "-type", "f", "(", "-name", "*.ts", "-o", "-name", "*.js", ")")
	findCmd.Dir = serverDir
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
	cmd.Dir = serverDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("eslint failed\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s passed", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("All files passed"), nil
}
