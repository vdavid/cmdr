package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunGoFmt formats Go code with gofmt.
func RunGoFmt(ctx *CheckContext) (CheckResult, error) {
	scriptsDir := filepath.Join(ctx.RootDir, "scripts")

	// Count Go files
	findCmd := exec.Command("find", ".", "-name", "*.go", "-type", "f")
	findCmd.Dir = scriptsDir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	if ctx.CI {
		// Check mode - list files that need formatting
		cmd := exec.Command("gofmt", "-s", "-l", ".")
		cmd.Dir = scriptsDir
		output, err := RunCommand(cmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("gofmt check failed\n%s", indentOutput(output))
		}
		if strings.TrimSpace(output) != "" {
			return CheckResult{}, fmt.Errorf("files need formatting, run gofmt -s -w . locally\n%s", indentOutput(output))
		}
	} else {
		// Fix mode - format files in place
		cmd := exec.Command("gofmt", "-s", "-w", ".")
		cmd.Dir = scriptsDir
		output, err := RunCommand(cmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("gofmt failed\n%s", indentOutput(output))
		}
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("All files already formatted"), nil
}
