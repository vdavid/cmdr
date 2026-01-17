package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// GocycloThreshold is the maximum cyclomatic complexity allowed.
const GocycloThreshold = 15

// RunGocyclo checks cyclomatic complexity of Go functions.
func RunGocyclo(ctx *CheckContext) (CheckResult, error) {
	scriptsDir := filepath.Join(ctx.RootDir, "scripts")

	// Ensure gocyclo is installed
	if !CommandExists("gocyclo") {
		installCmd := exec.Command("go", "install", "github.com/fzipp/gocyclo/cmd/gocyclo@latest")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install gocyclo: %w", err)
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

	// Run gocyclo with threshold
	cmd := exec.Command("gocyclo", "-over", fmt.Sprintf("%d", GocycloThreshold), ".")
	cmd.Dir = scriptsDir
	output, err := RunCommand(cmd, true)

	// gocyclo returns exit code 1 if it finds functions over the threshold
	if err != nil || strings.TrimSpace(output) != "" {
		if strings.TrimSpace(output) != "" {
			return CheckResult{}, fmt.Errorf("functions exceed complexity threshold of %d\n%s", GocycloThreshold, indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("gocyclo failed\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, complexity OK", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("Complexity OK"), nil
}
