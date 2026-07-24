# File system listing module

Backend directory reading, caching, sorting, and streaming for the file explorer. Handles 100k+ file directories with
non-blocking I/O and progress events.

## Module map

- **reading.rs**: low-level disk I/O (`list_directory_core()`, `get_single_entry()`, macOS metadata).
- **streaming.rs**: async streaming with progress events and cancellation, via the `ListingEventSink` trait
  (`TauriListingEventSink` for prod, `CollectorListingEventSink` for tests).
- **operations.rs**: synchronous frontend-facing API (lifecycle, cache accessors). `ListingStats` carries
  `total_physical_size` and `selected_physical_size` for dual-size display.
- **caching.rs**: `LISTING_CACHE` global, `CachedListing`, the incremental cache patch helpers, and the
  `notify_directory_changed` change-notification API.
- **diff_emitter.rs**: coalesces all `directory-diff` emits into one event per 50 ms trailing window.
- **brief_columns.rs**: `compute_brief_column_text_widths()`, per-column widest-filename widths for Brief mode.
- **sorting.rs**: `SortColumn`, `SortOrder`, `sort_entries()`.
- **metadata.rs**: `FileEntry` (`physical_size` from `st_blocks * 512`; `recursive_physical_size` from the drive index).
- **fuzzy_jump.rs**: `find_first_match()` (pure) powers type-to-jump, wrapped by the `find_first_fuzzy_match` command.

Full details (data flow, caching lifecycle, the orphan reaper, all decisions, cache-helper and notification API
catalogs, diff coalescing, metadata tiers): `DETAILS.md`.

## Invariants and gotchas

- **`get_file_range()` indices are over VISIBLE items only.** With `include_hidden=false` the frontend sees a dense
  array, so backend index N maps to a different absolute entry. Filtering happens in Rust.

- **Watcher diffs must update the cache AND emit an event.** The cache is the source of truth for `get_file_range()`;
  the event tells the frontend to re-fetch. Miss either and you get stale data or no UI update.
- **The full re-read watcher path re-sorts `new_entries` before `compute_diff` (looks like a double-sort, isn't).**
  `list_directory_core` always returns Name/Asc, but the listing may use another sort; without the re-sort, diff indices
  are computed against a differently-ordered list and add/remove positions come out wrong.
- **All `directory-diff` emits must go through `diff_emitter::enqueue_diff`, never `app.emit` directly.** Direct emits
  bypass the 50 ms coalescing and re-introduce per-file flicker on bulk operations. Cache mutations stay synchronous and
  inline; only the emit is deferred.
- **The orphan reaper keys on `last_accessed_ms`, not `created_at`.** Every read accessor and cache patch must bump it,
  or the reaper (6 h idle window) could evict a live pane. Not from `refresh_listing_index_sizes` though
  (background-indexing driven, not user activity).
- **Listing cancellation sets both `AtomicBool` and `tokio::sync::Notify`.** `cancel_listing()` does
  `cancelled.store(true)` + `cancel_notify.notify_waiters()`: the `Notify` drives async cancellation via `select!`, the
  `AtomicBool` covers sync check points and actually stops the task early. ❌ The `select!` cancel arm must never
  `listing_task.abort()`: `cancelled` IS the backend's cancel token, so returning detaches a safely-unwinding task,
  while aborting wedges an MTP phone mid-round-trip. `DETAILS.md` § "Cancelling a listing detaches, never aborts".
- **Watcher callbacks run on OS threads, not the tokio runtime.** Async work from a callback must use
  `tauri::async_runtime::spawn`; bare `tokio::spawn` panics ("there is no reactor running") and aborts the app. All
  FullRefresh dispatch funnels through `caching::spawn_full_refresh`, covering every producer (FSEvents, git, SMB, MTP,
  archive) at once. The incremental path stays sync.
- **Sequence counter lives on `CachedListing`, not `WatchedDirectory`.** SMB/MTP have no `WatchedDirectory`; keeping it
  there breaks their `directory-diff`.
- **A sort change invalidates the frontend's cached range.** Bump `cacheGeneration` to re-fetch.
- **New listing state hangs off a struct, not a `static`.** Fixtures go through `caching_test_support::TestListing`
  (unique id, RAII teardown); cache-wide assertions need a unique path. `DETAILS.md` § "Test isolation".
- **Finder tags are deferred and must survive re-stats.** `list_directory_core` never reads tags (`getxattr` is ~6× an
  `lstat`); `enrich_tags` fills them visible-range-first via `apply_tags_to_listing`, which replaces unconditionally so
  external removals propagate. A watcher re-stat builds entries with empty tags, so every modify path calls
  `carry_forward_tags` BEFORE storing/emitting, else an mtime touch blanks a file's dots. Don't route the enrich path
  through it (that would block real removals).
