package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
)

// RunClippy runs Clippy linter with auto-fix.
func RunClippy(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	rustDir := filepath.Join(desktopDir, "src-tauri")

	// Ensure llama-server resource exists (downloads on macOS, creates placeholder on Linux)
	downloadCmd := exec.Command("go", "run", "scripts/download-llama-server.go")
	downloadCmd.Dir = desktopDir
	if output, err := RunCommand(downloadCmd, true); err != nil {
		return CheckResult{}, fmt.Errorf("failed to prepare llama-server resource\n%s", indentOutput(output))
	}

	// Touch lib.rs to force clippy to re-lint (otherwise cached builds skip linting)
	libPath := filepath.Join(rustDir, "src", "lib.rs")
	touchCmd := exec.Command("touch", libPath)
	_ = touchCmd.Run() // Ignore errors, file might not exist in edge cases

	// In local mode, first run with --fix to auto-fix what we can
	if !ctx.CI {
		fixCmd := exec.Command("cargo", "clippy", "--all-targets", "--fix", "--allow-dirty", "--allow-staged")
		fixCmd.Dir = rustDir
		_, _ = RunCommand(fixCmd, true) // Ignore errors, we'll catch them in the check run
	}

	// Run clippy WITHOUT --fix to check for remaining issues (--fix ignores -D warnings)
	cmd := exec.Command("cargo", "clippy", "--all-targets", "--", "-D", "warnings")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("clippy errors found, run the check script locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("clippy found unfixable issues\n%s", indentOutput(output))
	}

	// Try to extract "Compiling X crates" from output
	re := regexp.MustCompile(`Compiling (\d+) crates?`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("Checked %d %s, no warnings", count, Pluralize(count, "crate", "crates"))), nil
	}

	// Fallback: count "Checking" lines
	re2 := regexp.MustCompile(`(?m)^\s*Checking`)
	checkingMatches := re2.FindAllString(output, -1)
	if len(checkingMatches) > 0 {
		count := len(checkingMatches)
		return Success(fmt.Sprintf("Checked %d %s, no warnings", count, Pluralize(count, "crate", "crates"))), nil
	}

	return Success("No warnings"), nil
}
