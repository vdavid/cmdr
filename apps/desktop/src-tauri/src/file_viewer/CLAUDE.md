# File viewer module (Rust backend)

Backends serve instant open, virtual scrolling, and background search.

Frontend counterparts: [route shell](../../../src/routes/viewer/CLAUDE.md) and
[FE primitives](../../../src/lib/file-viewer/CLAUDE.md).

## Module map

- `mod.rs`: public API, constants, `ViewerError`. `session.rs`: text sessions (backend switching, cancellation,
  encoding, drain-and-swap); `session_store.rs`: `ViewerSession` + `SESSIONS` (both open paths).
- `rows.rs` (the row rule), `row_walk.rs` (that rule read forward: `RowReader`,
  `collect_rows`, `search_rows`, and the row types), `range_read.rs` (range → UTF-8),
  `encoding.rs` (`FileEncoding` + detection), `full_load.rs` / `byte_seek.rs` / `line_index.rs` (the three backends),
  `search_matcher.rs`, `row_text.rs` + `log_view.rs` (the debug log's marker-free rows), `watcher.rs` (tail mode), `source.rs` +
  `volume_source.rs` (SMB) under `origin.rs`: one held handle per session, ❌ never `File::open` per fetch (#303,
  `DETAILS.md` § "Byte sources").
- Media (Image/PDF): `content_kind.rs` plus `media*.rs` (`media.rs`: the `cmdr-media://` token map).
  `DETAILS.md` § "Media rendering". Markdown: `markdown_*.rs`, where ammonia's allowlist is the XSS boundary
  (§ "Markdown rendering").
- `materialize.rs` + `pending_open.rs`: pulling a file the OS can't open (routed, or on a phone) into a bounded temp,
  with progress and cancel, reused by "Open with" (`open_with_extract.rs`). `DETAILS.md` § "Preview of a routed file".
- `headless.rs`: backend picks with no session, for `inspect_file`. `DETAILS.md` § "Headless reads".
- `analytics.rs`: `viewer_opened`; ❌ never a file name or extension.

## Must-knows

- **The text viewer serves ROWS, not physical lines** (`rows.rs`). Raw binary/hex reads use byte offsets. ❗ `continues: true` means CMDR ended that row, so
  ❌ never join it to the next with `\n`; walk chunk to chunk by `ChunkEnd` + `end_byte_offset`, ❌ never by row count
  or decoded lengths. `DETAILS.md` § "Rows, not lines".
- **`viewer_set_encoding`, `viewer_set_tail_mode`, and `viewer_reload` are `async` + `spawn_blocking` + 2 s timeout**
  (`blocking_viewer_op`): a sync call freezes the viewer window's IPC thread. ❌ Don't revert to plain `fn`.
- **The FSEvents subscribe runs on the manager thread, NOT inline in `open_session`** (it's `fseventsd`-bound). A test
  injecting watcher events calls `wait_for_watcher_subscribed()` first. `DETAILS.md` § "Gotchas (tail mode)".
- **Drain-and-swap-under-lock** for the ByteSeek→LineIndex upgrade and the encoding rebuild: a mid-rebuild `Grew`
  queues into `session.pending_grew`; the tail extend discards a stale backend via `Arc::ptr_eq`.
- **`SESSIONS` is freed on BOTH close paths**: the titlebar X never fires `viewer_close`, so
  `app_lifecycle::on_window_event`'s `Destroyed` branch covers `viewer-*`. The `cmdr-media://` token drops at that same
  choke point, ❌ nowhere else.
- **`search_cancel` must not null `session.search`**: the search thread writes `Cancelled` there, and nulling it first
  makes `search_poll` return `Idle`.
- **ISO-8859-1 uses a manual 1:1 table, NOT `encoding_rs::WINDOWS_1252`** (they disagree on `0x80-0x9F`), and UTF-16
  detection runs its parity heuristic BEFORE the UTF-8 fast path.
- **`SearchMatch.column` / `.length` are UTF-16 code units** within the ROW, so a column is bounded. **Reject
  cross-line regex** (`(?s)`, literal `\n`, `\n` escape) at build time; `(?m)` is fine.
- **CRLF: a row keeps its `\r`**.
- **Cancellation is per-read / per-search, never session-wide**, and checked inside the per-row loop.
- **`write_range_to_file` streams; `read_range` buffers.** The save is the 100 MiB clipboard refusal's way out: 1 MiB
  chunks under a stall watch, ❌ never `read_range` + `fs::write` or a total deadline. Lossy selections save source
  bytes: `DETAILS.md`.
- **Never `std::fs`-open a path the OS can't open**: a ROUTED one (`/…/foo.zip/inner`) or one whose volume's
  `paths_are_os_visible()` is false (`adb://…`); `open_session` sends both through `materialize_for_viewer`, after
  `volume_source` (SMB: smb2, no temp). ❌ Never an archive-only check, nor `supports_local_fs_access`.
- **Pulls stop between chunks, ❌ never by dropping an in-flight `next_chunk`** (that wedges MTP). Viewer pulls use a
  45 s stall rule; `inspect_file` uses its five-second path budget. `DETAILS.md` § "Watching a pull".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
