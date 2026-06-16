# Downloads watcher

Recursive `notify` watch on `~/Downloads` that emits a `download-detected` Tauri event when an eligible final-form file
appears (browser-style: a direct create of a final-name file, or a rename from a partial-suffix file to a final-name
file). FDA-gated.

## Module map

- **`filter.rs`**: `is_eligible(path)`: not hidden, not a partial-download suffix, regular file or symlink-to-file
- **`ignore_set.rs`**: `IgnoreSet`, a `HashMap<PathBuf, Instant>` + FIFO `VecDeque`, 5 s TTL, 1000-entry cap; suppresses
  events for paths Cmdr just wrote
- **`latest_ring.rs`**: `LatestRing`, a capacity-10 ring of `(PathBuf, Instant)`; re-pushing moves to the back
- **`watcher.rs`**: `DownloadsWatcher`, the debouncer handle + ignore set + ring; pure `classify_event()` /
  `translate_debounced()`; `AppHandleSink` (prod) vs `ChannelSink` (tests)
- **`runtime.rs`**: `Mutex<Option<DownloadsWatcher>>`; `refresh_runtime(&app)` aligns the handle with the FDA gate;
  owns the `note_pending_write_for_cmdr` hook API
- **`commands.rs`**: IPC (`go_to_latest_download`, `downloads_watcher_status`, `recheck_downloads_watcher_gate`,
  `set_global_go_to_latest_shortcut`)
- **`global_shortcut.rs`**: typed wrapper over `tauri-plugin-global-shortcut`; `GlobalShortcutManager<R: Registrar>`
  state machine

Full lifecycle, scope rationale, and v1 limits: [DETAILS.md](DETAILS.md).

## Must-knows

- **FDA gating contract: the watcher is alive iff `fda_gate::is_fda_pending_runtime() == false`.** `lib.rs` calls
  `runtime::refresh_runtime(&app)` at startup (after `set_fda_pending`), on every main-window `Focused(true)` (the
  "toggled FDA in System Settings, came back" path), and the Settings pane mount calls `recheck_downloads_watcher_gate`
  (recovers from a stale focus-event read). The watcher holds no FDA-protected state, so the closed-gate side is a pure
  no-op.
- **Cmdr-own-write hook: call `crate::downloads::note_pending_write_for_cmdr(&dest_path)` immediately before each write
  syscall**. It silently no-ops for paths outside the watched
  Downloads root (the prefix check lives in `IgnoreSet::note_pending`), so call sites invoke unconditionally. Don't move
  the filter to the call sites. Key on the **final** path: a browser rename `foo.zip.crdownload` → `foo.zip` arrives as
  `RenameMode::Both` and the watcher checks both halves; Cmdr never writes `.crdownload`, so always register the final
  destination.
- **Don't `tokio::spawn` from the notify callback.** The `notify-debouncer-full` callback runs on notify-rs's internal
  thread with no Tokio runtime; `tokio::spawn` panics ("no reactor running"). All the work is synchronous and cheap, so
  it stays inline. If async is ever needed, use `tauri::async_runtime::spawn`.
- **Global go-to-latest hotkey default is `⌃⌥⌘J`.** `⌘` maps to `Super`, NOT `Meta`, in the accelerator string
  (`binding_to_accelerator` and the FE mirror `global-shortcut-binding.ts`). The `global-hotkey` crate accepts
  `COMMAND`/`CMD`/`SUPER` but rejects `META` (errors `UnsupportedKey`). Keep both adapters on `Super`. The plugin uses
  Carbon's `RegisterEventHotKey`, so no Accessibility / Input Monitoring TCC grant is needed.
- **The hotkey handler raises the main window** (`unminimize` + `show` + `set_focus`) before emitting
  `global-shortcut-fired`. The user fires it from another app, so without the raise the result stays hidden behind the
  active app. Don't drop the raise. `lib.rs` calls `refresh_global_go_to_latest_shortcut(app)` at the same three points
  as the watcher, plus the `set_global_go_to_latest_shortcut` IPC.
- **`GlobalShortcutManager` register/unregister is idempotent**: re-registering the same binding is a no-op, swapping
  unregisters the previous first, and a `Conflict` stays remembered until the next successful register so the Settings
  row can surface "in use by another app" without re-attempting.
- **No `println!` / `eprintln!` / `dbg!`** (clippy denies crate-wide). Use `log::debug!(target: "downloads::watcher", …)`
  so `RUST_LOG=cmdr_lib::downloads=debug` filters this subsystem. See `logging/CLAUDE.md`.
- **Tests run against a tempdir, not `~/Downloads`.** `DownloadsWatcher::start_at(path, sink)` is the test entry point;
  production uses `::start(&app)` (resolves `dirs::download_dir()`). Tests use `unhidden_tempdir()` so the hidden-component
  check in `is_eligible` doesn't shadow positive assertions on macOS.
