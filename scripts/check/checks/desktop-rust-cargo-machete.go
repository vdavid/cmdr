package checks

import (
	"fmt"
	"os/exec"
	"path/filepath"
)

// RunCargoMachete is the fast local counterpart to cargo-udeps. It greps source
// files for `use crate;` patterns instead of compiling, so it runs in <1s on
// this codebase but has known blind spots: deps used only inside macro
// expansions or build.rs codegen need to be opted out via
// `[package.metadata.cargo-machete] ignored = [...]` in Cargo.toml.
//
// cargo-udeps remains the authoritative check (CIOnly: true); machete catches
// the common case (you removed the last `use foo;` but forgot to drop the dep)
// while iterating, udeps catches the long tail in CI.
func RunCargoMachete(ctx *CheckContext) (CheckResult, error) {
	rustDir := filepath.Join(ctx.RootDir, "apps", "desktop", "src-tauri")

	if !CommandExists("cargo-machete") {
		installCmd := exec.Command("cargo", "install", "cargo-machete", "--version", "0.9.2", "--locked")
		if output, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-machete\n%s", indentOutput(output))
		}
	}

	cmd := exec.Command("cargo", "machete")
	cmd.Dir = rustDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf(
			"cargo-machete found unused deps (false positives can be opted out via [package.metadata.cargo-machete] ignored=[\"name\"] in Cargo.toml)\n%s",
			indentOutput(output),
		)
	}

	return Success("No unused deps"), nil
}
