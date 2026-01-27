package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunStaticcheck runs staticcheck for static analysis.
func RunStaticcheck(ctx *CheckContext) (CheckResult, error) {
	staticcheckBin, err := EnsureGoTool("staticcheck", "honnef.co/go/tools/cmd/staticcheck@latest")
	if err != nil {
		return CheckResult{}, err
	}

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

			// Count packages
			listCmd := exec.Command("go", "list", "./...")
			listCmd.Dir = modDir
			listOutput, _ := RunCommand(listCmd, true)
			if strings.TrimSpace(listOutput) != "" {
				pkgCount += len(strings.Split(strings.TrimSpace(listOutput), "\n"))
			}

			cmd := exec.Command(staticcheckBin, "./...")
			cmd.Dir = modDir
			output, err := RunCommand(cmd, true)
			if err != nil {
				// Include both output and error message for debugging
				issueText := strings.TrimSpace(output)
				if issueText == "" {
					issueText = err.Error()
				}
				allIssues = append(allIssues, fmt.Sprintf("[%s]\n%s", modLabel, issueText))
			}
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("staticcheck found issues\n%s", indentOutput(strings.Join(allIssues, "\n")))
	}

	if pkgCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, no issues", pkgCount, Pluralize(pkgCount, "package", "packages"))), nil
	}
	return Success("No issues found"), nil
}
