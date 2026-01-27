package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunGoVet runs go vet to find likely mistakes.
func RunGoVet(ctx *CheckContext) (CheckResult, error) {
	allModules, err := FindAllGoModules(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string
	pkgCount := 0

	for goDir, modules := range allModules {
		baseDir := filepath.Join(ctx.RootDir, goDir)
		for _, mod := range modules {
			modDir := filepath.Join(baseDir, mod)
			modLabel := filepath.Join(goDir, mod)

			// Count packages in this module
			listCmd := exec.Command("go", "list", "./...")
			listCmd.Dir = modDir
			listOutput, _ := RunCommand(listCmd, true)
			if strings.TrimSpace(listOutput) != "" {
				pkgCount += len(strings.Split(strings.TrimSpace(listOutput), "\n"))
			}

			vetCmd := exec.Command("go", "vet", "./...")
			vetCmd.Dir = modDir
			output, err := RunCommand(vetCmd, true)
			if err != nil {
				allIssues = append(allIssues, fmt.Sprintf("[%s]\n%s", modLabel, output))
			}
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("go vet found issues\n%s", indentOutput(strings.Join(allIssues, "\n")))
	}

	if pkgCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, no issues", pkgCount, Pluralize(pkgCount, "package", "packages"))), nil
	}
	return Success("No issues found"), nil
}
