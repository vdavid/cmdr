# cmdr-multi-rename details

## Why its own crate

The browser demo (`crates/cmdr-listing-wasm`) should run the real sheet: the same masks, statuses, presets, previews,
and apply as the desktop, through the same code. A crate with no `tauri` in its tree is what lets the wasm build link
it. The pipeline, placeholder grammar, and TC semantics are documented in
`apps/desktop/src-tauri/src/multi_rename/DETAILS.md` for now.

## The host split

- **Here**: the pure engine (mask, transform, plan, error), the preset and history types with their list ops (`save_in`,
  `seed_in`, `history_entries`, …, and the `cmdr_recents::RecentItem` impls), the Results file's text and its private
  write and read (`names_file.rs`), and `MultiRename`: the sessions (`session.rs`), each session's media cache and
  loader (`media.rs`), and apply (`run.rs`).
- **The desktop host** (`apps/desktop/src-tauri/src/multi_rename/`): `app_host.rs` (the app's engine and its
  `MultiRenameHost`), `app_files.rs` (the `LAST_SPEC` and `FIELD_HISTORY` recents files, their `RecentEntry` impls, and
  `results_path` in the app's cache dir), and `preset_store.rs` (the presets file). It re-exports this crate
  (`pub use cmdr_multi_rename::*`), so app code names everything as `crate::multi_rename::…`. The IPC deadlines stay in
  `commands/multi_rename.rs`, around the engine's methods.
- **Decision/Why: `RecentEntry` is implemented in the host**: it names the app's file and its `state_recovery` key, and
  the orphan rule allows it because the trait is the app's.

## The engine and its host seam

- **`MultiRename` is a value, built once per host**: `MultiRename::builder(listings, runtime, host)`, then
  `.max_sessions(n)` (default `MAX_SESSIONS`, 4), then `.build()`. Cheap to clone; every clone is the same engine. Its
  sessions are a field, never a process static. **Why**: the demo's golden replay builds several engines per module, and
  a static would share sessions across them; the desktop's tests share one engine in parallel and raise the cap through
  the builder, which a `cfg!(test)` here could never see from the app's test build.
- **`MultiRenameHost` (`host.rs`) carries only what differs between hosts**:
  - `media_loader(volume_id, emit, runtime)`: the `Loader` that reads a session's files (the desktop's reads through the
    volume manager under `io_budget_for_volume`). A host with no reader answers `ReadError::NotSupported` per file,
    which `store` turns into `Unreadable` without counting toward `give_up`, so the read ends `done` at once.
  - `results_path(session_id)`: where Results writes; `None` makes `write_names` refuse `CouldntWriteNames`.
  - `results_supported()`: whether the host has Results at all, sent as `MultiRenameOpened.resultsAvailable` so the
    sheet disables it (the demo's `false`). **Decision/Why: a capability, never `results_path(..).is_some()`**: a
    desktop whose cache dir is missing keeps its `CouldntWriteNames` wording instead of reading as "needs the desktop
    app".
  - `rename_place(volume_id, dir, ready)`: ONE resolve answering `RenamePlace` (`ReadOnly`, `LocalDisk`, `Volume`,
    `NotConnected`). ❌ Don't split it into a read-only check plus a lookup: two resolves could disagree on
    `(volume_id, dir)`. `LocalDisk` fingerprints through `std::fs` on the runtime's blocking pool; `Volume` through the
    volume, under `io_budget`.
  - `io_budget(volume_id, base)` and `operations()` (an owned `Operations` handle; the desktop maps "not installed" to
    `CouldntStart`).
- **Decision/Why: `emit` is a `Loader` field the caller builds, never a host method.** The MCP tool reads through the
  same `SessionMedia` with a no-op emit and must send no `multi-rename-media-progress` for its `mcp-multi-rename`
  pseudo-session; a host-level emit would broadcast it. The payload is `MediaProgressEvent` (serde only); the desktop
  sends it as its own `MultiRenameMediaProgress` (the `tauri_specta::Event`), and `wire_test.rs` pins that the two
  serialize the same, since a browser host sends this payload as the event body.
- **The runtime is held, never ambient.** `SessionMedia::start(loader, runtime)` and `read_all(loader, runtime)` spawn
  the loader and its blocking reads (`rt::task::JoinSet::spawn_blocking_on`) on the runtime they're given; apply's
  blocking steps go through `deadline::blocking_typed_result_with_timeout_on`. The desktop hands in Tauri's runtime, the
  pool the IPC commands' blocking work always ran on; the MCP tool passes its own `Handle::current()`.
- **IPC-level deadlines stay in the hosts**: the desktop wraps each method in its `blocking_*` / `timeout_detached_*`
  budgets and the previews' `BlockingBudget`; a browser host calls the methods inline.

## The public surface

Capped by `index-crate-isolation` (`index-crate-isolation-ceilings.go`, measured exactly when the sessions joined): ten
public modules plus `MultiRename`, `MultiRenameHost`, and `RenamePlace` at the root; 18 methods on `MultiRename` (one
per thing the sheet asks a session, plus `builder` and the MCP tool's `media_over`); 110 module items. The modules stay
public because the wire types keep their paths for `bindings.ts` and the desktop names them by module. `SessionMedia`
exposes only what the MCP tool shares (`seed`, `adopt_where`, `read_all`, `progress`, `lookup`, `cancel`); everything
the engine alone calls is `pub(crate)`. `start_media_with` and `is_open` are behind the `testing` feature. A new `pub`
item is argued here first.

## The browser build

- The browser demo links it (`crates/cmdr-listing-wasm/src/multi_rename.rs`, its host: no media reader, no Results).
  `cmdr-media-meta` comes with its default features, readers included: they compile for wasm32, and fat LTO drops them
  since nothing there calls them (measured, and the `read` feature decided against, in
  `crates/cmdr-listing-wasm/DETAILS.md` § The build).
- In the browser, `spawn_blocking_on` runs the read inline and queues it as a finished task, so a whole media read runs
  in one task (no tokio coop budget there) and the throttle sends only the first and the final event.
- The loader's `catch_unwind` does nothing under the browser build's `panic = "abort"`; harmless while the demo's reader
  can't panic.
- `names_file.rs` uses `libc` and `std::os::unix` only under `cfg(unix)`; the browser never writes a Results file.
- The engine's own session-level cells over an in-memory volume are the demo's (`multi_rename_test.rs` there), and the
  Node smoke (`smoke-multi-rename.mjs`) is what proves the browser half: open, a media read to `done`, preview, apply,
  and undo on the real `.wasm`.
- `chrono::Local` reads the offset from JS `Date` there (chrono's default `wasmbind`), and uuid's `js` feature comes
  from `cmdr-fs`.

## Tests

The pure tests live beside their modules (`*_test.rs`), as do the session store's eviction tests (`session.rs`). The
session-level tests run over a real disk through the desktop's listing host and engine (`app_multi_rename`), so they
stay desktop host tests (`apps/desktop/src-tauri/src/multi_rename/`), along with the wire-shape pins (`wire_test.rs`).
