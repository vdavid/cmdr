package checks

import (
	"fmt"
	"os/exec"
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
	// machete walks DIRECTORIES, not the cargo graph, so it's handed each member's
	// path explicitly rather than the repo root. Pointing it at the root would also
	// sweep in `benchmarks/smb`, which `[workspace] exclude` deliberately keeps out
	// of the workspace and which carries unused deps of its own.
	members, err := WorkspaceMembers(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	args := []string{"machete"}
	for _, m := range members {
		args = append(args, m.Dir)
	}

	if !CommandExists("cargo-machete") {
		installCmd := exec.Command("cargo", "install", "cargo-machete", "--version", "0.9.2", "--locked")
		if output, err := RunCommand(installCmd, true); err != nil {
			return CheckResult{}, fmt.Errorf("failed to install cargo-machete\n%s", indentOutput(output))
		}
	}

	cmd := exec.Command("cargo", args...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf(
			"cargo-machete found unused deps (false positives can be opted out via [package.metadata.cargo-machete] ignored=[\"name\"] in Cargo.toml)\n%s",
			indentOutput(output),
		)
	}

	return Success(fmt.Sprintf("No unused deps in %d workspace %s",
		len(members), Pluralize(len(members), "member", "members"))), nil
}
