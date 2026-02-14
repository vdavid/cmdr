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
