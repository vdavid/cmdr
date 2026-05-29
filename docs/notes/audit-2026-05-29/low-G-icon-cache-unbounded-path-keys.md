# Icon cache `path:*` entries grow unbounded across navigation

**Severity:** low
**Lens:** G — Resource hygiene
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/icons.rs:24` (`ICON_CACHE: LazyLock<RwLock<HashMap<String, String>>>`)
- `apps/desktop/src-tauri/src/icons.rs:43-51` (`clear_directory_icon_cache` — only fires on theme / accent change)
- `apps/desktop/src-tauri/src/icons.rs:288, 295` (insertion of `path:<full path>` keys)

## What

`ICON_CACHE` is a process-global `HashMap<String, String>` with three key shapes:
- `ext:<lowercase ext>` — bounded by the count of unique extensions on the system (small).
- `dir` / `symlink-dir` / `symlink-file` / `file` — fixed, ~5 entries.
- `path:<full directory path>` — inserted by `refresh_directory_icons` / batched icon fetches when the OS reports a custom icon for a specific directory (signed/branded folders, app bundles, mounted volumes).

The cache has no size cap, no LRU, no TTL. The only eviction paths are:
- `clear_extension_icon_cache()` — fires when the "use app icons for documents" setting flips.
- `clear_directory_icon_cache()` — fires when the system theme or accent color changes; evicts `dir`, `symlink-dir`, and any `path:*`.

So during a normal session (no setting flips, no theme change), every `path:` entry accumulates for as long as the app runs.

## Why it matters

- Per-entry cost is small (32×32 WebP base64 data URL ≈ 1-4 KB) but linear in unique directory paths visited.
- A power user navigating widely across a large home folder + external volumes can hit thousands of `path:*` entries in an hour. At 2 KB average that's ~2 MB per thousand — bounded by the user's directory tree size but not by anything Cmdr controls.
- Theme changes are the only natural eviction; many users never flip themes during a session.

The volume usage is genuinely modest compared to the search index (~600 MB) or scan-preview leak above; that's why this is low. But it's a strictly linear function of "directories visited" with no upper bound from the code's side, and the documentation pattern elsewhere in the project (`LISTING_CACHE` triage via `snapshot_listings`, search index idle/backstop timers) suggests the intent is to bound long-lived caches.

## Evidence

`icons.rs` lines 24, 286-296:
```rust
static ICON_CACHE: LazyLock<RwLock<HashMap<String, String>>> = LazyLock::new(|| RwLock::new(HashMap::new()));
// …
let data_url = fetch_icon_for_path(&path_buf);
(format!("path:{}", path), data_url)
// …
for (icon_id, data_url) in dir_results {
    if let Some(url) = data_url {
        cache_icon(icon_id.clone(), url.clone());
        result.insert(icon_id, url);
    }
}
```

`clear_directory_icon_cache` (the only path-eviction site):
```rust
pub fn clear_directory_icon_cache() {
    ICON_CACHE.write().unwrap().retain(|key, _| key != "dir" && key != "symlink-dir" && !key.starts_with("path:"));
}
```
Caller is the theme/accent-changed handler.

## Suggested fix

Add an LRU cap to the `path:*` keyspace. Keep `ext:*` and the fixed special keys unbounded (their count is system-bounded). Something like:
- Wrap `ICON_CACHE`'s value or split into two maps: one for `ext:` + special keys (no cap), one for `path:` keys (LRU-capped at, say, 256 entries).
- Bump the LRU on every read in `get_cached_icon`.

Alternative: drop `path:*` entries on every `clear_listing` / `list_directory_end` for paths whose listings no longer exist. Less elegant, more coupling.

## Notes

- Cost is genuinely small. The reason to file is that it's an unbounded cache that grows linearly with user activity, which the audit charter calls out as a high-confidence "low" even when impact is modest.
- The font_metrics cache and `space_poller::LAST_SPACE` are similarly unbounded but their key spaces are bounded by font count and volume count respectively — natural caps. The icon `path:*` key space is bounded only by the user's filesystem.
