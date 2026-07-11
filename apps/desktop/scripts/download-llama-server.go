// Prepare llama-server binary and shared libraries for bundling.
//
// Downloads the llama.cpp release tarball, extracts only the files needed at
// runtime (llama-server binary + dylibs), and places them in src-tauri/resources/ai/.
// When APPLE_SIGNING_IDENTITY is set (CI release builds), each binary is codesigned
// with hardened runtime + secure timestamp so Apple notarization passes.
//
// Usage: go run scripts/download-llama-server.go
//
// The script is idempotent: it skips work if the marker file matches the
// expected version.
//
// On non-macOS platforms (e.g., Linux CI), creates an empty placeholder file
// since the AI feature is macOS-only.

package main

import (
	"archive/tar"
	"compress/gzip"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
)

// Version is the llama.cpp release version.
// Check for new versions at: https://github.com/ggml-org/llama.cpp/releases
const Version = "b7815"

// ExpectedSHA256 checksum of the upstream release archive.
// To compute for a new version:
//
//	curl -L "<url>" | shasum -a 256
const ExpectedSHA256 = "244d0d961821fa40dc1a621e292d38129698ed03cab4eac2518512fabce427e7"

// DownloadURL constructed from the version.
var DownloadURL = fmt.Sprintf(
	"https://github.com/ggml-org/llama.cpp/releases/download/%s/llama-%s-bin-macos-arm64.tar.gz",
	Version, Version,
)

// DestDir is the directory where extracted files are placed, relative to apps/desktop.
const DestDir = "src-tauri/resources/ai"

// PlaceholderFile is the single file created on non-macOS for Tauri resource bundling.
const PlaceholderFile = "src-tauri/resources/ai/llama-server"

// MarkerFile tracks which version is currently extracted, enabling idempotency.
const MarkerFile = "src-tauri/resources/ai/.version"

func main() {
	// On non-macOS platforms, create a placeholder file for CI builds.
	// The AI feature is macOS-only, but Tauri requires the resource to exist.
	if runtime.GOOS != "darwin" {
		if err := createPlaceholder(PlaceholderFile); err != nil {
			fail("create placeholder", err)
		}
		return
	}

	// Check if already extracted for this version. A DestDir that is itself a
	// symlink is the retired share-by-symlink worktree scheme: its marker
	// resolves THROUGH the link into the main clone, so it "matches" — but the
	// link dangles inside the Linux-E2E Docker bind-mount (the container can't
	// see the host's main clone), breaking the in-container build. Fall through
	// so the clone path below replaces it with a self-contained copy.
	if isCurrentVersion(MarkerFile, Version) && !isSymlink(DestDir) {
		fmt.Printf("llama-server %s already prepared, skipping\n", Version)
		return
	}

	// In a linked worktree, clone the main clone's populated resources/ai/
	// (APFS clonefile: instant, no extra space) instead of downloading 30 MB.
	// Falls through to the download path if we're in the main clone or the main
	// clone has a stale/missing version.
	if tryCloneFromMainClone() {
		return
	}

	if err := prepareForDarwin(); err != nil {
		fail("prepare llama-server", err)
	}
}

func fail(stage string, err error) {
	_, _ = fmt.Fprintf(os.Stderr, "Error: %s: %v\n", stage, err)
	os.Exit(1)
}

// prepareForDarwin runs the full macOS pipeline: download → verify → extract →
// (optional) codesign → write the version marker. Each step returns a wrapped
// error so main() handles exit codes in one place.
func prepareForDarwin() error {
	tmpPath, cleanup, err := makeTempArchive()
	if err != nil {
		return err
	}
	defer cleanup()

	fmt.Printf("Downloading llama-server %s...\n", Version)
	fmt.Printf("URL: %s\n", DownloadURL)
	if err := downloadFile(DownloadURL, tmpPath); err != nil {
		return fmt.Errorf("download: %w", err)
	}

	if err := verifyChecksum(tmpPath, ExpectedSHA256); err != nil {
		return err
	}
	fmt.Println("Download verified")

	if err := resetDir(DestDir); err != nil {
		return err
	}

	extracted, err := extractNeededFiles(tmpPath, DestDir)
	if err != nil {
		return fmt.Errorf("extract: %w", err)
	}
	fmt.Printf("Extracted %d files\n", len(extracted))

	if identity := os.Getenv("APPLE_SIGNING_IDENTITY"); identity != "" {
		if err := signAll(extracted, identity); err != nil {
			return err
		}
	}

	if err := os.WriteFile(MarkerFile, []byte(Version), 0o644); err != nil {
		return fmt.Errorf("write version marker: %w", err)
	}
	fmt.Println("llama-server prepared")
	return nil
}

// makeTempArchive creates a temp file path for the upstream tarball and returns
// a cleanup func that removes it. The file is closed before returning so the
// downloader can open it for writing.
func makeTempArchive() (string, func(), error) {
	tmpFile, err := os.CreateTemp("", "llama-server-*.tar.gz")
	if err != nil {
		return "", nil, fmt.Errorf("create temp file: %w", err)
	}
	tmpPath := tmpFile.Name()
	_ = tmpFile.Close()
	return tmpPath, func() { _ = os.Remove(tmpPath) }, nil
}

// verifyChecksum re-hashes the downloaded archive and bails on a mismatch.
func verifyChecksum(archivePath, expected string) error {
	actual, err := computeSHA256(archivePath)
	if err != nil {
		return fmt.Errorf("compute checksum: %w", err)
	}
	if actual != expected {
		return fmt.Errorf("checksum mismatch: expected %s, got %s", expected, actual)
	}
	return nil
}

// resetDir wipes and recreates dir so every version change starts from a clean slate.
func resetDir(dir string) error {
	if err := os.RemoveAll(dir); err != nil {
		return fmt.Errorf("clean destination: %w", err)
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("create destination: %w", err)
	}
	return nil
}

// signAll codesigns each extracted binary with the given identity (used in CI release builds).
func signAll(paths []string, identity string) error {
	fmt.Printf("Signing %d files with identity %q...\n", len(paths), identity)
	for _, path := range paths {
		if err := codesign(path, identity); err != nil {
			return fmt.Errorf("sign %s: %w", path, err)
		}
	}
	fmt.Println("All files signed")
	return nil
}

// extractNeededFiles extracts only the llama-server binary and .dylib files from
// the tarball into destDir. Returns the list of extracted file paths.
func extractNeededFiles(archivePath, destDir string) ([]string, error) {
	f, err := os.Open(archivePath)
	if err != nil {
		return nil, fmt.Errorf("open archive: %w", err)
	}
	defer func() { _ = f.Close() }()

	gz, err := gzip.NewReader(f)
	if err != nil {
		return nil, fmt.Errorf("gzip reader: %w", err)
	}
	defer func() { _ = gz.Close() }()

	tr := tar.NewReader(gz)
	var extracted []string
	symlinkTargets := make(map[string]string) // link name -> target name

	for {
		hdr, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("read tar entry: %w", err)
		}

		name := filepath.Base(hdr.Name)
		if name == "" || name == "." {
			continue
		}

		isNeeded := name == "llama-server" || strings.HasSuffix(name, ".dylib")
		if !isNeeded {
			continue
		}

		destPath := filepath.Join(destDir, name)

		switch hdr.Typeflag {
		case tar.TypeReg:
			if err := writeFile(destPath, tr, os.FileMode(hdr.Mode)); err != nil {
				return nil, fmt.Errorf("extract %s: %w", name, err)
			}
			extracted = append(extracted, destPath)

		case tar.TypeSymlink:
			// Defer symlink creation until all regular files are extracted
			target := filepath.Base(hdr.Linkname)
			symlinkTargets[name] = target
		}
	}

	// Create symlinks (e.g. libllama.dylib -> libllama.0.0.7815.dylib)
	for linkName, target := range symlinkTargets {
		linkPath := filepath.Join(destDir, linkName)
		_ = os.Remove(linkPath) // Remove if exists from previous run
		if err := os.Symlink(target, linkPath); err != nil {
			return nil, fmt.Errorf("symlink %s -> %s: %w", linkName, target, err)
		}
	}

	return extracted, nil
}

func writeFile(path string, r io.Reader, mode os.FileMode) error {
	// Ensure at least user-executable for binaries
	if mode == 0 {
		mode = 0o644
	}
	mode |= 0o755 // Make executable

	out, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, mode)
	if err != nil {
		return err
	}
	defer func() { _ = out.Close() }()

	_, err = io.Copy(out, r)
	return err
}

// codesign signs a binary with the given identity, hardened runtime, and secure timestamp.
func codesign(path, identity string) error {
	args := []string{
		"--force",
		"--options", "runtime",
		"--timestamp",
		"--sign", identity,
	}
	// In CI the identity lives in a dedicated keychain (see the "Set up llama-server
	// signing keychain" step in release.yml). Target it explicitly so identity
	// resolution can't be ambiguous and doesn't depend on the runner session's
	// login-keychain access (codesign fails with errSecInternalComponent when the
	// runner's launchd security session tries to use the login keychain's private key).
	// The keychain must ALSO be in the search list; --keychain alone isn't enough
	// (verified on the runner), which is why release.yml adds it there too.
	if keychain := os.Getenv("LLAMA_SIGN_KEYCHAIN"); keychain != "" {
		args = append(args, "--keychain", keychain)
	}
	args = append(args, path)
	cmd := exec.Command("codesign", args...)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%w: %s", err, string(output))
	}
	return nil
}

// tryCloneFromMainClone copies the main clone's populated resources/ai/ into
// DestDir when:
//   - we're inside a linked git worktree (not the main clone itself), and
//   - the main clone already has the target version extracted.
//
// The copy uses APFS clonefile (`cp -c`): instant and space-free on the same
// volume, with a plain copy fallback elsewhere. A copy (not a symlink) keeps
// the worktree self-contained — the Linux-E2E Docker container bind-mounts only
// the worktree, so a symlink into the main clone dangles there and breaks the
// in-container tauri build. The dir's INTERNAL relative dylib symlinks survive
// (`cp -R` copies links as links) and resolve fine inside the mount.
//
// Returns true on success. Returns false (without erroring) if we're outside a
// git repo, in the main clone, or the main clone's version doesn't match. The
// caller then falls back to downloading.
func tryCloneFromMainClone() bool {
	currentRoot, err := gitRevParse("--show-toplevel")
	if err != nil {
		return false
	}
	commonDir, err := gitRevParse("--git-common-dir")
	if err != nil {
		return false
	}
	if !filepath.IsAbs(commonDir) {
		commonDir = filepath.Join(currentRoot, commonDir)
	}
	mainRoot := filepath.Dir(commonDir)

	if mainRoot == currentRoot {
		return false
	}

	srcDir := filepath.Join(mainRoot, "apps", "desktop", DestDir)
	srcMarker := filepath.Join(srcDir, ".version")
	if !isCurrentVersion(srcMarker, Version) {
		return false
	}

	// Replace any existing DestDir (empty dir from a prior trap, stale files,
	// or a symlink from the retired share-by-symlink scheme). os.RemoveAll on a
	// symlink removes the link only, not the target: safe for the main clone.
	if err := os.RemoveAll(DestDir); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Warning: could not clear %s: %v\n", DestDir, err)
		return false
	}
	if err := os.MkdirAll(filepath.Dir(DestDir), 0o755); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Warning: could not create parent dir: %v\n", err)
		return false
	}
	// -c requests clonefile and fails cleanly when unsupported (for example a
	// worktree on a different volume), so retry as a plain copy.
	if err := exec.Command("cp", "-c", "-R", srcDir, DestDir).Run(); err != nil {
		if err := exec.Command("cp", "-R", srcDir, DestDir).Run(); err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Warning: could not copy from main clone: %v\n", err)
			return false
		}
	}
	fmt.Printf("Cloned %s from %s (version %s, APFS clonefile)\n", DestDir, srcDir, Version)
	return true
}

// isSymlink reports whether path itself is a symlink (without following it).
func isSymlink(path string) bool {
	info, err := os.Lstat(path)
	if err != nil || info == nil {
		return false
	}
	return info.Mode()&os.ModeSymlink != 0
}

func gitRevParse(arg string) (string, error) {
	out, err := exec.Command("git", "rev-parse", arg).Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func isCurrentVersion(markerPath, version string) bool {
	data, err := os.ReadFile(markerPath)
	if err != nil {
		return false
	}
	return strings.TrimSpace(string(data)) == version
}

func createPlaceholder(path string) error {
	if _, err := os.Stat(path); err == nil {
		fmt.Printf("Placeholder %s already exists, skipping\n", path)
		return nil
	}

	// Legacy worktree-in-Docker heal: the retired share-by-symlink scheme left
	// `resources/ai` as a symlink into the main clone's populated dir. Inside
	// Docker that target is a host path the container can't see, so the
	// symlink dangles and the MkdirAll below fails with "file exists". Detect
	// and clean up so the placeholder write lands in a real directory. (New
	// worktrees get a self-contained clonefile copy and never hit this.)
	dir := filepath.Dir(path)
	if info, lerr := os.Lstat(dir); lerr == nil && info.Mode()&os.ModeSymlink != 0 {
		if _, serr := os.Stat(dir); serr != nil {
			if err := os.Remove(dir); err != nil {
				return fmt.Errorf("remove dangling worktree symlink at %s: %w", dir, err)
			}
		}
	}

	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return fmt.Errorf("create directory: %w", err)
	}

	f, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}
	if err := f.Close(); err != nil {
		return fmt.Errorf("close file: %w", err)
	}

	fmt.Printf("Created placeholder %s (non-macOS build)\n", path)
	return nil
}

func computeSHA256(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func() { _ = f.Close() }()

	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func downloadFile(url, destPath string) error {
	out, err := os.Create(destPath)
	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}
	defer func() { _ = out.Close() }()

	resp, err := http.Get(url) //nolint:gosec // URL is a hardcoded constant, not user input
	if err != nil {
		return fmt.Errorf("http get: %w", err)
	}
	if resp == nil {
		return fmt.Errorf("http get: nil response")
	}
	defer func() { _ = resp.Body.Close() }()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("http status: %s", resp.Status)
	}

	if err := copyWithProgress(resp.Body, out, resp.ContentLength); err != nil {
		return err
	}

	return out.Close()
}

func copyWithProgress(src io.Reader, dst io.Writer, size int64) error {
	written := int64(0)
	lastPct := -10
	buf := make([]byte, 32*1024)

	for {
		n, err := src.Read(buf)
		if n > 0 {
			if _, writeErr := dst.Write(buf[:n]); writeErr != nil {
				return fmt.Errorf("write: %w", writeErr)
			}
			written += int64(n)
			if size > 0 {
				pct := int(float64(written) / float64(size) * 100)
				if pct >= lastPct+10 {
					fmt.Printf("  %d%% (%d / %d MB)\n", pct, written/(1024*1024), size/(1024*1024))
					lastPct = pct
				}
			}
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return fmt.Errorf("read: %w", err)
		}
	}
	return nil
}
