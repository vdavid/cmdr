package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

// nightlyToolchain is the exact nightly cargo-udeps runs on. Pinned for the same
// reason `rust-toolchain.toml` pins stable: a floating `+nightly` means a lint
// tightening upstream breaks the scheduled "Slow checks" job at a random time
// (and a compromised nightly would land transparently). This is the single
// source of truth; CI installs it by asking the check tool
// (`./scripts/check/check --print-nightly`), so don't repeat the date anywhere.
//
// To bump: pick a nightly at least 3 days old, edit this line, then run
// `pnpm check cargo-udeps` locally and fix whatever new lints it surfaces.
const nightlyToolchain = "nightly-2026-07-10"

// NightlyToolchain exposes the pinned nightly to the CLI (`--print-nightly`).
func NightlyToolchain() string { return nightlyToolchain }

// RunCargoUdeps detects unused dependencies.
func RunCargoUdeps(ctx *CheckContext) (CheckResult, error) {
	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	rustDir := filepath.Join(desktopDir, "src-tauri")

	// Ensure llama-server binaries exist (downloads on macOS, creates placeholder on Linux)
	downloadCmd := exec.Command("go", "run", "scripts/download-llama-server.go")
	downloadCmd.Dir = desktopDir
	if output, err := RunCommand(downloadCmd, true); err != nil {
		return CheckResult{}, fmt.Errorf("failed to prepare llama-server binaries\n%s", indentOutput(output))
	}

	// Check if cargo-udeps is installed
	if !CommandExists("cargo-udeps") {
		installCmd := exec.Command("cargo", "install", "cargo-udeps", "--version", "0.1.61", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-udeps: %w", err)
		}
	}

	// cargo-udeps requires nightly, and we install the pinned one up front rather
	// than reacting to a failed run: reading rustup's inventory is unambiguous,
	// while classifying a cargo failure would mean matching on its message.
	if err := ensureNightlyToolchain(); err != nil {
		return CheckResult{}, err
	}

	cmd := exec.Command("cargo", "+"+nightlyToolchain, "udeps", "--locked", "--all-targets")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("unused dependencies found\n%s", indentOutput(output))
	}
	return Success("No unused deps"), nil
}

// ensureNightlyToolchain installs the pinned nightly when rustup doesn't have it.
func ensureNightlyToolchain() error {
	listCmd := exec.Command("rustup", "toolchain", "list")
	listed, err := RunCommand(listCmd, true)
	if err != nil {
		return fmt.Errorf("failed to list rustup toolchains\n%s", indentOutput(listed))
	}
	if strings.Contains(listed, nightlyToolchain) {
		return nil
	}
	installCmd := exec.Command("rustup", "toolchain", "install", nightlyToolchain, "--profile", "minimal")
	if output, err := RunCommand(installCmd, true); err != nil {
		return fmt.Errorf("failed to install %s\n%s", nightlyToolchain, indentOutput(output))
	}
	return nil
}
