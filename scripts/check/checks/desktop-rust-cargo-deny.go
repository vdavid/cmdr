package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// RunCargoDeny enforces license and dependency policies.
//
// `deny.toml` lives at the workspace root, and the check runs from there. The
// policy was always workspace-wide in effect — cargo-deny reads the whole
// `cargo metadata` graph regardless of where it starts — but the config sat inside
// one member, which read as if it governed only that member. With more than one
// crate in the graph, that reading would have been wrong in a way nobody noticed.
func RunCargoDeny(ctx *CheckContext) (CheckResult, error) {
	if _, err := os.Stat(DenyConfigPath(ctx.RootDir)); os.IsNotExist(err) {
		return Skipped("no deny.toml"), nil
	}

	// Check if cargo-deny is installed
	if !CommandExists("cargo-deny") {
		installCmd := exec.Command("cargo", "install", "cargo-deny", "--version", "0.19.6", "--locked")
		if _, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-deny: %w", err)
		}
	}

	// `advisories` is in the set: `deny.toml` scopes the graph to the macOS targets we
	// ship and limits `unmaintained` to workspace crates, so this lane stays green on
	// Tauri's unfixable transitive noise while still failing on a real RUSTSEC
	// vulnerability anywhere in a shipped dependency.
	cmd := exec.Command("cargo", "deny", "check", "advisories", "licenses", "bans", "sources")
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("cargo-deny check failed\n%s", indentOutput(output))
	}
	return Success("Advisories, licenses, and deps OK"), nil
}

// DenyConfigPath is the single place the cargo-deny config's location is written
// down. `desktop-third-party-notices` derives its accepted-license list from the
// same file rather than duplicating it, so both have to agree on where it is.
func DenyConfigPath(rootDir string) string {
	return filepath.Join(rootDir, "deny.toml")
}
