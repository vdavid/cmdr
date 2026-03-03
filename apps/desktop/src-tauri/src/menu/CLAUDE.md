# Menu system

Native menu bar for macOS and Linux. Builds platform-specific menus from scratch, handles menu
events, syncs accelerator labels with user-customized shortcuts, and enables/disables items based on
window focus context.

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
- **Sort items**: emit `"menu-sort"` with field/direction payload
- **Tab context menu**: emits specific tab action events with tab index payload

### MenuState

Shared state managed via `tauri::State<MenuState<Wry>>`. Holds:
- Named `CheckMenuItem` references (`show_hidden_files`, `view_mode_full`, `view_mode_brief`) for
  checked-state sync
- `pin_tab` MenuItem reference for dynamic label changes ("Pin tab" / "Unpin tab")
- `view_submenu` + position indices for view mode accelerator updates (remove/recreate/reinsert)
- `items: HashMap<String, MenuItemEntry>` for the ~20 regular MenuItems that need accelerator
  updates and enable/disable
- `context: MenuContext` for right-click context menu (path, filename)

### Accelerator sync

Menu accelerators must match user-customized shortcuts. Since Tauri has no `set_accelerator()` API,
updating an accelerator requires removing the old item, creating a new one with the new accelerator,
and reinserting at the same position. `update_menu_item_accelerator()` handles regular items via the
HashMap; `update_view_mode_accelerator()` handles CheckMenuItems separately to preserve checked state.

The frontend triggers this via `invoke('update_menu_accelerator')` from `shortcuts-store.ts`.

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

## Platform differences

| Aspect | macOS | Linux |
|--------|-------|-------|
| App menu | Dedicated "cmdr" menu with About, License, Settings | No app menu; About under Help, Settings/License under Edit |
| Predefined items | Hide, Hide Others, Show All, Quit, Window items (no clipboard/Edit PredefinedMenuItems) | None (GTK has no equivalent) |
| Accelerators | Full set | Omitted for F2 (Rename) and others with GTK interception issues |
| Mnemonics | Not used | `&` prefixes for GTK keyboard navigation, unique per submenu |
| Help search | Native NSMenu search field via `setHelpMenu:` | Not available |
| System cleanup | objc2 strips injected Edit items | Not needed |

## Menu structure

Both platforms share: File, Edit, View (with Sort by submenu), Go, Tab, Help.
macOS adds: cmdr (app menu), Window. See `docs/specs/native-menus-plan.md` for the full item table.

Viewer windows get a minimal menu: File (Close), Edit (clipboard), View (Word wrap), and on macOS
also Window and Help.

## Mapping functions

- `menu_id_to_command(id) -> Option<(command_id, CommandScope)>`: menu item ID to command registry
- `command_id_to_menu_id(id) -> Option<menu_item_id>`: reverse lookup for accelerator updates
- Both are exhaustive match statements kept in sync manually

## Gotchas

- **No `Menu::default()`**: Both platforms build from scratch. The old approach inherited system
  defaults that added unwanted items.
- **Tab as accelerator**: Switch pane uses Tab, which could conflict with menu bar accessibility
  navigation. If issues arise, omit the accelerator and rely on JS dispatch.
- **⌘A for Select all**: PredefinedMenuItems (Undo, Redo, Cut, Copy, Paste, Select All) were removed
  from the Edit menu — they are irrelevant to a file manager. "Select all" uses ⌘A as its native
  menu accelerator, routed through `execute-command` → `handleCommandExecute`.
- **Pin tab label**: `pin_tab` in MenuState is updated dynamically by the frontend to show
  "Pin tab" or "Unpin tab" based on the active tab's state.
