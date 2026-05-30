# `LISTING_CACHE` / `WATCHER_MANAGER` entries are evicted only on explicit `list_directory_end`; no TTL backstop

**Severity:** low
**Lens:** B / G — Resource hygiene (watcher leaks)
**Confidence:** low

## Location
`apps/desktop/src-tauri/src/file_system/listing/operations.rs:120` (`list_directory_end`), `apps/desktop/src-tauri/src/file_system/watcher.rs:159` (`stop_watching`).

## What
Each `list_directory_start` mints a fresh UUID `listing_id`, inserts a `CachedListing` into `LISTING_CACHE` and a `notify` debouncer into `WATCHER_MANAGER.watches`. Both are removed only when the frontend calls `list_directory_end(listing_id)`. There's no time-based sweeper: `CachedListing.created_at` exists but is used only for tie-breaking in `try_get_watched_listing`, not for eviction.

## Why it matters
If the frontend ever fails to pair an `end` with a `start` (navigation race, panic, dropped IPC), that listing's cache entry and its live FS watcher leak for the process lifetime — a slow accumulation of FSEvents/inotify watchers and cached entry vectors. Watchers are a finite OS resource (Linux `fs.inotify.max_user_watches` defaults ~8192). This is the documented explicit-lifecycle contract, and no concrete FE path that drops the `end` call was found, hence low confidence — flagging because it's the one long-lived map in the listing subsystem with no backstop eviction.

## Evidence
`list_directory_start` → insert into `LISTING_CACHE` + `WATCHER_MANAGER`; the only removal site is `list_directory_end(listing_id)`. No periodic sweep keyed on `created_at`/last-access.

## Suggested fix
Verify the FE always calls `list_directory_end` on every navigation/teardown path (including error and volume-unmount). If that can't be guaranteed exhaustively, add an idle-TTL sweep keyed on `created_at` (or last-access) as a safety net. No action needed if the FE pairing is provably exhaustive.

## Notes
The SMB soak test (`smb_soak_copy_loop`) watches FD/RSS growth for the SMB path but doesn't exercise the local listing-watcher lifecycle. A long-running local-navigation soak would catch a real leak here if one exists.
