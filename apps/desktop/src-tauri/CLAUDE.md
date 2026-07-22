# Rust backend (`src-tauri/`)

The Tauri 2 + Rust backend. Subsystem must-knows live in each module's colocated `CLAUDE.md`; the full map is
`docs/architecture.md`. These rules apply to all Rust under here.

## Rust rules

- ❌ No `eprintln!` / `println!` / `dbg!`: they bypass the fern logger (no level filter, file output, or error-report
  capture) and clippy denies them. Use `log::{debug,info,warn,error}!` with a scoped `target:`. See
  `src/logging/CLAUDE.md`.
- ❌ No bare `.lock()` / `.read()` / `.write().unwrap()` on a std `Mutex` / `RwLock`: a poisoned lock aborts the whole
  app. Use `*_ignore_poison()` (recover) or `.expect("…poison…<why aborting is correct>")` (abort). Enforced by
  `lock-poison`; see `src/ignore_poison.rs`.
- ❌ No bare `.unwrap()` in production: it's a silent panic. Handle the error (`?` / `ok_or` / `match`) where the value
  can genuinely be absent, or `.expect("<concrete why it can't fail>")` for a true invariant. Enforced by
  `clippy::unwrap_used`; `#[test]` fns are exempt (`clippy.toml` `allow-unwrap-in-tests`), but test *helper* fns outside
  `#[test]` aren't, so they use `.expect("…")` too.
- ❌ Never build with raw `cargo build` (white screen, no embedded frontend). Use `pnpm tauri build` or the
  `tauri-wrapper.ts build` wrapper, which runs `beforeBuildCommand`. See `../scripts/CLAUDE.md`.
- ❌ Every `unsafe {}` block (and `unsafe impl`) needs a `// SAFETY:` comment on the immediately-preceding line, stating
  the concrete invariant that makes THAT site sound (receiver/pointer validity, selector ABI match, thread, Create-vs-Get
  ownership, success-gate) — specific, never boilerplate. Enforced by `clippy::undocumented_unsafe_blocks`. Rote FFI is
  documented per-site; ❌ never blanket-exempt a file with `#[allow(clippy::undocumented_unsafe_blocks)]`.
- ❌ AppKit/Cocoa main-thread-only calls (NSWindow, NSColor, NSPasteboard, NSApplication, drag) must take or assert an
  `objc2::MainThreadMarker` (proof you're on-main). A sync `#[tauri::command]` must NOT touch AppKit: hop via
  `app.run_on_main_thread()` and return through an `mpsc` channel (pattern: `accent_color.rs`, `commands/clipboard.rs`).
  Thread-safe Apple APIs (NSURL resource values, NSFileManager, NSUserDefaults, LaunchServices, Keychain, IOKit, Mach)
  are exempt.

## Tauri commands and capabilities

- ❌ Tauri APIs fail silently without permission. When you call a new Tauri API from a window (`setMinSize`, `setTitle`,
  plugin commands, anything new), add the matching permission to that window's capability file in
  `src-tauri/capabilities/{default,settings,viewer}.json`, and `await` the call in try/catch so failures surface. More
  [here](capabilities/CLAUDE.md).
- Check the FDA gate before reading TCC-protected paths (`~/Downloads`, `~/Documents`, etc.) or calling `NSWorkspace` icon /
  LaunchServices APIs at launch. Such access stack macOS TCC popups during onboarding, bad UX. [Details](../src/lib/onboarding/CLAUDE.md)

## Platform constraints (filesystem and IPC)

These cut across modules; all existing commands follow them, so apply them to new code too.

- **Sync `#[tauri::command]` funcs block the IPC handler thread.** If one hangs, app looks frozen. → Every FS-touching
  command must be `async` with `blocking_with_timeout` (2s default). New FS commands MUST follow this. See `commands/`.
- **Network-mount syscalls block indefinitely.** `statfs`, `readdir`, `metadata()`, NSURL resource queries, and
  `realpath` can wait 30-120 s on slow/hung mounts. Wrap every one in `blocking_with_timeout`; see timeout tiers
  [here](src/commands/CLAUDE.md).
- Use **two-layer timeout defense** on critical paths (volume switching, path resolution, space queries): BE:
  `blocking_with_timeout` (2-15 s) + FE `withTimeout` (500 ms-3 s) that races the IPC call and returns a
  fallback.
- **Never use rayon for calls into macOS frameworks** (NSURL/FileProvider/NSWorkspace): the synchronous XPC round-trips
  can blow rayon's 2 MB worker stack. Use dedicated 8 MB-stack OS threads. See pattern [here](src/file_system/CLAUDE.md).

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
