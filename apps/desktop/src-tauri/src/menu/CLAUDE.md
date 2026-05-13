# Menu system

Native menu bar for macOS and Linux. Builds platform-specific menus from scratch, handles menu
events, syncs accelerator labels with user-customized shortcuts, and enables/disables items based on
window focus context.

## File layout

- `mod.rs` — shared types (`MenuState`, `MenuItems`, `MenuItemEntry`, `CommandScope`, `ViewMode`,
  `FileContextInfo`, `ContextMenuResult`), constants (all menu item IDs), ID mapping functions
  (`menu_id_to_command`, `command_id_to_menu_id`), platform-aware accelerator/label helpers, the
  `build_menu` dispatcher, context menu builders, viewer menu builder, and accelerator update
  functions.
- `macos.rs` — `build_menu_macos` (full macOS menu bar), `cleanup_macos_menus` (removes
  system-injected Edit items, registers Help menu), `set_macos_menu_icons` (SF Symbol icons via
  objc2 FFI), and their helpers.
- `open_with.rs` (macOS) — `build_open_with_submenu` for the file context menu's "Open with"
  submenu. Returns the submenu plus a `bundle_id → app_path` map that callers stash in
  `MenuState.context.open_with_apps` so `on_menu_event` can resolve dynamic `open-with:<bundle-id>`
  click targets.
- `linux.rs` — `build_menu_linux` (full Linux/GTK menu bar with mnemonics, no F-key accelerators).

## Key concepts

### Unified dispatch

Menu clicks route through a single `"execute-command"` Tauri event. `on_menu_event` (in `lib.rs`)
looks up the clicked menu item ID via `menu_id_to_command()`, which returns the command registry ID
and a `CommandScope` (App or FileScoped). File-scoped commands check `main_window.is_focused()`
before emitting. The frontend has one listener that calls `handleCommandExecute(commandId)`.

Exceptions that do NOT use `"execute-command"`:
- **CheckMenuItems** (show hidden files, view modes): toggling checked state + emitting would
  double-toggle, so these emit `"settings-changed"` / `"view-mode-changed"` directly
- **Close tab** (⌘W): checks if a non-main window has focus and closes it instead of emitting
  `tab.close`
- **Sort items**: emit `"menu-sort"` with field/direction payload. The four shortcut-bound columns
  (`SORT_BY_{NAME,EXTENSION,MODIFIED,SIZE}_ID`) are *also* listed in `menu_id_to_command` and
  registered in the items HashMap — purely so user-customized accelerators flow through the
  generic update path. The on_menu_event special-case fires first, so the generic dispatch is
  never reached at click time.
- **Tab context menu**: emits specific tab action events with tab index payload
- **Open with** (macOS): items have dynamic IDs like `open-with:com.apple.Xcode` that can't be
  enumerated in `menu_id_to_command`. `on_menu_event` prefix-matches `open-with:` and calls
  `file_system::open_with::open_paths_with` directly, looking up the app URL via
  `MenuState.context.open_with_apps[bundle_id]` and the launch paths via
  `MenuState.context.paths`. The "Other..." entry shows an `NSOpenPanel` filtered to `.app`
  bundles and launches the chosen app the same way.

### MenuState

Shared state managed via `tauri::State<MenuState<Wry>>`. Holds:
- Named `CheckMenuItem` references (`show_hidden_files`, plus four per-pane view-mode items:
  `view_mode_full_left/right` and `view_mode_brief_left/right`) for checked-state sync
- `pin_tab` MenuItem reference for dynamic label changes ("Pin tab" / "Unpin tab")
- `view_left_pane_submenu` / `view_right_pane_submenu` — the two pane-scoped submenus that hold
  the Full/Brief CheckMenuItems (Full at position 0, Brief at position 1). Used by
  `rebuild_view_mode_items` to remove/recreate/reinsert items when accelerators move on focus change.
- Cached view-mode state (`view_mode_active_pane`, `view_mode_left`, `view_mode_right`,
  `view_mode_full_accel`, `view_mode_brief_accel`) used by `rebuild_view_mode_items` to
  attach the keyboard accelerator only to the currently-active pane's pair
- `items: HashMap<String, MenuItemEntry>` for the ~20 regular MenuItems that need accelerator
  updates and enable/disable
- `context: MenuContext` for right-click context menu — `path` (primary right-clicked file),
  `filename`, `paths` (full selection if the right-clicked file is part of it, else `[path]`),
  and (macOS) `open_with_apps` (`bundle_id → app_path` map populated when "Open with" submenu
  is built, consumed by `on_menu_event` on click)

### Accelerator sync

Menu accelerators must match user-customized shortcuts. Since Tauri has no `set_accelerator()` API,
updating an accelerator requires removing the old item, creating a new one with the new accelerator,
and reinserting at the same position. `update_menu_item_accelerator()` handles regular items via the
HashMap; `rebuild_view_mode_items()` handles the four per-pane view-mode CheckMenuItems together
because they share a single accelerator pair (⌘1 / ⌘2 by default) that "follows" the active pane.

The frontend triggers regular-item updates via `invoke('update_menu_accelerator')` from
`shortcuts-store.ts`, and triggers view-mode rebuilds via `invoke('update_view_mode_menu')` from
`DualPaneExplorer.svelte` on focus change, swap, and any view-mode toggle.

### Per-pane view modes

The View menu nests two pane-scoped submenus: `View > Left pane > {Full view, Brief view}` and
`View > Right pane > {Full view, Brief view}`. Both pairs of `CheckMenuItem`s always exist; only
the **active** pane's pair carries the keyboard accelerator (⌘1/⌘2 by default). When focus
switches between panes, the frontend pushes `update_view_mode_menu(activePane, leftMode, rightMode)`,
and the backend's `rebuild_view_mode_items` removes and recreates the items inside their parent
pane submenu so the accelerator visibly migrates to the newly-active pair. This makes the per-pane
scope discoverable while keeping ⌘1/⌘2 as a focus-aware shortcut for the active pane.

Click-on-inactive-pane works without changing focus: opening `View > Right pane > Brief view`
while the left pane is active emits `view-mode-changed` with `pane: "right"`, and the frontend
updates the right pane's mode without touching focus. The frontend then pushes
`update_view_mode_menu` so the check states stay consistent.

### Context-aware enable/disable

`set_menu_context("explorer" | "other")` enables/disables file-scoped menu items. Called by the
frontend when Settings or file viewer gains/loses focus. Iterates the `items` HashMap and sets
`enabled` on each. This is a visual hint reinforcing the focus guard in `on_menu_event`.

### macOS cleanup (objc2)

`cleanup_macos_menus()` runs post-construction via objc2 FFI:
1. Removes system-injected Edit items (Writing Tools, AutoFill, Dictation, Emoji & Symbols)
2. Registers the Help menu via `NSApplication.setHelpMenu:` so macOS adds the search field

Uses `objc2::exception::catch` because NSMenu operations can raise ObjC exceptions inside Tauri's
`did_finish_launching` callback, which aborts on panic.

### SF Symbol icons (macOS only)

`set_macos_menu_icons()` runs post-construction via objc2 FFI, walking
`NSApplication.mainMenu()` and calling `NSImage(systemSymbolName:)` + `setImage:` on each
`NSMenuItem` matched by title. This produces true template images that auto-tint on
selection highlighting. Also handles nested submenus (Sort by) via
`apply_sf_symbols_to_nested_submenu`.

Context menus don't get SF Symbols for our own items — Tauri doesn't expose the raw `NSMenu`
pointer for context menus, and rasterized SF Symbol bitmaps via `IconMenuItem` look poor (no
template auto-tinting). However, **full-color non-template images do render correctly** through
`IconMenuItem`, and that's what the "Open with" submenu uses for app-bundle icons (loaded via
`file_system::open_with::load_app_icon` from the `.icns` in each app's `Contents/Resources`).

## Platform differences

| Aspect | macOS | Linux |
|--------|-------|-------|
| App menu | Dedicated "cmdr" menu with About, License, Settings | No app menu; About under Help, Settings/License under Edit |
| Predefined items | Hide, Hide Others, Show All, Quit, Window items, Undo/Redo | None (GTK has no equivalent) |
| Accelerators | Full set | Omitted for F2 (Rename) and others with GTK interception issues |
| Mnemonics | Not used | `&` prefixes for GTK keyboard navigation, unique per submenu |
| Help search | Native NSMenu search field via `setHelpMenu:` | Not available |
| System cleanup | objc2 strips injected Edit items | Not needed |
| Menu icons | SF Symbols via objc2 (menu bar) and IconMenuItem (context menus) | Not supported |

## Menu structure

Both platforms share: File, Edit, View (with Sort by and Zoom submenus), Go, Tab, Help.

The **Zoom** submenu (`build_zoom_submenu`) holds the text-size presets (75/100/125/150 %) plus Zoom in (`Cmd+Plus`) /
Zoom out (`Cmd+Minus`) / 100 % (`Cmd+0`). Items are `App`-scoped so the keyboard accelerators fire in any focused window.
Linux skips the in/out accelerators because GTK intercepts `Cmd+Plus` / `Cmd+Minus` at the toolkit level — the JS
shortcut dispatch path covers Linux.
macOS adds: cmdr (app menu), Window. See the menu item ID constants in `mod.rs` for the full item list.

Viewer windows get a minimal menu: File (Close), Edit (clipboard), View (Word wrap), and on macOS
also Window and Help.

## Mapping functions

- `menu_id_to_command(id) -> Option<(command_id, CommandScope)>`: menu item ID to command registry
- `command_id_to_menu_id(id) -> Option<menu_item_id>`: reverse lookup for accelerator updates
- Both are exhaustive match statements kept in sync manually

## Key decisions

**Decision**: Build all menus from scratch instead of patching `Menu::default()`.
**Why**: `Menu::default()` inherits OS-injected items (Edit: Writing Tools, AutoFill, Dictation on macOS) that are irrelevant to a file manager and can't be reliably removed before display. Building from scratch gives full control over every item. The cleanup pass via objc2 (`cleanup_macos_menus`) handles only items injected *after* construction by AppKit.

**Decision**: Route most menu clicks through a single `"execute-command"` Tauri event with a command registry ID.
**Why**: The frontend already has a unified command dispatch system (keyboard shortcuts, command palette, MCP tools all use it). Routing menu clicks through the same path avoids duplicating command handling logic. The few exceptions (CheckMenuItems, sort, close-tab) exist because they need side effects *before* or *instead of* the generic emit (toggling checked state, attaching payloads, or closing non-main windows).

**Decision**: Accelerator updates via remove/recreate/reinsert instead of in-place mutation.
**Why**: Tauri's menu API has no `set_accelerator()` method. The only way to change a displayed accelerator is to destroy the old `MenuItem`, create a new one with the new accelerator string, and reinsert it at the same position in the parent submenu. This is why `MenuState` tracks both the `Submenu` reference and the positional index for every updatable item.

**Decision**: Omit F-key and Tab/Space accelerators on Linux.
**Why**: GTK intercepts F2-F8, Tab, and Space at the toolkit level before events reach the webview. Registering them as menu accelerators causes double-handling or silent swallowing. On Linux these keys are dispatched purely through JS keydown handlers, bypassing the native menu system entirely.

**Decision**: Dual enable/disable guard -- `set_menu_context` (visual) + `is_focused()` check (behavioral).
**Why**: `set_menu_context("other")` greys out file-scoped items so users see they're unavailable, but this is a visual hint only. The real guard is in `on_menu_event`, which checks `main_window.is_focused()` before emitting file-scoped commands. Both layers are needed because menu accelerators fire even when items appear disabled on some platforms.

**Decision**: CheckMenuItems (view modes, show hidden) use separate event paths instead of `"execute-command"`.
**Why**: CheckMenuItems auto-toggle their checked state on click. If the click also emitted `"execute-command"` and the frontend toggled the setting, the state would double-toggle (menu toggles once, frontend toggles again). Instead, these items emit `"settings-changed"` or `"view-mode-changed"` directly, treating the menu click as the authoritative state change.

**Decision**: Per-pane View submenus (`View > Left pane > …`, `View > Right pane > …`) with the accelerator following the active pane.
**Why**: The previous single Full/Brief pair always targeted the active pane, but that scope was invisible in the menu — testers were slow to figure out how to change the inactive pane's view. Nesting each pane's Full/Brief items inside its own submenu makes the scope obvious without cluttering the View root. The accelerator is attached only to the active pane's pair (and migrates on focus change via `rebuild_view_mode_items`) so the shortcut remains accurate — pressing ⌘1 always affects the active pane, and the visible binding sits next to the items it actually targets.

**Decision**: SF Symbol icons only on the menu bar, not on context menus.
**Why**: Tauri doesn't support SF Symbols natively. For the menu bar, we walk `NSApplication.mainMenu()` post-construction via objc2 FFI and set SF Symbols directly on `NSMenuItem` objects — this produces true template images that auto-tint correctly. Context menus don't get icons because Tauri doesn't expose the raw `NSMenu` pointer, and the alternative (rasterized bitmaps via `IconMenuItem`) produces visually poor results (no template tinting, wrong size/weight).

## Gotchas

- **No `Menu::default()`**: Both platforms build from scratch. The old approach inherited system
  defaults that added unwanted items.
- **Tab as accelerator**: Switch pane uses Tab, which could conflict with menu bar accessibility
  navigation. If issues arise, omit the accelerator and rely on JS dispatch.
- **Custom MenuItems for Cut/Copy/Paste**: The Edit menu uses custom MenuItems (not
  PredefinedMenuItems) for Cut, Copy, Paste, and Move here. In `on_menu_event`, these are handled
  specially: if the main window is focused, they route through `execute-command` so the frontend can
  decide between file clipboard and text clipboard (via `document.activeElement` check). If a
  non-main window is focused (viewer, settings), `send_native_clipboard_action()` in `lib.rs` sends
  the native `copy:`/`cut:`/`paste:` selector through the responder chain via
  `NSApplication.sendAction:to:from:` — replicating what PredefinedMenuItems do internally. This
  ensures text clipboard works natively in all windows. Undo and Redo remain PredefinedMenuItems
  since they only apply to text fields.
- **⌘A dual routing**: "Select all" uses ⌘A as a native menu accelerator (so it's visible in the
  Edit menu). Since macOS intercepts it before the webview, the frontend's `handleCommandExecute`
  checks `document.activeElement` — if it's an input/textarea, it calls `.select()` for text
  selection; otherwise it selects files. This avoids PredefinedMenuItem::select_all which would
  conflict with the custom MenuItem.
- **Pin tab label**: `pin_tab` in MenuState is updated dynamically by the frontend to show
  "Pin tab" or "Unpin tab" based on the active tab's state.
