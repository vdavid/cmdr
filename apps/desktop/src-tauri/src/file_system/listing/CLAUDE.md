# File system listing module

Backend directory reading, caching, sorting, and streaming for the file explorer, over 100k+ file directories, with
non-blocking I/O and progress events.


## Module map
- **reading.rs** disk I/O, **streaming.rs** async streaming with progress + cancellation (`ListingEventSink`),
  **operations.rs** the sync API, **cached_listing.rs** `CachedListing` + `LISTING_CACHE`, **caching.rs** patch
  helpers / `notify_directory_changed`, **orphan_reaper.rs** the 6 h backstop, **mutation.rs** `notify_mutation`.
- **diff.rs** the `DiffChange` vocabulary plus `compute_diff`, **diff_emitter.rs** coalescing (50 ms trailing window),
  **visible_rows.rs** / **path_index.rs** row numbers and paths materialized once so accessors index instead of walk,
  **listing_host.rs** `AppListings` for out-of-crate backends, **sorting.rs**, **brief_columns.rs**, **fuzzy_jump.rs**.
  `FileEntry` is `cmdr-fs`'s, aliased as `listing::metadata`.

## Invariants and gotchas
- **Neither a row number nor a path indexes `entries`.** Rows drop dotfiles and in-flight scratch
  (`file_system::staging`), so `CachedListing::rows` is the ONLY filter point, on READ, never on cache fill. Every path
  goes through the path map: `indices_of_paths` for a batch, `index_of_path` for one. ❗ A MUTATING caller resolves
  BEFORE `entries_mut`, which drops both maps. `entries` is private so no accessor grows its own filter — three had,
  each a row off — and re-deriving per item wedged a 74k directory. `DETAILS.md` §§ "Row numbers", "Entries by path".
- **Listing read commands are `async`**: a sync `#[tauri::command]` runs on Tauri 2's main thread, so one slow accessor
  stops the app answering IPC.
- **Watcher diffs must update the cache AND emit an event**, else stale data or no update.
- **Refreshes of ONE directory stay serialized** (`notify_full_refresh`): concurrently an older read lands last, stranding a pane missing files. `DETAILS.md` § "Serializing full refreshes".
- **Rows a PANE sees that no volume holds are folded in by `listing_overlays::decorate`**, between enrich and the sort,
  in all THREE read paths (`streaming.rs`, `operations.rs`, the watcher's full refresh); miss one and a refresh strips
  them. `has_overlay_rows` then makes the fresh-listing oracle decline that listing, so no walker meets a row with no
  inode. `DETAILS.md` § "The overlay step".
- **The full re-read watcher path re-sorts `new_entries` before `compute_diff`** (looks like a double-sort, isn't):
  `list_directory_core` returns Name/Asc, so without it add/remove indices are wrong.
- **All `directory-diff` emits go through `diff_emitter::enqueue_diff`, never `app.emit`**: a direct emit bypasses the
  50 ms coalescing and re-introduces per-file flicker. Only the emit is deferred; cache writes stay synchronous.
- **The orphan reaper keys on `last_accessed_ms`, not `created_at`**: every read accessor and cache patch must bump it,
  or it evicts a live pane. ❌ Never from `refresh_listing_index_sizes` (background work).
- **`read_directory_with_progress` holds a `priority::foreground` lease for its whole body**, so an SMB upload and the
  index scan stand aside for a slow folder. RAII: ❌ never bind it to `_`. `DETAILS.md` § "The foreground lease".
- ❌ **The `select!` cancel arm must never `listing_task.abort()`**: returning detaches a safely-unwinding task,
  aborting wedges an MTP phone mid-round-trip. `DETAILS.md` § "Cancelling a listing detaches".
- **Watcher callbacks run on OS threads, not the tokio runtime**, where a bare `tokio::spawn` panics and aborts the app:
  use `tauri::async_runtime::spawn`, and dispatch FullRefresh through `caching::spawn_full_refresh`.
- **The sequence counter is on `CachedListing`, not `WatchedDirectory`**; SMB/MTP have none.
- **A sort change invalidates the frontend's cached range**; bump `cacheGeneration` to re-fetch.
- **New listing state hangs off a struct, not a `static`**; fixtures use `caching_test_support::TestListing`.
- **Finder tags are deferred and must survive re-stats.** `list_directory_core` never reads them, and every modify path
  calls `carry_forward_tags` BEFORE storing, else an mtime touch blanks a file's dots. ❌ Never route enrich through it.

Data flow, the caching lifecycle, the orphan reaper, decisions, the helper catalogs, diff coalescing, metadata tiers,
the overlay step, and tags: `DETAILS.md`. Read it before any non-trivial work here.
