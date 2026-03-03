# File viewer module (Rust backend)

Provides three backend strategies for serving file content line-by-line with instant open, virtual scrolling, and background search.

## Key files

- `mod.rs` — public API, constants (1MB threshold, 256-line checkpoints, 8KB backward scan limit)
- `session.rs` — session orchestration, backend switching, search state
- `full_load.rs` — loads entire file into `String` (<1MB files)
- `byte_seek.rs` — seeks by byte offset, scans backward for newline (instant open)
- `line_index.rs` — sparse newline index (1 checkpoint per 256 lines), SIMD-accelerated via `memchr`

## Backend selection logic

```rust
if file_size < 1MB {
    FullLoadBackend
} else {
    // Start with ByteSeek (instant)
    ByteSeekBackend
    // Spawn background thread to build LineIndex
    // Upgrade to LineIndexBackend when ready
}
```

## Tauri commands

- `viewer_open(path)` → `ViewerOpenResult` (session ID, metadata, initial lines, backend type)
- `viewer_get_lines(session_id, target_type, target_value, count)` → `LineChunk`
- `viewer_search_start(session_id, query)` → starts background search
- `viewer_search_poll(session_id)` → `SearchPollResult` (matches, progress, status)
- `viewer_search_cancel(session_id)` → cancels running search
- `viewer_close(session_id)` → frees resources
- `viewer_setup_menu(label)` — builds viewer menu with word wrap item
- `viewer_set_word_wrap(label, checked)` — syncs menu state

## Key decisions

**Decision**: Three-backend architecture (FullLoad / ByteSeek / LineIndex) instead of one general-purpose backend.
**Why**: The core constraint is that opening a file must feel instant regardless of size. FullLoad is simplest and gives perfect random access, but loading a 1 GB file into memory is unacceptable. ByteSeek opens any file in O(1) time but can't seek by line number (only by byte offset or fraction). LineIndex gives O(1) line seeking but requires a full file scan first. The three-tier approach gives instant open (ByteSeek), then upgrades to precise line navigation (LineIndex) once the background scan finishes.

**Decision**: ByteSeek-to-LineIndex upgrade happens in a background thread with a 5-second timeout.
**Why**: On fast SSDs, indexing a 1 GB file takes ~2 seconds and the upgrade is seamless. But on slow disks or network drives, indexing could take minutes. The 5s timeout prevents the indexer from hammering a slow volume indefinitely. If it times out, the session stays in ByteSeek mode — the user can still scroll (via fraction seeking) and search, they just don't get exact line numbers.

**Decision**: Search always uses a fresh `ByteSeekBackend` instance in a separate thread, even when the session uses `LineIndex`.
**Why**: Search is a streaming full-file scan regardless of backend — the line index doesn't help find text matches. Using `ByteSeekBackend` for search keeps the search thread independent of the session's primary backend, avoiding lock contention. Opening a fresh file handle also means search doesn't interfere with the user scrolling in the main session.

**Decision**: `SearchMatch.column` and `.length` use UTF-16 code units instead of byte or char offsets.
**Why**: The frontend is JavaScript, where `String.prototype.length` and `String.prototype.substring()` count UTF-16 code units. If the backend returned byte offsets or Unicode scalar offsets, the frontend would need to convert on every match highlight, which is error-prone for text with emoji or CJK characters. Matching the JS string model eliminates an entire class of off-by-one bugs in the highlight rendering.

**Decision**: Sparse checkpoints every 256 lines instead of indexing every line.
**Why**: Indexing every line in a 100M-line file would need ~800 MB of offset data (8 bytes each). At 256-line intervals, the same file needs ~3 MB. The trade-off is that seeking to a specific line requires reading forward up to 255 lines from the nearest checkpoint, which takes <1ms on any modern disk — well within the 16ms frame budget for 60fps scrolling.

**Decision**: Session map (`VIEWER_SESSIONS`) is a global `LazyLock<Mutex<HashMap>>` rather than Tauri managed state.
**Why**: Same reasoning as the AI manager — viewer sessions need to be accessed from background threads (search, indexing) that don't have an `AppHandle`. A global makes the session cache accessible from any context without threading an `AppHandle` through every call chain.

## Gotchas

- **VIEWER_SESSIONS is unbounded** — grows with each `viewer_open`. Must call `viewer_close` when window closes (not automatic).
- **LineIndex build is async** — `ViewerSession` upgrades backend when ready. Frontend sees backend type change via status query.
- **Search state per session** — only one search can run per session. Starting a new search cancels the previous one.
- **UTF-16 offsets for JS compatibility** — `SearchMatch.column` and `.length` are in UTF-16 code units, matching JS `String.substring()`.
- **ByteSeek backward scan limit** — 8KB max. If newline not found, line starts at scan boundary (truncated).
- **LineIndex memory** — O(total_lines / 256) for checkpoints. For a 100M line file: ~390K checkpoints × 8 bytes = ~3MB.

## Performance targets

- **Open latency**: <10ms for any file size (ByteSeek), <50ms for 1GB file after LineIndex builds
- **Scroll latency**: <16ms (60fps) for 50-line fetch
- **Search**: ~500MB/s (SIMD-accelerated), progress updates every 10MB
