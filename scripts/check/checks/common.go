package checks

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
)

// App represents the application a check belongs to.
type App string

const (
	AppDesktop   App = "desktop"
	AppWebsite   App = "website"
	AppApiServer App = "api-server"
	AppDashboard App = "dashboard"
	AppScripts   App = "scripts"
	// AppCrates is the shared Rust crates under `crates/`. It exists so
	// crate-boundary checks have a selector of their own; the crates' code is also
	// covered by the desktop Rust lanes, which run workspace-wide.
	AppCrates App = "crates"
	AppOther  App = "other"
)

// AppDisplayName returns a human-readable name for an app with icon.
func AppDisplayName(app App) string {
	switch app {
	case AppDesktop:
		return "🖥️  Desktop"
	case AppWebsite:
		return "🌐 Website"
	case AppApiServer:
		return "🌐 API server"
	case AppDashboard:
		return "📊 Analytics dashboard"
	case AppCrates:
		return "📦 Crates"
	case AppScripts:
		return "📜 Scripts"
	default:
		return string(app)
	}
}

// ResultCode indicates the outcome of a check.
type ResultCode int

const (
	ResultSuccess ResultCode = iota
	ResultWarning
	ResultSkipped
)

// CheckResult is returned by checks on success.
type CheckResult struct {
	Code        ResultCode
	Message     string
	MadeChanges bool // true if the check modified files (for example, formatted code)
	Total       int  // items checked (-1 = N/A)
	Issues      int  // items needing attention (-1 = N/A)
	Changes     int  // files modified (-1 = N/A)
}

// Success creates a success result with the given message (no changes made).
func Success(message string) CheckResult {
	return CheckResult{Code: ResultSuccess, Message: message, Total: -1, Issues: -1, Changes: -1}
}

// SuccessWithChanges creates a success result indicating files were modified.
func SuccessWithChanges(message string) CheckResult {
	return CheckResult{Code: ResultSuccess, Message: message, MadeChanges: true, Total: -1, Issues: -1, Changes: -1}
}

// Skipped creates a skipped result with the given reason.
func Skipped(reason string) CheckResult {
	return CheckResult{Code: ResultSkipped, Message: reason, Total: -1, Issues: -1, Changes: -1}
}

// CheckContext holds the context for running checks.
type CheckContext struct {
	CI      bool
	RootDir string
}

// CheckFunc is the function signature for check implementations.
type CheckFunc func(ctx *CheckContext) (CheckResult, error)

// CheckDefinition defines a check's metadata and implementation.
type CheckDefinition struct {
	ID                string
	Nickname          string // Short alias shown in --help and accepted by --check (if empty, ID is used)
	DisplayName       string
	App               App
	Tech              string
	IsSlow            bool
	IsFast            bool // true = included in --fast (pre-commit lane). Curated, not derived.
	CIOnly            bool // true = run only when --ci is set (or when explicitly named via --check)
	FreestyleIncompat bool // true = can NOT run on freestyle.sh VMs (Rust compilation, Docker, etc.)
	// NeedsSmb declares that this check requires the smb-consumer Docker stack
	// to be running. The check runner manages the stack lifecycle for the union
	// of selected checks with this flag (see scripts/check/smb_orchestrator.go).
	// Without it, each such check tried to own the lifecycle itself and parallel
	// runs trampled each other via stop.sh.
	NeedsSmb SmbMode // "" = no SMB needed; "core" = integration tests; "e2e" = e2e tests
	// CpuWeight is the average number of CPU cores this check keeps busy while it
	// runs (cold/working profile, rounded). The runner admits checks so the sum
	// of concurrent weights stays within the core budget, so two CPU-heavy checks
	// don't pile on top of each other and oversubscribe the machine. 0 means
	// unmeasured and is treated as 1 (light). Calibrated from the contention
	// sweep in `docs/notes/check-cpu-contention.md`; weights account for Docker-VM
	// CPU too (`rust-tests-linux` / `e2e-linux` burn cores in the VM that the host
	// process never shows).
	CpuWeight int
	// Exclusive names a resource this check needs to itself: two checks naming
	// the same one never run at the same time, whatever the CPU budget allows.
	// CpuWeight can't express this, because the constraint isn't cores. Cargo
	// takes an EXCLUSIVE lock on its build directory for a whole command, so two
	// cargo lanes sharing `target/` serialize no matter what the runner does; the
	// only question is whether they do it visibly. Undeclared, the loser sits on
	// "Blocking waiting for file lock on build directory" while still holding its
	// weight, so the budget it reserved goes unused and a quiet run looks hung.
	// Declaring it costs no wall clock (they were serial already) and hands that
	// weight back to the lanes that can actually use it.
	Exclusive string
	// NotInCI documents WHY this check intentionally has no step in any GitHub
	// workflow. The ci-coverage check enforces the invariant both ways: a check
	// that's neither referenced by a workflow nor carrying a NotInCI reason
	// fails the suite (someone added a check and forgot to wire it into CI),
	// and a check that has a reason AND a workflow reference also fails (the
	// reason went stale — remove it). Empty = must be referenced in a workflow.
	NotInCI string
	// Inputs lists path globs (relative to repo root) describing what this check
	// reads. The input-fingerprint cache (see scripts/check/checks/fingerprint.go)
	// hashes the union of these plus the global inputs (GlobalInputs) and skips
	// the check when nothing in its input set changed since it last passed. Be
	// conservative: when unsure whether a check reads a path, include it. A
	// too-wide list only costs speed; a too-narrow one costs correctness (a real
	// change goes unchecked). CI is the authoritative backstop (--ci runs fresh),
	// so a wrong list here can't ship a regression, only mask one locally until
	// the next CI run. Globs use git pathspec semantics (`dir/**` matches
	// everything under dir). An empty Inputs list means the check is fingerprinted
	// on the global inputs alone, so it re-runs on any toolchain/runner change but
	// nothing else — only correct for checks that genuinely read no repo files.
	Inputs    []string
	DependsOn []string
	Run       CheckFunc
}

// ResourceCargoBuildDir is the `Exclusive` resource for the shared cargo build
// directory. Declare it on every check whose cargo command COMPILES against
// `target/` (`clippy`, `nextest`, `udeps`, anything regenerating through a test
// run). Commands that only read metadata (`cargo metadata`, `cargo about`,
// `cargo deny`, `cargo machete`) take the package-cache lock, never the build
// one, so they stay undeclared and keep running alongside everything.
//
// Two lanes deliberately don't hold it: `rust-tests-linux` builds inside its
// container's own `CARGO_TARGET_DIR`, and `rustdoc` owns a private directory
// (see `desktop-rust-rustdoc.go`).
const ResourceCargoBuildDir = "cargo-build-dir"

// GlobalInputs are paths that affect every check's fingerprint regardless of its
// own Inputs: a toolchain bump (.mise.toml), an edit to the runner's own source
// (scripts/check/**, which includes the registry where Inputs lists live), or a
// change to the root lockfiles every check's tooling resolves against. Mirrors
// the ".mise.toml + ci.yml in every filter" rule in ci.yml's change-detection
// block. Conservative by design: scripts/check/** alone means any edit to a
// check invalidates the whole cache, which is correct (the runner's behavior
// just changed) and cheap (the next run re-establishes it).
var GlobalInputs = []string{
	".mise.toml",
	"scripts/check/**",
}

// EffectiveCpuWeight returns the scheduling weight clamped to [1, capacity], so
// an unset weight counts as light (1) and an over-budget weight can still run
// once nothing else holds the budget (it never deadlocks the admission gate).
func (c *CheckDefinition) EffectiveCpuWeight(capacity int) int {
	w := c.CpuWeight
	if w < 1 {
		w = 1
	}
	if capacity > 0 && w > capacity {
		w = capacity
	}
	return w
}

// SmbMode names the SMB consumer container set a check needs. Mirrors the
// modes accepted by apps/desktop/test/smb-servers/start.sh.
type SmbMode string

const (
	SmbModeNone SmbMode = ""
	SmbModeCore SmbMode = "core" // guest, auth, both, readonly, flaky, slow
	SmbModeE2E  SmbMode = "e2e"  // guest, auth, 50shares, unicode
)

// processTracker keeps track of all running child processes so they can be
// killed as a group on Ctrl+C. Each command is started with its own process
// group (Setpgid), so killing -pgid cleans up all its descendants too.
var processTracker = struct {
	mu    sync.Mutex
	procs map[*exec.Cmd]struct{}
}{procs: make(map[*exec.Cmd]struct{})}

// KillAllProcesses sends SIGTERM to the process group of every tracked child, then
// force-removes any container a check left running.
//
// The container part matters because the runner `os.Exit`s straight after this on
// Ctrl+C, so no check's `defer` ever runs. Killing a `docker exec` client doesn't stop
// the process inside the container (exec doesn't proxy signals), so without this a
// cancelled `rust-tests-linux` would leave a container compiling away on its own.
func KillAllProcesses() {
	processTracker.mu.Lock()
	for cmd := range processTracker.procs {
		if cmd.Process != nil {
			// Kill the entire process group (negative PID).
			_ = syscall.Kill(-cmd.Process.Pid, syscall.SIGTERM)
		}
	}
	processTracker.mu.Unlock()

	RemoveTrackedContainers()
}

// RunCommand executes a command and captures its output.
// The command is started in its own process group so that all of its
// descendants can be killed together on shutdown.
func RunCommand(cmd *exec.Cmd, captureOutput bool) (string, error) {
	var stdout, stderr bytes.Buffer
	if captureOutput {
		cmd.Stdout = &stdout
		cmd.Stderr = &stderr
	} else {
		cmd.Stdout = os.Stdout
		cmd.Stderr = os.Stderr
	}

	// Give the child its own process group so we can kill the whole tree.
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}

	if err := cmd.Start(); err != nil {
		return "", err
	}

	processTracker.mu.Lock()
	processTracker.procs[cmd] = struct{}{}
	processTracker.mu.Unlock()

	err := cmd.Wait()

	processTracker.mu.Lock()
	delete(processTracker.procs, cmd)
	processTracker.mu.Unlock()

	output := stdout.String()
	if stderr.Len() > 0 {
		output += stderr.String()
	}
	return output, err
}

// CommandExists checks if a command exists in PATH.
func CommandExists(name string) bool {
	_, err := exec.LookPath(name)
	return err == nil
}

// ResolveDevSecret returns one of the dev/CI tooling secrets (an API key or token),
// resolving in order:
//  1. the NAME environment variable (how CI passes a GitHub secret), then
//  2. the `secret` helper (David's sops store) when it's on PATH, so a local `pnpm check`
//     picks up the key without him exporting it.
//
// It returns "" when neither yields a value (the caller then skips or fails). CI boxes have
// neither the env var populated for optional keys nor the `secret` helper, so this degrades to
// the same graceful "no key" as before. It never touches the macOS Keychain: these secrets
// live in the sops store now.
func ResolveDevSecret(name string) string {
	if v := strings.TrimSpace(os.Getenv(name)); v != "" {
		return v
	}
	if !CommandExists("secret") {
		return ""
	}
	out, err := RunCommand(exec.Command("secret", name), true)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(out)
}

// InstallPinnedBinary downloads a `.tar.gz`, verifies it against the sha256 it
// was pinned to, and installs the executable named `binaryName` from it at
// `destination`.
//
// The checksum is the pin, the way `--version` + `--locked` is the pin on a
// `cargo install` and `@vX.Y.Z` is on a `go install` (`CLAUDE.md` § Pin every
// tool install): a downloaded binary gets EXECUTED, so a release asset that
// changed underneath us has to fail loudly instead of running. Callers hold the
// expected checksum next to the version they derived the URL from, so the two
// can't drift apart.
//
// Installing through a temp file plus rename keeps an interrupted download from
// leaving a half-written executable where the next run would find and trust it.
func InstallPinnedBinary(url, sha256Hex, binaryName, destination string) error {
	client := &http.Client{Timeout: 2 * time.Minute}
	response, err := client.Get(url)
	if err != nil {
		return fmt.Errorf("couldn't download %s: %w", binaryName, err)
	}
	if response == nil {
		// Unreachable: `Get` returns a response whenever it returns no error.
		// Stated anyway because nilaway can't see that contract through the
		// stdlib, and a silenced warning would cost more than this line.
		return fmt.Errorf("couldn't download %s: %s answered nothing", binaryName, url)
	}
	defer func() { _ = response.Body.Close() }()
	if response.StatusCode != http.StatusOK {
		return fmt.Errorf("couldn't download %s: %s answered %s", binaryName, url, response.Status)
	}
	archive, err := io.ReadAll(response.Body)
	if err != nil {
		return fmt.Errorf("couldn't read the %s download: %w", binaryName, err)
	}

	sum := sha256.Sum256(archive)
	if actual := hex.EncodeToString(sum[:]); actual != sha256Hex {
		return fmt.Errorf(
			"the %s download doesn't match its pinned checksum (got %s, expected %s).\n"+
				"The release asset changed under us: verify what's being served before updating the pin.\n"+
				"  %s",
			binaryName, actual, sha256Hex, url)
	}

	binary, err := extractFromTarGz(archive, binaryName)
	if err != nil {
		return err
	}
	return installExecutable(binary, destination)
}

// extractFromTarGz pulls one named executable out of a gzipped tarball,
// wherever in the archive it sits: release tarballs usually nest it under a
// `<tool>-<version>-<triple>/` directory beside a README and licenses.
func extractFromTarGz(archive []byte, binaryName string) ([]byte, error) {
	decompressed, err := gzip.NewReader(bytes.NewReader(archive))
	if err != nil {
		return nil, fmt.Errorf("the %s download isn't valid gzip: %w", binaryName, err)
	}
	defer func() { _ = decompressed.Close() }()

	reader := tar.NewReader(decompressed)
	for {
		header, err := reader.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("couldn't read the %s tarball: %w", binaryName, err)
		}
		if header.Typeflag != tar.TypeReg || filepath.Base(header.Name) != binaryName {
			continue
		}
		binary, err := io.ReadAll(reader)
		if err != nil {
			return nil, fmt.Errorf("couldn't read %s out of the tarball: %w", binaryName, err)
		}
		return binary, nil
	}
	return nil, fmt.Errorf("the downloaded tarball holds no `%s` executable", binaryName)
}

// installExecutable writes a binary where it belongs, via a temp file in the
// same directory so the rename that publishes it is atomic.
func installExecutable(binary []byte, destination string) error {
	directory := filepath.Dir(destination)
	if err := os.MkdirAll(directory, 0o755); err != nil {
		return fmt.Errorf("couldn't create %s: %w", directory, err)
	}
	temp, err := os.CreateTemp(directory, "."+filepath.Base(destination)+"-*")
	if err != nil {
		return fmt.Errorf("couldn't stage the %s install: %w", filepath.Base(destination), err)
	}
	tempPath := temp.Name()
	defer func() { _ = os.Remove(tempPath) }() // no-op once the rename succeeded

	if _, err := temp.Write(binary); err != nil {
		_ = temp.Close()
		return fmt.Errorf("couldn't write %s: %w", tempPath, err)
	}
	if err := temp.Close(); err != nil {
		return fmt.Errorf("couldn't write %s: %w", tempPath, err)
	}
	if err := os.Chmod(tempPath, 0o755); err != nil {
		return fmt.Errorf("couldn't make %s executable: %w", tempPath, err)
	}
	if err := os.Rename(tempPath, destination); err != nil {
		return fmt.Errorf("couldn't install %s: %w", destination, err)
	}
	return nil
}

// EnsureGoTool ensures a Go tool is installed and returns the path to the binary.
// If the tool is already in PATH, returns just the name. Otherwise installs it
// and returns the full path to the installed binary.
func EnsureGoTool(name, installPath string) (string, error) {
	if CommandExists(name) {
		return name, nil
	}

	// Get Go's bin directory
	goBin := getGoBinDir()
	if goBin == "" {
		return "", fmt.Errorf("could not determine Go bin directory")
	}

	// Install the tool
	installCmd := exec.Command("go", "install", installPath)
	if _, err := RunCommand(installCmd, true); err != nil {
		return "", fmt.Errorf("failed to install %s: %w", name, err)
	}

	// Return full path to the binary
	return filepath.Join(goBin, name), nil
}

// getGoBinDir returns the directory where go install puts binaries.
func getGoBinDir() string {
	// First check GOBIN
	cmd := exec.Command("go", "env", "GOBIN")
	if output, err := RunCommand(cmd, true); err == nil {
		if bin := strings.TrimSpace(output); bin != "" {
			return bin
		}
	}

	// Fall back to GOPATH/bin
	cmd = exec.Command("go", "env", "GOPATH")
	if output, err := RunCommand(cmd, true); err == nil {
		if gopath := strings.TrimSpace(output); gopath != "" {
			return filepath.Join(gopath, "bin")
		}
	}

	// Last resort: ~/go/bin
	if home, err := os.UserHomeDir(); err == nil {
		return filepath.Join(home, "go", "bin")
	}

	return ""
}

// indentOutput indents each non-empty line of output.
func indentOutput(output string) string {
	lines := strings.Split(output, "\n")
	var result strings.Builder
	for _, line := range lines {
		if strings.TrimSpace(line) != "" {
			result.WriteString("      ")
			result.WriteString(line)
			result.WriteString("\n")
		}
	}
	return result.String()
}

// EnsurePnpmDependencies runs pnpm install to ensure all dependencies are installed.
// Skips the install if pnpm-lock.yaml hasn't changed since the last successful run.
// In CI mode, uses --frozen-lockfile and always runs (never skips).
// Returns true if the install was skipped.
func EnsurePnpmDependencies(ctx *CheckContext) (skipped bool, err error) {
	lockfilePath := filepath.Join(ctx.RootDir, "pnpm-lock.yaml")
	markerPath := filepath.Join(ctx.RootDir, "node_modules", ".pnpm-install-marker")

	if !ctx.CI {
		if lockInfo, lockErr := os.Stat(lockfilePath); lockErr == nil {
			if markerContent, markerErr := os.ReadFile(markerPath); markerErr == nil {
				recorded := string(markerContent)
				current := lockInfo.ModTime().UTC().Format("2006-01-02T15:04:05.000000000Z")
				if recorded == current {
					return true, nil
				}
			}
		}
	}

	args := []string{"install"}
	if ctx.CI {
		args = append(args, "--frozen-lockfile")
	}

	cmd := exec.Command("pnpm", args...)
	cmd.Dir = ctx.RootDir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return false, fmt.Errorf("pnpm install failed:\n%s", indentOutput(output))
	}

	// Write marker with lockfile's current mtime
	if lockInfo, lockErr := os.Stat(lockfilePath); lockErr == nil {
		mtime := lockInfo.ModTime().UTC().Format("2006-01-02T15:04:05.000000000Z")
		_ = os.WriteFile(markerPath, []byte(mtime), 0644)
	}

	return false, nil
}

// Pluralize returns singular if count is 1, plural otherwise.
// Example: Pluralize(1, "file", "files") returns "file"
// Example: Pluralize(5, "file", "files") returns "files"
func Pluralize(count int, singular, plural string) string {
	if count == 1 {
		return singular
	}
	return plural
}

// runOxfmtCheck runs oxfmt formatting check/fix for a given directory.
// extensions is optional. If nil, file count is parsed from oxfmt output instead of `find`.
func runOxfmtCheck(ctx *CheckContext, dir string, extensions []string) (CheckResult, error) {
	if ctx.CI {
		checkCmd := exec.Command("pnpm", "exec", "oxfmt", "--check", ".")
		checkCmd.Dir = dir
		checkOutput, err := RunCommand(checkCmd, true)
		fileCount := parseOxfmtFileCount(checkOutput)
		if err != nil {
			return CheckResult{}, fmt.Errorf("code is not formatted, run `pnpm exec oxfmt .` locally\n%s", indentOutput(checkOutput))
		}
		result := Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		result.Issues = 0
		result.Changes = 0
		return result, nil
	}

	// Non-CI: check first, then format if needed
	checkCmd := exec.Command("pnpm", "exec", "oxfmt", "--check", ".")
	checkCmd.Dir = dir
	checkOutput, checkErr := RunCommand(checkCmd, true)
	fileCount := parseOxfmtFileCount(checkOutput)

	if checkErr != nil {
		fmtCmd := exec.Command("pnpm", "exec", "oxfmt", ".")
		fmtCmd.Dir = dir
		fmtOutput, err := RunCommand(fmtCmd, true)
		if err != nil {
			return CheckResult{}, fmt.Errorf("oxfmt formatting failed\n%s", indentOutput(fmtOutput))
		}

		var needsFormat int
		for line := range strings.SplitSeq(strings.TrimSpace(checkOutput), "\n") {
			if strings.TrimSpace(line) != "" && !strings.HasPrefix(line, "Checking") && !strings.HasPrefix(line, "Finished") && !strings.HasPrefix(line, "Format") {
				needsFormat++
			}
		}

		result := SuccessWithChanges(fmt.Sprintf("Formatted %d of %d %s", needsFormat, fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		result.Issues = needsFormat
		result.Changes = needsFormat
		return result, nil
	}

	result := Success(fmt.Sprintf("%d %s already formatted", fileCount, Pluralize(fileCount, "file", "files")))
	result.Total = fileCount
	result.Issues = 0
	result.Changes = 0
	return result, nil
}

// parseOxfmtFileCount extracts the file count from oxfmt output like "Finished in 150ms on 25 files using 16 threads."
func parseOxfmtFileCount(output string) int {
	for line := range strings.SplitSeq(output, "\n") {
		if strings.HasPrefix(line, "Finished in ") {
			var count int
			if _, err := fmt.Sscanf(line, "Finished in %s on %d files", new(string), &count); err == nil {
				return count
			}
		}
	}
	return 0
}

// runESLintCheck runs ESLint check/fix for a given directory.
// extensions are the file extensions to count (like []string{"*.ts", "*.svelte", "*.js"}).
// If requireConfig is true, skips when eslint.config.js is missing.
func runESLintCheck(ctx *CheckContext, dir string, extensions []string, requireConfig bool) (CheckResult, error) {
	if requireConfig {
		if _, err := os.Stat(filepath.Join(dir, "eslint.config.js")); os.IsNotExist(err) {
			return Skipped("no eslint.config.js"), nil
		}
	}

	// Count lintable files
	findArgs := buildFindArgs("src", extensions)
	findCmd := exec.Command("find", findArgs...)
	findCmd.Dir = dir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "lint")
	} else {
		cmd = exec.Command("pnpm", "lint:fix")
	}
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("lint errors found, run pnpm lint:fix locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("eslint found unfixable errors\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		result := Success(fmt.Sprintf("%d %s passed", fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		return result, nil
	}
	return Success("All files passed"), nil
}

// runStylelintCheck lints (and locally fixes) CSS and Svelte `<style>` blocks in a given app dir.
// The app supplies the globs via its own `stylelint` / `stylelint:fix` package scripts.
func runStylelintCheck(ctx *CheckContext, dir string) (CheckResult, error) {
	findCmd := exec.Command("find", "src", "-type", "f", "-name", "*.css")
	findCmd.Dir = dir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	var cmd *exec.Cmd
	if ctx.CI {
		cmd = exec.Command("pnpm", "stylelint")
	} else {
		cmd = exec.Command("pnpm", "stylelint:fix")
	}
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		if ctx.CI {
			return CheckResult{}, fmt.Errorf("CSS lint errors found, run pnpm stylelint:fix locally\n%s", indentOutput(output))
		}
		return CheckResult{}, fmt.Errorf("stylelint found unfixable errors\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		result := Success(fmt.Sprintf("%d CSS %s valid", fileCount, Pluralize(fileCount, "file", "files")))
		result.Total = fileCount
		return result, nil
	}
	return Success("All CSS valid"), nil
}

// runKnipCheck finds unused files, exports, and dependencies in a given app dir.
func runKnipCheck(ctx *CheckContext, dir string) (CheckResult, error) {
	findCmd := exec.Command("find", "src", "-type", "f", "(", "-name", "*.ts", "-o", "-name", "*.svelte", ")")
	findCmd.Dir = dir
	findOutput, _ := RunCommand(findCmd, true)
	fileCount := 0
	if strings.TrimSpace(findOutput) != "" {
		fileCount = len(strings.Split(strings.TrimSpace(findOutput), "\n"))
	}

	cmd := exec.Command("pnpm", "knip")
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("knip found unused code or dependencies\n%s", indentOutput(output))
	}

	if fileCount > 0 {
		return Success(fmt.Sprintf("%d %s checked, no unused code", fileCount, Pluralize(fileCount, "file", "files"))), nil
	}
	return Success("No unused code"), nil
}

// runImportCyclesCheck uses oxlint's import plugin to detect circular imports in a given app dir.
func runImportCyclesCheck(ctx *CheckContext, dir string) (CheckResult, error) {
	cmd := exec.Command("pnpm", "exec", "oxlint",
		"--import-plugin",
		"--allow", "all",
		"--deny", "import/no-cycle",
		"src",
	)
	cmd.Dir = dir
	output, err := RunCommand(cmd, true)
	if err != nil {
		return CheckResult{}, fmt.Errorf("circular imports detected\n%s", indentOutput(output))
	}

	return Success("No circular imports"), nil
}

// buildFindArgs constructs arguments for a find command to locate files with given extensions.
func buildFindArgs(searchDir string, extensions []string) []string {
	args := []string{searchDir, "-type", "f", "("}
	for i, ext := range extensions {
		if i > 0 {
			args = append(args, "-o")
		}
		args = append(args, "-name", ext)
	}
	args = append(args, ")")
	return args
}

// GetGoDirectories returns all directories in the repo that contain Go code.
// Each returned path is relative to rootDir.
func GetGoDirectories() []string {
	return []string{
		"scripts",
		"apps/desktop/scripts",
	}
}

// FindGoModules finds all go.mod files in the given directory and returns
// the directories containing them.
func FindGoModules(rootDir string) ([]string, error) {
	findCmd := exec.Command("find", ".", "-name", "go.mod", "-type", "f")
	findCmd.Dir = rootDir
	output, err := RunCommand(findCmd, true)
	if err != nil {
		return nil, err
	}

	var modules []string
	for line := range strings.SplitSeq(strings.TrimSpace(output), "\n") {
		if line != "" {
			// Get directory containing go.mod
			dir := strings.TrimSuffix(line, "/go.mod")
			dir = strings.TrimPrefix(dir, "./")
			if dir == "go.mod" {
				dir = "."
			}
			modules = append(modules, dir)
		}
	}
	return modules, nil
}

// FindAllGoModules finds Go modules across all Go directories in the repo.
// Returns a map of base directory to list of module subdirectories.
func FindAllGoModules(rootDir string) (map[string][]string, error) {
	result := make(map[string][]string)
	for _, goDir := range GetGoDirectories() {
		fullPath := filepath.Join(rootDir, goDir)
		modules, err := FindGoModules(fullPath)
		if err != nil {
			return nil, fmt.Errorf("failed to find modules in %s: %w", goDir, err)
		}
		result[goDir] = modules
	}
	return result, nil
}
