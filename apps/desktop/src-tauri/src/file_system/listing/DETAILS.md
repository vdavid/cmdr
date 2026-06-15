# File system listing details

Depth and rationale for the listing module. `CLAUDE.md` holds the must-knows that prevent silent breakage; the
narrative, data flow, and decision rationale live here. For profiling listing performance, see
`docs/guides/benchmarking-file-loading.md`.

## Data flow

```
Frontend                          Backend
   |                                   |
   |--- listDirectoryStart ----------->| (returns immediately)
   |<-- { listingId, status: loading } |
   |                                   |
   |                            [background task spawns]
   |<--- listing-opening event --------| (just before read_dir)
   |<--- listing-progress event -------| (every 200ms, { listingId, loadedCount })
   |<--- listing-read-complete event --| (when read_dir finishes, { listingId, totalCount })
   |                            [sorting + caching + watcher start]
   |<--- listing-complete event -------| (ready, { listingId, totalCount, volumeRoot })
   |                                   |
   |-- getFileRange(listingId, ...) -->| (on-demand fetching)
   |<-- [FileEntry, FileEntry, ...]    |
```

## Caching

- **`LISTING_CACHE`**: global `RwLock<HashMap<String, CachedListing>>`, keyed by `listing_id` (UUID per navigation).
- **`CachedListing`**: `{ volume_id, path, entries, sort_by, sort_order, directory_sort_mode, sequence, created_at,
  last_accessed_ms }`.
- **`caching::snapshot_listings()`**: lightweight summary of every active listing (id, volume, path, entry count, age).
  Used by `cmdr://state` so error reports surface orphan listings (started but not bound to a pane).
- **Concurrency**: multiple listings coexist (different panes, rapid navigation), each with a unique ID.

### Lifecycle

1. `list_directory_start_streaming()` receives the listing ID from the frontend, spawns a task.
2. The background task reads the directory, sorts, stores in the cache.
3. Frontend calls `get_file_range()` for visible entries (on-demand).
4. Frontend calls `find_file_indices()` to batch-resolve file names to indices (selection adjustment during operations).
5. Frontend calls `get_paths_at_indices()` / `get_files_at_indices()` for batch selection lookups (transfer dialogs,
   delete dialog, drag, clipboard).
6. `list_directory_end()` stops the watcher and removes from the cache (primary, fast eviction).

### Backstop reaper

`start_orphan_listing_reaper` (spawned in `lib.rs` setup) sweeps every `REAPER_SWEEP_INTERVAL` (30 min) and tears down
any listing idle past `ORPHAN_IDLE_WINDOW` (6 h) via the same `list_directory_end` path, so a leaked listing (close IPC
never delivered) can't pin its entry vector and OS watcher for the whole session. Pure, clock-injectable seam:
`orphan_ids(now_ms, window_ms, …)` and `reap_orphaned_listings_at(now_ms, window_ms)`. Mirrors the search index's
idle/backstop timers and the file viewer's window-`Destroyed` net.

It keys on `last_accessed_ms`, NOT `created_at`. `created_at` is stamped once and never refreshed, so an age-based reaper
keyed on it would evict a pane open all session. `last_accessed_ms` (an `AtomicU64` of ms-since-a-process-epoch) is
bumped by every operation that proves the listing still backs a live pane: the read accessors (`get_file_range`,
`get_total_count`, `get_file_at`, `get_listing_stats`, the index/path/batch lookups), `resort_listing`, and every
watcher/notify cache patch (`insert_entry_sorted` / `remove_entry_by_path` / `update_entry_sorted` /
`update_listing_entries`). `AtomicU64` so read accessors stamp it lock-free under a shared `LISTING_CACHE.read()`. The
6 h window is deliberately generous: we'd rather never evict a live listing than aggressively reclaim.
`refresh_listing_index_sizes` intentionally does NOT touch it: it's driven by background indexing, not user/FS activity,
so touching there could keep a truly-orphaned listing alive indefinitely.

## Decisions

- **Streaming with a background task, not chunked IPC**: chunked needs multiple IPC calls and complex state tracking.
  Streaming spawns a `tokio::spawn` task and emits events; the frontend stays responsive (Tab works, ESC cancels).
- **Cancellation via `AtomicBool` checked per-entry**: network folders iterate slowly (seconds per entry); a per-entry
  check keeps ESC responsive (cancel within ~100 ms).
- **Three-stage progress (opening → progress → read-complete → complete)**: `listing-opening` (about to start slow
  I/O), `listing-progress` (loaded N, every 200 ms via `list_directory_core_with_progress`), `listing-read-complete`
  (all read, sorting now), `listing-complete` (ready to render).
- **Sort after read, before caching**: the frontend expects sorted order. Sorting 50k entries takes ~15 ms, done in the
  background task after all entries are collected.
- **Enrichment at cache-write time, not on `get_file_range`**: every path that stores entries (streaming, watcher
  update, re-sort) enriches first. Index freshness is event-driven: `index-dir-updated` → `refreshIndexSizes` →
  `refresh_listing_index_sizes` (write-locks the cache, re-enriches entries). This keeps `get_listing_stats` read-only
  while it sees up-to-date `recursive_size`. The frontend calls `refreshListingIndexSizes` before `fetchListingStats`.
- **Hidden-file filtering in Rust, not the frontend**: visible count is unknown until all files are read. APIs accept
  `include_hidden: bool` and filter during `get_file_range()` iteration.
- **Font metrics in a Rust binary cache, not frontend canvas measurement**: measuring 50k filenames in JS is slow. The
  frontend measures each code point's width once via Canvas and ships the table to Rust; later text-width queries are
  hash lookups in the cached `.bin` table. `calculate_max_width()` is the entry point, used by
  `brief_columns::compute_brief_column_text_widths` to size each Brief column to its widest filename.
- **Sequence counter on `CachedListing`, not `WatchedDirectory`**: SMB and MTP volumes don't use FSEvents
  (`supports_watching() == false`), so they have no `WatchedDirectory`. With the sequence on the watcher,
  `increment_sequence` returned `None` and `directory-diff` events never fired for those volumes. The `AtomicU64` on
  `CachedListing` works for all volume types; the FSEvents path uses the same counter.
- **`ListingEventSink` trait decouples streaming from Tauri** (same pattern as `OperationEventSink`):
  `read_directory_with_progress` emits events, but `tauri::AppHandle` can't be created in tests.
  `CollectorListingEventSink` captures events for assertions. `Arc<dyn ListingEventSink>` (not `&dyn`) because the sink
  is cloned into `tokio::spawn` for progress callbacks.
- **Watcher starts AFTER listing-complete**: watcher diffs rely on cached entries; starting before the cache is
  populated would miss initial state.
- **Incremental watcher path with fallback to full re-read**: most FS changes touch a few files. The incremental path
  stats each changed path, classifies add/remove/modify against the cache, and patches in-place via
  `insert_entry_sorted` / `remove_entry_by_path` / `update_entry_sorted`. Falls back to full `handle_directory_change`
  when events exceed 500 or contain unknown kinds (`Any` / `Other`), which can't be reliably classified.
- **Synthetic diff for entry creation (`emit_synthetic_entry_diff`)**: `create_directory` / `create_file` return before
  the watcher fires; without it the new entry wouldn't appear until the next debounce (~200 ms). The command handler
  stats the new entry, inserts into all affected listings, and emits a `directory-diff` immediately. The watcher's later
  duplicate is prevented by `has_entry`.

## Cache helpers (caching.rs)

Used by the watcher's incremental path and synthetic mkdir to patch listings without full re-reads:

- `find_listings_for_path(path)`: all listing IDs whose directory matches the path (multiple panes/tabs may show the
  same directory).
- `find_listings_for_path_on_volume(volume_id, path)`: same, also filtered by volume ID. Prevents false matches when two
  volumes serve overlapping paths.
- `try_get_watched_listing(volume_id, path)`: the fresh-listing oracle for write-op pre-flight scans. Returns
  `Some(entries)` when a cached listing exists for `(volume_id, path)` and `listing_is_watched(path) == true`
  (delegated to the backend via the `Volume` trait), else `None`. When multiple listings exist for the same pair (two
  panes), picks the most-recently-updated one deterministically: highest `sequence` (an `AtomicU64`), ties broken by
  latest `created_at`. Entries are cloned out under the cache `RwLock`, then the lock is released before the volume call
  (cheap clone for a flat `Vec<FileEntry>`, < 5 ms for 15k entries; matters because otherwise the volume call holds the
  cache lock across an await and blocks pane navigation). See the freshness-contract section in `volume/CLAUDE.md` for
  per-backend debounce windows callers must tolerate.
- `insert_entry_sorted(listing_id, entry)`: inserts in sorted position, returns the insertion index.
- `remove_entry_by_path(listing_id, path)`: removes by file path, returns the removed index and entry.
- `update_entry_sorted(listing_id, entry)`: updates an existing entry (remove + re-insert if sort position changed),
  returns `ModifyResult`.
- `has_entry(listing_id, path)`: whether a path exists in the cached listing (classifies watcher events add vs modify).
- `get_listing_path(listing_id)`: the directory path for a listing (filters watcher events to direct children).

## Change notification API (caching.rs)

`notify_directory_changed(volume_id, parent_path, change)`: unified entry point for notifying the listing system that a
directory changed on a volume. `DirectoryChange` variants:

- `Added(FileEntry)`: single add, patches via `insert_entry_sorted`.
- `Removed(String)`: single remove by name, patches via `remove_entry_by_path`.
- `Modified(FileEntry)`: single modify, patches via `update_entry_sorted`.
- `Renamed { old_name, new_entry }`: same-dir rename (remove old + insert new).
- `FullRefresh`: re-reads via the Volume trait, computes a diff against the cache.

All variants enrich entries with index data and queue `directory-diff` events through `diff_emitter::enqueue_diff`.
Natural deduplication: `insert_entry_sorted` returns `None` for duplicates, `remove_entry_by_path` returns `None` if
already removed. Callers: `Volume::notify_mutation()` (after each successful create/delete/rename on all volume types)
and the `rename_file` command (local FS renames). `emit_synthetic_entry_diff` remains a legacy fallback for
`create_file` / `create_directory` on volumes where `supports_local_fs_access()` is `true`.

## Diff event coalescing (diff_emitter.rs)

All `directory-diff` emit paths funnel through `diff_emitter::enqueue_diff(listing_id, changes)` instead of calling
`app.emit` directly. The module buffers changes per listing and flushes one combined event after a 50 ms trailing
window. Producers: `caching::notify_added` / `notify_removed` / `notify_modified`; `caching::notify_full_refresh`
(SMB `STATUS_NOTIFY_ENUM_DIR` re-reads); `watcher::handle_directory_change_incremental`;
`watcher::handle_directory_change` (full re-read fallback); `commands::file_system::write_ops::emit_synthetic_entry_diff`
(`create_file` / `create_directory`); `mtp::connection::event_loop::compute_and_emit_diffs`.

**Why**: a 5k-file bulk delete used to fire one `directory-diff` per file. The frontend handler in `FilePane.svelte`
runs ~5 IPC calls per event (`getTotalCount`, `refetchColumnWidths`, `fetchEntryUnderCursor`, `fetchListingStats`, plus
a virtual-list re-fetch), so the source pane flickered heavily (the brief view's columns collapsed to width-of-name on
every recompute). Coalescing into one event per 50 ms caps the FE work at ≤ 20 emits/sec/listing and the flicker goes
away.

**Why it's safe**: only the IPC emit is deferred. Cache mutations stay synchronous and inline at the call site, so
`get_file_range` always sees the latest entries. Per-change `index` values stay correct because each producer computes
them against the cache state at the moment it mutates.

**Cleanup**: `list_directory_end` calls `diff_emitter::drop_pending(listing_id)` so an in-flight buffer for a closed
listing doesn't fire a trailing event. The E2E `flush_all_watchers` helper (`#[cfg(feature = "playwright-e2e")]`) also
calls `flush_all_pending()` so tests don't have to wait out the 50 ms window.

## File metadata tiers

Tiers 1-2 are fetched eagerly (stat + uid→name), tiers 3-4 deferred. With 50k+ files, each metadata piece has a
different cost: Tier 1 (name, size, dates, permissions) is free from a single `stat()`; Tier 2 (owner name, symlink
target) is ~1 μs and cacheable; Tier 3 (macOS Spotlight/NSURL metadata) costs ~50-100 μs/file; Tier 4 (EXIF, PDF) costs
1-100 ms+ and reads file content. See [full tier table](../../../../../../docs/notes/file-metadata-tiers.md).

macOS extended metadata (`addedAt`, `openedAt`) needs `listxattr()` / `getxattr()` beyond the fast
`fs::read_dir()` + `metadata()` path. Available via `get_extended_metadata_batch()` but not wired into the streaming
path yet.
