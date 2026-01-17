package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
)

// RunJscpdRust detects code duplication in Rust files.
func RunJscpdRust(ctx *CheckContext) (CheckResult, error) {
	rustSrcDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri", "src")

	// Check if jscpd is available via npx
	cmd := exec.Command("npx", "jscpd", "--version")
	if _, err := RunCommand(cmd, true); err != nil {
		installCmd := exec.Command("npm", "install", "-g", "jscpd")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install jscpd: %w", err)
		}
	}

	// Run jscpd on Rust source files
	cmd = exec.Command("npx", "jscpd",
		rustSrcDir,
		"--format", "rust",
		"--min-lines", "5",
		"--min-tokens", "100",
		"--threshold", "2",
		"--ignore", "**/test*.rs,**/*_test.rs",
		"--reporters", "console",
	)
	output, err := RunCommand(cmd, true)
	if err != nil {
		if strings.Contains(output, "duplicated lines") || strings.Contains(output, "threshold") {
			return CheckResult{}, fmt.Errorf("code duplication exceeds threshold (2%%)\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("jscpd failed\n%s", indentOutput(output))
	}

	// Parse duplication percentage
	re := regexp.MustCompile(`(\d+\.?\d*)% \(`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		return Success(fmt.Sprintf("%s%% duplication", matches[1])), nil
	}
	return Success("No significant duplication"), nil
}
