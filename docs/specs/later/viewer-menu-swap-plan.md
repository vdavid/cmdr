# Per-window menu bar for viewer (macOS)

## Context

On macOS, Tauri 2's `window.set_menu()` is a dead call — macOS has a single app-level menu bar and
Tauri doesn't swap it on window focus (tauri-apps/tauri#5768). The viewer's `build_viewer_menu()`
and `viewer_setup_menu()` exist and work on Linux, but do nothing on macOS.

**Fix:** swap the app-level menu via `app.set_menu()` when windows gain focus. macOS-only — Linux
keeps its working per-window menus.

## Approach

Build both menus at startup, store in `MenuState`. New Tauri command `activate_window_menu` replaces
`set_menu_context`. Each window calls it on focus-gain:

| Window | Calls | Effect (macOS) | Effect (Linux) |
|--------|-------|----------------|----------------|
| Main | `activate_window_menu("main")` | Swap to main menu + enable items | Enable items |
| Viewer | `activate_window_menu("viewer")` | Swap to viewer menu | No-op (per-window menu) |
| Settings/Debug | `activate_window_menu("other")` | Swap to main menu + disable items | Disable items |

Focus-loss handlers are unnecessary: the gaining window's handler fires immediately after, and when
the entire app loses focus, the menu bar isn't visible anyway (macOS shows the focused app's menu).

`cleanup_macos_menus()` runs after every `app.set_menu()` call because macOS re-injects Edit junk.

An `active_menu_kind` tracker avoids redundant swaps (viewer→viewer, main→main).

## Files to change

### Rust

**`apps/desktop/src-tauri/src/menu/mod.rs`**
- Add `ActiveMenuKind` enum: `Main`, `Viewer` (macOS-only via `#[cfg]`)
- Add `ViewerMenuItems<R>` struct: `{ menu: Menu<R>, word_wrap: CheckMenuItem<R> }`
- Change `build_viewer_menu` return type from `Menu<R>` to `ViewerMenuItems<R>`
- Add to `MenuState` (all `#[cfg(target_os = "macos")]`):
  - `main_menu: Mutex<Option<Menu<R>>>`
  - `viewer_menu: Mutex<Option<Menu<R>>>`
  - `active_menu_kind: Mutex<ActiveMenuKind>`
  - `viewer_word_wrap: Mutex<Option<CheckMenuItem<R>>>`
- Add `cleanup_macos_menus_from_command(app)` — wraps `app.run_on_main_thread(cleanup_macos_menus)`
  for safe use from Tauri command threads

**`apps/desktop/src-tauri/src/commands/ui.rs`**
- Add `activate_window_menu` command (takes `kind: "main" | "viewer" | "other"`)
  - `"main"`: on macOS, swap to main menu if needed + cleanup; then enable items (reuse existing
    `set_menu_context` logic)
  - `"viewer"`: on macOS, swap to viewer menu if needed + cleanup; on Linux, no-op
  - `"other"`: on macOS, swap to main menu if needed + cleanup; then disable items
- Keep `set_menu_context` as a private helper (remove `#[tauri::command]` and the public IPC
  registration)

**`apps/desktop/src-tauri/src/commands/file_viewer.rs`**
- `viewer_setup_menu`: make it a no-op on macOS (`#[cfg]` early return). Linux path unchanged but
  calls `build_viewer_menu(...).menu` (adjusted for new return type).
- `viewer_set_word_wrap`: on macOS, access `menu_state.viewer_word_wrap` directly (O(1) instead of
  tree walk). Linux path unchanged (tree walk on per-window menu).

**`apps/desktop/src-tauri/src/lib.rs`**
- Clone main menu before passing to `app.set_menu()`, store clone in `MenuState.main_menu`
- Build viewer menu at startup, store `menu` and `word_wrap` in `MenuState`
- Register `activate_window_menu` in `invoke_handler`, remove `set_menu_context`

### TypeScript

**`apps/desktop/src/lib/tauri-commands/app-state.ts`**
- Add `activateWindowMenu(kind: 'main' | 'viewer' | 'other')` wrapping
  `invoke('activate_window_menu', { kind })`
- Remove `setMenuContext` (no longer a Tauri command)

**`apps/desktop/src/lib/tauri-commands/index.ts`**
- Export `activateWindowMenu`, remove `setMenuContext` export

**`apps/desktop/src/routes/(main)/+page.svelte`**
- Change `setupWindowFocusListener` to call `activateWindowMenu(focused ? 'main' : 'other')`
  (Note: keeping both focused/unfocused for the edge case where main loses focus to a non-Cmdr
  window that doesn't have our focus handler, like a system dialog)
- Update import

**`apps/desktop/src/routes/viewer/+page.svelte`**
- Add `onFocusChanged` listener in `onMount`:
  - On focus: `activateWindowMenu('viewer')` + `viewerSetWordWrap(label, wordWrap)` (syncs shared
    menu checkbox to this viewer's word wrap state)
- Add unlisten to `cleanupListeners()`

**`apps/desktop/src/routes/settings/+page.svelte`**
- Add `onFocusChanged` listener in `onMount`: on focus → `activateWindowMenu('other')`
- Add unlisten in `onDestroy`

### Docs

**`apps/desktop/src-tauri/src/menu/CLAUDE.md`**
- Add section on macOS per-window menu swap: stored menus, `activate_window_menu`, cleanup after
  swap, `ActiveMenuKind` tracker

## Edge cases

- **Multiple viewers open:** the shared viewer menu checkbox syncs to the focused viewer's word wrap
  state via `viewerSetWordWrap` called right after `activateWindowMenu("viewer")`.
- **Viewer → Settings:** settings handler swaps to main menu + disables items. Correct.
- **`CLOSE_TAB_ID` exemption:** still needed in the disable-items logic (for "other" context). When
  the viewer menu is active, it uses `PredefinedMenuItem::close_window` instead, so Cmd+W works
  natively without the exemption.
- **Accelerator updates:** only affect main menu items. Viewer menu has no customizable accelerators.
  No changes needed in `update_menu_accelerator`.
- **`on_menu_event` for `VIEWER_WORD_WRAP_ID`:** works unchanged — it's an app-level event handler,
  finds the focused `viewer-*` window and emits to it.

## Verification

1. `cargo clippy` + `cargo nextest run` in `src-tauri`
2. `pnpm vitest run` in `apps/desktop`
3. `./scripts/check.sh --check rustfmt --check clippy --check svelte-check --check desktop-svelte-eslint --check desktop-svelte-prettier`
4. Manual testing with MCP:
   - Open app → verify main menu shows all items
   - Open viewer (F3) → verify viewer menu appears (File: Close, Edit: Copy/Select All, View:
     Word wrap)
   - Toggle word wrap in viewer → verify checkbox toggles
   - Open second viewer → verify menu stays as viewer, word wrap reflects second viewer's state
   - Click back to main → verify main menu restores with all items enabled
   - Open Settings → verify main menu with items disabled
   - Close Settings → verify main menu with items re-enabled
   - Cmd+W in viewer → closes viewer
   - Cmd+W in Settings → closes Settings

## Task list

### Milestone 1: Rust menu infrastructure
- [ ] Add `ActiveMenuKind`, `ViewerMenuItems`, new `MenuState` fields to `menu/mod.rs`
- [ ] Update `build_viewer_menu` return type and extract word wrap ref
- [ ] Add `cleanup_macos_menus_from_command` wrapper

### Milestone 2: New command + wiring
- [ ] Add `activate_window_menu` command to `commands/ui.rs`
- [ ] Make `set_menu_context` a private helper (remove `#[tauri::command]`)
- [ ] Fix `viewer_setup_menu` and `viewer_set_word_wrap` for macOS
- [ ] Wire up in `lib.rs`: store menus at startup, register new command

### Milestone 3: Frontend
- [ ] Add `activateWindowMenu` to TS wrappers, remove `setMenuContext`
- [ ] Update main page focus handler
- [ ] Add focus handler to viewer page
- [ ] Add focus handler to settings page

### Milestone 4: Docs + checks
- [ ] Update `menu/CLAUDE.md`
- [ ] Run checks, fix any issues
- [ ] Manual test with MCP servers
