package checks

// The pinned-tool installer: dev secrets, checksum-pinned binary downloads, `cargo-nextest`,
// and the version-pinned Go analyzers.

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
	"time"
)

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

// NextestVersion pins `cargo-nextest` for every lane that shells out to it, host and
// container alike. Pinned rather than `latest` because the parsers in this package are
// written against this exact output, and because the contention re-run's profile
// semantics (a per-test override beating a profile-level `slow-timeout`) were verified
// against it.
const NextestVersion = "0.9.136"

// EnsureCargoNextest installs `cargo-nextest` at NextestVersion unless that exact version
// is already on PATH. Any other version gets replaced: a `~/.cargo/bin` that outlives a
// pin bump (a dev machine, the M1's CI slots, a restored Rust cache) otherwise kept the
// old binary forever, and the parsers here are written against NextestVersion's output.
//
// ❗ Every check that shells out to `cargo nextest` must call this first, even one that
// only *lists* tests. CI runs `nextest-filter-coverage` before the lanes that run tests,
// so a lane relying on a sibling check to install nextest passes only while the Rust
// cache happens to carry the binary. When that cache misses, the check dies on
// `error: no such command: nextest`, which reads as broken tooling rather than a missing
// install.
func EnsureCargoNextest() error {
	if CommandExists("cargo-nextest") {
		output, _ := exec.Command("cargo", "nextest", "--version").Output()
		if nextestVersionIs(string(output), NextestVersion) {
			return nil
		}
	}
	// `--force`, so a different installed version gets replaced instead of refused.
	installCmd := exec.Command("cargo", "install", "cargo-nextest", "--version", NextestVersion, "--locked", "--force")
	if _, err := RunCommand(installCmd, true); err != nil {
		return fmt.Errorf("failed to install cargo-nextest: %w", err)
	}
	return nil
}

// nextestVersionIs says whether `cargo nextest --version` output names exactly want. Its
// first line reads `cargo-nextest 0.9.136 (1d5bf1ec9 2026-05-16)`.
func nextestVersionIs(output, want string) bool {
	firstLine, _, _ := strings.Cut(output, "\n")
	fields := strings.Fields(firstLine)
	return len(fields) >= 2 && fields[0] == "cargo-nextest" && fields[1] == want
}

// GoToolsBinPath is where the pinned Go analyzers live, alongside the check cache in the one
// scratch directory the repo already ignores. Repo-local on purpose: `go install` into the user's
// `~/go/bin` would fight whatever versions they keep there for other projects.
const GoToolsBinPath = "node_modules/.cache/cmdr-go-tools"

// goToolBuildInfo is what `go version -m <binary>` says about how a tool was built: the module
// version it came from, and the Go toolchain that compiled it. Both decide a reinstall.
type goToolBuildInfo struct {
	ModuleVersion string
	GoVersion     string
}

// EnsureGoTool returns the path to a Go tool built from the module version the caller pins,
// installing it when what's on disk doesn't match that pin or was built by a different Go than
// `.mise.toml` names.
//
// ❗ It deliberately ignores whatever binary happens to be named `<name>` on PATH. Trusting PATH
// made the version pins decorative on a developer machine: a `~/go/bin` holding x/tools v0.42.0
// silently answered every local run while the repo pinned v0.49.0, so local checks ran months-old
// analyzers, and the supply-chain reasoning behind pinning (DETAILS § Key decisions) held only in
// CI, where no such binary exists. It also breaks loudly rather than usefully: a tool built by an
// older Go can't parse the newer stdlib, which surfaces as "method must have no type parameters"
// inside `math/rand` rather than as "your tool is stale".
func EnsureGoTool(rootDir, name, installPath string) (string, error) {
	wantModuleVersion, err := pinnedModuleVersion(installPath)
	if err != nil {
		return "", err
	}
	goVersion, err := MiseGoVersion(rootDir)
	if err != nil {
		return "", fmt.Errorf("cannot decide whether %s is current: %w", name, err)
	}

	binDir := filepath.Join(rootDir, GoToolsBinPath)
	binPath := filepath.Join(binDir, name)
	if goToolIsCurrent(binPath, wantModuleVersion, "go"+goVersion) {
		return binPath, nil
	}

	installCmd := exec.Command("go", "install", installPath)
	installCmd.Dir = rootDir
	installCmd.Env = append(os.Environ(), "GOBIN="+binDir)
	if output, err := RunCommand(installCmd, true); err != nil {
		return "", fmt.Errorf("failed to install %s (%s): %w\n%s", name, installPath, err, output)
	}

	return binPath, nil
}

// goToolIsCurrent reports whether the binary on disk came from the pinned module version and was
// built by the wanted Go. Anything it can't read (missing file, unreadable build info) counts as
// "not current", so the caller reinstalls rather than running something unidentified.
func goToolIsCurrent(binPath, wantModuleVersion, wantGoVersion string) bool {
	if _, err := os.Stat(binPath); err != nil {
		return false
	}
	output, err := RunCommand(exec.Command("go", "version", "-m", binPath), true)
	if err != nil {
		return false
	}
	info, ok := parseGoToolBuildInfo(output)
	if !ok {
		return false
	}
	return info.ModuleVersion == wantModuleVersion && info.GoVersion == wantGoVersion
}

// parseGoToolBuildInfo reads `go version -m` output, whose first line ends in the Go toolchain
// (`/path/to/deadcode: go1.27.1`) and whose `mod` line carries the module and its version
// (`\tmod\tgolang.org/x/tools\tv0.49.0\th1:...`).
func parseGoToolBuildInfo(output string) (goToolBuildInfo, bool) {
	var info goToolBuildInfo
	for line := range strings.SplitSeq(output, "\n") {
		fields := strings.Fields(line)
		if info.GoVersion == "" && len(fields) >= 2 && strings.HasSuffix(fields[0], ":") {
			if last := fields[len(fields)-1]; strings.HasPrefix(last, "go1.") {
				info.GoVersion = last
			}
			continue
		}
		if len(fields) >= 3 && fields[0] == "mod" {
			info.ModuleVersion = fields[2]
		}
	}
	return info, info.GoVersion != "" && info.ModuleVersion != ""
}

// pinnedModuleVersion pulls the `@version` off an install path. An unpinned path (or `@latest`) is
// an error rather than a fallback: without an exact version there's nothing to compare an installed
// binary against, and a fresh checkout would pull whatever is newest, which is the supply-chain hole
// the pins exist to close.
func pinnedModuleVersion(installPath string) (string, error) {
	module, version, found := strings.Cut(installPath, "@")
	if !found || version == "" {
		return "", fmt.Errorf("tool install path %q must pin an exact version (`@vX.Y.Z`)", installPath)
	}
	if version == "latest" {
		return "", fmt.Errorf("tool install path %q must pin an exact version, never `@latest`", module)
	}
	return version, nil
}
