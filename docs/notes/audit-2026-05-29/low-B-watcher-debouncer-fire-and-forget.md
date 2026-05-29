# Git watcher debouncer callback ignores `recompute_and_emit` errors silently

**Severity:** low
**Lens:** B — Concurrency
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/file_system/git/watcher.rs:71-79` plus `recompute_and_emit` at lines 149-169

## What
The notify-debouncer callback fires `recompute_and_emit(&app_for_cb, &watcher_root)` synchronously from the notify-rs internal thread. That function calls `discover_repo` + `repo_info` (which runs gix's `is_dirty` walk over the worktree) — synchronous, possibly expensive on a 50k-file repo.

```rust
let mut debouncer = new_debouncer(Duration::from_millis(200), None, move |result| {
    if result.is_err() { return; }
    recompute_and_emit(&app_for_cb, &watcher_root);  // sync gix walk on notify thread
});
```

## Why it matters
The notify-rs debouncer maintains its own thread; running a synchronous repo walk inside the callback ties up that thread for the duration. While one event batch is being processed, subsequent `.git/*` events are buffered. For a casual repo that's fine, but for a large repo where `is_dirty` lands at ~60 ms p95 (per the docs), back-to-back fast index changes (e.g., `git commit -a` followed quickly by `git stash`) can queue up.

This is a small concern in absolute terms — the 200 ms debounce already coalesces — but the callback path also calls `super::status::invalidate_status_cache` and `invalidate_virtual_listings`, the latter walking every open listing's path and posting `notify_directory_changed`. If any of those `notify_directory_changed` calls then dispatch async work via `tokio::spawn` from inside the notify thread, there's no runtime guarantee on the notify thread (the file_system/CLAUDE.md gotcha "Never `tokio::spawn` from the notify-rs debouncer callback" calls this out for a different module).

I traced `notify_directory_changed` -> `notify_full_refresh` -> `tokio::spawn` (in `caching.rs:323` and `:332`). The `tokio::spawn` calls there require a runtime context. The git watcher debouncer runs on notify-rs's internal thread, NOT a tokio worker. If `notify_directory_changed` ever reaches the `tokio::spawn` branch from this callback chain, it panics with "there is no reactor running."

The path: `recompute_and_emit` -> `invalidate_virtual_listings` -> `refresh_local_listings_under` -> `notify_directory_changed(volume_id, listing_path, DirectoryChange::FullRefresh)` — which is exactly the branch that hits `tokio::spawn`.

There's a guard in `notify_directory_changed` (`let has_app = WATCHER_MANAGER.read().ok().and_then(|m| m.app_handle.clone()).is_some();`) that returns early if no AppHandle is registered, but in production after `setup()` completes, the AppHandle is always registered — so the guard does not help.

## Evidence
- `git/watcher.rs:71-79`: synchronous closure runs `recompute_and_emit` directly.
- `git/watcher.rs:168`: `invalidate_virtual_listings(&root)` called from `recompute_and_emit`.
- `git/watcher.rs:196-222`: `refresh_local_listings_under` calls `notify_directory_changed(.., FullRefresh)`.
- `file_system/listing/caching.rs:315-334`: the `FullRefresh` branch always calls `tokio::spawn(notify_full_refresh(...))`.
- `file_system/CLAUDE.md` (modified) § Gotchas: "Never `tokio::spawn` from the notify-rs debouncer callback. The callback runs on the notify-rs internal thread which has no Tokio runtime context, so `tokio::spawn` panics. ... This bit the file watcher's full-reread fallback path."

Confidence is medium because I haven't confirmed whether the git tests exercise a `.git/HEAD` change with a real listing open at `.git/branches/...` — the panic would happen at runtime if that combination is hit. Bench tests and unit tests using `invalidate_for_test` (which calls `invalidate_virtual_listings` directly, bypassing the watcher) would not reproduce it because they run in `#[tokio::test]` context.

## Suggested fix
Hop the recompute work onto the tokio runtime, matching the pattern the file_system listing watcher uses for its full-reread fallback:

```rust
let app = app_for_cb.clone();
let root = watcher_root.clone();
tauri::async_runtime::spawn(async move {
    tauri::async_runtime::spawn_blocking(move || {
        recompute_and_emit(&app, &root);
    }).await.ok();
});
```

`tauri::async_runtime::spawn` is the safe form (the file_system module uses it specifically because the notify thread has no tokio context). The synchronous gix walk goes inside `spawn_blocking`.

## Notes
File the panic case as a follow-up integration test: subscribe to a repo, open a `.git/branches/foo` listing, mutate the index to fire the watcher, assert no panic.
