# Clipboard

File clipboard operations: Cmd+C/X/V for files, with macOS system clipboard interop.

## Files

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports; `#[cfg]`-driven switch between prod NSPasteboard and the E2E mock; exposes `snapshot_mock_clipboard` and `clear_mock_clipboard` under the E2E feature |
| `pasteboard.rs` | macOS NSPasteboard FFI via `objc2`. Compiled when `target_os = "macos"` AND `playwright-e2e` is OFF. Honors `CMDR_CLIPBOARD_BACKEND=mock` at startup to delegate to the shared store without recompiling. |
| `mock.rs` | In-process mock backend. Compiled when `target_os = "macos"` AND `playwright-e2e` is ON. Same fn signatures as `pasteboard.rs`. |
| `store.rs` | `LazyLock<Mutex<Option<ClipboardEntry>>>` shared by both the mock module and the prod module's env-driven override. |
| `state.rs` | Cut state management (`LazyLock<RwLock<Option<CutState>>>`) |

## Key decisions

**Decision**: Follow Finder's copy-at-source, decide-at-paste model. Cmd+X sets an internal cut flag; the decision to
move vs copy happens at paste time.
**Why**: Ensures interop with Finder. Cmd+X cut state is Cmdr-internal only, so pasting Cmdr-cut files in Finder does a
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

**Decision**: E2E mock is a `#[cfg(feature = "playwright-e2e")]` module-level switch, not a `dyn` trait.
**Why**: Three call sites already hop to the main thread via `app.run_on_main_thread()` and pass `PathBuf` values; a
trait object would add `Send` bounds the `objc2` types resist. A `cfg`-driven module swap keeps every call site
byte-identical between configurations and removes the prod-only `objc2` link cost from E2E builds.
Acceptance: a full E2E run leaves `pbpaste` unchanged.

**Decision**: Runtime `CMDR_CLIPBOARD_BACKEND=mock` env override lives inside `pasteboard.rs`, not `mod.rs`.
**Why**: The override is a debugging tool for prod-feature builds. Sampled once via `LazyLock` at first access so the
hot path is a single atomic load. Both compile-time and runtime mock paths share `store.rs`, so a test that flips the
env in one process sees the same data the E2E mock module sees in another. See "Mock-backend convention" in
[`docs/specs/instance-isolation-plan.md`](../../../../docs/specs/instance-isolation-plan.md).

## Gotchas

- **NSPasteboard is NOT thread-safe**: all pasteboard calls dispatch to the main thread via `run_on_main_thread` (same
  pattern as drag code).
- **Both file URLs and plain text are written**: pasting in a text editor gives newline-separated file paths (matches
  Finder behavior).
- **MTP paths are excluded**: MTP virtual paths can't be represented as `public.file-url`. The UI suggests F5/F6.
- **Linux is stubbed**: `#[cfg(target_os = "macos")]` gates all NSPasteboard code. Linux needs `text/uri-list` MIME
  handling (future work).
