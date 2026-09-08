package main

import (
	"os"
	"path/filepath"
	"testing"
)

// TestWriteOpacityStatusFile_WritesCountWhenEnvSet covers the check-runner
// side channel: when CMDR_A11Y_OPACITY_STATUS_FILE is set and there are
// findings, the count lands in the file so the wrapper can tell a clean run
// apart from an advisory-only one without parsing stdout (see the package
// doc comment and `desktop-svelte-a11y-contrast.go`).
func TestWriteOpacityStatusFile_WritesCountWhenEnvSet(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "count.txt")
	t.Setenv("CMDR_A11Y_OPACITY_STATUS_FILE", path)

	writeOpacityStatusFile(12)

	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("expected status file to be written: %v", err)
	}
	if string(got) != "12\n" {
		t.Errorf("status file contents = %q, want %q", got, "12\n")
	}
}

// TestWriteOpacityStatusFile_NoopWhenCountZero covers the "fully clean" run:
// no findings, no file, so a leftover file from a previous run (the wrapper
// always uses a fresh temp dir, but this is still the contract) never reads
// as "findings remain".
func TestWriteOpacityStatusFile_NoopWhenCountZero(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "count.txt")
	t.Setenv("CMDR_A11Y_OPACITY_STATUS_FILE", path)

	writeOpacityStatusFile(0)

	if _, err := os.Stat(path); err == nil {
		t.Errorf("expected no status file to be written for count=0")
	}
}

// TestWriteOpacityStatusFile_NoopWhenEnvUnset covers a direct/manual
// `go run .`: no env var, no file, no error — the human-readable report is
// the only output.
func TestWriteOpacityStatusFile_NoopWhenEnvUnset(t *testing.T) {
	t.Setenv("CMDR_A11Y_OPACITY_STATUS_FILE", "")
	// Should not panic or attempt to write anywhere.
	writeOpacityStatusFile(5)
}
