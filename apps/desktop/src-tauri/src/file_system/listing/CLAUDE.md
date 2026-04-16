# File system listing module

Backend directory reading, caching, sorting, and streaming for the file explorer. Handles 100k+ file directories with non-blocking I/O and progress events.

## Architecture

### Module structure

- **mod.rs** – Public API exports, re-exports for crate-internal use
- **reading.rs** – Low-level disk I/O (`list_directory_core()`, `get_single_entry()`, macOS metadata)
- **streaming.rs** – Async streaming with progress events, cancellation. Uses `ListingEventSink` trait (same pattern as `OperationEventSink` in write_operations) to decouple from Tauri. `TauriListingEventSink` for production, `CollectorListingEventSink` for tests
- **operations.rs** – Synchronous frontend-facing API (lifecycle, cache accessors). `ListingStats` includes `total_physical_size` and `selected_physical_size` for dual-size display
- **caching.rs** – `LISTING_CACHE` global state, `CachedListing` struct, cache helpers for incremental updates
- **sorting.rs** – `SortColumn`, `SortOrder`, `sort_entries()`
- **metadata.rs** – `FileEntry` struct, macOS extended metadata. `FileEntry` has `physical_size: Option<u64>` (populated from `st_blocks * 512`) and `recursive_physical_size: Option<u64>` (populated from drive index)

### Data flow

```
Frontend                          Backend
   |                                   |
   |--- listDirectoryStart ----------->| (returns immediately)
   |<-- { listingId, status: loading } |
   |                                   |
   |                            [background task spawns]
   |                                   |
   |<--- listing-opening event --------| (just before read_dir)
   |<--- listing-progress event -------| (every 200ms)
   |     { listingId, loadedCount }    |
   |                                   |
   |<--- listing-read-complete event --| (when read_dir finishes)
   |     { listingId, totalCount }     |
   |                                   |
   |                            [sorting + caching + watcher start]
   |                                   |
   |<--- listing-complete event -------| (ready for use)
   |     { listingId, totalCount,      |
   |       maxFilenameWidth,           |
   |       volumeRoot }               |
   |                                   |
   |-- getFileRange(listingId, ...) -->| (on-demand fetching)
   |<-- [FileEntry, FileEntry, ...]    |
```

### Caching strategy

**LISTING_CACHE**: Global `RwLock<HashMap<String, CachedListing>>`
**Key**: `listing_id` (UUID per navigation)
**Value**: `CachedListing { volume_id, path, entries, sort_by, sort_order, directory_sort_mode, sequence }`

**Lifecycle**:
1. `list_directory_start_streaming()` receives listing ID from frontend, spawns task
2. Background task reads directory, sorts, stores in cache
3. Frontend calls `get_file_range()` for visible entries (on-demand)
4. Frontend calls `find_file_indices()` to batch-resolve file names to indices (used by selection adjustment during operations)
5. Frontend calls `get_paths_at_indices()` or `get_files_at_indices()` for batch selection lookups (transfer dialogs, delete dialog, drag, clipboard)
6. `list_directory_end()` stops watcher, removes from cache

**Concurrency**: Multiple listings can coexist (different panes, rapid navigation). Each has unique ID.

## Key decisions

**Decision**: Streaming with background task, not chunked IPC
**Why**: Chunked approach required multiple IPC calls, complex state tracking. Streaming spawns a `tokio::spawn` async task, emits events. Frontend stays responsive—Tab works, ESC cancels via `tokio::select!`-style polling.

**Decision**: Cancellation via `AtomicBool` checked per-entry
**Why**: Network folders iterate slowly (seconds per entry). Checking on each iteration ensures responsive cancellation. ESC → cancel within ~100ms.

**Decision**: Three-stage progress: opening → progress → read-complete → complete
**Why**: Gives user fine-grained feedback:
- `listing-opening`: "About to start slow I/O" (for network folders)
- `listing-progress`: "Loaded N files..." (every 200ms, via `list_directory_core_with_progress`)
- `listing-read-complete`: "All files read, sorting now"
- `listing-complete`: "Ready to render"

**Decision**: Sorting happens AFTER read, BEFORE caching
**Why**: Frontend expects sorted order. Sorting 50k entries takes ~15ms (fast enough). Done in background task after all entries collected.

**Decision**: Enrichment at cache-write time, not on `get_file_range`
**Why**: All paths that store entries in `LISTING_CACHE` (streaming, watcher update, re-sort) enrich before storing. Index freshness is handled event-driven: `index-dir-updated` → `refreshIndexSizes` → `refresh_listing_index_sizes` (dedicated IPC command that write-locks the cache and re-enriches entries). This keeps `get_listing_stats` as a read-only operation while ensuring it sees up-to-date `recursive_size` values. The frontend calls `refreshListingIndexSizes` before `fetchListingStats` so the cache is fresh when stats are computed.

**Decision**: Hidden files filtering in Rust, not frontend
**Why**: Cannot know visible count until all files read. APIs accept `include_hidden: bool`, filter during `get_file_range()` iteration.

**Decision**: Font metrics in Rust binary cache, not frontend canvas measurement
**Why**: Measuring 50k filenames in JS is slow. Rust precomputes metrics for system fonts, stores in `.bin` cache. `calculate_max_width()` is a hash lookup.

**Decision**: Sequence counter lives on `CachedListing`, not on `WatchedDirectory`
**Why**: SMB and MTP volumes don't use FSEvents (`supports_watching() == false`), so they never get a `WatchedDirectory` entry. With the sequence on the watcher, `increment_sequence` returned `None` and `directory-diff` events were never emitted for those volumes. Moving the `AtomicU64` to `CachedListing` makes it work for all volume types. The FSEvents watcher path also uses this same counter now.

**Decision**: `ListingEventSink` trait decouples streaming from Tauri (same pattern as `OperationEventSink` in write_operations)
**Why**: `read_directory_with_progress` needs to emit events, but `tauri::AppHandle` can't be created in tests. The trait allows `CollectorListingEventSink` to capture events for assertions. `Arc<dyn ListingEventSink>` is used (not `&dyn`) because the sink is cloned into `tokio::spawn` for progress callbacks.

**Decision**: File watcher starts AFTER listing complete
**Why**: Watcher diffs rely on cached entries. Starting before cache is populated would miss initial state.

**Decision**: Incremental watcher path with fallback to full re-read
**Why**: Most FS changes are a few files (save, rename, drop). Re-reading an entire 50k-entry directory for one changed file is wasteful. The incremental path processes individual events: stat each changed path, classify as add/remove/modify against the cache, then use `insert_entry_sorted`/`remove_entry_by_path`/`update_entry_sorted` to patch the cache in-place. Falls back to full `handle_directory_change` when events exceed 500 or contain unknown event kinds (`Any`/`Other`), since those can't be reliably classified.

**Decision**: Synthetic diff for entry creation (`emit_synthetic_entry_diff`)
**Why**: `create_directory` and `create_file` return before the watcher fires. Without a synthetic diff, the new entry wouldn't appear until the next debounce cycle (~200ms). The command handler stats the new entry, inserts it into all affected listings via `insert_entry_sorted`, and emits a `directory-diff` event immediately. The watcher later sees the same change but `has_entry` prevents duplicates.

**Decision**: Re-sort `new_entries` before `compute_diff` in full re-read path
**Why**: `list_directory_core` always returns entries sorted by Name/Asc, but the cached listing may use a different sort. Without re-sorting, diff indices would be wrong (comparing two differently-ordered lists). The re-sort aligns `new_entries` with the cached sort order so `compute_diff` produces correct indices.

**Decision**: File metadata tiers — Tier 1-2 eagerly (stat + uid→name), Tier 3-4 deferred.
**Why**: With 50k+ files, each metadata piece has different performance cost. Tier 1 (name, size, dates, permissions) is free from a single `stat()`. Tier 2 (owner name, symlink target) is ~1μs and cacheable. Tier 3 (macOS Spotlight/NSURL metadata) costs ~50-100μs/file. Tier 4 (EXIF, PDF) costs 1-100ms+ and reads file content. See [full tier table](../../../../../../docs/notes/file-metadata-tiers.md).

## Gotchas

**Gotcha**: Watcher callbacks (FSEvents) run on OS threads, not the tokio runtime
**Why**: The FSEvents debouncer callback is called from an OS thread. Functions like `handle_directory_change` and
`notify_directory_changed(FullRefresh)` are async, so callers in watcher callbacks use `tokio::spawn` to dispatch
the async work onto the runtime. The incremental watcher path (`handle_directory_change_incremental`) remains sync
since it only does cache lookups and `stat` calls via `get_single_entry`.

### Cache helpers (caching.rs)

Used by the watcher's incremental path and synthetic mkdir to patch listings without full re-reads:
- `find_listings_for_path(path)` — returns all listing IDs whose directory matches the given path (multiple panes/tabs may show the same directory)
- `find_listings_for_path_on_volume(volume_id, path)` — same, but also filters by volume ID. Prevents false matches when two volumes serve overlapping paths.
- `insert_entry_sorted(listing_id, entry)` — inserts an entry in sorted position, returns the insertion index
- `remove_entry_by_path(listing_id, path)` — removes an entry by its file path, returns the removed index and entry
- `update_entry_sorted(listing_id, entry)` — updates an existing entry (remove + re-insert if sort position changed), returns `ModifyResult`
- `has_entry(listing_id, path)` — checks if a path exists in the cached listing (used to classify watcher events as add vs modify)
- `get_listing_path(listing_id)` — returns the directory path for a listing (used to filter watcher events to direct children)

### Change notification API (caching.rs)

`notify_directory_changed(volume_id, parent_path, change)` — unified entry point for notifying the listing system that a directory changed on a volume. Accepts a `DirectoryChange` enum:
- `Added(FileEntry)` — single entry added, patches cache via `insert_entry_sorted`
- `Removed(String)` — single entry removed by name, patches cache via `remove_entry_by_path`
- `Modified(FileEntry)` — single entry modified, patches cache via `update_entry_sorted`
- `Renamed { old_name, new_entry }` — same-dir rename, remove old + insert new
- `FullRefresh` — re-reads directory via Volume trait, computes diff against cache

All variants enrich entries with index data and emit `directory-diff` events. Natural deduplication: `insert_entry_sorted` returns `None` for duplicates, `remove_entry_by_path` returns `None` if already removed.

**Callers**: `Volume::notify_mutation()` (called after each successful create/delete/rename on all volume types) and the `rename_file` command (for local filesystem renames). The old `emit_synthetic_entry_diff` remains as a legacy fallback for `create_file`/`create_directory` on volumes where `supports_local_fs_access()` returns `true`.

**Gotcha**: Background task runs to completion even if cancelled on frontend
**Why**: `loadGeneration` discards stale results, but Rust keeps iterating. Mitigation: `AtomicBool` checked per-entry stops early.

**Gotcha**: `get_file_range()` with `include_hidden=false` skips hidden entries
**Why**: Indices are for VISIBLE items only. If item 5 is hidden, index 5 in `include_hidden=false` mode is actually item 6 in the full list. Backend handles filtering, frontend sees dense array.

**Gotcha**: Watcher diffs must update both cache AND emit events
**Why**: Cache is source of truth for `get_file_range()`. Events notify frontend to re-fetch visible range. Missing either = stale data or no UI update.

**Gotcha**: Sorting changes invalidate cached range on frontend
**Why**: Frontend cache holds entries in old sort order. Backend re-sorts, but frontend must re-fetch. `cacheGeneration` bump triggers this.

**Gotcha**: macOS extended metadata (addedAt, openedAt) requires extra syscalls
**Why**: `list_directory_core()` uses fast `fs::read_dir()` + `metadata()`. Extended metadata needs `listxattr()`/`getxattr()`. Available via `get_extended_metadata_batch()` but not wired into streaming path yet.

**Gotcha**: `CANCELLATION_POLL_INTERVAL` is 100ms, but check happens per-entry
**Why**: Named confusingly. The interval is for waiting on channels, not polling the flag. Actual cancellation is checked on EVERY entry iteration.

**Gotcha**: Double-sort in the full re-read watcher path is intentional
**Why**: `list_directory_core` returns entries in Name/Asc order. The watcher's `handle_directory_change` re-sorts them to match the listing's current sort params before calling `compute_diff`. This looks redundant but is required — without it, diff indices would be computed against a differently-ordered list, producing incorrect add/remove positions.
