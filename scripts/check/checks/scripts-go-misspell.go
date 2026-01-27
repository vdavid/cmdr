package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunMisspell checks for spelling mistakes.
func RunMisspell(ctx *CheckContext) (CheckResult, error) {
	misspellBin, err := EnsureGoTool("misspell", "github.com/client9/misspell/cmd/misspell@latest")
	if err != nil {
		return CheckResult{}, err
	}

	goDirs := GetGoDirectories(ctx.RootDir)
	totalFileCount := 0
	var allIssues []string

	for _, goDir := range goDirs {
		fullPath := filepath.Join(ctx.RootDir, goDir)

		// Count Go files
		findCmd := exec.Command("find", ".", "-name", "*.go", "-type", "f")
		findCmd.Dir = fullPath
		findOutput, _ := RunCommand(findCmd, true)
		if strings.TrimSpace(findOutput) != "" {
			totalFileCount += len(strings.Split(strings.TrimSpace(findOutput), "\n"))
		}

		cmd := exec.Command(misspellBin, "-error", ".")
		cmd.Dir = fullPath
		output, err := RunCommand(cmd, true)
		if err != nil {
			issueText := strings.TrimSpace(output)
			if issueText == "" {
				issueText = err.Error()
			}
			allIssues = append(allIssues, fmt.Sprintf("[%s]\n%s", goDir, issueText))
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("spelling mistakes found\n%s", indentOutput(strings.Join(allIssues, "\n")))
	}

	if totalFileCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, no misspellings", totalFileCount, Pluralize(totalFileCount, "file", "files"))), nil
	}
	return Success("No misspellings"), nil
}
