# Rust backend (`src-tauri/`)

The Tauri 2 + Rust backend. Subsystem must-knows live in each module's colocated `CLAUDE.md`; the full map is
`docs/architecture.md`. These rules apply to all Rust under here.

## Rust rules

- ❌ No `eprintln!` / `println!` / `dbg!`: they bypass the fern logger (no level filter, file output, or error-report
  capture), and clippy denies them. Use `log::{debug,info,warn,error}!` with a scoped `target:`. See
  `src/logging/CLAUDE.md`.
- ❌ No bare `.lock()` / `.read()` / `.write().unwrap()` on a std `Mutex` / `RwLock`: a poisoned lock aborts the app.
  Use `*_ignore_poison()` (recover) or `.expect("…poison…<why aborting is correct>")` (abort). Enforced by
  `lock-poison`; helpers live in `crate::ignore_poison`.
- ❌ No bare `.unwrap()` in production: it's a silent panic. Handle the error (`?` / `ok_or` / `match`), or
  `.expect("<concrete why it can't fail>")` for a true invariant. Enforced by `clippy::unwrap_used`; `#[test]` fns are
  exempt, test *helper* fns outside one aren't.
- ❌ Don't drop a typed answer: a `()`-returning fn discarding a delegate's `bool` or outcome enum leaves the IPC
  command or MCP tool above it inventing a success. Return it (the `PauseOutcome` pattern) or justify the drop with
  `// allowed-discarded-outcome: <why>`. Enforced by `discarded-outcome`.
- ❌ No `thiserror` / `anyhow` as a direct dependency, anywhere in the workspace: errors are hand-rolled enums with a
  manual `From`, each variant carrying the data a caller acts on rather than a sentence. Transitive copies are fine.
- ❌ In a Rust test, never hand-roll a poll loop, a fixed sleep, or a constant-path fixture dir: all three pass silently
  or collide. `crate::test_support` replaces them (`wait_until`, `wait_until_async`, `TestDir`). Rules:
  `docs/testing.md`.
- ❌ Never build with raw `cargo build` (white screen, no embedded frontend). Use `pnpm tauri build` or the
  `tauri-wrapper.ts build` wrapper. See `../scripts/CLAUDE.md`.
- ❌ Every `unsafe {}` block and `unsafe impl` needs a `// SAFETY:` comment above it naming the concrete invariant that
  makes THAT site sound: specific, never boilerplate, and ❌ never a blanket file exemption. Enforced by
  `clippy::undocumented_unsafe_blocks`.
- ❌ AppKit/Cocoa main-thread-only calls (NSWindow, NSColor, NSPasteboard, NSApplication, drag) must take or assert an
  `objc2::MainThreadMarker`, and a sync `#[tauri::command]` must NOT touch AppKit: hop via `app.run_on_main_thread()`
  and return through an `mpsc` channel (pattern: `accent_color.rs`, `commands/clipboard.rs`). Which Apple APIs are
  thread-safe and exempt: `DETAILS.md`.

## Tauri commands and capabilities

- ❌ Tauri APIs fail silently without permission. Calling a new one from a window (`setMinSize`, `setTitle`, a plugin
  command) means adding that permission to the window's `src-tauri/capabilities/{default,settings,viewer}.json`, and
  `await`ing the call in try/catch so failures surface. More [here](capabilities/CLAUDE.md).
- Check the FDA gate before reading TCC-protected paths (`~/Downloads`, `~/Documents`) or calling `NSWorkspace` icon /
  LaunchServices APIs at launch: they stack macOS TCC popups during onboarding.
  [Details](../src/lib/onboarding/CLAUDE.md)

## Platform constraints (filesystem and IPC)

- **Sync `#[tauri::command]` funcs block the IPC handler thread**, so one hang looks like a frozen app. Every
  FS-touching command must be `async` with `blocking_with_timeout` (2 s default). See `commands/`.
- **Network-mount syscalls block indefinitely.** `statfs`, `readdir`, `metadata()`, NSURL resource queries, and
  `realpath` can wait 30-120 s on a slow or hung mount. Wrap every one; timeout tiers
  [here](src/commands/CLAUDE.md).
- **Two-layer timeout defense** on critical paths (volume switching, path resolution, space queries): backend
  `blocking_with_timeout` (2-15 s) plus a frontend `withTimeout` (500 ms-3 s) that races the call and returns a
  fallback.
- ❌ **Never use rayon for calls into macOS frameworks** (NSURL/FileProvider/NSWorkspace): the synchronous XPC
  round-trips can blow rayon's 2 MB worker stack. Use dedicated 8 MB-stack OS threads. Pattern
  [here](src/file_system/CLAUDE.md).

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
