# File system listing module

Backend directory reading, caching, sorting, and streaming for the file explorer. Handles 100k+ file directories with non-blocking I/O and progress events.

## Architecture

### Module structure

- **mod.rs** – Public API exports, re-exports for crate-internal use
- **reading.rs** – Low-level disk I/O (`list_directory_core()`, `get_single_entry()`, macOS metadata)
- **streaming.rs** – Async streaming with progress events, cancellation
- **operations.rs** – Synchronous frontend-facing API (lifecycle, cache accessors)
- **caching.rs** – `LISTING_CACHE` global state, `CachedListing` struct
- **sorting.rs** – `SortColumn`, `SortOrder`, `sort_entries()`
- **metadata.rs** – `FileEntry` struct, macOS extended metadata

### Data flow

```
Frontend                          Backend
   |                                   |
   |-- listDirectoryStartStreaming -->| (returns immediately)
   |<-- { listingId, status: loading }|
   |                                   |
   |                            [background task spawns]
   |                                   |
   |<--- listing-opening event --------| (just before read_dir)
   |<--- listing-progress event -------| (every 500ms)
   |     { listingId, loadedCount }    |
   |                                   |
   |<--- listing-read-complete event --| (when read_dir finishes)
   |     { listingId, totalCount }     |
   |                                   |
   |                            [sorting + caching + watcher start]
   |                                   |
   |<--- listing-complete event -------| (ready for use)
   |     { listingId, totalCount,      |
   |       maxFilenameWidth }          |
   |                                   |
   |-- getFileRange(listingId, ...) -->| (on-demand fetching)
   |<-- [FileEntry, FileEntry, ...]    |
```

### Caching strategy

**LISTING_CACHE**: Global `RwLock<HashMap<String, CachedListing>>`
**Key**: `listing_id` (UUID per navigation)
**Value**: `CachedListing { volume_id, path, entries, sort_by, sort_order }`

**Lifecycle**:
1. `list_directory_start_streaming()` generates ID, spawns task
2. Background task reads directory, sorts, stores in cache
3. Frontend calls `get_file_range()` for visible entries (on-demand)
4. `list_directory_end()` stops watcher, removes from cache

**Concurrency**: Multiple listings can coexist (different panes, rapid navigation). Each has unique ID.

## Key decisions

**Decision**: Streaming with background task, not chunked IPC
**Why**: Chunked approach required multiple IPC calls, complex state tracking. Streaming spawns `tokio::task::spawn_blocking()`, emits events. Frontend stays responsive—Tab works, ESC cancels.

**Decision**: Cancellation via `AtomicBool` checked per-entry
**Why**: Network folders iterate slowly (seconds per entry). Checking on each iteration ensures responsive cancellation. ESC → cancel within ~100ms.

**Decision**: Three-stage progress: opening → progress → read-complete → complete
**Why**: Gives user fine-grained feedback:
- `listing-opening`: "About to start slow I/O" (for network folders)
- `listing-progress`: "Loaded N files..." (every 500ms)
- `listing-read-complete`: "All files read, sorting now"
- `listing-complete`: "Ready to render"

**Decision**: Sorting happens AFTER read, BEFORE caching
**Why**: Frontend expects sorted order. Sorting 50k entries takes ~15ms (fast enough). Done in background task after all entries collected.

**Decision**: Hidden files filtering in Rust, not frontend
**Why**: Cannot know visible count until all files read. APIs accept `include_hidden: bool`, filter during `get_file_range()` iteration.

**Decision**: Font metrics in Rust binary cache, not frontend canvas measurement
**Why**: Measuring 50k filenames in JS is slow. Rust precomputes metrics for system fonts, stores in `.bin` cache. `calculate_max_width()` is a hash lookup.

**Decision**: File watcher starts AFTER listing complete
**Why**: Watcher diffs rely on cached entries. Starting before cache is populated would miss initial state.

## Gotchas

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
