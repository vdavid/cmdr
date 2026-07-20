//go:build darwin

package main

import (
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestBulkMatchesLstat is the guard on the whole tool: if the packed-attribute
// layout in bulk_darwin.go is wrong, or a future macOS packs it differently, the
// bulk numbers would be quietly bogus and the spike's conclusion with them.
func TestBulkMatchesLstat(t *testing.T) {
	dir := t.TempDir()
	for i, size := range []int{0, 1, 511, 4096, 100000} {
		name := filepath.Join(dir, "file-"+strings.Repeat("n", i)+".bin")
		if err := os.WriteFile(name, make([]byte, size), 0o600); err != nil {
			t.Fatalf("writing fixture: %v", err)
		}
	}
	if err := os.Mkdir(filepath.Join(dir, "subdir"), 0o700); err != nil {
		t.Fatalf("creating fixture dir: %v", err)
	}
	if err := os.Symlink("file-.bin", filepath.Join(dir, "link")); err != nil {
		t.Fatalf("creating fixture symlink: %v", err)
	}

	lstat, err := Measure(MethodLstat, dir, 0)
	if err != nil {
		t.Fatalf("lstat pass: %v", err)
	}
	bulk, err := Measure(MethodBulk, dir, 128*1024)
	if err != nil {
		t.Fatalf("bulk pass: %v", err)
	}

	if bulk.Entries != lstat.Entries {
		t.Errorf("entries: bulk %d, lstat %d", bulk.Entries, lstat.Entries)
	}
	if bulk.Dirs != lstat.Dirs {
		t.Errorf("dirs: bulk %d, lstat %d", bulk.Dirs, lstat.Dirs)
	}
	if bulk.LogicalBytes != lstat.LogicalBytes {
		t.Errorf("logical bytes: bulk %d, lstat %d", bulk.LogicalBytes, lstat.LogicalBytes)
	}
	if bulk.PhysicalBytes != lstat.PhysicalBytes {
		t.Errorf("physical bytes: bulk %d, lstat %d", bulk.PhysicalBytes, lstat.PhysicalBytes)
	}
}

// TestBulkSmallBufferStillCompletes pins that a buffer too small to hold the
// whole directory just means more syscalls, not lost entries: a re-anchor
// streams a 1.4M-entry directory through a fixed buffer.
func TestBulkSmallBufferStillCompletes(t *testing.T) {
	dir := t.TempDir()
	const wantEntries = 200
	for i := 0; i < wantEntries; i++ {
		// Names of varying length, so entries are not uniformly sized and a
		// batch boundary can fall anywhere.
		name := fmt.Sprintf("file-%04d-%s", i, strings.Repeat("x", i%40))
		if err := os.WriteFile(filepath.Join(dir, name), []byte("hello"), 0o600); err != nil {
			t.Fatalf("writing fixture: %v", err)
		}
	}
	res, err := Measure(MethodBulk, dir, entryHeaderLen*8)
	if err != nil {
		t.Fatalf("bulk pass: %v", err)
	}
	if res.Entries != wantEntries {
		t.Errorf("entries: got %d, want %d", res.Entries, wantEntries)
	}
	if res.LogicalBytes != wantEntries*5 {
		t.Errorf("logical bytes: got %d, want %d", res.LogicalBytes, wantEntries*5)
	}
}

func TestParseBulkBatchRejectsBadLength(t *testing.T) {
	buf := make([]byte, 128)
	binary.LittleEndian.PutUint32(buf, 4) // shorter than one packed entry
	if _, err := parseBulkBatch(buf, 1); err == nil {
		t.Fatal("expected an error for an implausible entry length")
	}
}
