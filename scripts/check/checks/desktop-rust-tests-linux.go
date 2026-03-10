package checks

import (
	"fmt"
	"os/exec"
)

// RunRustTestsLinux runs Rust tests in a Linux Docker container.
// This catches platform-specific issues before CI.
func RunRustTestsLinux(ctx *CheckContext) (CheckResult, error) {
	// Check if Docker is available
	if !CommandExists("docker") {
		return Skipped("Docker not installed"), nil
	}

	// Check if Docker daemon is running
	checkCmd := exec.Command("docker", "info")
	if _, err := RunCommand(checkCmd, true); err != nil {
		return Skipped("Docker not running"), nil
	}

	// Mount the whole repo so cargo can find the workspace root Cargo.toml (and its Cargo.lock).
	// Working directory is the Rust crate inside the workspace.
	cmd := exec.Command("docker", "run", "--rm",
		"-v", ctx.RootDir+":/repo",
		"-w", "/repo/apps/desktop/src-tauri",
		"-e", "CARGO_TARGET_DIR=/tmp/cargo-target",
		"rust:latest",
		"sh", "-c", "apt-get update && apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libacl1-dev && cargo test --no-fail-fast")
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("rust tests failed on Linux\n%s", indentOutput(output))
	}
	return Success("All tests passed on Linux"), nil
}
