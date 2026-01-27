package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunGoFmt formats Go code with gofmt.
func RunGoFmt(ctx *CheckContext) (CheckResult, error) {
	goDirs := GetGoDirectories()

	totalFileCount := 0
	var allNeedsFormat []string
	var allCheckOutput strings.Builder

	for _, goDir := range goDirs {
		fullPath := filepath.Join(ctx.RootDir, goDir)

		// Count Go files
		findCmd := exec.Command("find", ".", "-name", "*.go", "-type", "f")
		findCmd.Dir = fullPath
		findOutput, _ := RunCommand(findCmd, true)
		if strings.TrimSpace(findOutput) != "" {
			totalFileCount += len(strings.Split(strings.TrimSpace(findOutput), "\n"))
		}

		// Check which files need formatting (-l lists them)
		checkCmd := exec.Command("gofmt", "-s", "-l", ".")
		checkCmd.Dir = fullPath
		checkOutput, err := RunCommand(checkCmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("gofmt check failed in %s\n%s", goDir, indentOutput(checkOutput))
		}

		// Parse files that need formatting
		if strings.TrimSpace(checkOutput) != "" {
			for _, file := range strings.Split(strings.TrimSpace(checkOutput), "\n") {
				allNeedsFormat = append(allNeedsFormat, filepath.Join(goDir, file))
			}
			allCheckOutput.WriteString(checkOutput)
		}

		// Non-CI mode: format if needed
		if !ctx.CI && strings.TrimSpace(checkOutput) != "" {
			fmtCmd := exec.Command("gofmt", "-s", "-w", ".")
			fmtCmd.Dir = fullPath
			output, fmtErr := RunCommand(fmtCmd, true)
			if fmtErr != nil {
				return CheckResult{}, fmt.Errorf("gofmt failed in %s\n%s", goDir, indentOutput(output))
			}
		}
	}

	if ctx.CI {
		if len(allNeedsFormat) > 0 {
			return CheckResult{}, fmt.Errorf("files need formatting, run gofmt -s -w . locally\n%s", indentOutput(allCheckOutput.String()))
		}
		return Success(fmt.Sprintf("%d %s already formatted", totalFileCount, Pluralize(totalFileCount, "file", "files"))), nil
	}

	if len(allNeedsFormat) > 0 {
		return SuccessWithChanges(fmt.Sprintf("Formatted %d of %d %s", len(allNeedsFormat), totalFileCount, Pluralize(totalFileCount, "file", "files"))), nil
	}

	return Success(fmt.Sprintf("%d %s already formatted", totalFileCount, Pluralize(totalFileCount, "file", "files"))), nil
}
