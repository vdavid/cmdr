# Menu system

Native menu bar for macOS and Linux: builds platform menus from scratch in the user's language, handles events, syncs
accelerators to user shortcuts, and enables items by focus context.

## Module map

- `mod.rs` (shared types, enums, event structs, menu state), `command_map.rs` (item IDs + the `menu_id_to_command` /
  `command_id_to_menu_id` maps).
- `menu_items.rs` / `menu_structure.rs`: piece builders and hierarchical assembly. `menu_handlers.rs`: events and
  live-update helpers. `media_index_items.rs`: the image-search-items decider. `macos.rs` / `linux.rs`: platform menu
  bars, assembling shared pieces (`build_sort_submenu`, `build_zoom_submenu`, `build_view_mode_items`) around their own
  layouts. `macos_appkit.rs`: the objc2 passes that fix the bar up. `open_with.rs`: the macOS "Open with" submenu.
  `rebuild.rs`: rebuilding the bar in a new language.

## Must-knows

- **Build menus from scratch; never `Menu::default()`.** It inherits OS-injected Edit items (Writing Tools, AutoFill,
  Dictation) that can't be removed before display. `cleanup_macos_menus` only strips what AppKit injects *after* it.
- **Accelerator changes go remove/recreate/reinsert, not in-place** (Tauri has no `set_accelerator()`). `MenuState`
  tracks each item's submenu and index, so **adding or moving one item shifts every `register_item` index after it**,
  mangling a different item on the first rebind. `register_item_positions_match_submenu_order` fails on a mismatch; keep
  the position comments truthful. It parses both platform files, which is why their near-identical `register_item`
  blocks stay put (`DETAILS.md`).
- **CheckMenuItems (view modes, show hidden) must NOT use `"execute-command"`.** They auto-toggle on click, so emitting
  it too double-toggles; they emit `"settings-changed"` / `"view-mode-changed"` directly. Sort emits `"menu-sort"`;
  close-tab and "Open with" have own paths. `DETAILS.md`.
- **File-scoped commands are dual-guarded**: `activate_window_menu("other")` greys them out (visual only); the real
  guard is `main_window.is_focused()` in `on_menu_event`. Accelerators fire even when items look off.
- **`OPERATION_START_ITEM_IDS` greys out while a dialog is up or Ask Cmdr has focus**, and `set_menu_context` re-applies
  it LAST (its loop enables every explorer item, so a focus round-trip would re-offer Copy). Chrome only. ❌ Every gated
  id must be `FileScoped`: greying `App`-scoped `Edit > Paste` would kill ⌘V elsewhere. Test-pinned;
  `src/lib/file-explorer/pane/DETAILS.md` § "The operation-start gate".
- **macOS swaps the app menu bar on focus-gain (`activate_window_menu`); Linux uses per-window menus.** One app-level
  bar, so each window's focus handler swaps `app.set_menu()` between the main and viewer menus. Re-run
  `cleanup_macos_menus` after every swap (Edit items get re-injected), and re-apply `set_macos_menu_icons` swapping back
  to main (SF Symbols don't survive it). `window.set_menu()` is a macOS no-op. `DETAILS.md`.
- **Custom (not Predefined) MenuItems for Cut/Copy/Paste/Move here/Select all**: in non-main windows these forward the
  native `copy:`/`cut:`/`paste:`/`selectAll:` selector via `send_native_edit_action()`; without it ⌘A and clipboard are
  dead in settings/viewer text fields. ❌ Don't swap to `PredefinedMenuItem::select_all`: it conflicts. The predefined
  items we DO use take explicit text; muda hardcodes English.
- **`Select all` / `Deselect all` live in `Select`, not `Edit`**: they act on files, not text. `DETAILS.md`.
- **Linux omits F-key, Tab, Space, and `Cmd+Plus`/`Cmd+Minus` accelerators** (GTK intercepts them); those keys dispatch
  through JS keydown there instead.
- **Every label comes from `menu_t("menu.…")`, ❌ never a literal.** `rebuild.rs` rebuilds the whole bar when the
  language moves, re-running cleanup + icons and emitting `menu-bar-rebuilt` so the frontend re-pushes what only it
  knows. Linux mnemonics are ALLOCATED per submenu from the translated labels (`Mnemonics`): a free letter depends on
  the language.
- **Trailing `…` means the dialog can change WHAT the command acts on** (`Copy…` takes a destination), not merely that
  it confirms (`Delete`). Always U+2026, never `...` (enforced by `menu_labels_end_with_the_ellipsis_character`).
  Per-item verdicts: `DETAILS.md`.
- **Menus and items are keyed by ID, never title** (titles get translated): `macos_appkit.rs` resolves IDs to live
  titles only at the AppKit boundary. Rebinding a shortcut replaces an item, so `update_menu_accelerator` re-applies
  its icon.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here.
