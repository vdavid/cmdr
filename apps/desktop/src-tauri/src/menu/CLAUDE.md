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
  CheckMenuItems (one ⌘1/⌘2 pair following the active pane). `MenuState` tracks each item's submenu and index for this,
  so **adding or moving one item shifts every `register_item` index after it**, mangling a different item on the first
  rebind. `register_item_positions_match_submenu_order` fails on a mismatch; keep the position comments truthful too.
- **CheckMenuItems (view modes, show hidden) must NOT use `"execute-command"`.** They auto-toggle on click, so
  emitting it too would double-toggle; they emit `"settings-changed"` / `"view-mode-changed"` directly. Sort items emit
  `"menu-sort"`; close-tab and "Open with" have their own paths. Some are still registered in `menu_id_to_command` /
  `MenuState.items` purely for accelerator sync (`DETAILS.md`).
- **File-scoped commands are dual-guarded**: `activate_window_menu("other")` greys them out (visual only); the real
  guard is `main_window.is_focused()` in `on_menu_event`. Accelerators fire even when items look disabled.
- **`OPERATION_START_ITEM_IDS` greys out while a dialog is up or Ask Cmdr has focus** (`commands/menu.rs`), and
  `set_menu_context` re-applies it LAST — its loop enables every explorer item, so a focus round-trip would otherwise
  re-offer Copy. Chrome only (accelerators still fire); the refusals live in `mcp/executor` and the two frontend gates.
  ❌ Every gated id must be `FileScoped`: `Edit > Paste` is `App`-scoped because other windows forward the native
  `paste:` selector through it, so greying it would kill ⌘V in their text fields. A test pins this.
- **macOS swaps the app menu bar on focus-gain (`activate_window_menu`); Linux uses per-window menus.** macOS has one
  app-level menu bar (tauri-apps/tauri#5768), so each window's focus handler swaps `app.set_menu()` between the main and
  viewer menus (stored in `MenuState`); `active_menu_kind` skips redundant swaps. Re-run `cleanup_macos_menus` after
  every swap (Edit items get re-injected); swapping back to main also re-applies `set_macos_menu_icons` (SF Symbols
  don't survive `app.set_menu()`). `window.set_menu()` is a macOS no-op, so `viewer_setup_menu` early-returns there and
  `viewer_set_word_wrap` flips the stored `viewer_word_wrap` CheckMenuItem (O(1)). See `DETAILS.md`.
- **Custom (not Predefined) MenuItems for Cut/Copy/Paste/Move here/Select all**: in non-main windows these forward the
  native `copy:`/`cut:`/`paste:`/`selectAll:` selector via `send_native_edit_action()`; without it ⌘A and clipboard are
  dead in settings/viewer text fields. ⌘A on the main window routes through `execute-command` (frontend checks
  `document.activeElement`). Don't swap to `PredefinedMenuItem::select_all`: it conflicts with the custom item.
- **`Select all` / `Deselect all` live in `Select`, not `Edit`**: they operate on files, not text. The file-vs-text
  distinction is load-bearing; read the decision (`DETAILS.md`) before moving them back.
- **Linux omits F-key, Tab, Space, and `Cmd+Plus`/`Cmd+Minus` accelerators** (GTK intercepts them at the toolkit
  level, causing double-handling or silent swallowing); those keys dispatch through JS keydown there instead.
- **Trailing `…` means the dialog can change WHAT the command acts on** (`Copy…` takes a destination), not merely
  whether it runs (`Delete` confirms). Same label in menu bar and context menu; always U+2026, never `...` (enforced by
  `menu_labels_use_the_ellipsis_character`). Per-item verdicts and why: `DETAILS.md`.
- **macOS SF Symbol map matches by exact title string**, including the `\u{2026}` ellipsis: keep the `MenuItem` title and
  the symbol map byte-identical, or the item silently loses its icon. Menu bar only, never context menus (`DETAILS.md`).
- **⌘G / ⌘J double-dispatch on macOS**: the combo fires both the native menu and the JS keydown. Safe unsuppressed
  (⌘G dialog-open is idempotency-guarded, ⌘J re-reveal is idempotent). Expect two log lines per ⌘J press.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
