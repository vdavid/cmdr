# Rust backend (`src-tauri/`)

The Tauri 2 + Rust backend. Subsystem must-knows live in each module's colocated `CLAUDE.md`; the full subsystem map is
[`/docs/architecture.md`](../../../docs/architecture.md). These rules apply to all Rust under here.

## Rust rules

- ❌ No `eprintln!` / `println!` / `dbg!`: they bypass the fern logger (no level filter, no file output, not in
  error-report bundles) and clippy denies them. Use `log::{debug,info,warn,error}!` with a scoped `target:`. See
  [`src/logging/CLAUDE.md`](src/logging/CLAUDE.md).
- ❌ No bare `.lock()` / `.read()` / `.write().unwrap()` on a std `Mutex` / `RwLock`: a poisoned lock aborts the whole
  app. Use `*_ignore_poison()` (recover) or `.expect("…poison…<why aborting is correct>")` (abort). Enforced by
  `lock-poison`; see `src/ignore_poison.rs`.
- ❌ No bare `.unwrap()` in production: it's a silent panic. Handle the error (`?` / `ok_or` / `match`) where the value
  can genuinely be absent, or use `.expect("<concrete why it can't fail>")` for a true invariant (the sanctioned form,
  per the lock-poison rule). Enforced by `clippy::unwrap_used`; `#[test]` fns are exempt (`clippy.toml`
  `allow-unwrap-in-tests`), but test *helper* fns outside `#[test]` aren't, so they use `.expect("…")` too.
- ❌ Never build the app with raw `cargo build` (white screen, no embedded frontend). Use `pnpm tauri build` or the
  `tauri-wrapper.js build` wrapper, which runs `beforeBuildCommand`. See [`../scripts/CLAUDE.md`](../scripts/CLAUDE.md).
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
  `src-tauri/capabilities/{default,settings,viewer}.json`, and `await` the call in try/catch so failures surface instead
  of looking like a broken feature. See [`capabilities/CLAUDE.md`](capabilities/CLAUDE.md).
- ❌ Don't read TCC-protected paths (`~/Downloads`, `~/Documents`, iCloud, etc.) or call `NSWorkspace` icon /
  LaunchServices APIs at launch without the FDA gate: they stack macOS TCC popups during onboarding (we hit 5-10 once).
  Use `crate::fda_gate::is_fda_pending_runtime()`. See `src/fda_gate.rs` and
  [`lib/onboarding/CLAUDE.md`](../src/lib/onboarding/CLAUDE.md).

## Platform constraints (filesystem and IPC)

These cut across modules; all existing commands follow them, so apply them to new code too.

- **Synchronous `#[tauri::command]` functions block the IPC handler thread.** If one hangs (a syscall on a dead network
  mount), every later IPC call queues behind it and the app looks frozen. So every filesystem-touching command is
  `async` with `blocking_with_timeout` (2 s default). New filesystem commands MUST follow this; see `commands/` for
  examples.
- **Network-mount syscalls block indefinitely.** `statfs`, `readdir`, `metadata()`, NSURL resource queries, and
  `realpath` can wait 30-120 s on slow/hung mounts. Wrap every one in `blocking_with_timeout`; see
  [`commands/CLAUDE.md`](src/commands/CLAUDE.md) for the timeout tiers.
- **Two-layer timeout defense** on critical paths (volume switching, path resolution, space queries): the backend
  `blocking_with_timeout` (2-15 s) plus a frontend `withTimeout` (500 ms-3 s) that races the IPC call and returns a
  fallback. Apply both when adding IPC on a slow path.
- **Never use rayon for calls into macOS frameworks** (NSURL/FileProvider/NSWorkspace): the synchronous XPC round-trips
  can blow rayon's 2 MB worker stack. Use dedicated 8 MB-stack OS threads. See `file_system/CLAUDE.md` for the pattern.

Full details: [DETAILS.md](DETAILS.md).
