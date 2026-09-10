# File viewer module (Rust backend)

Three backend strategies for serving file content line-by-line: instant open, virtual scrolling, background search.

Frontend counterparts: [route shell](../../../src/routes/viewer/CLAUDE.md) and
[FE primitives](../../../src/lib/file-viewer/CLAUDE.md).

## Module map

- `mod.rs`: public API, constants, `ViewerError`.
- `session.rs`: orchestration, backend switching, per-read cancel registry, encoding-switch, drain-and-swap.
- `range_read.rs` (range → one UTF-8 string), `encoding.rs` (`FileEncoding` + detection), `full_load.rs` /
  `byte_seek.rs` / `line_index.rs` (the three backends), `search_matcher.rs`, `watcher.rs` (shared tail-mode watcher).
- Backend selection: `< 1MB` → `FullLoad`; else `ByteSeek` (instant open) + a background `LineIndex` upgrade.
- Media (Image/PDF): `content_kind.rs`, `media.rs` (`cmdr-media://` token map), `media_protocol.rs` (scheme handler),
  `media_backend.rs`, `media_session.rs`. See `DETAILS.md` § "Media rendering".
- `materialize.rs`: pulls a file the OS can't open (routed, or on a phone) into a bounded temp; the agent's
  `inspect_file` calls its routes-only half. `pending_open.rs`: an open's pull progress, cancel, and session handoff.
  `DETAILS.md` § "Preview of a routed file" and § "Watching a pull".
- `headless.rs`: `open_text_backend` and `open_scan_backend`, the backend picks with no session around them, for the
  agent's `inspect_file`; `content_kind::looks_binary` is its text-vs-binary call. `DETAILS.md` § "Headless reads".
- `analytics.rs`: `viewer_opened`; ❌ never a file name or extension.

## Must-knows

- **`viewer_set_encoding`, `viewer_set_tail_mode`, and `viewer_reload` are `async` + `spawn_blocking` + 2 s timeout**
  (`blocking_viewer_op`): a sync call freezes the viewer window's IPC thread behind scroll and search. ❌ Don't revert
  to plain `fn`.
- **The FSEvents subscribe runs on the manager thread, NOT inline in `open_session`**: it's `fseventsd`-bound (seconds
  under load). `catch_up_after_subscribe` closes the append window; a test injecting watcher events calls
  `wait_for_watcher_subscribed()` first. `DETAILS.md` § "Gotchas (tail mode)".
- **Drain-and-swap-under-lock** for the ByteSeek→LineIndex upgrade and the encoding rebuild: a mid-rebuild `Grew` queues
  into `session.pending_grew` under the lock the watcher writers hold. The tail extend re-checks the backend with
  `Arc::ptr_eq`, discarding a stale one.
- **`ViewerSession.backend` is `Arc<ArcSwap<Box<dyn FileViewerBackend>>>`**: rebuilds swap it without blocking
  `get_lines`. Each backend is immutable.
- **`SESSIONS` is freed on BOTH close paths.** The titlebar X never fires `viewer_close`; the `WindowEvent::Destroyed`
  branch in `app_lifecycle::on_window_event` covers `viewer-*` labels. The `cmdr-media://` token drops at that same
  choke point (`media::drop_token`), ❌ nowhere else.
- **`search_cancel` must not null `session.search`**: the search thread writes `Cancelled` there, and nulling it first
  makes `search_poll` return `Idle`.
- **`SearchMatch.column` / `.length` are UTF-16 code units** (JS `String.substring()`). **Reject cross-line regex**
  (`(?s)`, literal `\n`, `\n` escape) at build time; `(?m)` is fine.
- **ISO-8859-1 uses a manual 1:1 table, NOT `encoding_rs::WINDOWS_1252`** (they disagree on `0x80-0x9F`). UTF-16
  detection runs its parity heuristic BEFORE the UTF-8 fast path (ASCII-as-UTF-16 is valid UTF-8).
- **CRLF: line readers keep `\r` in the line string**; `range_read`'s byte arithmetic depends on it.
- **Cancellation is per-read / per-search, never session-wide**, and checked inside the per-line loop.
- **Never `std::fs`-open a path the OS can't open**: a ROUTED one (`/…/foo.zip/inner`) or one whose volume's
  `paths_are_os_visible()` is false (`adb://…`). `open_session` sends both through `materialize_for_viewer`. ❌ Never an
  archive-only check, nor `supports_local_fs_access` (direct SMB says `false` yet opens fine).
- **A pull stops between chunks, ❌ never by dropping an in-flight `next_chunk`** (that wedges an MTP phone). Abandon and
  deliver share one lock, so a window closed mid-open never strands a session. No total deadline, only a 45 s stall
  rule. `DETAILS.md` § "Watching a pull".

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
