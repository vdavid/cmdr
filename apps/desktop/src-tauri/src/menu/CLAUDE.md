# Menu system

Native menu bars for macOS and Linux, built from scratch in the user's language.

## Module map

- `mod.rs` (shared types and menu state), `command_map.rs` (item IDs + the two id↔command maps).
- `menu_items.rs` / `menu_structure.rs` build and assemble the pieces; `macos.rs` / `linux.rs` lay out each platform's
  bar; `macos_appkit.rs` is the objc2 boundary the three right-click extras cross (`services_context.rs`,
  `share_submenu.rs`, `context_menu_icons.rs`). One line per file: DETAILS § File layout.

## Must-knows

- **Build menus from scratch; never `Menu::default()`.** It inherits OS-injected Edit items that can't be removed
  before display; `cleanup_macos_menus` only strips what AppKit injects *after* it.
- **The three macOS right-click extras, all `DETAILS.md`, and ❗ every loan must outlive `popup()` — ❌ never
  `let _ =`.** `Services`: AppKit owns ONE such menu and the app menu already has it, so `ServicesLoan` borrows it,
  aimed at the RIGHT-CLICKED rows. `Share`: ours, built from `file_system/share.rs`'s enumeration, ids
  `share-service:<index>`, closing with `Edit extensions`, ❌ never empty (no service, no item). SF Symbols: set on
  `NSMenuDidBeginTrackingNotification`, ❌ never through `IconMenuItem` — Tauri exposes no `NSMenu` here and muda's
  bitmaps can't be TEMPLATE images, so a glyph would vanish in one appearance and go dark when highlighted.
  `IconMenuItem` stays right for real pixels (app, share, tag icons).
- **Accelerator changes go remove/recreate/reinsert** (Tauri has no `set_accelerator()`), and `MenuState` tracks each
  item's submenu and index — so **adding or moving one item shifts every later `register_item` index**, mangling a
  different item on the first rebind. `register_item_positions_match_submenu_order` catches it, which is why both
  platform files keep near-identical blocks. Keep those position comments truthful.
- **CheckMenuItems (view modes, show hidden) must NOT use `"execute-command"`**: they auto-toggle on click, so
  emitting it too double-toggles. They emit `"settings-changed"` / `"view-mode-changed"`; sort emits `"menu-sort"`;
  close-tab and "Open with" have own paths.
- **File-scoped commands are dual-guarded**: `activate_window_menu("other")` greys them out (visual only); the real
  guard is `main_window.is_focused()` in `on_menu_event`. Accelerators fire even when items look off.
- **`OPERATION_START_ITEM_IDS` greys out while a dialog is up or Ask Cmdr has focus**, and `set_menu_context`
  re-applies it LAST, or a focus round-trip re-offers Copy. ❌ Every gated id must be `FileScoped`: greying `App`-scoped
  `Edit > Paste` kills ⌘V elsewhere. `src/lib/file-explorer/pane/DETAILS.md` § "The operation-start gate".
- **macOS swaps the app menu bar on focus-gain (`activate_window_menu`); Linux uses per-window menus.** One app-level
  bar, so each window's focus handler `app.set_menu()`s between main and viewer. Re-run `cleanup_macos_menus` after
  every swap, and `set_macos_menu_icons` on the way back to main (SF Symbols don't survive it). `window.set_menu()` is
  a macOS no-op.
- **Custom (not Predefined) MenuItems for Cut/Copy/Paste/Move here/Select all**: in non-main windows they forward the
  native selector via `send_native_edit_action()`, or ⌘A and the clipboard are dead in settings/viewer text fields. ❌
  Don't swap to `PredefinedMenuItem::select_all`: it conflicts. Predefined items need explicit text (muda's is English).
- **Linux omits F-key, Tab, Space, and `Cmd+Plus`/`Cmd+Minus` accelerators** (GTK intercepts them); JS keydown
  dispatches those there.
- **Every label comes from `menu_t("menu.…")`, ❌ never a literal.** `rebuild.rs` rebuilds the bar when the language
  moves and emits `menu-bar-rebuilt` so the frontend re-pushes what only it knows. Linux mnemonics are ALLOCATED per
  submenu from the translated labels: a free letter depends on the language.
- **Trailing `…` means the dialog can change WHAT the command acts on** (`Copy…` takes a destination), not merely that
  it confirms (`Delete`). Always U+2026 (`menu_labels_end_with_the_ellipsis_character`). Verdicts: `DETAILS.md`.
- **Menus and items are keyed by ID, never title** (titles get translated); `macos_appkit.rs` resolves IDs to live
  titles only at the AppKit boundary. Rebinding a shortcut replaces an item, so `update_menu_accelerator` re-applies
  its icon.

Architecture, flows, decisions: `DETAILS.md`. Read it before any non-trivial work here.
