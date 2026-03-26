# Clipboard

File clipboard operations: Cmd+C/X/V for files, with macOS system clipboard interop.

## Files

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports; `#[cfg(target_os = "macos")]` gates for pasteboard |
| `pasteboard.rs` | NSPasteboard FFI via `objc2`: write/read file URLs + plain text |
| `state.rs` | Cut state management (`LazyLock<RwLock<Option<CutState>>>`) |

## Key decisions

**Decision**: Follow Finder's copy-at-source, decide-at-paste model. Cmd+X sets an internal cut flag; the decision to
move vs copy happens at paste time.
**Why**: Ensures interop with Finder. Cmd+X cut state is Cmdr-internal only — pasting Cmdr-cut files in Finder does a
copy (Finder doesn't know about our cut flag). This matches third-party file managers (Path Finder, ForkLift).

**Decision**: Direct NSPasteboard access via `objc2` instead of a Tauri clipboard plugin.
**Why**: The codebase already uses `objc2` for drag image detection. The official `tauri-plugin-clipboard-manager` only
supports text/images, not file URLs. Direct access gives full control without adding dependencies.

**Decision**: Cut state lives in Rust backend, not frontend.
**Why**: The backend is authoritative for file operations. Keeping cut state in Rust avoids sync issues between frontend
and backend. Frontend queries via IPC when needed.

**Decision**: On paste, validate that clipboard paths still match the stored cut state paths. If another app replaced the
clipboard, clear stale cut state and paste as copy.
**Why**: Prevents accidental moves of the wrong files when the clipboard changed between cut and paste.

## Gotchas

- **NSPasteboard is NOT thread-safe** — all pasteboard calls dispatch to the main thread via `run_on_main_thread` (same
  pattern as drag code).
- **Both file URLs and plain text are written** — pasting in a text editor gives newline-separated file paths (matches
  Finder behavior).
- **MTP paths are excluded** — MTP virtual paths can't be represented as `public.file-url`. The UI suggests F5/F6.
- **Linux is stubbed** — `#[cfg(target_os = "macos")]` gates all NSPasteboard code. Linux needs `text/uri-list` MIME
  handling (future work).
