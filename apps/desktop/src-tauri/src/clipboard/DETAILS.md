# Clipboard details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is the decision record.

## Copy-at-source, decide-at-paste

Follows Finder's model. Cmd+X sets an internal cut flag; the move-vs-copy decision happens at paste time. The cut state
is Cmdr-internal only, so pasting Cmdr-cut files in Finder does a copy (Finder doesn't know about our flag). This
matches third-party file managers (Path Finder, ForkLift).

## Direct NSPasteboard via `objc2`, not a Tauri plugin

The codebase already uses `objc2` for drag image detection, and the official `tauri-plugin-clipboard-manager` only
supports text/images, not file URLs. Direct access gives full control without adding a dependency.

## Cut state in Rust, not the frontend

The backend is authoritative for file operations, so keeping cut state in Rust avoids frontend/backend sync issues. The
frontend queries via IPC when needed. On paste, the backend validates that the live clipboard paths still match the
stored cut-state paths; a mismatch (another app replaced the clipboard) clears the stale cut state and falls back to a
copy.

## E2E mock as a `#[cfg]` module swap, not a `dyn` trait

Three call sites already hop to the main thread via `app.run_on_main_thread()` and pass `PathBuf` values; a trait object
would add `Send` bounds the `objc2` types resist. A `cfg`-driven module swap keeps every call site byte-identical
between configurations and removes the prod-only `objc2` link cost from E2E builds. Acceptance: a full E2E run leaves
`pbpaste` unchanged.

## Runtime `CMDR_CLIPBOARD_BACKEND=mock` override

Lives inside `pasteboard.rs` (not `mod.rs`) because it's a debugging tool for prod-feature builds. Sampled once via
`LazyLock` at first access, so the hot path is a single atomic load. Both the compile-time mock path and this runtime
override share `store.rs`, so a test that flips the env in one process sees the same data the E2E mock module sees in
another. See the "Mock-backend convention" in
[`docs/tooling/instance-isolation.md`](../../../../../docs/tooling/instance-isolation.md).
