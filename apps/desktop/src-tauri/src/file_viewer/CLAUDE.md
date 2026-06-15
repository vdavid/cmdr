# File viewer module (Rust backend)

Three backend strategies for serving file content line-by-line: instant open, virtual scrolling, background search.

Frontend counterparts: [route shell](../../../src/routes/viewer/CLAUDE.md) and
[FE primitives](../../../src/lib/file-viewer/CLAUDE.md) (open-viewer helper, binary-warning classifier).

## Module map

- `mod.rs`: public API, constants (1MB threshold, 256-line checkpoints, 8KB backward scan), `ViewerError`.
- `session.rs`: orchestration, backend switching, per-read cancel registry, encoding-switch, drain-and-swap.
- `range_read.rs` stitches a `(line, offset)` range into one UTF-8 string. `encoding.rs`: `FileEncoding` + detection +
  `NewlineScanner`. `full_load.rs` / `byte_seek.rs` / `line_index.rs`: the three backends. `search_matcher.rs`:
  `Matcher`, huge-line chunking. `watcher.rs`: shared tail-mode watcher singleton.
- Backend selection: `< 1MB` → `FullLoad`; else `ByteSeek` (instant open) with a background `LineIndex` build that
  upgrades when ready.
- Media (Image/PDF): `content_kind.rs` (`classify_viewer_content`), `media.rs` (`cmdr-media://` token map),
  `media_protocol.rs` (async scheme handler), `media_backend.rs` (no-op `MediaBackend`), `media_session.rs` (the
  media-open path, built on `session.rs`'s `ViewerSession`). See [DETAILS.md](DETAILS.md) § "Media rendering".

## Must-knows

- **`viewer_set_encoding`, `viewer_set_tail_mode`, and `viewer_reload` are `async` + `spawn_blocking` + 2 s timeout**
  (via `blocking_viewer_op`), not synchronous: a sync call would freeze the viewer window's IPC thread behind concurrent
  scroll/search. Don't revert to plain `fn`. The watcher manager thread calls
  `reload` / `apply_tail_extend` directly (already off the IPC thread), so those stay synchronous.
- **The FSEvents subscribe runs on the manager thread, NOT inline in `open_session`.** It's blocking and
  `fseventsd`-bound (~100 ms idle, seconds under load), so inline it risks the 2 s `viewer_open` timeout. The
  open→subscribe append window is closed by `catch_up_after_subscribe`. Tests injecting synthetic watcher events must
  call `wait_for_watcher_subscribed()` first. See [DETAILS.md](DETAILS.md) § "Gotchas (tail mode)".
- **Drain-and-swap-under-lock protocol** for both the ByteSeek→LineIndex upgrade and the encoding rebuild: a `Grew`
  event arriving mid-rebuild would be silently dropped, so it queues into `session.pending_grew` under one lock the
  watcher writers also hold. The tail-extend race re-checks the backend `Arc` with `Arc::ptr_eq` after the extend,
  discarding a stale one. See [DETAILS.md](DETAILS.md) § "Decisions".
- **`ViewerSession.backend` is `Arc<ArcSwap<Box<dyn FileViewerBackend>>>`** (not `Arc<dyn>` or `RwLock`): background
  rebuilds replace the backend without blocking the `get_lines` read path. Each backend is immutable.
- **`SESSIONS` is freed on BOTH close paths.** The titlebar-X path never fires `viewer_close`; it's covered by a
  `WindowEvent::Destroyed` branch in `lib.rs::on_window_event` for `viewer-*` labels (via `WINDOW_TO_SESSION`). Without
  it, titlebar-closed viewers leak their session until quit. The `cmdr-media://` token is dropped at this same choke
  point (`media::drop_token`); don't drop it elsewhere, or a closed viewer leaks a live token mapping a path. The scheme
  handler serves `Content-Type` from stored magic bytes (never the extension), does its OWN `spawn_blocking` + timeout
  (504, not `blocking_with_timeout`), and 404s an unknown token. See [DETAILS.md](DETAILS.md) § "Media rendering".
- **`search_cancel` must not null `session.search`**: the cancel flag is where the search thread writes `Cancelled`;
  nulling first lands the write in a dropped state and `search_poll` returns `Idle`. See [DETAILS.md](DETAILS.md).
- **`SearchMatch.column` / `.length` are UTF-16 code units** (match JS `String.substring()`), avoiding highlight
  off-by-ones. **Reject cross-line regex** (`(?s)`, literal `\n`, `\n` escape) at build time; `(?m)` is fine.
- **ISO-8859-1 uses a manual 1:1 byte→codepoint table, NOT `encoding_rs::WINDOWS_1252`** (they disagree on `0x80-0x9F`).
  UTF-16 detection runs the parity heuristic BEFORE the UTF-8 fast path (ASCII-as-UTF-16 is valid UTF-8).
- **CRLF: line readers keep `\r` in the line string** (all three backends split only on `\n`). `range_read`'s byte
  arithmetic depends on this; stripping `\r` later needs the same change there. See [DETAILS.md](DETAILS.md) § "Gotchas".
- **Cancellation is per-read / per-search, never session-wide**: `read_range` and `search` check the cancel flag inside
  the per-line loop (not just between chunks), so concurrent reads don't race a shared flag.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
