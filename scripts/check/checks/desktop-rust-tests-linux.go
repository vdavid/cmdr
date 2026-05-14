package checks

import (
	"fmt"
	"os/exec"
	"regexp"
	"strings"
)

// goVersion must match `.mise.toml`'s `go` entry. The container provisioning script
// below downloads this exact tarball so build.rs can invoke
// `go run scripts/download-llama-server.go` (which Tauri's beforeBuildCommand needs).
// Debian's `golang-go` apt package lags too far behind to track mise reliably.
const goVersion = "1.25.7"

// provisionScript installs the GTK/WebKit dev libraries Tauri's compile step needs,
// plus a matching Go toolchain and cargo-nextest, then runs the test suite. Each step
// short-circuits on failure via `set -e`. dpkg's architecture names (amd64 / arm64)
// line up with Go's download filenames, so a single $(dpkg --print-architecture)
// covers both x86 and ARM.
//
// nextest (vs raw `cargo test`) is required: a handful of tests (e.g.
// `ai::api_keys::tests::*`) rely on per-test process isolation because the underlying
// secret-store backend caches `CMDR_DATA_DIR` in a `LazyLock` on first access. `cargo
// test` runs siblings as threads in one process and silently shares that cache,
// producing cross-test state leaks. nextest spawns a fresh process per test, matching
// macOS local and CI behavior. Precompiled binary from get.nexte.st (no `cargo install`
// recompile) keeps the cold-cache run fast.
var provisionScript = fmt.Sprintf(`set -e
apt-get update
apt-get install -y --no-install-recommends \
  libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libacl1-dev \
  curl ca-certificates
curl -fsSL https://go.dev/dl/go%s.linux-$(dpkg --print-architecture).tar.gz | tar -xz -C /usr/local
export PATH=/usr/local/go/bin:$PATH
curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /usr/local/bin
cargo nextest run --no-fail-fast 2>&1`, goVersion)

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
		"sh", "-c", provisionScript)
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
