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
| Predefined items | Hide, Hide Others, Show All, Quit, Window items, Undo/Redo | None (GTK has no equivalent) |
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

## Gotchas

- **No `Menu::default()`**: Both platforms build from scratch. The old approach inherited system
  defaults that added unwanted items.
- **Tab as accelerator**: Switch pane uses Tab, which could conflict with menu bar accessibility
  navigation. If issues arise, omit the accelerator and rely on JS dispatch.
- **Custom MenuItems for Cut/Copy/Paste**: The Edit menu uses custom MenuItems (not
  PredefinedMenuItems) for Cut, Copy, Paste, and Move here. This routes ⌘C/⌘V/⌘X through
  `execute-command` dispatch so the frontend can decide between text clipboard (when an input is
  focused) and file clipboard (when the file list has focus). Text clipboard is handled via
  `document.execCommand` / `navigator.clipboard` API in the frontend handler. Undo and Redo remain
  PredefinedMenuItems since they only apply to text fields.
- **⌘A dual routing**: "Select all" uses ⌘A as a native menu accelerator (so it's visible in the
  Edit menu). Since macOS intercepts it before the webview, the frontend's `handleCommandExecute`
  checks `document.activeElement` — if it's an input/textarea, it calls `.select()` for text
  selection; otherwise it selects files. This avoids PredefinedMenuItem::select_all which would
  conflict with the custom MenuItem.
- **Pin tab label**: `pin_tab` in MenuState is updated dynamically by the frontend to show
  "Pin tab" or "Unpin tab" based on the active tab's state.
