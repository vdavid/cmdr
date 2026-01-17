package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunWebsitePrettier runs Prettier on the website.
func RunWebsitePrettier(ctx *CheckContext) (CheckResult, error) {
	websiteDir := filepath.Join(ctx.RootDir, "apps", "website")

	// Count files
	findCmd := exec.Command("find", "src", "-type", "f", "(", "-name", "*.ts", "-o", "-name", "*.astro", "-o", "-name", "*.css", "-o", "-name", "*.js", ")")
	findCmd.Dir = websiteDir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	// Check which files need formatting (--list-different lists them)
	// Note: prettier exits with code 1 if files differ, so we ignore the error
	// Prettier's default behavior respects .gitignore files in current dir and parents
	checkCmd := exec.Command("pnpm", "exec", "prettier", "--list-different", ".")
	checkCmd.Dir = websiteDir
	checkOutput, _ := RunCommand(checkCmd, true)

	// Parse files that need formatting
	var needsFormat []string
	if strings.TrimSpace(checkOutput) != "" {
		needsFormat = strings.Split(strings.TrimSpace(checkOutput), "\n")
	}

	if ctx.CI {
		if len(needsFormat) > 0 {
			return CheckResult{}, fmt.Errorf("code is not formatted, run pnpm format locally\n%s", indentOutput(checkOutput))
		}
		return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}

	// Non-CI mode: format if needed
	if len(needsFormat) > 0 {
		fmtCmd := exec.Command("pnpm", "format")
		fmtCmd.Dir = websiteDir
		output, err := RunCommand(fmtCmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("prettier formatting failed\n%s", indentOutput(output))
		}
		return SuccessWithChanges(fmt.Sprintf("Formatted %d of %d %s", len(needsFormat), fileCount, Pluralize(fileCount, "file", "files"))), nil
	}

	return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
}
