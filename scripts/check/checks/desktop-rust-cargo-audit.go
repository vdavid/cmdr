package checks

import (
	"fmt"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
)

// RunCargoAudit checks for security vulnerabilities.
func RunCargoAudit(ctx *CheckContext) (CheckResult, error) {
	// Check if cargo-audit is installed
	if !CommandExists("cargo-audit") {
		installCmd := exec.Command("cargo", "install", "cargo-audit")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-audit: %w", err)
		}
	}

	// Ignore advisories for upstream Tauri dependencies with no fix available:
	// RUSTSEC-2023-0071: rsa timing sidechannel (via sspi → smb, no fix released)
	// RUSTSEC-2024-0413..0416: gtk-rs GTK3 bindings unmaintained (used by wry/tao)
	// RUSTSEC-2024-0421..0424: gtk-sys unmaintained variants
	cmd := exec.Command("cargo", "audit",
		"--ignore", "RUSTSEC-2023-0071",
		"--ignore", "RUSTSEC-2024-0413",
		"--ignore", "RUSTSEC-2024-0414",
		"--ignore", "RUSTSEC-2024-0415",
		"--ignore", "RUSTSEC-2024-0416",
		"--ignore", "RUSTSEC-2024-0421",
		"--ignore", "RUSTSEC-2024-0422",
		"--ignore", "RUSTSEC-2024-0423",
		"--ignore", "RUSTSEC-2024-0424",
	)
	cmd.Dir = ctx.RootDir
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
