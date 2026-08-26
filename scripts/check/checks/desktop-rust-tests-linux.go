package checks

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

// goVersion must match `.mise.toml`'s `go` entry. The container provisioning script
// below downloads this exact tarball so build.rs can invoke
// `go run scripts/download-llama-server.go` (which Tauri's beforeBuildCommand needs).
// Debian's `golang-go` apt package lags too far behind to track mise reliably.
const goVersion = "1.25.7"

// containerNextestVersion pins the container's cargo-nextest to the same version the host
// lanes install. Two reasons it can't be `latest`: the "pin every tool install" rule, and
// the contention re-run, whose profile semantics (a per-test override beating a
// profile-level `slow-timeout`) were verified against this exact version. A container
// silently drifting to a newer nextest would classify starvation differently from the host
// lanes with nothing to say it had.
const containerNextestVersion = NextestVersion

// containerKeepAlive bounds the idle container's lifetime. The container outlives the test
// exec on purpose (the contention re-run execs back into it, warm), so PID 1 is a sleep
// rather than the suite. The normal exit path is the deferred `docker rm -f`; this cap is
// what stops a hard-killed check runner (SIGKILL, so no defer) from leaving a container
// parked forever. Generous because a cold container run compiles the whole workspace, and
// a worst-case re-run adds ~12 minutes on top.
const containerKeepAlive = 4 * time.Hour

// provisionScript installs the GTK/WebKit dev libraries Tauri's compile step needs,
// plus a matching Go toolchain and cargo-nextest. It deliberately stops there: the test
// run and any contention re-run are separate `docker exec`s into the same container, so
// a re-run pays neither the provisioning nor the compile again. Each step
// short-circuits on failure via `set -e`. dpkg's architecture names (amd64 / arm64)
// line up with Go's download filenames AND with nextest's pre-built URLs
// (`https://get.nexte.st/<version>/linux` for x86, `…/linux-arm` for ARM), so a single
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
var provisionScriptTemplate = `set -e
export DEBIAN_FRONTEND=noninteractive
PROVISION_LOG=/cmdr-logs/provision.log
mkdir -p /cmdr-logs

ARCH=$(dpkg --print-architecture)
case "$ARCH" in
  amd64) NEXTEST_PLATFORM=linux ;;
  arm64) NEXTEST_PLATFORM=linux-arm ;;
  *) echo "unsupported architecture: $ARCH" >&2; exit 1 ;;
esac
NEXTEST_URL=https://get.nexte.st/%[2]s/${NEXTEST_PLATFORM}

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

curl -fsSL https://go.dev/dl/go%[1]s.linux-${ARCH}.tar.gz | tar -xz -C /usr/local
curl -LsSf "$NEXTEST_URL" | tar zxf - -C /usr/local/bin`

// buildProvisionScript fills in the pinned Go and nextest versions.
func buildProvisionScript() string {
	return fmt.Sprintf(provisionScriptTemplate, goVersion, containerNextestVersion)
}

// linuxSelectionArgs computes the cargo package selection for `linux`, NOT for this
// machine: the container is always Linux even though the host running the check is a Mac.
// Getting that wrong leaves `cmdr-fsevent-stream` in the selection set, where it fails at
// `cargo check` with `E0455: link kind 'framework' is only supported on Apple targets`.
func linuxSelectionArgs(rootDir string) ([]string, error) {
	members, err := WorkspaceMembers(rootDir)
	if err != nil {
		return nil, err
	}
	return CargoSelectionArgs(members, "linux"), nil
}

// containerNextestScript is the ONE place a `cargo nextest run` command is built for the
// container, so the main run and the contention re-run can't drift apart in selection,
// `--locked`, or PATH. Go is on PATH because `build.rs` shells out to it.
//
// Every argument is single-quoted: the re-run passes a nextest filter expression
// (`test(=a::b) + test(=c::d)`) whose spaces and parens `sh -c` would otherwise split.
func containerNextestScript(args ...string) string {
	quoted := make([]string, 0, len(args))
	for _, a := range args {
		quoted = append(quoted, shellQuote(a))
	}
	return "export PATH=/usr/local/go/bin:$PATH\ncargo nextest run " + strings.Join(quoted, " ") + " 2>&1"
}

// shellQuote wraps an argument for `sh -c`, escaping any embedded single quote.
func shellQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// RunRustTestsLinux runs Rust tests in a Linux Docker container.
// This catches platform-specific issues before CI.
//
// The container is started detached and each phase (provision, test run, contention
// re-run) is a `docker exec` into it, rather than one `docker run sh -c <everything>`.
// That's what lets a red run re-run its failures alone the way the host lanes do: the
// re-run lands in the same container, with the same toolchain and the same warm
// `CARGO_TARGET_DIR`, so it costs seconds instead of a fresh provision plus a full
// workspace rebuild. The container is removed on every exit path.
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
	// the apt log survives the container. macOS auto-cleans /tmp on reboot; we don't
	// otherwise prune.
	logDir := fmt.Sprintf("/tmp/cmdr-rust-tests-linux-%d", time.Now().Unix())
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		return CheckResult{}, fmt.Errorf("failed to create log dir: %w", err)
	}
	provisionLog := filepath.Join(logDir, "provision.log")

	selection, err := linuxSelectionArgs(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}

	container := fmt.Sprintf("cmdr-rust-tests-linux-%d-%d", os.Getpid(), time.Now().UnixNano())
	if err := startTestContainer(container, ctx.RootDir, logDir); err != nil {
		return CheckResult{}, err
	}
	defer removeTestContainer(container)

	if out, err := dockerExec(container, buildProvisionScript()); err != nil {
		return CheckResult{}, fmt.Errorf("provisioning the Linux test container failed (provision log: %s)\n%s",
			provisionLog, indentOutput(out))
	}

	testArgs := append([]string{"--locked"}, selection...)
	testArgs = append(testArgs, "--no-fail-fast")
	output, err := dockerExec(container, containerNextestScript(testArgs...))
	// Before the verdict branch, so a red run records WHICH tests went red
	// (`test-log.go`); the contention re-run below is deliberately not recorded.
	ctx.RecordTests(ParseNextestResults(output)...)
	if err != nil {
		// Same verdict machinery as the host lanes: a failure is re-run alone before it's
		// believed. This lane needs it MORE, not less. The container's cores are a slice of
		// a host that may be running three Playwright shards and four cargo processes, so a
		// starved test here looks exactly like a hung one. Measured 2026-07-30: three
		// consecutive runs of an unchanged tree each timed out on a DIFFERENT small set,
		// and one of them (`sqlite_util::tests::cached_pages_come_from_the_shared_slab`)
		// runs in 0.07 s natively.
		summary := trimRustTestProgress(trimBuildNoise(output))
		return resolveRustFailure(
			fmt.Sprintf("rust tests failed on Linux (provision log: %s)", provisionLog),
			dockerContentionRunner(container, selection),
			dockerLoadSampler(container),
			summary)
	}

	// Same retry-rescued case as the macOS lane: nextest exits 0, so without this the
	// Linux lane reports green while hiding the flake.
	if flaky := ParseFlakyTests(output); len(flaky) > 0 {
		return CheckResult{
			Code:    ResultWarning,
			Message: fmt.Sprintf("All tests passed on Linux; %s (provision log: %s)", FlakySummary(flaky), provisionLog),
			Total:   -1,
			Issues:  len(flaky),
			Changes: -1,
		}, nil
	}
	return Success(fmt.Sprintf("All tests passed on Linux (provision log: %s)", provisionLog)), nil
}

// startTestContainer brings up the detached container every phase execs into.
//
// The whole repo is mounted so cargo can find the workspace root Cargo.toml (and its
// Cargo.lock, and `.config/nextest.toml`, which is where the contention profiles live).
// Working directory is the workspace root, since the run is workspace-wide.
//
// PID 1 is a bounded `sleep`, not the suite: the container has to outlive the test exec
// for the contention re-run to reuse it. `--rm` plus the deferred removal is the normal
// path; the sleep is the backstop for a check runner that never gets to run its defers.
func startTestContainer(name, rootDir, logDir string) error {
	cmd := exec.Command("docker", "run", "-d", "--rm",
		"--name", name,
		"-v", rootDir+":/repo",
		"-v", logDir+":/cmdr-logs",
		"-w", "/repo",
		"-e", "CARGO_TARGET_DIR=/tmp/cargo-target",
		"rust:latest",
		"sleep", strconv.Itoa(int(containerKeepAlive.Seconds())))

	// Tracked BEFORE the start returns: a container that came up while the command was
	// being interrupted is exactly the one that would otherwise be orphaned.
	containerTracker.mu.Lock()
	containerTracker.names[name] = struct{}{}
	containerTracker.mu.Unlock()

	if out, err := RunCommand(cmd, true); err != nil {
		return fmt.Errorf("failed to start the Linux test container: %w\n%s", err, indentOutput(out))
	}
	return nil
}

// containerTracker holds the containers a check has running, so `KillAllProcesses` can
// remove them on Ctrl+C. The runner `os.Exit`s there, so a `defer` alone isn't enough.
var containerTracker = struct {
	mu    sync.Mutex
	names map[string]struct{}
}{names: make(map[string]struct{})}

// RemoveTrackedContainers force-removes every container a check still has running.
func RemoveTrackedContainers() {
	containerTracker.mu.Lock()
	names := make([]string, 0, len(containerTracker.names))
	for name := range containerTracker.names {
		names = append(names, name)
	}
	containerTracker.mu.Unlock()

	for _, name := range names {
		removeTestContainer(name)
	}
}

// removeTestContainer tears the container down on every exit path, pass or fail. Bounded
// so a wedged daemon can't turn cleanup into the thing that hangs the check.
func removeTestContainer(name string) {
	rmCtx, cancel := context.WithTimeout(context.Background(), dockerControlTimeout)
	defer cancel()
	_, _ = RunCommand(exec.CommandContext(rmCtx, "docker", "rm", "-f", name), true)

	containerTracker.mu.Lock()
	delete(containerTracker.names, name)
	containerTracker.mu.Unlock()
}

// dockerExec runs a shell script inside the live container and returns its combined output.
func dockerExec(container, script string) (string, error) {
	return RunCommand(exec.Command("docker", "exec", container, "sh", "-c", script), true)
}

// dockerControlTimeout bounds the small housekeeping docker calls (load sampling,
// teardown). The test execs themselves stay unbounded, as they were: their deadlines are
// nextest's, not the wall clock's.
const dockerControlTimeout = 30 * time.Second

// dockerContentionRunner re-runs the named tests inside the SAME container the failing run
// used, under one of the contention profiles. Reusing the container is what makes the
// re-run affordable: the toolchain is provisioned and `CARGO_TARGET_DIR` is warm, so
// nothing recompiles and only the named tests execute.
//
// The package selection is carried over verbatim. A re-run that selected differently could
// find no tests at all and read as "everything passed alone", which is the failure mode
// that would turn every real Linux failure into a warn.
func dockerContentionRunner(container string, selection []string) ContentionRunner {
	return func(profile string, names []string) (string, error) {
		out, err := dockerExec(container, containerNextestScript(containerRerunArgs(profile, selection, names)...))
		out = StripANSI(out)
		if err != nil && !nextestRanRE.MatchString(out) {
			return "", fmt.Errorf("contention re-run under profile %s could not run in the container: %w", profile, err)
		}
		return out, nil
	}
}

// containerRerunArgs builds one contention stage's `cargo nextest run` arguments. Pure, so
// the selection-parity and profile contract is testable without a container.
func containerRerunArgs(profile string, selection, names []string) []string {
	args := append([]string{"--locked", "--profile", profile}, selection...)
	return append(args, "-E", NextestFilterExpr(names))
}

// dockerLoadSampler answers "was the machine quiet during the re-run?" from both sides of
// the VM boundary, and takes the worse of the two.
//
// Neither number alone is enough. On macOS the host's load average sees the Linux VM as a
// handful of vCPU threads, so a container saturated from the inside (this suite, plus the
// second container the `--include-slow` lane runs) barely moves it. The container's own
// `/proc/loadavg` sees that, but is blind to the Playwright shards and cargo processes
// outside the VM that are actually competing for the same cores. Either side being busy
// means the re-run wasn't quiet.
//
// This only ever decides whether a "needed headroom" verdict is reported as real slowness
// or as inconclusive. An unreadable load reads as 0 (quiet), which is the safe direction:
// it keeps the run red rather than softening it.
func dockerLoadSampler(container string) LoadSampler {
	return func() float64 {
		return max(LoadPerCore(), containerLoadPerCore(container))
	}
}

func containerLoadPerCore(container string) float64 {
	loadCtx, cancel := context.WithTimeout(context.Background(), dockerControlTimeout)
	defer cancel()
	cmd := exec.CommandContext(loadCtx, "docker", "exec", container, "sh", "-c", "cat /proc/loadavg; nproc")
	out, err := RunCommand(cmd, true)
	if err != nil {
		return 0
	}
	return parseContainerLoadPerCore(out)
}

// parseContainerLoadPerCore reads `cat /proc/loadavg; nproc` output: the 1-minute load
// average from the first line's first field, the core count from the second line. Anything
// unparseable reports 0 rather than a guess.
func parseContainerLoadPerCore(out string) float64 {
	lines := strings.Split(strings.TrimSpace(out), "\n")
	if len(lines) < 2 {
		return 0
	}
	fields := strings.Fields(lines[0])
	if len(fields) == 0 {
		return 0
	}
	load, err := strconv.ParseFloat(fields[0], 64)
	if err != nil {
		return 0
	}
	cores, err := strconv.Atoi(strings.TrimSpace(lines[len(lines)-1]))
	if err != nil || cores <= 0 {
		return 0
	}
	return load / float64(cores)
}

var compilingLineRe = regexp.MustCompile(`(?m)^\s*Compiling \w+ v`)

// trimBuildNoise drops cargo's pre-test build chatter by keeping everything
// after the last `Compiling …` line. If no Compiling line exists (nothing
// needed rebuilding, or the failure came before cargo got that far), the
// output is returned as-is. Apt is silenced at source via -qq +
// DEBIAN_FRONTEND=noninteractive in provisionScript, and provisioning is its
// own exec, so a no-Compiling failure already comes back clean.
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
//	              `        PASS [   0.094s] (  42/4802) cmdr_lib foo::bar`
//
// The progress counter is its own optional group because nextest right-aligns the
// index to the total's width, so it can hold spaces (`(  42/4802)`). Folding it
// into the binary field instead let exactly the 1-99 range slip through the filter.
//
// Anchored to the start of the line (with optional leading whitespace for the
// nextest form) so panic-message bodies that quote these phrases can't be
// misclassified. FAIL/LEAK/TIMEOUT/SLOW/bench results and every non-test line
// fall through unchanged.
var testProgressNoiseRE = regexp.MustCompile(
	`^(?:test .+ \.\.\. (?:ok|ignored(?:, .*)?)|\s+(?:PASS|SKIP) \[[^\]]*\]\s+(?:\([^)]*\)\s+)?\S+ \S.*)$`,
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
	// Normalise first: nextest colours its output under a forced-colour environment, and
	// every pattern here is line-anchored, so an unnormalised buffer keeps all ~6 000
	// progress lines and buries the diagnosis under them.
	lines := strings.Split(StripANSI(output), "\n")
	kept := make([]string, 0, len(lines))
	for _, line := range lines {
		if testProgressNoiseRE.MatchString(line) {
			continue
		}
		kept = append(kept, line)
	}
	return strings.Join(kept, "\n")
}
