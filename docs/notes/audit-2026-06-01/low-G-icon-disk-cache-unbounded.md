# On-disk icon cache grows without bound across sessions

**Severity:** low
**Lens:** G — Resource hygiene
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/icons/disk_cache.rs:105-145` (`store` / `store_in`)
`apps/desktop/src-tauri/src/icons/disk_cache.rs:152-161` (`clear_all` — the only deletion path)

## What
The persistent warm-tier icon cache writes one JSON sidecar file per real-folder icon id
(`special:*`, `pkg:*`, `path:*`) under `<data_dir>/icon-cache/`, keyed by an FNV-1a digest of the icon
id. There is no entry cap, no LRU, and no age-based eviction. The only removal path is `clear_all`,
which wipes the entire directory and only fires on a system theme/accent change. The in-memory `path:*`
/ `pkg:*` cache IS LRU-capped (`PATH_KEY_CAP`), but that cap never propagates to the on-disk tier.

## Why it matters
Every distinct package (`.app`/`.bundle`) and custom-icon / volume folder the user ever browses leaves
a sidecar that persists forever. A power user who browses thousands of distinct `.app` bundles, mounted
volumes, and custom-icon folders over weeks accumulates thousands of small files in one flat directory.
This is slow disk growth (each sidecar holds a base64 WebP data URL — a few KB), not memory, and a
re-iconed folder overwrites in place (same digest filename), so it's not pathological — but it's
genuinely unbounded and never trimmed except by an unrelated theme change. A flat directory with tens of
thousands of files also degrades the `load`/`store` directory operations on some filesystems.

## Evidence
```rust
pub fn store(icon_id: &str, real_path: &str, data_url: &str) {
    let Some(dir) = CACHE_DIR.as_ref() else { return; };
    store_in(dir, icon_id, real_path, data_url);   // writes one sidecar per icon id, forever
}

/// Drops the entire on-disk cache. Called on a theme/accent change ...
pub fn clear_all() {                                // the ONLY deletion path
    if let Some(dir) = CACHE_DIR.as_ref()
        && let Err(e) = fs::remove_dir_all(dir) { ... }
}
```
The in-memory cap that does NOT reach disk:
```rust
// icons/mod.rs
while self.path_lru.len() > PATH_KEY_CAP {
    if let Some(evicted) = self.path_lru.pop_front() { self.entries.remove(&evicted); }
}
```

## Suggested fix
Bound the on-disk tier the same way the in-memory tier is bounded. Cheapest: on `store_in`, after a
successful write, opportunistically count entries and, when over a cap (say a few thousand), delete the
oldest sidecars by file mtime — a single `read_dir` + sort + truncate, run at most every Nth store to
amortize cost. Alternatively prune entries whose own mtime is older than a generous TTL on startup. Keep
it best-effort and non-fatal, consistent with the module's "graceful miss" philosophy.

## Notes
This is the slowest-moving item in the lens and explicitly a cross-session disk concern, not an
in-session memory leak. Filed because the module's own doc calls out that `path:*`/`pkg:*` are
"unbounded (grow with folders visited)" and caps them in memory — the on-disk mirror simply didn't
inherit the cap. Could reasonably be deferred past launch.
