# Clipboard

File clipboard (Cmd+C/X/V) with macOS NSPasteboard interop. Follows Finder's copy-at-source, decide-at-paste model:
Cmd+X sets a Cmdr-internal cut flag, and the move-vs-copy decision happens at paste time.

## Module map

- `mod.rs`: re-exports; `#[cfg]` switch between the prod NSPasteboard backend and the E2E mock; exposes
  `snapshot_mock_clipboard` / `clear_mock_clipboard` under the `playwright-e2e` feature.
- `pasteboard.rs`: macOS NSPasteboard FFI via `objc2`. Compiled when `target_os = "macos"` AND `playwright-e2e` is OFF.
- `mock.rs`: in-process mock with the same fn signatures. Compiled when `target_os = "macos"` AND `playwright-e2e` is ON.
- `store.rs`: `LazyLock<Mutex<Option<ClipboardEntry>>>` shared by the mock module and the prod env-driven override.
- `state.rs`: cut state (`LazyLock<RwLock<Option<CutState>>>`).

## Gotchas

- **NSPasteboard is NOT thread-safe**: all pasteboard calls dispatch to the main thread via `run_on_main_thread` (same
  pattern as the drag code).
- **Cut state lives in Rust, not the frontend** (backend is authoritative for file ops). On paste, validate that the
  clipboard paths still match the stored cut-state paths; if another app replaced the clipboard, clear the stale cut
  state and paste as a copy. This prevents moving the wrong files.
- **Both file URLs and plain text are written**, so pasting into a text editor gives newline-separated paths (matches
  Finder).
- **MTP paths are excluded**: they can't be a `public.file-url`. The UI suggests F5/F6.
- **Linux is stubbed**: `#[cfg(target_os = "macos")]` gates all NSPasteboard code. Linux needs `text/uri-list` (future).
- **Runtime `CMDR_CLIPBOARD_BACKEND=mock`** lives in `pasteboard.rs` (a prod-build debugging tool), sampled once via
  `LazyLock`. Both the compile-time mock and this runtime override share `store.rs`, so a test flipping the env in one
  process sees the same data the E2E mock module sees in another. See the "Mock-backend convention" in
  [`docs/tooling/instance-isolation.md`](../../../../../docs/tooling/instance-isolation.md).

Full details (why `objc2` over a Tauri plugin, why the E2E mock is a `cfg` module swap rather than a `dyn` trait, the
copy-at-source rationale): [DETAILS.md](DETAILS.md).
