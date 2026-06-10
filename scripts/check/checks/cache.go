package checks

import (
	"encoding/json"
	"os"
	"path/filepath"
	"time"
)

// The per-worktree check cache records, for each check that last PASSED, the
// fingerprint of the inputs it passed on. A later run that produces the same
// fingerprint can skip the check: identical inputs, already-green result. Only
// passing runs are cached (failures and warns always re-run); a corrupt or
// missing cache degrades to "run everything", never an error.
//
// It lives under node_modules/.cache/ so it shares node_modules' fate: blowing
// away node_modules (or the whole worktree) discards it cleanly, the same
// shared-fate trick the pnpm-install marker uses. Writes are atomic (temp+rename)
// so a killed run can't leave a half-written file that then reads as corrupt.

// CheckCachePath is the per-worktree cache location, relative to the repo root.
const CheckCachePath = "node_modules/.cache/cmdr-check-cache.json"

// CacheEntry records one passing run of a check.
type CacheEntry struct {
	Fingerprint string `json:"fingerprint"`
	// Message is the pass's summary line, replayed on a cache hit so the skip line
	// shows real context ("12 tests passed") rather than a bare "(cached)".
	Message string `json:"message"`
	// PassedAt is when the cached pass ran (informational; helps a human reading
	// the cache file).
	PassedAt time.Time `json:"passedAt"`
}

// CheckCache maps check ID → its last passing entry.
type CheckCache struct {
	Entries map[string]CacheEntry `json:"entries"`
}

// LoadCheckCache reads the cache from disk. A missing or corrupt file yields an
// empty cache and no error: the worst case is running everything, which is safe.
func LoadCheckCache(rootDir string) *CheckCache {
	empty := &CheckCache{Entries: map[string]CacheEntry{}}
	b, err := os.ReadFile(filepath.Join(rootDir, CheckCachePath))
	if err != nil {
		return empty
	}
	var c CheckCache
	if err := json.Unmarshal(b, &c); err != nil || c.Entries == nil {
		return empty
	}
	return &c
}

// Save writes the cache atomically (temp file + rename). Errors are returned but
// are non-fatal to the caller — a failed cache write only costs the next run a
// re-check, it never breaks the current one.
func (c *CheckCache) Save(rootDir string) error {
	cachePath := filepath.Join(rootDir, CheckCachePath)
	if err := os.MkdirAll(filepath.Dir(cachePath), 0o755); err != nil {
		return err
	}
	b, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	tmp, err := os.CreateTemp(filepath.Dir(cachePath), ".cmdr-check-cache-*.tmp")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	if _, err := tmp.Write(b); err != nil {
		tmp.Close()
		os.Remove(tmpName)
		return err
	}
	if err := tmp.Close(); err != nil {
		os.Remove(tmpName)
		return err
	}
	return os.Rename(tmpName, cachePath)
}
