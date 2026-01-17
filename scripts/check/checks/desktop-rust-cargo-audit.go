package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// RunCargoAudit checks for security vulnerabilities.
func RunCargoAudit(ctx *CheckContext) (CheckResult, error) {
	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

	// Check if cargo-audit is installed
	if !CommandExists("cargo-audit") {
		installCmd := exec.Command("cargo", "install", "cargo-audit")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-audit: %w", err)
		}
	}

	cmd := exec.Command("cargo", "audit")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		// Check if it's just informational (no vulnerabilities found)
		if strings.Contains(output, "0 vulnerabilities found") {
			return Success("No vulnerabilities found"), nil
		}
		return CheckResult{}, fmt.Errorf("security vulnerabilities found\n%s", indentOutput(output))
	}

	// Extract crate count from output
	re := regexp.MustCompile(`Scanning (\d+) crates?`)
	matches := re.FindStringSubmatch(output)
	if len(matches) > 1 {
		count, _ := strconv.Atoi(matches[1])
		return Success(fmt.Sprintf("Scanned %d %s", count, Pluralize(count, "crate", "crates"))), nil
	}
	return Success("No vulnerabilities found"), nil
}
