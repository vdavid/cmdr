# Menu system

Native menu bars for macOS and Linux, built from scratch in the user's language.

## Module map

`menu_bar.rs` holds both bars as data (vocabulary: `menu_spec.rs`; builder: `menu_bar_builder.rs`),
`file_context_menu.rs` the file right-click menu, `menu_structure.rs` the smaller context menus plus the viewer menu,
`macos_appkit.rs` the objc2 boundary every right-click extra crosses. One line per file: DETAILS § File layout.

## Must-knows

- **Build from scratch; ❌ never `Menu::default()`**: it inherits OS-injected Edit items nothing can remove before
  display.
- **❗ A macOS loan must outlive `popup()`; ❌ never `let _ =`** (Services, context icons, header, tag row, live menu).
- **The file context menu asks nothing slow on the main thread**: disk, provider, and LaunchServices facts go through
  `context_menu_facts.rs` (100 ms grace), and late ones land via `context_menu_live.rs`. While it's up, ❌ never touch
  the top-level `Menu` (muda has it borrowed: panic) or insert above rows (the highlight moves). DETAILS § Slow facts.
- **Every menu image goes through ONE door, `set_menu_item_image`**, because macOS 27 hides an item's image unless the
  item opts in, silently (it reads back non-nil, nothing draws). Clippy refuses `NSMenuItem::setImage` and
  `IconMenuItem` elsewhere. A context-menu image is an `ItemImage` in `context_menu_icons::image_runs`.
- **A popup accelerator is a LABEL the payload carries, ❌ never a literal**: `context_item()` resolves its own menu id
  through `frontend_shortcut_to_menu_text`, ❌ not the bar's floored `frontend_shortcut_to_accelerator`.
- **A BAR combo with no ⌘/⌃/⌥, or one whose key muda can't name (`+`, `*`, `ö`), is DISPLAYED, never registered** —
  `⇧8` IS `*`, and AppKit fires a registered one app-wide. `displayed_item` carries the glyph. ❗ `setAttributedTitle:`
  also rewrites `title`, so `find_ns_item` matches only to the first TAB.
- **Accelerator changes go remove/recreate/reinsert**, so `MenuState` tracks each item's submenu and index; a live
  label moves through `MenuState::set_item_label`, ❌ never a bare `set_text`.
- **Four families skip `"execute-command"`**: CheckMenuItems auto-toggle (it would double-toggle) and send
  `"settings-changed"` / `"view-mode-changed"`; sort sends `"menu-sort"`; Cut / Copy / Paste / Move here / Select all
  are Custom (❌ not Predefined) items forwarding the native selector via `send_native_edit_action()` outside the main
  window, or ⌘A and the clipboard die there; all four of the VIEWER bar's are Custom too, on their own `VIEWER_*` ids —
  Copy / Select all emit `ViewerEditAction` to the viewer in front (a native selector would grab its status bar), while
  Cut / Paste forward the selector to its search box, ❗ Predefined off macOS or ⌘X / ⌘V die there.
- **Enabled state has ONE writer, `apply_menu_item_states`**: store a new input and add it to `menu_item_enabled`, ❌
  never a direct `set_enabled`. The viewer bar's Cut / Paste are in there too, following its search box
  (`viewer_search_focus`). Greying is chrome — the real guard is `main_window.is_focused()` in `on_menu_event`;
  accelerators fire even when items look off.
- **macOS swaps ONE app-level bar on focus-gain (`activate_window_menu`)**; Linux uses per-window menus, and
  `window.set_menu()` is a macOS no-op. ❗ `cleanup_macos_menus`, `set_macos_menu_icons`, and `set_display_accelerators`
  survive nothing: re-apply all three after every swap, rebuild, and recreate.
- **Every label comes from `menu_t("menu.…")`, ❌ never a literal** (muda's Predefined text is English), **and
  everything is keyed by ID, never title**. A language change rebuilds the whole bar and emits `menu-bar-rebuilt`, so
  whatever the frontend pushed must be pushed again.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
