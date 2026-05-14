package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

// goVersion must match `.mise.toml`'s `go` entry. The container provisioning script
// below downloads this exact tarball so build.rs can invoke
// `go run scripts/download-llama-server.go` (which Tauri's beforeBuildCommand needs).
// Debian's `golang-go` apt package lags too far behind to track mise reliably.
const goVersion = "1.25.7"

// provisionScript installs the GTK/WebKit dev libraries Tauri's compile step needs,
// plus a matching Go toolchain and cargo-nextest, then runs the test suite. Each step
// short-circuits on failure via `set -e`. dpkg's architecture names (amd64 / arm64)
// line up with Go's download filenames AND with nextest's pre-built URLs
// (`https://get.nexte.st/latest/linux` for x86, `…/linux-arm` for ARM), so a single
// $(dpkg --print-architecture) covers both. Installing the wrong-arch nextest binary
// caused a silent OrbStack crash on Apple Silicon (`Dynamic loader not found:
// /lib64/ld-linux-x86-64.so.2`). Cargo triggers a rustup toolchain sync, then execs
// nextest, which is when the x86 binary hit the arm64 dynamic-loader wall.
//
// nextest (vs raw `cargo test`) is required: a handful of tests (e.g.
// `ai::api_keys::tests::*`) rely on per-test process isolation because the underlying
// secret-store backend caches `CMDR_DATA_DIR` in a `LazyLock` on first access. `cargo
// test` runs siblings as threads in one process and silently shares that cache,
// producing cross-test state leaks. nextest spawns a fresh process per test, matching
// macOS local and CI behavior. Precompiled binary from get.nexte.st (no `cargo install`
// recompile) keeps the cold-cache run fast.
//
// Apt output: silenced via -qq + DEBIAN_FRONTEND=noninteractive + redirection to
// /cmdr-logs/provision.log (host-mounted to a per-run dir under /tmp). On success the
// log file is preserved for post-mortem; on apt failure the full log is dumped to
// stderr (captured by the Go side and shown to the user). The Success message
// includes the host log path so it's discoverable in the 1% case where someone wants
// to inspect what got installed.
var provisionScript = fmt.Sprintf(`set -e
export DEBIAN_FRONTEND=noninteractive
PROVISION_LOG=/cmdr-logs/provision.log
mkdir -p /cmdr-logs

ARCH=$(dpkg --print-architecture)
case "$ARCH" in
  amd64) NEXTEST_URL=https://get.nexte.st/latest/linux ;;
  arm64) NEXTEST_URL=https://get.nexte.st/latest/linux-arm ;;
  *) echo "unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

{
  echo "=== apt-get update ==="
  apt-get update -qq
  echo "=== apt-get install ==="
  apt-get install -y -qq --no-install-recommends \
    libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libacl1-dev \
    curl ca-certificates
} >> "$PROVISION_LOG" 2>&1 || {
  echo "--- apt failed; full provision log follows ---" >&2
  cat "$PROVISION_LOG" >&2
  exit 1
}

curl -fsSL https://go.dev/dl/go%s.linux-${ARCH}.tar.gz | tar -xz -C /usr/local
export PATH=/usr/local/go/bin:$PATH
curl -LsSf "$NEXTEST_URL" | tar zxf - -C /usr/local/bin
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

	// Per-run host log dir, bind-mounted into the container at /cmdr-logs so
	// the apt log survives `--rm`. macOS auto-cleans /tmp on reboot; we don't
	// otherwise prune.
	logDir := fmt.Sprintf("/tmp/cmdr-rust-tests-linux-%d", time.Now().Unix())
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		return CheckResult{}, fmt.Errorf("failed to create log dir: %w", err)
	}
	provisionLog := filepath.Join(logDir, "provision.log")

	// Mount the whole repo so cargo can find the workspace root Cargo.toml (and its Cargo.lock).
	// Working directory is the Rust crate inside the workspace.
	cmd := exec.Command("docker", "run", "--rm",
		"-v", ctx.RootDir+":/repo",
		"-v", logDir+":/cmdr-logs",
		"-w", "/repo/apps/desktop/src-tauri",
		"-e", "CARGO_TARGET_DIR=/tmp/cargo-target",
		"rust:latest",
		"sh", "-c", provisionScript)
	output, err := RunCommand(cmd, true)
	if err != nil {
		summary := trimRustTestProgress(trimBuildNoise(output))
		return CheckResult{}, fmt.Errorf("rust tests failed on Linux (provision log: %s)\n%s", provisionLog, indentOutput(summary))
	}
	return Success(fmt.Sprintf("All tests passed on Linux (provision log: %s)", provisionLog)), nil
}

var compilingLineRe = regexp.MustCompile(`(?m)^\s*Compiling \w+ v`)

// trimBuildNoise drops cargo's pre-compile setup (apt/dpkg/rustup chatter)
// by keeping everything after the last `Compiling …` line. If no Compiling
// line exists (provisioning failed before cargo ran), the output is returned
// as-is. Apt is silenced at source via -qq + DEBIAN_FRONTEND=noninteractive
// in provisionScript, so a no-Compiling failure already comes back clean
// (rustup info + actual error, no apt noise to filter).
//
// Nothing is ever truncated by length: if the test run produces 500 lines of
// real failures, all 500 survive.
func trimBuildNoise(output string) string {
	if locs := compilingLineRe.FindAllStringIndex(output, -1); len(locs) > 0 {
		lastEnd := locs[len(locs)-1][1]
		if nl := strings.IndexByte(output[lastEnd:], '\n'); nl >= 0 {
			if trimmed := strings.TrimLeft(output[lastEnd+nl+1:], "\n"); trimmed != "" {
				return trimmed
			}
		}
	}
	return output
}

// testProgressNoiseRE matches per-test pass/skip lines that are pure noise on
// a failure. Two formats are recognised:
//
//	cargo test    `test foo::bar ... ok`
//	              `test foo::bar ... ignored, <reason>`
//	cargo nextest `        PASS [   0.001s] cmdr_lib foo::bar`
//	              `        SKIP [   0.001s] cmdr_lib foo::bar`
//	              `        PASS [   0.001s] cmdr_lib foo::bar (reason)`
//
// Anchored to the start of the line (with optional leading whitespace for the
// nextest form) so panic-message bodies that quote these phrases can't be
// misclassified. FAIL/LEAK/TIMEOUT/SLOW/bench results and every non-test line
// fall through unchanged.
var testProgressNoiseRE = regexp.MustCompile(
	`^(?:test .+ \.\.\. (?:ok|ignored(?:, .*)?)|\s+(?:PASS|SKIP) \[[^\]]*\] \S+ \S.*)$`,
)

// trimRustTestProgress drops `test … ok` / `test … ignored…` / nextest
// `PASS [...]` and `SKIP [...]` lines from cargo test or cargo nextest
// output. Everything else is kept verbatim: `running N tests` headers,
// FAIL/FAILED markers, the `failures:` block (panic stdout + listing), the
// `test result:` / `Summary` tally, `error:` lines, and any other text.
//
// The filter is single-pass and per-line, so it survives weird interleaving
// (multiple test binaries, multi-line panic messages, debconf noise after the
// suite exits) and can only ever keep too much, never drop a real signal.
func trimRustTestProgress(output string) string {
	lines := strings.Split(output, "\n")
	kept := make([]string, 0, len(lines))
	for _, line := range lines {
		if testProgressNoiseRE.MatchString(line) {
			continue
		}
		kept = append(kept, line)
	}
	return strings.Join(kept, "\n")
}
