package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunGovulncheck checks for known vulnerabilities in Go dependencies.
func RunGovulncheck(ctx *CheckContext) (CheckResult, error) {
	scriptsDir := filepath.Join(ctx.RootDir, "scripts")

	// Ensure govulncheck is installed
	if !CommandExists("govulncheck") {
		installCmd := exec.Command("go", "install", "golang.org/x/vuln/cmd/govulncheck@latest")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install govulncheck: %w", err)
		}
	}

	modules, err := FindGoModules(scriptsDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string

	for _, mod := range modules {
		modDir := filepath.Join(scriptsDir, mod)

		cmd := exec.Command("govulncheck", "./...")
		cmd.Dir = modDir
		output, err := RunCommand(cmd, true)
		if err != nil {
			allIssues = append(allIssues, fmt.Sprintf("[%s]\n%s", mod, output))
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("vulnerabilities found\n%s", indentOutput(strings.Join(allIssues, "\n")))
	}

	modCount := len(modules)
	return Success(fmt.Sprintf("Scanned %d %s, no vulnerabilities", modCount, Pluralize(modCount, "module", "modules"))), nil
}
