package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// RunCargoUdeps detects unused dependencies.
func RunCargoUdeps(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	rustDir := filepath.Join(desktopDir, "src-tauri")

	// Ensure llama-server resource exists (downloads on macOS, creates placeholder on Linux)
	downloadCmd := exec.Command("go", "run", "scripts/download-llama-server.go")
	downloadCmd.Dir = desktopDir
	if output, err := RunCommand(downloadCmd, true); err != nil {
		return CheckResult{}, fmt.Errorf("failed to prepare llama-server resource\n%s", indentOutput(output))
	}

	// Check if cargo-udeps is installed
	if !CommandExists("cargo-udeps") {
		installCmd := exec.Command("cargo", "install", "cargo-udeps", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-udeps: %w", err)
		}
	}

	// cargo-udeps requires nightly
	cmd := exec.Command("cargo", "+nightly", "udeps", "--all-targets")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		// Check if nightly is not installed
		if strings.Contains(output, "toolchain 'nightly'") {
			installCmd := exec.Command("rustup", "toolchain", "install", "nightly")
			if _, err := RunCommand(installCmd, true); err != nil {
				return CheckResult{}, fmt.Errorf("failed to install nightly")
			}
			// Retry
			cmd = exec.Command("cargo", "+nightly", "udeps", "--all-targets")
			cmd.Dir = rustDir
			output, err = RunCommand(cmd, true)
		}
		if err != nil {
			return CheckResult{}, fmt.Errorf("unused dependencies found\n%s", indentOutput(output))
		}
	}
	return Success("No unused deps"), nil
}
