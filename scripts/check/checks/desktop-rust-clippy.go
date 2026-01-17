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
	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")
	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("cargo", "clippy", "--all-targets", "--", "-D", "warnings")
	} else {
		cmd = exec.Command("cargo", "clippy", "--all-targets", "--fix", "--allow-dirty", "--allow-staged", "--", "-D", "warnings")
	}
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
