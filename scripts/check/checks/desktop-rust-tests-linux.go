package checks

import (
	"fmt"
	"os/exec"
	"regexp"
	"strings"
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
		return CheckResult{}, fmt.Errorf("rust tests failed on Linux\n%s", indentOutput(trimBuildNoise(output)))
	}
	return Success("All tests passed on Linux"), nil
}

var compilingLineRe = regexp.MustCompile(`(?m)^\s*Compiling \w+ v`)

// trimBuildNoise strips apt-get and cargo compilation output, keeping
// everything after the last "Compiling ..." line. Falls back to the
// last 50 lines if no Compiling line is found (e.g. build failure).
func trimBuildNoise(output string) string {
	locs := compilingLineRe.FindAllStringIndex(output, -1)
	if len(locs) > 0 {
		lastEnd := locs[len(locs)-1][1]
		// Find the end of that line
		nl := strings.IndexByte(output[lastEnd:], '\n')
		if nl >= 0 {
			trimmed := strings.TrimLeft(output[lastEnd+nl+1:], "\n")
			if trimmed != "" {
				return trimmed
			}
		}
	}

	// Fallback: show last 50 lines
	lines := strings.Split(output, "\n")
	if len(lines) > 50 {
		return strings.Join(lines[len(lines)-50:], "\n")
	}
	return output
}
