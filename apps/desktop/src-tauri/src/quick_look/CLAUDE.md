# Quick Look

Native macOS Quick Look (`QLPreviewPanel`) integration. Shift+Space opens a real AppKit preview panel over Cmdr; arrow
keys keep navigating the file list while the panel tracks the cursor.

## Module map

- **`mod.rs`**: module root. `QuickLookState = Mutex<QuickLookController>` (`Mutex<()>` on non-macOS), `init_state()`,
  and the `QuickLookKeyEvent` serde payload.
- **`controller.rs`**: macOS-only. `QuickLookController` (bookkeeping), `QuickLookDelegate` (data source + delegate +
  close observer), `define_class!` glue, key-event translation, and state-machine unit tests.

The three Tauri commands (`quick_look_open`, `quick_look_set_path`, `quick_look_close`) live in `commands/quick_look.rs`, not
here, to keep this module thin. Two events flow out: `quick-look-key` (keyboard events the panel didn't want, payload
mirrors a DOM `KeyboardEvent`; the FE re-routes through the focused pane) and `quick-look-closed` (fires whenever the
panel leaves the screen: our `orderOut:`, the ✕ button, or Esc; the FE flips `isOpen = false`).

Full details (decisions, NSOpenPanel coexistence, the testing gap, multi-selection extension, deps): `DETAILS.md`.

## Must-knows

- **The panel is process-wide (`sharedPreviewPanel`), behind a singleton `Mutex<QuickLookController>`.** Don't "new one
  each time"; opening installs ourselves as data source + delegate and orders the shared panel front.
- **Gate on `Volume::paths_are_os_visible()`, NOT `Path::exists()` and ❌ not `supports_local_fs_access()`.** MTP
  virtual paths return `false` from `exists()` even when the file is real on the device, and `QLPreviewPanel` needs an
  `NSURL` to a local file. Non-FS volumes no-op (debug log). The volume kind is the correct signal; Finder doesn't
  preview MTP either. Direct SMB reads over smb2 (`supports_local_fs_access() == false`) but stays OS-mounted, so its
  paths ARE previewable; keying on the other flag silently killed Quick Look on an SMB pane.
- **All three commands hop to the AppKit main thread** via `app.run_on_main_thread()` + a one-shot `mpsc`, wrapped in
  `blocking_with_timeout` (2 s) so a wedged AppKit pump can't freeze the IPC pool. Keep new entry points on this
  pattern.
- **Escape has two owners during opening.** The main webview catches it before Quick Look takes key focus; a local
  AppKit monitor catches it once addressed to our panel, before Quick Look's own event routing. Keep the monitor scoped
  to the panel's window number and our delegate so other windows retain Escape. See `DETAILS.md` § Opening and Escape.
- **The close observer is the single source of truth for `is_open`; don't add a parallel flip.** `panel.orderOut(nil)`
  makes `QLPreviewPanel` post its close notification. A second flip in `close_on_main` would race the observer's and
  break a quick reopen.
- **Never hold the controller mutex across `orderOut`.** With the open/close animation off, the close notification
  can arrive in the same event turn, and the observer locks the same non-reentrant mutex: a deadlock. That's why
  `close_on_main` takes `&Mutex<Self>` and reads `is_open` in a scoped guard.
- **`#[unsafe(method_id(...))]` bodies need a single tail expression**: no early `return`, no `?` (both produce
  intermediate `Option`s the macro can't coerce). Compute the value once and let the macro wrap it. See
  `previewItemAtIndex`.
