# Menu system

Native menu bar for macOS and Linux. Builds platform menus from scratch, handles events, syncs accelerator labels to
user shortcuts, and enables/disables items by focus context.

## Module map

- `mod.rs`: shared types, enums, event structs, and menu state.
- `command_map.rs`: menu item ID constants + the `menu_id_to_command` / `command_id_to_menu_id` maps (re-exported via
  `mod.rs`).
- `menu_items.rs` / `menu_structure.rs`: small-piece builders and hierarchical assembly (`build_menu` dispatcher,
  context menus, viewer menu).
- `menu_handlers.rs`: event handling + live-update helpers. `media_index_items.rs`: the image-search-items decider.
  `macos.rs` / `linux.rs`: platform menu bars. `open_with.rs`: the "Open with" submenu (macOS).

## Must-knows

- **Build menus from scratch; never `Menu::default()`.** It inherits OS-injected Edit items (Writing Tools, AutoFill,
  Dictation) that can't be reliably removed before display. `cleanup_macos_menus` (objc2) only strips items AppKit
  injects *after* construction.
- **Accelerator changes go remove/recreate/reinsert, not in-place** (Tauri has no `set_accelerator()`).
  `update_menu_item_accelerator` handles HashMap items; `rebuild_view_mode_items` handles the four per-pane view-mode
  CheckMenuItems (they share one ⌘1/⌘2 pair that follows the active pane). `MenuState` tracks both the `Submenu`
  reference and positional index per updatable item for this.
- **CheckMenuItems (view modes, show hidden) must NOT use `"execute-command"`.** They auto-toggle their checked state on
  click, so emitting `execute-command` too would double-toggle. They emit `"settings-changed"` / `"view-mode-changed"`
  directly. Sort items emit `"menu-sort"`; close-tab and "Open with" have their own paths. The four sort columns and the
  `…` selection items are still registered in `menu_id_to_command` / `MenuState.items` only so user accelerators flow
  through the generic update path.
- **File-scoped commands are dual-guarded**: `activate_window_menu("other")` greys them out (visual hint only); the real
  guard is `main_window.is_focused()` in `on_menu_event` before emitting. Both are needed: accelerators fire even when
  items look disabled on some platforms.
- **macOS swaps the app menu bar on focus-gain (`activate_window_menu`); Linux uses per-window menus.** macOS has one
  app-level menu bar (tauri-apps/tauri#5768), so each window's focus handler swaps `app.set_menu()` between the main and
  viewer menus (stored in `MenuState`); `active_menu_kind` skips redundant swaps. After every swap re-run
  `cleanup_macos_menus` (Edit items get re-injected); swapping back to main also re-applies `set_macos_menu_icons`
  (SF Symbols don't survive `app.set_menu()`). `window.set_menu()` is a macOS no-op, so `viewer_setup_menu` early-returns
  there and `viewer_set_word_wrap` flips the stored `viewer_word_wrap` CheckMenuItem (O(1)). See [DETAILS.md](DETAILS.md).
- **Custom (not Predefined) MenuItems for Cut/Copy/Paste/Move here/Select all**: in non-main windows these forward the
  native `copy:`/`cut:`/`paste:`/`selectAll:` selector via `send_native_edit_action()`; without it ⌘A and clipboard are
  dead in settings/viewer text fields. ⌘A on the main window routes through `execute-command` (frontend checks
  `document.activeElement` for text-vs-file). Don't swap to `PredefinedMenuItem::select_all`: it conflicts with the
  custom item.
- **`Select all` / `Deselect all` live in the `Select` top-level menu, not `Edit`.** Cmdr's `selection.selectAll`
  operates on files, not text. Don't move them back without re-reading the decision: the file-vs-text distinction is
  load-bearing.
- **Linux omits F-key, Tab, and Space accelerators** (GTK intercepts them at the toolkit level, causing double-handling
  or silent swallowing); those keys dispatch through JS keydown on Linux instead. Linux also skips `Cmd+Plus`/`Cmd+Minus`
  zoom accelerators for the same reason.
- **macOS SF Symbol map matches by exact title string**, including the `\u{2026}` ellipsis: keep the `MenuItem` title and
  the symbol map byte-identical. SF Symbols are set only on the menu bar (objc2 walk of `NSApplication.mainMenu()`); our
  context-menu items get none (Tauri doesn't expose the raw `NSMenu` pointer). "Open with" app icons are the exception
  (full-color non-template images via `IconMenuItem`).
- **⌘G / ⌘J double-dispatch on macOS**: the combo fires both the native menu and the JS keydown. Safe here without
  suppression (⌘G dialog-open is idempotency-guarded, ⌘J re-reveal is idempotent). Expect two log lines per ⌘J press.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
