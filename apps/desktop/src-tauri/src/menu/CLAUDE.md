# Menu system

Native menu bar for macOS and Linux: builds platform menus from scratch in the user's language, handles events, syncs
accelerators, and enables items by focus.

## Module map

- `mod.rs` (shared types, enums, events, menu state), `command_map.rs` (item IDs + the two id↔command maps).
- `menu_items.rs` / `menu_structure.rs`: piece builders and assembly. `install.rs`: the startup build.
  `menu_handlers.rs`: clicks; `accelerators.rs` and `view_mode_items.rs`: live updates. `media_index_items.rs`: the
  image-search-items decider. `macos.rs` / `linux.rs`: platform menu bars, assembling shared pieces around their
  layouts. `macos_appkit.rs`: the objc2 fix-up passes. `open_with.rs`: the macOS "Open with" submenu. `rebuild.rs`:
  rebuilding in a new language. `mnemonics.rs`: the Linux underline-letter allocator.

## Must-knows

- **Build menus from scratch; never `Menu::default()`.** It inherits OS-injected Edit items (Writing Tools, AutoFill,
  Dictation) that can't be removed before display. `cleanup_macos_menus` only strips what AppKit injects *after* it.
- **`cleanup_macos_menus` also hands the Services item AppKit's own menu**, or `Cmdr > Services` stays near-empty. ❌
  `NSApplication.setServicesMenu:` isn't the simplification it looks like: ignored once AppKit owns one.
  `../services_menu/CLAUDE.md`.
- **Accelerator changes go remove/recreate/reinsert, not in-place** (Tauri has no `set_accelerator()`). `MenuState`
  tracks each item's submenu and index, so **adding or moving one item shifts every `register_item` index after it**,
  mangling a different item on the first rebind. `register_item_positions_match_submenu_order` fails on a mismatch, and
  it parses both platform files, which is why their near-identical blocks stay. Keep the position comments truthful.
- **CheckMenuItems (view modes, show hidden) must NOT use `"execute-command"`.** They auto-toggle on click, so emitting
  it too double-toggles; they emit `"settings-changed"` / `"view-mode-changed"`. Sort emits `"menu-sort"`; close-tab and
  "Open with" have own paths. `DETAILS.md`.
- **File-scoped commands are dual-guarded**: `activate_window_menu("other")` greys them out (visual only); the real
  guard is `main_window.is_focused()` in `on_menu_event`. Accelerators fire even when items look off.

- **`OPERATION_START_ITEM_IDS` greys out while a dialog is up or Ask Cmdr has focus**, and `set_menu_context` re-applies
  it LAST (its loop enables every explorer item, so a focus round-trip would re-offer Copy). Chrome only. ❌ Every gated
  id must be `FileScoped`: greying `App`-scoped `Edit > Paste` kills ⌘V elsewhere.
  `src/lib/file-explorer/pane/DETAILS.md` § "The operation-start gate".
- **macOS swaps the app menu bar on focus-gain (`activate_window_menu`); Linux uses per-window menus.** One app-level
  bar, so each window's focus handler swaps `app.set_menu()` between main and viewer. Re-run `cleanup_macos_menus` after
  every swap, and re-apply `set_macos_menu_icons` swapping back to main (SF Symbols don't survive it).
  `window.set_menu()` is a macOS no-op. `DETAILS.md`.
- **Custom (not Predefined) MenuItems for Cut/Copy/Paste/Move here/Select all**: in non-main windows they forward the
  native selector via `send_native_edit_action()`, or ⌘A and the clipboard are dead in settings/viewer text fields. ❌
  Don't swap to `PredefinedMenuItem::select_all`: it conflicts. Predefined items take explicit text (muda's is English).
- **`Select all` / `Deselect all` live in `Select`, not `Edit`**: they act on files, not text (`DETAILS.md`).
- **Linux omits F-key, Tab, Space, and `Cmd+Plus`/`Cmd+Minus` accelerators** (GTK intercepts them); they dispatch
  through JS keydown there.
- **Every label comes from `menu_t("menu.…")`, ❌ never a literal.** `rebuild.rs` rebuilds the whole bar when the
  language moves, re-running cleanup + icons and emitting `menu-bar-rebuilt` so the frontend re-pushes what only it
  knows. Linux mnemonics are ALLOCATED per submenu from the translated labels: a free letter depends on the language.
- **Trailing `…` means the dialog can change WHAT the command acts on** (`Copy…` takes a destination), not merely that
  it confirms (`Delete`). Always U+2026 (`menu_labels_end_with_the_ellipsis_character`). Verdicts: `DETAILS.md`.
- **Menus and items are keyed by ID, never title** (titles get translated); `macos_appkit.rs` resolves IDs to live
  titles only at the AppKit boundary. Rebinding a shortcut replaces an item, so `update_menu_accelerator` re-applies its
  icon.

Architecture, flows, decisions: `DETAILS.md`. Read it before any non-trivial work here.
