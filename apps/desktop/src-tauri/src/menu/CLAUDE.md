# Menu system

Native menu bars for macOS and Linux, built from scratch in the user's language.

## Module map

- `mod.rs` (shared types and menu state), `command_map.rs` (item IDs + the two id↔command maps).
- `menu_items.rs` / `menu_structure.rs` build and assemble the pieces; `macos.rs` / `linux.rs` lay out each platform's
  bar; `macos_appkit.rs` is the objc2 boundary the right-click extras cross (`services_context.rs`, `share_submenu.rs`,
  `context_menu_icons.rs`, `context_menu_header.rs`). One line per file: DETAILS § File layout.

## Must-knows

- **Build menus from scratch; ❌ never `Menu::default()`.** It inherits OS-injected Edit items that can't be removed
  before display; `cleanup_macos_menus` only strips what AppKit injects *after*.
- **The macOS right-click extras, all `DETAILS.md`, and ❗ every loan must outlive `popup()` — ❌ never
  `let _ =`.** `Services`: AppKit owns ONE, already in the app menu, so `ServicesLoan` borrows it, aimed at the
  RIGHT-CLICKED rows. `Share`: ours, from `file_system/share.rs`'s enumeration, ids
  `share-service:<index>`, closing with `Edit extensions…`, ❌ never empty. SF Symbols: set on
  `NSMenuDidBeginTrackingNotification`, ❌ never `IconMenuItem` — muda's bitmaps can't be TEMPLATE images, so a glyph
  vanishes in one appearance and goes dark when highlighted. `IconMenuItem` stays right for real pixels.
- **The context menu's first line is a disabled HEADER naming what it acts on** (the selection vs the clicked row). Rust
  picks the shape from `context_paths.len()` and formats NOTHING: ❌ every number arrives pre-rendered from the
  frontend; ❌ no file KIND. `context_menu_header.rs`.
- **Accelerator changes go remove/recreate/reinsert** (Tauri has no `set_accelerator()`), and `MenuState` tracks each
  item's submenu and index — so **adding or moving one item shifts every later `register_item` index**, mangling a
  different item on the first rebind. `register_item_positions_match_submenu_order` catches it, which is why both
  platform files keep near-identical blocks; keep those position comments truthful.
- **CheckMenuItems (view modes, show hidden) must NOT use `"execute-command"`**: they auto-toggle, so emitting it
  double-toggles. They emit `"settings-changed"` / `"view-mode-changed"`; sort emits `"menu-sort"`; close-tab and
  "Open with" have own paths.
- **File-scoped commands are dual-guarded**: `activate_window_menu("other")` greys them (visual only); the real guard is
  `main_window.is_focused()` in `on_menu_event` — accelerators fire even when items look off.
- **Enabled state has ONE writer, `apply_menu_item_states`**: store a new input and add it to `menu_item_enabled`, ❌
  never a direct `set_enabled`. Check items revert a click the dialog gate refuses. ❌ `OPERATION_START_ITEM_IDS` stay
  `FileScoped`. DETAILS § Dialog refusals.
- **macOS swaps the app menu bar on focus-gain (`activate_window_menu`); Linux uses per-window menus.** One app-level
  bar, so each window's focus handler `app.set_menu()`s between main and viewer. Re-run `cleanup_macos_menus` after
  every swap, and `set_macos_menu_icons` on the way back (SF Symbols don't survive it). `window.set_menu()` is a macOS
  no-op.
- **Custom (not Predefined) MenuItems for Cut/Copy/Paste/Move here/Select all**: in non-main windows they forward the
  native selector via `send_native_edit_action()`, or ⌘A and the clipboard die in settings/viewer text fields. ❌ Not
  `PredefinedMenuItem::select_all`: it conflicts. Predefined items need explicit text (muda's is English).
- **Linux omits F-key, Tab, Space, and `Cmd+Plus`/`Cmd+Minus` accelerators** (GTK intercepts them); JS keydown
  dispatches them there.
- **Every label comes from `menu_t("menu.…")`, ❌ never a literal.** `rebuild.rs` rebuilds the bar on a language change
  and emits `menu-bar-rebuilt` so the frontend re-pushes what only it knows. Linux mnemonics are ALLOCATED per submenu
  from the translated labels.
- **Trailing `…` means the dialog can change WHAT the command acts on** (`Copy…` takes a destination), not that it
  merely confirms (`Delete`). Always U+2026 (`menu_labels_end_with_the_ellipsis_character`). `DETAILS.md`.
- **Menus and items are keyed by ID, never title** (translation moves titles); `macos_appkit.rs` resolves IDs to live
  titles only at the AppKit boundary. Rebinding replaces an item, so `update_menu_accelerator` re-applies its icon.

Architecture, flows, decisions: `DETAILS.md`. Read it before any non-trivial work here.
