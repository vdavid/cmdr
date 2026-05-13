# Sort order shortcuts

Research and decisions behind the `Cmd+3..6` / `Cmd+F3..F6` shortcuts that change the sort column.

## What other file managers do

| App             | Shortcut style | Mapping                                                                                             |
| --------------- | -------------- | --------------------------------------------------------------------------------------------------- |
| Commander One   | `⌥⌘1..9`       | Name, Extension, Size, Date Modified, Date Created, Date Added, Date Last Opened, Kind, Permissions |
| Total Commander | `Ctrl+F3..F7`  | Name, Extension, Time, Size, Unsorted                                                               |
| ForkLift        | `⌃⌘1..n`       | Custom set per column                                                                               |
| Finder          | `⌃⌥⌘1..6`      | Name, Kind, Last Opened, Added, Modified, Size                                                      |
| Cmdr (was)      | `⌘1`/`⌘2`      | Full view / Brief view (view mode, not sort)                                                        |

## Decisions

### Primary shortcuts: `⌘3..6`

- `⌘3` — Name
- `⌘4` — Extension (not "Kind" — that's an abstraction over extension that adds no value and is often wrong)
- `⌘5` — Date modified
- `⌘6` — Size

Why this set, this order:

- Builds on our existing `⌘1`/`⌘2` (Full/Brief view) — keeps the number-key block coherent: view shape (1, 2), then sort
  (3, 4, 5, 6).
- Single-modifier so it's fast to hit one-handed.
- Date modified before Size matches what people sort by most often after Name/Extension.
- "Date created", "Date added", "Kind", "Permissions", etc. get no default shortcut. Users who want them can bind one in
  Settings > Shortcuts.

### Alternative shortcuts: `⌘F3..F6`

- `⌘F3` — Name
- `⌘F4` — Extension
- `⌘F5` — Date modified
- `⌘F6` — Size

The F-row mirrors Total Commander's `Ctrl+F3..F7` muscle memory. Pairing both shortcut styles costs us nothing — both
bindings show up in Settings > Shortcuts, and either fires the same action.

`⌘F3` collides with the macOS default "Show Desktop" Mission Control shortcut (System Settings → Keyboard → Keyboard
Shortcuts → Mission Control). When that's enabled, the system swallows the keystroke before it reaches Cmdr. `⌘3` is the
primary; `⌘F3` is the parity alt. Users who hit the collision can either disable the macOS shortcut or rely on `⌘3`.

### Same-key-press toggles order

Pressing the active sort column's shortcut a second time flips ascending ↔ descending. This already works for column
header clicks (via `getNewSortOrder` in `pane/sorting-handlers.ts`) — the same code path runs for shortcut-driven sorts,
so no extra logic was needed.

### Menu reorder

The "View > Sort by" submenu now lists:

1. Name
2. Extension
3. Date modified
4. Size
5. Date created

Date modified moved up by one. The shortcut order (`⌘3..6`) matches the menu order.

## Platform notes

- **macOS**: Native menu accelerators (`Cmd+3..6`) attach to the menu items. `⌘F3..F6` are registered in the JS shortcut
  dispatch only — Tauri menus support a single accelerator per item.
- **Linux**: `Cmd+3..6` attach to the GTK menu items the same way (Tauri's `Cmd+` maps to `Ctrl+` on Linux). `⌘F3..F6`
  go through the JS dispatch only; GTK intercepts F-row keys at the toolkit level for menu accelerators per
  `apps/desktop/src-tauri/src/menu/CLAUDE.md`.
- **Both**: The JS shortcut dispatcher and the menu's `menu-sort` event both end up calling
  `DualPaneExplorer.setSortColumn`, which routes to `handleSortChange` — same behavior regardless of entry point.

## Files touched

- `apps/desktop/src/lib/commands/command-registry.ts` — added `shortcuts` arrays for the four sort commands
- `apps/desktop/src-tauri/src/menu/mod.rs` — `build_sort_submenu` now takes accelerator params and reorders Date
  modified above Size; sort items added to the items HashMap and to `command_id_to_menu_id` so user-customized shortcuts
  re-flow into the menu
- `apps/desktop/src-tauri/src/menu/macos.rs` and `linux.rs` — pass `Cmd+3..6` accelerators; register sort items in items
  HashMap
- `apps/desktop/src/lib/shortcuts/shortcuts-store.ts` — added the four sort command IDs to `menuCommands`
