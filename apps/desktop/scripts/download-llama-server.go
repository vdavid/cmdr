// Prepare llama-server binary and shared libraries for bundling.
//
// Downloads the llama.cpp release tarball, extracts only the files needed at
// runtime (llama-server binary + dylibs), and places them in src-tauri/resources/ai/.
// When APPLE_SIGNING_IDENTITY is set (CI release builds), each binary is codesigned
// with hardened runtime + secure timestamp so Apple notarization passes.
//
// Usage: go run scripts/download-llama-server.go
//
// The script is idempotent — it skips work if the marker file matches the
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
			_, _ = fmt.Fprintf(os.Stderr, "Error creating placeholder: %v\n", err)
			os.Exit(1)
		}
		return
	}

	// Check if already extracted for this version
	if isCurrentVersion(MarkerFile, Version) {
		fmt.Printf("llama-server %s already prepared, skipping\n", Version)
		return
	}

	// Download the upstream tarball to a temp file
	tmpFile, err := os.CreateTemp("", "llama-server-*.tar.gz")
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error creating temp file: %v\n", err)
		os.Exit(1)
	}
	tmpPath := tmpFile.Name()
	_ = tmpFile.Close()
	defer func() { _ = os.Remove(tmpPath) }()

	fmt.Printf("Downloading llama-server %s...\n", Version)
	fmt.Printf("URL: %s\n", DownloadURL)

	if err := downloadFile(DownloadURL, tmpPath); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error downloading: %v\n", err)
		os.Exit(1)
	}

	// Verify checksum
	actualChecksum, err := computeSHA256(tmpPath)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error computing checksum: %v\n", err)
		os.Exit(1)
	}
	if actualChecksum != ExpectedSHA256 {
		_, _ = fmt.Fprintf(os.Stderr, "Checksum mismatch!\n  Expected: %s\n  Got:      %s\n", ExpectedSHA256, actualChecksum)
		os.Exit(1)
	}
	fmt.Println("Download verified")

	// Clean destination directory (fresh extraction each time version changes)
	if err := os.RemoveAll(DestDir); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error cleaning destination: %v\n", err)
		os.Exit(1)
	}
	if err := os.MkdirAll(DestDir, 0o755); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error creating destination: %v\n", err)
		os.Exit(1)
	}

	// Extract only llama-server binary and .dylib files
	extracted, err := extractNeededFiles(tmpPath, DestDir)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error extracting: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("Extracted %d files\n", len(extracted))

	// Sign binaries if APPLE_SIGNING_IDENTITY is set (CI release builds)
	identity := os.Getenv("APPLE_SIGNING_IDENTITY")
	if identity != "" {
		fmt.Printf("Signing %d files with identity %q...\n", len(extracted), identity)
		for _, path := range extracted {
			if err := codesign(path, identity); err != nil {
				_, _ = fmt.Fprintf(os.Stderr, "Error signing %s: %v\n", path, err)
				os.Exit(1)
			}
		}
		fmt.Println("All files signed")
	}

	// Write version marker for idempotency
	if err := os.WriteFile(MarkerFile, []byte(Version), 0o644); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error writing version marker: %v\n", err)
		os.Exit(1)
	}

	fmt.Println("llama-server prepared")
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
	cmd := exec.Command("codesign",
		"--force",
		"--options", "runtime",
		"--timestamp",
		"--sign", identity,
		path,
	)
	output, err := cmd.CombinedOutput()
	if err != nil {
		return fmt.Errorf("%w: %s", err, string(output))
	}
	return nil
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
