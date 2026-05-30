# Per-path directory-icon cache (`path:` keys) has no per-entry eviction — latent unbounded growth

**Severity:** low **Lens:** G — Resource hygiene **Confidence:** medium

## Location

`apps/desktop/src-tauri/src/icons.rs:278-308` (`refresh_icons_for_directory`), FE mirror
`apps/desktop/src/lib/icon-cache.ts:218-220`

## What

`refresh_icons_for_directory` fetches a custom folder icon per directory and caches it in the global `ICON_CACHE` under
`format!("path:{}", path)` — one entry per distinct directory full-path. Unlike the `dir` / `ext:*` / `file` keys (a
tiny, inherently bounded set), `path:` keys are unbounded in the number of directories a user can visit. The only
eviction is a wholesale `clear_directory_icon_cache()` on theme/accent change; there is no cap, no TTL, no
per-navigation eviction. In current code this is **latent** rather than live: the normal listing path uses
`prefetchIcons` with bounded icon-ids, and there's no component caller of `refresh_directory_icons` (only the generated
binding), so the `path:` branch isn't exercised today. But the growth path is wired end-to-end and would activate the
moment a caller feeds directory paths.

## Why it matters

If/when the per-folder-icon feature is wired to fire on navigation, a long session browsing thousands of distinct
folders accumulates one base64 WebP data-URL string per folder in both the Rust `ICON_CACHE` and the FE `memoryCache`
(the FE also persists `path:` keys to localStorage), with no bound — steady RSS growth proportional to folders-visited
over a multi-hour session.

## Evidence

```rust
// icons.rs — every directory path becomes its own permanent cache key
.map(|path| {
    let path_buf = PathBuf::from(path);
    let data_url = fetch_icon_for_path(&path_buf);
    (format!("path:{}", path), data_url)
})
// ...
for (icon_id, data_url) in dir_results {
    if let Some(url) = data_url {
        cache_icon(icon_id.clone(), url.clone());  // inserted; only cleared wholesale on theme change
```

## Suggested fix

Before wiring any per-navigation caller of `refresh_directory_icons`, give the `path:`-keyed entries an LRU cap (a few
hundred entries) or evict them when the owning listing ends (`list_directory_end`), the same lifecycle the listing cache
already uses. The shared `dir` / `ext:` / `file` keys can stay uncapped (inherently bounded). Mirror the cap on the FE
`memoryCache` and skip persisting `path:` keys to localStorage.

## Notes

- No CLAUDE.md documents a cap for `ICON_CACHE`; the file-explorer "Icon registry pattern" decision describes the
  bounded `ext:` / `iconId` design but not the per-path branch. Filed as low because it's a latent (currently-unreached)
  path, not a live leak — but it's worth fencing before the feature ships.
