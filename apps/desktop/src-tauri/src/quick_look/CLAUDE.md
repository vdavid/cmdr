# Quick Look

Native macOS Quick Look (`QLPreviewPanel`) integration. Shift+Space opens a real AppKit preview panel over Cmdr; arrow
keys keep navigating the file list while the panel tracks the cursor.

## Module map

- **`mod.rs`**: module root. `QuickLookState = Mutex<QuickLookController>` (`Mutex<()>` on non-macOS), `init_state()`,
  and the `QuickLookKeyEvent` serde payload.
- **`controller.rs`**: macOS-only. `QuickLookController` (bookkeeping), `QuickLookDelegate` (data source + delegate +
  close observer), `define_class!` glue, key-event translation, and state-machine unit tests.

The three Tauri commands (`quick_look_open`, `quick_look_set_path`, `quick_look_close`) live in `commands/ui.rs`, not
here, to keep this module thin. Two events flow out: `quick-look-key` (keyboard events the panel didn't want, payload
mirrors a DOM `KeyboardEvent`; the FE re-routes through the focused pane) and `quick-look-closed` (fires whenever the
panel leaves the screen: our `orderOut:`, the ✕ button, or Esc; the FE flips `isOpen = false`).

Full details (decisions, NSOpenPanel coexistence, the testing gap, multi-selection extension, deps): [DETAILS.md](DETAILS.md).

## Must-knows

- **The panel is process-wide (`sharedPreviewPanel`), behind a singleton `Mutex<QuickLookController>`.** Don't "new one
  each time"; opening installs ourselves as data source + delegate and orders the shared panel front.
- **Gate on `Volume::supports_local_fs_access()`, NOT `Path::exists()`.** MTP virtual paths return `false` from
  `exists()` even when the file is real on the device, and `QLPreviewPanel` needs an `NSURL` to a local file. Non-FS
  volumes no-op (debug log). The volume kind is the correct signal; Finder doesn't preview MTP either.
- **All three commands hop to the AppKit main thread** via `app.run_on_main_thread()` + a one-shot `mpsc`, wrapped in
  `blocking_with_timeout` (2 s) so a wedged AppKit pump can't freeze the IPC pool. Keep new entry points on this
  pattern.
- **The close observer is the single source of truth for `is_open`; don't add a parallel flip.** `panel.orderOut(nil)`
  posts `NSWindowWillCloseNotification` asynchronously (empirically `QLPreviewPanel` posts it on `orderOut:` too, ~200 ms
  after the close IPC returns; verified via `apps/desktop/test/manual/quick-look-mcp.md`). A synchronous flip in
  `close_on_main` would race the observer's late flip and break a quick reopen.
- **`#[unsafe(method_id(...))]` bodies need a single tail expression**: no early `return`, no `?` (both produce
  intermediate `Option`s the macro can't coerce). Compute the value once and let the macro wrap it. See
  `previewItemAtIndex`.
