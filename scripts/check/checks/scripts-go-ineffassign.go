package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunIneffassign detects ineffectual assignments.
func RunIneffassign(ctx *CheckContext) (CheckResult, error) {
	ineffassignBin, err := EnsureGoTool("ineffassign", "github.com/gordonklaus/ineffassign@latest")
	if err != nil {
		return CheckResult{}, err
	}

	allModules, err := FindAllGoModules(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string
	fileCount := 0

	for goDir, modules := range allModules {
		baseDir := filepath.Join(ctx.RootDir, goDir)
		for _, mod := range modules {
			modDir := filepath.Join(baseDir, mod)
			modLabel := filepath.Join(goDir, mod)

			// Count Go files in this module
			findCmd := exec.Command("find", ".", "-name", "*.go", "-type", "f")
			findCmd.Dir = modDir
			findOutput, _ := RunCommand(findCmd, true)
			if strings.TrimSpace(findOutput) != "" {
				fileCount += len(strings.Split(strings.TrimSpace(findOutput), "\n"))
			}

			cmd := exec.Command(ineffassignBin, "./...")
			cmd.Dir = modDir
			output, err := RunCommand(cmd, true)
			if err != nil {
				issueText := strings.TrimSpace(output)
				if issueText == "" {
					issueText = err.Error()
				}
				allIssues = append(allIssues, fmt.Sprintf("[%s]\n%s", modLabel, issueText))
			}
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("ineffectual assignments found\n%s", indentOutput(strings.Join(allIssues, "\n")))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, no ineffectual assignments", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("No ineffectual assignments"), nil
}
