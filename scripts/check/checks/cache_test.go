package checks

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestCacheRoundTrip(t *testing.T) {
	dir := t.TempDir()
	c := &CheckCache{Entries: map[string]CacheEntry{
		"oxfmt": {Fingerprint: "abc123", Message: "25 files formatted", PassedAt: time.Now()},
	}}
	if err := c.Save(dir); err != nil {
		t.Fatalf("Save: %v", err)
	}
	loaded := LoadCheckCache(dir)
	got, ok := loaded.Entries["oxfmt"]
	if !ok {
		t.Fatal("entry not loaded")
	}
	if got.Fingerprint != "abc123" || got.Message != "25 files formatted" {
		t.Fatalf("round-trip mismatch: %+v", got)
	}
}

func TestLoadMissingCacheIsEmpty(t *testing.T) {
	c := LoadCheckCache(t.TempDir())
	if c == nil || c.Entries == nil {
		t.Fatal("missing cache should load as a non-nil empty cache")
	}
	if len(c.Entries) != 0 {
		t.Fatalf("expected 0 entries, got %d", len(c.Entries))
	}
}

func TestLoadCorruptCacheIsEmpty(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, CheckCachePath)
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte("{not valid json"), 0o644); err != nil {
		t.Fatal(err)
	}
	c := LoadCheckCache(dir)
	if c == nil || len(c.Entries) != 0 {
		t.Fatal("corrupt cache must degrade to an empty cache, never panic or error")
	}
}

func TestSaveCreatesCacheDir(t *testing.T) {
	dir := t.TempDir()
	c := &CheckCache{Entries: map[string]CacheEntry{}}
	if err := c.Save(dir); err != nil {
		t.Fatalf("Save should create node_modules/.cache: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dir, CheckCachePath)); err != nil {
		t.Fatalf("cache file not written: %v", err)
	}
}
