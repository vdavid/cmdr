package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunRustfmt formats Rust code.
func RunRustfmt(ctx *CheckContext) (CheckResult, error) {
	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

	// Count .rs files for the message
	findCmd := exec.Command("find", "src", "-name", "*.rs", "-type", "f")
	findCmd.Dir = rustDir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	if findOutput == "" {
		fileCount = 0
	}

	// Check which files need formatting (--files-with-diff lists them)
	checkCmd := exec.Command("cargo", "fmt", "--", "--check", "--files-with-diff")
	checkCmd.Dir = rustDir
	checkOutput, checkErr := RunCommand(checkCmd, true)

	// Parse files that need formatting
	var needsFormat []string
	if strings.TrimSpace(checkOutput) != "" {
		for _, line := range strings.Split(strings.TrimSpace(checkOutput), "\n") {
			// Only count lines that look like file paths (end with .rs)
			if strings.HasSuffix(line, ".rs") {
				needsFormat = append(needsFormat, line)
			}
		}
	}

	if ctx.CI {
		if checkErr != nil || len(needsFormat) > 0 {
			return CheckResult{}, fmt.Errorf("code is not formatted, run cargo fmt locally\n%s", indentOutput(checkOutput))
		}
		result := Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		result.Issues = 0
		result.Changes = 0
		return result, nil
	}

	// Non-CI mode: format if needed
	if len(needsFormat) > 0 {
		fmtCmd := exec.Command("cargo", "fmt")
		fmtCmd.Dir = rustDir
		output, err := RunCommand(fmtCmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("rust formatting failed\n%s", indentOutput(output))
		}
		result := SuccessWithChanges(fmt.Sprintf("Formatted %d of %d %s", len(needsFormat), fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		result.Issues = len(needsFormat)
		result.Changes = len(needsFormat)
		return result, nil
	}

	result := Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files")))
	result.Total = fileCount
	result.Issues = 0
	result.Changes = 0
	return result, nil
}
