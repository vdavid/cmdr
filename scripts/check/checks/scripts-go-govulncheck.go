package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunGovulncheck checks for known vulnerabilities in Go dependencies.
func RunGovulncheck(ctx *CheckContext) (CheckResult, error) {
	govulncheckBin, err := EnsureGoTool("govulncheck", "golang.org/x/vuln/cmd/govulncheck@latest")
	if err != nil {
		return CheckResult{}, err
	}

	allModules, err := FindAllGoModules(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string
	modCount := 0

	for goDir, modules := range allModules {
		baseDir := filepath.Join(ctx.RootDir, goDir)
		for _, mod := range modules {
			modDir := filepath.Join(baseDir, mod)
			modLabel := filepath.Join(goDir, mod)
			modCount++

			cmd := exec.Command(govulncheckBin, "./...")
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
		return CheckResult{}, fmt.Errorf("vulnerabilities found\n%s", indentOutput(strings.Join(allIssues, "\n")))
	}

	return Success(fmt.Sprintf("Scanned %d %s, no vulnerabilities", modCount, Pluralize(modCount, "module", "modules"))), nil
}
