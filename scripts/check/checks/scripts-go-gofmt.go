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

	// Check which files need formatting (-l lists them)
	checkCmd := exec.Command("gofmt", "-s", "-l", ".")
	checkCmd.Dir = scriptsDir
	checkOutput, err := RunCommand(checkCmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("gofmt check failed\n%s", indentOutput(checkOutput))
	}

	// Parse files that need formatting
	var needsFormat []string
	if strings.TrimSpace(checkOutput) != "" {
		needsFormat = strings.Split(strings.TrimSpace(checkOutput), "\n")
	}

	if ctx.CI {
		if len(needsFormat) > 0 {
			return CheckResult{}, fmt.Errorf("files need formatting, run gofmt -s -w . locally\n%s", indentOutput(checkOutput))
		}
		return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}

	// Non-CI mode: format if needed
	if len(needsFormat) > 0 {
		fmtCmd := exec.Command("gofmt", "-s", "-w", ".")
		fmtCmd.Dir = scriptsDir
		output, fmtErr := RunCommand(fmtCmd, true)
		if fmtErr != nil {
			return CheckResult{}, fmt.Errorf("gofmt failed\n%s", indentOutput(output))
		}
		return SuccessWithChanges(fmt.Sprintf("Formatted %d of %d %s", len(needsFormat), fileCount, Pluralize(fileCount, "file", "files"))), nil
	}

	return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
}
