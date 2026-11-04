# File viewer host (Rust backend)

The app's host for the viewer, whose core is `crates/cmdr-viewer` (read its `CLAUDE.md` first). Frontend: the
[route shell](../../../src/routes/viewer/CLAUDE.md) and [FE primitives](../../../src/lib/file-viewer/CLAUDE.md).

## Module map

- `app_host.rs`: the app's one `Viewer` (`app_viewer()`) and its answers to the core's seams. `events.rs`: the Tauri
  events for the core's facts. `mod.rs` re-exports the core.
- `media_protocol.rs`: the `cmdr-media://` scheme, over the core's `media_serve.rs`. `materialize.rs`: pulling a file
  the OS can't open into a bounded temp, reused by "Open with" (`open_with_extract.rs`). `markdown_image_fetch.rs`: a
  document's pictures. `analytics.rs`: `viewer_opened`; ❌ never a file name or extension.

## Must-knows

- **Every viewer call goes through `app_viewer()`**; ❌ never a second `Viewer` in the app: the scheme handler and the
  thumbnails resolve tokens from its table.
- **`viewer_set_encoding`, `viewer_set_tail_mode`, and `viewer_reload` are `async` + `spawn_blocking` + 2 s timeout**
  (`blocking_viewer_op`): a sync call freezes the viewer window's IPC thread. ❌ Don't revert to plain `fn`.
- **A session is freed on BOTH close paths**: the titlebar X never fires `viewer_close`, so
  `app_lifecycle::on_window_event`'s `Destroyed` branch calls `close_session_for_window` for `viewer-*`.
- **The event structs keep their names and doc text** (`ViewerMediaLoad`, `ViewerPullProgress`): the event name and
  `bindings.ts` derive from them.
- **Pulls stop between chunks, ❌ never by dropping an in-flight `next_chunk`** (that wedges MTP). Viewer pulls use a
  45 s stall rule; `inspect_file` uses its five-second path budget. `DETAILS.md` § "Watching a pull".
- **Never `std::fs`-open a path the OS can't open**: `materialize_for_viewer` is what the core's `pulls` seam reaches.
  ❌ Never an archive-only check, nor `supports_local_fs_access`.

The host's flows and decisions: `DETAILS.md`. Read it before any non-trivial work here.
