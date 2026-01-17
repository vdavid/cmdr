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

	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("cargo", "fmt", "--check")
	} else {
		cmd = exec.Command("cargo", "fmt")
	}
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("code is not formatted, run cargo fmt locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("rust formatting failed\n%s", indentOutput(output))
	}

	if ctx.CI {
		return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}

	// In non-CI mode, check if any files were changed
	gitCmd := exec.Command("git", "diff", "--name-only", "--", "*.rs")
	gitCmd.Dir = rustDir
	gitOutput, _ := RunCommand(gitCmd, true)
	changedFiles := strings.Split(strings.TrimSpace(gitOutput), "\n")
	if len(changedFiles) == 1 && changedFiles[0] == "" {
		return Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	changedCount := len(changedFiles)
	return Success(fmt.Sprintf("Formatted %d of %d %s", changedCount, fileCount, Pluralize(fileCount, "file", "files"))), nil
}
