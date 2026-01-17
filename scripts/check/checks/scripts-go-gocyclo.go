package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// GocycloThreshold is the maximum cyclomatic complexity allowed.
const GocycloThreshold = 15

// RunGocyclo checks cyclomatic complexity of Go functions.
func RunGocyclo(ctx *CheckContext) (CheckResult, error) {
	scriptsDir := filepath.Join(ctx.RootDir, "scripts")

	// Ensure gocyclo is installed
	if !CommandExists("gocyclo") {
		installCmd := exec.Command("go", "install", "github.com/fzipp/gocyclo/cmd/gocyclo@latest")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install gocyclo: %w", err)
		}
	}

	modules, err := FindGoModules(scriptsDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to find Go modules: %w", err)
	}

	var allIssues []string
	fileCount := 0

	for _, mod := range modules {
		modDir := filepath.Join(scriptsDir, mod)

		// Count Go files in this module
		findCmd := exec.Command("find", ".", "-name", "*.go", "-type", "f")
		findCmd.Dir = modDir
		findOutput, _ := RunCommand(findCmd, true)
		if strings.TrimSpace(findOutput) != "" {
			fileCount += len(strings.Split(strings.TrimSpace(findOutput), "\n"))
		}

		// Run gocyclo with threshold
		cmd := exec.Command("gocyclo", "-over", fmt.Sprintf("%d", GocycloThreshold), ".")
		cmd.Dir = modDir
		output, err := RunCommand(cmd, true)

		// gocyclo returns exit code 1 if it finds functions over the threshold
		if err != nil || strings.TrimSpace(output) != "" {
			if strings.TrimSpace(output) != "" {
				// Rewrite file paths to be relative to repo root for clarity
				// gocyclo format: "<complexity> <package> <function> <file>:<line>"
				lines := strings.Split(strings.TrimSpace(output), "\n")
				for i, line := range lines {
					// Find the last space before the file:line part and prefix the file path
					parts := strings.Fields(line)
					if len(parts) >= 4 {
						parts[3] = fmt.Sprintf("scripts/%s/%s", mod, parts[3])
						lines[i] = strings.Join(parts, " ")
					}
				}
				allIssues = append(allIssues, strings.Join(lines, "\n"))
			}
		}
	}

	if len(allIssues) > 0 {
		return CheckResult{}, fmt.Errorf("functions exceed complexity threshold of %d\n%s", GocycloThreshold, indentOutput(strings.Join(allIssues, "\n")))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, complexity OK", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("Complexity OK"), nil
}
