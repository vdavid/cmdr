package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunDeadcode runs Go's deadcode tool to find unreachable functions.
func RunDeadcode(ctx *CheckContext) (CheckResult, error) {
	// Ensure deadcode is installed
	deadcodePath, err := EnsureGoTool("deadcode", "golang.org/x/tools/cmd/deadcode@latest")
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to install deadcode: %w", err)
	}

	// Find all Go modules
	modules, err := FindAllGoModules(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string
	modulesChecked := 0

	for baseDir, subModules := range modules {
		for _, subModule := range subModules {
			modulePath := filepath.Join(ctx.RootDir, baseDir, subModule)

			cmd := exec.Command(deadcodePath, "./...")
			cmd.Dir = modulePath
			output, err := RunCommand(cmd, true)

			// deadcode exits 0 even when it finds issues, output goes to stdout
			if err != nil {
				return CheckResult{}, fmt.Errorf("deadcode failed in %s: %w\n%s", modulePath, err, output)
			}

			// Parse output - each line is a dead code issue
			output = strings.TrimSpace(output)
			if output != "" {
				for _, line := range strings.Split(output, "\n") {
					if line != "" {
						// Prefix with module path for clarity
						relPath := filepath.Join(baseDir, subModule)
						allIssues = append(allIssues, fmt.Sprintf("%s: %s", relPath, line))
					}
				}
			}

			modulesChecked++
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("found %d unreachable %s:\n%s",
			len(allIssues),
			Pluralize(len(allIssues), "function", "functions"),
			strings.Join(allIssues, "\n"))
	}

	return Success(fmt.Sprintf("%d %s checked, no dead code",
		modulesChecked,
		Pluralize(modulesChecked, "module", "modules"))), nil
}
