# Dock tile menu

The menu behind a right-click on Cmdr's Dock icon: Open Cmdr, Search files…, Go to
folder…, Connect to server…, then the bookmarks, then the open tabs. macOS appends
`Options ▸` / `Show All Windows` / `Quit` below it, and we keep those.

## Module map

- **`rows.rs`**: every decision, over plain data. Which rows exist, dedup,
  disambiguation, the cap. No AppKit.
- **`native.rs`**: the `NSMenu`, hand-built with objc2. **`sources.rs`**: the two
  live reads and why neither can stall.
- **`mod.rs`**: `install()`, the two ObjC callbacks, and what a click does.

## Must-knows

- ❗ **`applicationDockMenu:` runs on the main thread while the Dock waits.** So:
  ❌ **never let a panic reach Objective-C** (both callbacks wrap their WHOLE body in
  `catch_unwind`; the AppKit half adds `objc2::exception::catch`), and ❌ **never
  block**: no I/O, no syscall on a user path, no lock a slow thread can hold.
- ❌ **Never `stat` a path here**, not even `exists()`. A favorite on a sleeping NAS
  would hang the Dock for minutes. `rows.rs` judges a path by its SHAPE; a row naming
  a folder that's gone lands on the pane's ordinary refusal, which is far better.
- **Both sources are read LIVE and non-blockingly**: `favorites::store::list_cached`
  (`try_lock`, ❌ never `list`, which seeds the file behind the disk lock) and
  `mcp::pane_state::tabs_focused_first` (`try_read` ×3). An unavailable source
  contributes no rows for that one right-click.
- **We build the `NSMenu` ourselves.** Tauri exposes none (`Submenu::inner()` is
  `pub(crate)`), and ❌ muda must not become a direct dependency: it works only while
  cargo unifies our copy with Tauri's.
- **Clicks bypass `menu_handlers::handle_menu_event` entirely**, which is the point:
  its `CommandScope::FileScoped` guard drops anything fired while the main window
  isn't focused, and a Dock right-click is unfocused by definition.
- **An item's `tag` is its row index**, and it's answered, never trusted: a bounds-
  checked `get`, ❌ never an index.
- ⚠️ **`command_id` is the third place Rust names a frontend command id.**
  `rust-command-id-drift.test.ts` scans it; keep the two in step.
- **New label? It's a RAW `menu.dock.*` key** through `menu_t`, ❌ never `t()`, with
  apostrophes SINGLE. No capture can photograph a native menu, so the `@key`
  description is the translator's only aid.

Why each source is safe, the disambiguation rules, the memory contract on the
returned menu, and what's deliberately out of scope: `DETAILS.md`.
