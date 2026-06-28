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
- **brief_columns.rs**: `compute_brief_column_text_widths()`, per-column widest-filename widths for Brief mode (wrapped
  by the `get_brief_column_text_widths` IPC command).
- **sorting.rs**: `SortColumn`, `SortOrder`, `sort_entries()`.
- **metadata.rs**: `FileEntry` (`physical_size` from `st_blocks * 512`; `recursive_physical_size` from the drive index).
- **fuzzy_jump.rs**: `find_first_match()` (pure) powers type-to-jump, wrapped by the `find_first_fuzzy_match` command.

Full details (data flow, caching lifecycle, the orphan reaper, all decisions, cache-helper and notification API
catalogs, diff coalescing, metadata tiers): [DETAILS.md](DETAILS.md).

## Invariants and gotchas

- **`get_file_range()` indices are over VISIBLE items only.** With `include_hidden=false`, hidden entries are skipped, so
  backend index N maps to a different absolute entry; the frontend sees a dense array. Filtering happens in Rust.
- **Watcher diffs must update the cache AND emit an event.** The cache is the source of truth for `get_file_range()`;
  the event tells the frontend to re-fetch the visible range. Miss either and you get stale data or no UI update.
- **The full re-read watcher path re-sorts `new_entries` before `compute_diff` (looks like a double-sort, isn't).**
  `list_directory_core` always returns Name/Asc order, but the cached listing may use a different sort. Without the
  re-sort, diff indices are computed against a differently-ordered list and add/remove positions come out wrong.
- **All `directory-diff` emits must go through `diff_emitter::enqueue_diff`, never `app.emit` directly.** Direct emits
  bypass the 50 ms coalescing and re-introduce the per-file flicker on bulk operations. Cache mutations stay synchronous
  and inline; only the emit is deferred.
- **The orphan reaper keys on `last_accessed_ms`, not `created_at`.** Every read accessor and cache patch must keep
  bumping `last_accessed_ms`, or the reaper (6 h idle window) could evict a live pane. Do NOT bump it from
  `refresh_listing_index_sizes` (background-indexing driven, not user activity).
- **Listing cancellation sets both `AtomicBool` and `tokio::sync::Notify`.** `cancel_listing()` does
  `cancelled.store(true)` + `cancel_notify.notify_waiters()`: the `Notify` drives instant async cancellation via
  `select!`, the `AtomicBool` covers sync check points where `.await` isn't available. The background task runs to
  completion regardless; the per-entry `AtomicBool` check is what stops it early (the frontend `loadGeneration` only
  discards stale results).
- **FSEvents watcher callbacks run on OS threads, not the tokio runtime.** Async work from a callback
  (`handle_directory_change`, `notify_directory_changed(FullRefresh)`) must be dispatched via `tokio::spawn`. The
  incremental path (`handle_directory_change_incremental`) stays sync (only cache lookups and `get_single_entry` stats).
- **Sequence counter lives on `CachedListing`, not `WatchedDirectory`.** SMB/MTP have no `WatchedDirectory`; keeping it
  there breaks `directory-diff` for those volumes.
- **A sort change invalidates the frontend's cached range.** Bump `cacheGeneration` so the frontend re-fetches.
- **Finder tags are deferred and must survive re-stats.** `list_directory_core` never reads tags (a `getxattr` is ~6×
  an `lstat`; too costly inline — see [DETAILS.md](DETAILS.md)). The `enrich_tags` command fills them visible-range-first
  via `apply_tags_to_listing` (replaces unconditionally, incl. clearing to empty so external removals propagate). Because
  a watcher re-stat builds entries with empty tags, every modify path calls `carry_forward_tags` BEFORE storing/emitting,
  or an unrelated change (mtime touch, chmod) would blank a file's dots. Don't route the enrich path through
  `carry_forward_tags` (it would block real removals).
