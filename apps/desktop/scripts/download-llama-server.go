// Download llama-server binary from llama.cpp releases.
//
// This script downloads the llama-server binary and shared libraries needed for
// local AI features. It's run automatically before dev/build commands.
//
// Usage: go run scripts/download-llama-server.go
//
// The script is idempotent — it skips the download if the file already exists
// and matches the expected checksum.
//
// On non-macOS platforms (e.g., Linux CI), creates an empty placeholder file
// since the AI feature is macOS-only.

package main

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
)

// Version is the llama.cpp release version.
// Check for new versions at: https://github.com/ggml-org/llama.cpp/releases
const Version = "b7815"

// ExpectedSHA256 checksum of the downloaded archive.
// To compute for a new version:
//
//	curl -L "<url>" | shasum -a 256
const ExpectedSHA256 = "244d0d961821fa40dc1a621e292d38129698ed03cab4eac2518512fabce427e7"

// DownloadURL constructed from the version.
var DownloadURL = fmt.Sprintf(
	"https://github.com/ggml-org/llama.cpp/releases/download/%s/llama-%s-bin-macos-arm64.tar.gz",
	Version, Version,
)

// DestPath is the destination path relative to the working directory (apps/desktop).
const DestPath = "src-tauri/resources/llama-server.tar.gz"

func main() {
	// Resolve destination path relative to current working directory.
	// This script is expected to be run from apps/desktop/ directory.
	destPath := DestPath

	// On non-macOS platforms, create a placeholder file for CI builds.
	// The AI feature is macOS-only, but Tauri requires the resource to exist.
	if runtime.GOOS != "darwin" {
		if err := createPlaceholder(destPath); err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error creating placeholder: %v\n", err)
			os.Exit(1)
		}
		return
	}

	// Check if file already exists with correct checksum
	if fileExistsWithChecksum(destPath, ExpectedSHA256) {
		fmt.Println("llama-server.tar.gz already exists with correct checksum, skipping download")
		return
	}

	// Ensure destination directory exists
	if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error creating directory: %v\n", err)
		os.Exit(1)
	}

	fmt.Printf("Downloading llama-server %s...\n", Version)
	fmt.Printf("URL: %s\n", DownloadURL)

	if err := downloadFile(DownloadURL, destPath); err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error downloading: %v\n", err)
		os.Exit(1)
	}

	// Verify checksum
	actualChecksum, err := computeSHA256(destPath)
	if err != nil {
		_, _ = fmt.Fprintf(os.Stderr, "Error computing checksum: %v\n", err)
		os.Exit(1)
	}

	if actualChecksum != ExpectedSHA256 {
		err := os.Remove(destPath)
		if err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error removing corrupted file: %v\n", err)
		} // Clean up corrupted file
		_, _ = fmt.Fprintf(os.Stderr, "Checksum mismatch!\n  Expected: %s\n  Got:      %s\n", ExpectedSHA256, actualChecksum)
		os.Exit(1)
	}

	fmt.Println("Download complete and verified")
}

func createPlaceholder(destPath string) error {
	// Check if file already exists
	if _, err := os.Stat(destPath); err == nil {
		fmt.Printf("Placeholder %s already exists, skipping\n", destPath)
		return nil
	}

	// Ensure destination directory exists
	if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
		return fmt.Errorf("create directory: %w", err)
	}

	// Create empty placeholder file
	f, err := os.Create(destPath)
	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}
	if err := f.Close(); err != nil {
		return fmt.Errorf("close file: %w", err)
	}

	fmt.Printf("Created placeholder %s (non-macOS build)\n", destPath)
	return nil
}

func fileExistsWithChecksum(path, expectedChecksum string) bool {
	checksum, err := computeSHA256(path)
	if err != nil {
		return false
	}
	return checksum == expectedChecksum
}

func computeSHA256(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer func(f *os.File) {
		_ = f.Close()
	}(f)

	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func downloadFile(url, destPath string) error {
	// Create temporary file for atomic write
	tmpPath := destPath + ".tmp"
	defer func(name string) {
		err := os.Remove(name)
		if err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error removing temporary file: %v\n", err)
		}
	}(tmpPath) // Clean up on failure

	out, err := os.Create(tmpPath)
	if err != nil {
		return fmt.Errorf("create file: %w", err)
	}
	defer func(out *os.File) {
		err := out.Close()
		if err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error closing file: %v\n", err)
		}
	}(out)

	resp, err := http.Get(url)
	if err != nil {
		return fmt.Errorf("http get: %w", err)
	}
	if resp == nil {
		return fmt.Errorf("http get: nil response")
	}
	defer func(Body io.ReadCloser) {
		err := Body.Close()
		if err != nil {
			_, _ = fmt.Fprintf(os.Stderr, "Error closing response body: %v\n", err)
		}
	}(resp.Body)

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("http status: %s", resp.Status)
	}

	if err := copyWithProgress(resp.Body, out, resp.ContentLength); err != nil {
		return err
	}

	// Close before rename
	err = out.Close()
	if err != nil {
		return fmt.Errorf("close: %w", err)
	}

	// Atomic rename
	if err := os.Rename(tmpPath, destPath); err != nil {
		return fmt.Errorf("rename: %w", err)
	}

	return nil
}

func copyWithProgress(src io.Reader, dst io.Writer, size int64) error {
	written := int64(0)
	lastPct := -10 // Start at -10 so we print 0%
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
