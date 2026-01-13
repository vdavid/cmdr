# File loading

How directory listings are loaded, from user action to rendered list.

## Overview

When a user navigates to a directory, the app uses **streaming directory loading**:

1. Frontend requests a directory listing (non-blocking)
2. Rust reads the directory in a background task
3. Progress events are emitted every 500ms
4. On completion, the frontend renders the file list

This architecture prevents UI freezing when opening large directories or slow network paths.

## Event flow diagram

```
Frontend                          Rust Backend
   |                                   |
   |-- listDirectoryStartStreaming --->|
   |<-- { listingId, status: loading } | (immediate return)
   |                                   |
   |                                   | (background task starts)
   |                                   |
   |<---- listing-progress event ------| (every 500ms)
   |     { listingId, loadedCount }    |
   |                                   |
   |<---- listing-read-complete event -| (when read_dir finishes)
   |     { listingId, totalCount }     |
   |                                   |
   |                                   | (sorting, caching, etc.)
   |                                   |
   |<---- listing-complete event ------|
   |     { listingId, totalCount,      |
   |       maxFilenameWidth }          |
   |                                   |
   |-- getFileRange(listingId, ...) -->| (on-demand fetching)
   |<-- [FileEntry, FileEntry, ...]    |
   |                                   |
```

### Error handling

```
Frontend                          Rust Backend
   |                                   |
   |<---- listing-error event ---------|
   |     { listingId, message }        |
   |                                   |
```

### Cancellation

```
Frontend                          Rust Backend
   |                                   |
   |-- cancelListing(listingId) ------>|
   |                                   |
   |<---- listing-cancelled event -----|
   |     { listingId }                 |
   |                                   |
```

## Data flow layers

### 1. Frontend: FilePane.svelte

The [FilePane](../../apps/desktop/src/lib/file-explorer/FilePane.svelte) component orchestrates directory loading.

**Key function:** `loadDirectory(path, selectName?)`

1. Cancels any in-progress listing
2. Shows loading state with progress indicator
3. Calls `listDirectoryStartStreaming()` (returns immediately)
4. Subscribes to streaming events
5. On `listing-complete`: stores listing ID, sets total count, updates history
6. On-demand fetching via `getFileRange()` for visible entries

**Cancellation handling:** Users can press ESC to cancel loading. The `handleCancelLoading()` function cancels the Rust
task and navigates back in history (or to home if history is empty).

### 2. IPC layer: tauri-commands.ts

The [tauri-commands](../../apps/desktop/src/lib/tauri-commands.ts) module provides typed wrappers for Rust commands.

**Streaming API functions:**

- `listDirectoryStartStreaming(path, includeHidden, sortBy, sortOrder)` → `StreamingListingStartResult`
- `cancelListing(listingId)` → void
- `getFileRange(listingId, start, count, includeHidden)` → `FileEntry[]`
- `listDirectoryEnd(listingId)` → void

### 3. Rust commands: commands/file_system.rs

The [file_system commands](../../apps/desktop/src-tauri/src/commands/file_system.rs) expose Tauri commands that call the
file system operations.

**Commands:**

- `list_directory_start_streaming` - Starts async background task, returns immediately with listing ID
- `cancel_listing` - Sets cancellation flag on in-progress listing
- `get_file_range` - Returns entries from cached listing
- `list_directory_end` - Cleans up the listing cache

### 4. File system operations: file_system/operations.rs

The [operations module](../../apps/desktop/src-tauri/src/file_system/operations.rs) contains the core logic.

**Key function:** `list_directory_start_streaming()`

1. Generates listing ID
2. Creates cancellation state in `STREAMING_STATE` cache
3. Spawns background task with `tokio::spawn`
4. Returns immediately with `{ listingId, status: "loading" }`

**Background task:** `read_directory_with_progress()`

1. Iterates directory entries with `fs::read_dir()`
2. Checks cancellation flag on each entry
3. Emits `listing-progress` event every 500ms
4. Emits `listing-read-complete` event when read_dir finishes
5. Sorts entries
6. Caches in `LISTING_CACHE`
7. Starts file watcher (if supported)
8. Emits `listing-complete` event

## Progress display

While loading, the `LoadingIcon` component shows:

- A spinning loader animation
- "Loaded N files..." with the current count (updated every 500ms)
- "All N files loaded, just a moment now." when read_dir finishes (before sorting/caching)
- "Press ESC to cancel and go back" hint

## Cancellation behavior

Users can cancel an in-progress directory load by pressing ESC. When cancelled:

1. The Rust task stops iterating directory entries
2. A `listing-cancelled` event is emitted
3. The frontend navigates back in history, or to home (`~`) if history is empty

## Latency characteristics

| Scenario           | Behavior                                                     |
| ------------------ | ------------------------------------------------------------ |
| Local directory    | Usually completes before first progress event                |
| 50k files          | Progress updates every 500ms, ~2-3s total                    |
| Slow network path  | Progress visible immediately, user can cancel anytime        |
| Navigation away    | Previous listing cancelled automatically                     |

## Key design decisions

1. **Streaming over chunking**: The previous chunk-based approach required multiple IPC calls. Streaming uses events
   which are more efficient and provide better UX for slow operations.

2. **Cancellation support**: Network paths can be very slow. Users can cancel anytime without waiting.

3. **Progress indication**: The 500ms progress events give users feedback that something is happening.

4. **Virtual scrolling**: Entries are cached on the backend. Frontend fetches only visible rows via `getFileRange()`.

5. **History timing**: Path is only added to history on successful completion, preventing broken history entries.
