# cmdr-viewer

The file viewer's core, Tauri-free: sessions, rows, the three backends, search, encodings, Markdown, and the media token
model. A host builds one `Viewer` over its `host::ViewerHost`: the desktop app
(`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`) and the browser demo. Frontend: the route shell
(`apps/desktop/src/routes/viewer/CLAUDE.md`).

## Module map

- `viewer.rs`: `Viewer`, the value that owns it all. `session.rs` (text sessions) + `session_store.rs`. `lib.rs`: the
  API, `ViewerError`.
- `rows.rs` (the row rule), `row_walk.rs` (read forward), `range_read.rs`, `encoding.rs`, the three backends
  (`full_load.rs` / `byte_seek.rs` / `line_index.rs`), `search_matcher.rs`, `row_text.rs` + `log_view.rs`, `watcher.rs`
  (tail mode, native only), `source.rs` + `volume_source.rs` under `origin.rs`: one held handle per session, ❌ never
  `File::open` per fetch (#303, `DETAILS.md` § "Byte sources").
- Media: `content_kind.rs`, `media*.rs`. Markdown: `markdown_*.rs`, where ammonia's allowlist is the XSS boundary.
- Seams (`DETAILS.md` § "Host seams"): `host/`, `worker.rs` (every job), `bridge.rs` (every async call),
  `media_serve.rs`. `testing.rs`: the shared test viewer.

## Must-knows

- **Rows, not physical lines** (`rows.rs`); raw binary/hex reads use byte offsets. ❗ `continues: true` means CMDR ended
  that row, so ❌ never join it to the next with `\n`; walk by `ChunkEnd` + `end_byte_offset`, ❌ never row counts or
  decoded lengths. `DETAILS.md` § "Rows, not lines".
- **No app reach, no ambient runtime**: an app concern arrives through `host/`; a job starts in `worker.rs`, a future
  runs on the `Viewer`'s runtime via `bridge.rs`, a clock read goes through `cmdr_fs::rt`. `clippy.toml` bans the rest.
- **It builds for `wasm32`**, where jobs run inline, the bridge polls once, and nothing falls back to the OS; a `cfg`
  half must keep native behavior byte-identical. `DETAILS.md` § "The browser build".
- **The FSEvents subscribe runs on the manager thread, NOT inline in `open_session`**. A test injecting watcher events
  calls `wait_for_watcher_subscribed()` first. `DETAILS.md` § "Gotchas (tail mode)".
- **Drain-and-swap-under-lock** for the ByteSeek→LineIndex upgrade and the encoding rebuild: a mid-rebuild `Grew` queues
  into `session.pending_grew`; the tail extend discards a stale backend via `Arc::ptr_eq`.
- **`close_session` is the one choke point** freeing a session, its media tokens, and its temp; a host covers both close
  paths (the app: `viewer_close` and the window's `Destroyed`).
- **`search_cancel` must not null `session.search`**: the search thread writes `Cancelled` there.
- **ISO-8859-1 uses a manual 1:1 table, NOT `encoding_rs::WINDOWS_1252`**, and UTF-16 detection runs its parity
  heuristic BEFORE the UTF-8 fast path.
- **`SearchMatch.column` / `.length` are UTF-16 code units** within the ROW. **Reject cross-line regex** (`(?s)`,
  literal `\n`, `\n` escape) at build time; `(?m)` is fine. **CRLF: a row keeps its `\r`**.
- **Cancellation is per-read / per-search, never session-wide**, checked inside the per-row loop.
- **`write_range_to_file` streams; `read_range` buffers** (1 MiB chunks under a stall watch, ❌ never `read_range` +
  `fs::write`).
- **Never `std::fs`-open a path the OS can't open** (routed, or `paths_are_os_visible()` false): `open_session` asks the
  host's `pulls` after `volume_source`. **Media classifies through the session's `OpenedFile`**, ❌ never a backend
  check.
- **No type here shares a name with a host's event** (specta would emit both into `bindings.ts`).

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
