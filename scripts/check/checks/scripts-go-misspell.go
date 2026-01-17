package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunMisspell checks for spelling mistakes.
func RunMisspell(ctx *CheckContext) (CheckResult, error) {
	scriptsDir := filepath.Join(ctx.RootDir, "scripts")

	// Ensure misspell is installed
	if !CommandExists("misspell") {
		installCmd := exec.Command("go", "install", "github.com/client9/misspell/cmd/misspell@latest")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install misspell: %w", err)
		}
	}

	// Count Go files
	findCmd := exec.Command("find", ".", "-name", "*.go", "-type", "f")
	findCmd.Dir = scriptsDir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	cmd := exec.Command("misspell", "-error", ".")
	cmd.Dir = scriptsDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("spelling mistakes found\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, no misspellings", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("No misspellings"), nil
}
