package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunCargoDeny enforces license and dependency policies.
func RunCargoDeny(ctx *CheckContext) (CheckResult, error) {
	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

	// Check if deny.toml exists
	denyToml := filepath.Join(rustDir, "deny.toml")
	if _, err := os.Stat(denyToml); os.IsNotExist(err) {
		return Skipped("no deny.toml"), nil
	}

	// Check if cargo-deny is installed
	if !CommandExists("cargo-deny") {
		installCmd := exec.Command("cargo", "install", "cargo-deny")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-deny: %w", err)
		}
	}

	cmd := exec.Command("cargo", "deny", "check", "licenses", "bans", "sources")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("cargo-deny check failed\n%s", indentOutput(output))
	}
	return Success("Licenses and deps OK"), nil
}
