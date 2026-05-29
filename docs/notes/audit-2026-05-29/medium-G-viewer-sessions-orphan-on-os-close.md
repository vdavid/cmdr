# Viewer SESSIONS orphan when window is closed via the OS title bar

**Severity:** medium
**Lens:** G — Resource hygiene
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/file_viewer/session.rs:117` (global `SESSIONS`)
- `apps/desktop/src-tauri/src/file_viewer/CLAUDE.md:86` (gotcha already noted)
- `apps/desktop/src/routes/viewer/+page.svelte:875` (`onDestroy` does NOT call `viewerClose`)
- `apps/desktop/src/routes/viewer/+page.svelte:417` (`closeWindow` does call `viewerClose`)
- `apps/desktop/src-tauri/src/lib.rs:978` (window-event handler only acts on `label() == "main"`)

## What

`SESSIONS` is a process-global `LazyLock<Mutex<HashMap<String, ViewerSession>>>` that grows on every `viewer_open` and is freed only by `viewer_close`. The frontend `viewerClose` IPC is fired exclusively from `closeWindow()` (Escape, Cmd+W, the in-window Cancel button). The Svelte `onDestroy` hook in the viewer route deliberately does NOT call `viewerClose` — it only tears down listeners, scroll, and search composables. There is no Rust-side `WindowEvent::Destroyed` / `CloseRequested` handler for `viewer-*` labels in `lib.rs`.

When the user closes the viewer window via the macOS title-bar red button, the WKWebView is destroyed without giving Svelte time to run `onDestroy` against a live IPC bridge. The backing `ViewerSession` (which holds the backend `Arc` — `FullLoadBackend` may own the entire file as `String` up to ~1 MB; `LineIndexBackend` holds a ~`total_lines / 256` checkpoint vec; both keep a file handle alive) stays in `SESSIONS` for the rest of the app session.

## Why it matters

- Linear leak in resident memory keyed to "how many viewer windows the user closes via the title bar." Per session, `FullLoadBackend` pins the full file content in RAM (up to 1 MB), and `LineIndexBackend` pins a checkpoint vector (~3 MB for a 100 M-line file per the doc).
- Open file descriptor stays held for the backend's underlying file handle. Removable volumes can't unmount cleanly while held.
- Power users opening many files for quick peeks compound this over hours-long sessions.

The accompanying `active_reads: Mutex<HashMap<u64, Arc<AtomicBool>>>` per session is much smaller and bounded by in-flight reads, but it's pinned for the same lifetime.

## Evidence

`apps/desktop/src/routes/viewer/+page.svelte`:
```svelte
onDestroy(() => {
    cleanupAccentColor()
    cleanupTextSize()
    cleanupListeners()
    search.destroy()
    scroll.destroy()
    indexingPoll.stop()
})
```
No `viewerClose(sessionId)` call. Only `closeWindow()` (line 417) issues it.

`apps/desktop/src-tauri/src/lib.rs`:
```rust
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { .. } = event
        && window.label() == "main"
    {
        // … main-window-only cleanup …
    }
})
```
No branch for `viewer-*` labels.

`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md` line 86 documents this explicitly: "SESSIONS is unbounded: grows with each `viewer_open`. Must call `viewer_close` when window closes (not automatic)."

## Suggested fix

Add a Rust-side window-event handler (`CloseRequested` or `Destroyed`) for windows whose label starts with `"viewer-"`. Look up the session ID associated with that window (could be derived from the label, or tracked in a `WINDOWS_TO_SESSIONS` map populated by `viewer_open`) and call the existing session-close logic. This makes the cleanup OS-event-driven instead of relying on a frontend code path that the title-bar close skips.

Alternatively (or additionally), parse the session ID from the window label and have the Rust handler call the existing close function directly so the frontend never needs to remember the IPC call.

## Notes

- The `closeWindow()` path is correct; the gap is only the title-bar-X case.
- Documented gotcha doesn't make it OK to ship — title-bar close is the most-discoverable way to close a window on macOS.
- The same pattern (no auto-cleanup on OS close) likely applies to other windows that hold backend resources, but the viewer is the highest-cost case.
