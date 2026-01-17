package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunStaticcheck runs staticcheck for static analysis.
func RunStaticcheck(ctx *CheckContext) (CheckResult, error) {
	scriptsDir := filepath.Join(ctx.RootDir, "scripts")

	// Ensure staticcheck is installed
	if !CommandExists("staticcheck") {
		installCmd := exec.Command("go", "install", "honnef.co/go/tools/cmd/staticcheck@latest")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install staticcheck: %w", err)
		}
	}

	modules, err := FindGoModules(scriptsDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string
	pkgCount := 0

	for _, mod := range modules {
		modDir := filepath.Join(scriptsDir, mod)

		// Count packages
		listCmd := exec.Command("go", "list", "./...")
		listCmd.Dir = modDir
		listOutput, _ := RunCommand(listCmd, true)
		if strings.TrimSpace(listOutput) != "" {
			pkgCount += len(strings.Split(strings.TrimSpace(listOutput), "\n"))
		}

		cmd := exec.Command("staticcheck", "./...")
		cmd.Dir = modDir
		output, err := RunCommand(cmd, true)
		if err != nil {
			allIssues = append(allIssues, fmt.Sprintf("[%s]\n%s", mod, output))
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
