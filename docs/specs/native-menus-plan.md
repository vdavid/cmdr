# Native menus overhaul

## Problem

The macOS menu bar is cluttered with irrelevant system defaults (Writing Tools, AutoFill, Dictation, Emoji & Symbols)
because `build_menu_macos` starts from `Menu::default()` and patches it. Meanwhile, important file operations (Copy,
Move, Delete, New folder) have no menu presence — they're only accessible via keyboard shortcuts and the command
palette.

The menu event handler (`on_menu_event` in `lib.rs`) and the frontend keyboard dispatcher (`handleCommandExecute` in
`+page.svelte`) are two parallel dispatch systems for the same commands. Menu clicks go through ~15 individual Tauri
event listeners, while keyboard shortcuts go through centralized dispatch. This is fragile and hard to extend.

## Goals

1. **Clean menus**: Build macOS menus from scratch (like Linux already does), removing all system defaults we don't want
2. **Complete menus**: Add all user-facing file operations to the menu bar
3. **Keep Help search**: macOS automatically adds a search field to any menu named "Help" — we keep this for free
4. **Unified dispatch**: Menu clicks and keyboard shortcuts converge on a single code path (`handleCommandExecute`)
5. **Synced shortcut labels**: When the user customizes shortcuts in Settings, menu accelerator labels update to match
6. **Context-aware menus**: File operations are disabled when not in the file explorer (Settings window, file viewer)
7. **Preserve existing features**: Command palette, shortcut customization, and MCP integration remain unchanged

## Non-goals

- Changing the command registry structure or shortcut storage format
- Adding new keyboard shortcuts (only surfacing existing ones in menus)

## Architecture

### Key decision: JS dispatch stays primary

Native menu accelerators intercept keypresses before the webview sees them. This means:
- A command with a menu accelerator → native menu handles the keypress → `on_menu_event` fires → frontend
- A command without a menu accelerator → JS `handleGlobalKeyDown` handles it → `handleCommandExecute`

Both paths lead to `handleCommandExecute`. The menu is an alternative entry point, not a replacement for JS dispatch.
We keep JS dispatch because:
- It handles commands that have no menu item (volume choosers, selection toggle, network commands, etc.)
- It handles secondary shortcuts (e.g., `nav.parent` has both `Backspace` and `⌘↑`, but the menu only shows one)
- It powers the command palette
- It has scope/context awareness via `isModalDialogOpen()` that menus don't have

### Unified event flow

**Before** (current): `on_menu_event` has a ~140-line if/else chain that maps menu IDs to ~15 different Tauri events,
each with its own frontend listener.

**After**: `on_menu_event` maps menu IDs to command registry IDs and emits a single `"execute-command"` event. The
frontend has one listener that calls `handleCommandExecute(commandId)`.

This eliminates the individual Tauri events (`show-command-palette`, `switch-pane`, `swap-panes`, `start-rename`,
`new-tab`, `close-tab`, `toggle-pin-tab`, `navigation-action`) and their ~10 corresponding `unlistenX` cleanup calls.

`show-about`, `show-license-key-dialog`, and `open-settings` also move to the unified path — they map to `app.about`,
`app.settings`, and the license command which `handleCommandExecute` already handles. MCP commands that currently emit
these events should also emit `"execute-command"` instead, keeping the path consistent.

**CheckMenuItem exception**: `show_hidden_files` and view mode items auto-toggle their checked state in Tauri when
clicked. We can NOT route these through `handleCommandExecute` because that function also toggles state, which would
double-toggle. These items keep their current special handling: Rust syncs the CheckMenuItem state, then emits the
existing `"settings-changed"` / `"view-mode-changed"` events directly. This means these commands have two code paths:
keyboard → `handleCommandExecute` → toggle function, and menu click → Rust CheckMenuItem sync → direct event. This
asymmetry is a known compromise — both paths produce the same result, but the implementer should not try to merge them.

**Close tab exception**: `CLOSE_TAB_ID` (⌘W) has special behavior: if a non-main window (viewer, settings) is focused,
it closes that window instead of a tab. This is standard macOS "close the front window" behavior. This logic stays in
`on_menu_event` — it checks the focused window before deciding whether to emit `"execute-command"` for `tab.close` or
close the focused window directly.

### Focus guard for menu accelerators (critical)

Menu accelerators fire globally — across all windows. When Settings or a file viewer has focus and the user presses F5,
the menu accelerator fires `on_menu_event`, which would emit to the main window and trigger a copy dialog behind the
focused window. This is wrong.

The Rust-side `on_menu_event` handler MUST check `main_window.is_focused()` before emitting `"execute-command"` for
file-scoped commands. This is already done for some commands (Go, Rename) and needs to be extended to all file
operations. This is a functional guard, not just visual — the menu enable/disable (Milestone 4) is an additional visual
hint, not a replacement for this check.

For app-level commands (Quit, Settings, About, Command palette): always emit, regardless of window focus.

### Context-aware enable/disable

When the user is in Settings or a file viewer window, file-operation menu items (Copy, Move, Delete, etc.) should be
grayed out as a visual hint. Implementation:

1. Frontend detects context changes (Settings window open/close, main window focus/blur)
2. Calls a Tauri command `set_menu_context("explorer" | "other")`
3. Rust enables/disables the relevant menu items via `set_enabled(bool)`

This is visual feedback reinforcing the focus guard above. Both are needed: the guard prevents execution, the
enable/disable communicates to the user.

## Menu structure

### macOS

| Menu       | Items |
|------------|-------|
| **cmdr**   | About cmdr, See license details..., separator, Settings... ⌘,, separator, `PredefinedMenuItem::hide`, `::hide_others`, `::show_all`, separator, `::quit` |
| **File**   | Open, View F3, Edit in editor F4, separator, Copy... F5, Move... F6, New folder F7, Delete F8, Delete permanently ⇧F8, separator, Rename F2, separator, Show in Finder ⌥⌘O, Get info ⌘I, Quick look |
| **Edit**   | Select all ⌘A, Deselect all ⌘⇧A, separator, Copy path ⌃⌘C, Copy filename |
| **View**   | Full view ⌘1, Brief view ⌘2, separator, Show hidden files ⌘⇧., Sort by → submenu, separator, Switch pane Tab, Swap panes ⌘U, separator, Command palette... ⌘⇧P |
| **Go**     | Back ⌘[, Forward ⌘], separator, Parent folder ⌘↑ |
| **Tab**    | New tab ⌘T, Close tab ⌘W, separator, Next tab ⌃Tab, Previous tab ⌃⇧Tab, separator, Pin tab, Close other tabs |
| **Window** | `::minimize`, `::maximize`, separator, `::bring_all_to_front` |
| **Help**   | *(macOS auto-adds search field; verify this works with Tauri 2's menu abstraction)* |

`::` prefix denotes `PredefinedMenuItem` variants — these provide native macOS behavior (correct labels, standard
shortcuts, system integration).

### Linux

Same structure minus the cmdr app menu. Settings and license go under Edit (existing pattern). About goes under Help.
Mnemonics (`&` prefixes for GTK keyboard navigation) are handled as a dedicated pass in Milestone 7 — see below.

### Naming conventions

- **"Copy..." / "Move..."** (with ellipsis) since they open a confirmation dialog. Placed in the File menu, the context
  makes them unambiguous vs clipboard Copy in Edit. No need for "Copy files..." — just "Copy..." is clear enough.
- **"Copy path" / "Copy filename"** move to Edit menu alongside clipboard operations — they're copy-to-clipboard
  actions, which is Edit territory.
- **"Edit in editor"** to distinguish from the Edit menu.
- **"View" (F3)** for the file viewer.
- **"Delete permanently"** included for discoverability (⇧F8 is not obvious).
- Sentence case throughout per style guide.

### Viewer window menu

`build_viewer_menu` also uses `Menu::default()` on macOS and inherits the same junk. Rebuild it from scratch with a
minimal menu: File (Close ⌘W), Edit (clipboard PredefinedMenuItems), View (Word wrap toggle), Window
(Minimize, Zoom), Help. No file operations — they're irrelevant in the viewer context.

The viewer's Close (⌘W) is independent of the main window's close-tab exception — it's a simple
`PredefinedMenuItem::close_window` or equivalent, since the viewer is always a non-main window with no tabs.

## Risks and edge cases

1. **Help menu search**: macOS adds a search field to the Help menu via `NSMenu`. Tauri 2 creates native `NSMenu`
   instances, so this should work. Verify in Milestone 1 — if it doesn't, add a placeholder "Search" item that opens
   the command palette as a fallback.

2. **GTK accelerator interception**: On Linux, GTK menu accelerators intercept keypresses before the webview (documented
   in existing code comments for F2/Rename). For commands where this is a problem, omit the accelerator on Linux and
   let JS dispatch handle the keyboard shortcut. The menu still shows the shortcut text in the label (e.g.,
   "Re&name\tF2" if Tauri supports tab-separated hints, otherwise just "Re&name").

3. **Accelerator update cost**: Updating a menu accelerator requires removing and reinserting the menu item (Tauri
   limitation — no `set_accelerator()` API). For ~25 items at startup, this is fine. For runtime changes, it's a rare
   operation (user explicitly editing shortcuts in Settings).

4. **Tab as accelerator**: "Switch pane" uses Tab as its shortcut. Tab as a menu accelerator might conflict with
   accessibility navigation in menu bars. Test this — if it causes issues, omit the accelerator and rely on JS
   dispatch.

5. **⌘A for Select all**: The PredefinedMenuItems (Undo, Redo, Cut, Copy, Paste, Select All) were removed from the Edit
   menu because they are irrelevant to a file manager. With `PredefinedMenuItem::select_all` removed, there is no
   conflict, so "Select all" uses ⌘A as its native menu accelerator. This means ⌘A goes through `on_menu_event` →
   `execute-command` → `handleCommandExecute` → `selection.selectAll`.

6. **License key command**: There's no `app.licenseKey` command in the registry. To route the license menu item through
   `"execute-command"`, add a new `app.licenseKey` command to the registry and a handler in `handleCommandExecute`.

## Implementation milestones

Milestones 1–2 are sequential. Milestones 3, 4, and 5 can be done in parallel after 2 is complete. Milestone 6 is
the final pass. Milestone 7 (Linux mnemonics) runs after 5 and 6.

### Milestone 1: Build menus from scratch

Rewrite `build_menu_macos` to construct all menus explicitly, using `build_menu_linux` as a template. The resulting
menu structure matches the table above. Keep all existing menu item IDs and `MenuState` fields — this is a refactor of
menu construction, not a behavior change.

Add new menu item IDs: `FILE_COPY_ID`, `FILE_MOVE_ID`, `FILE_NEW_FOLDER_ID`, `FILE_DELETE_ID`,
`FILE_DELETE_PERMANENTLY_ID`, `FILE_VIEW_ID`, `SELECT_ALL_ID`, `DESELECT_ALL_ID`, `NEXT_TAB_ID`, `PREV_TAB_ID`,
`CLOSE_OTHER_TABS_ID`. Use `FILE_` prefix for file operations to avoid ambiguity (e.g., `FILE_DELETE_ID` vs a generic
`DELETE_ID`).

Add the Tab submenu (currently tab items are in File on macOS). Move them to a dedicated Tab menu on both platforms.

The "Sort by →" submenu in View contains items for each sort field (Name, Extension, Size, Modified, Created) and
order (Ascending, Descending). These map to the existing `sort.*` commands in the registry. The submenu items use
the existing `SORT_*` IDs if they exist, or new ones following the same pattern.

Also rebuild `build_viewer_menu` from scratch with the minimal menu described above.

Wire up new menu items in `on_menu_event` using the existing pattern (individual event emitting) — this gets them
working immediately. The unified dispatch cleanup happens in Milestone 2.

### Milestone 2: Unify menu → frontend dispatch

1. Create a `menu_id_to_command_id` mapping in `menu.rs` that maps each menu item ID string to its command registry
   counterpart (e.g., `FILE_COPY_ID → "file.copy"`, `COMMAND_PALETTE_ID → "app.commandPalette"`)
2. Simplify `on_menu_event`:
   - Look up command ID from the mapping
   - For file-scoped commands: check `main_window.is_focused()` before proceeding (focus guard)
   - For CheckMenuItems (show hidden, view modes): keep special handling (sync checked state, emit
     `"settings-changed"` / `"view-mode-changed"` directly — NOT through `"execute-command"` to avoid double-toggle)
   - For everything else: emit `"execute-command"` with `{ commandId }` payload
3. Frontend: add one listener for `"execute-command"` → calls `handleCommandExecute(commandId)`
4. Remove individual menu event listeners that are now redundant: `show-command-palette`, `switch-pane`, `swap-panes`,
   `start-rename`, `new-tab`, `close-tab`, `toggle-pin-tab`, `navigation-action`, `show-about`,
   `show-license-key-dialog`, `open-settings`, and their corresponding `unlistenX` variables
5. Update MCP event emitters to use `"execute-command"` where they currently emit the old individual events.
   Specifically: any MCP handler that emits `show-about`, `show-license-key-dialog`, `open-settings`,
   `show-command-palette`, `switch-pane`, `swap-panes`, `start-rename`, `new-tab`, `close-tab`, or `toggle-pin-tab`
   should emit `"execute-command"` with the corresponding command ID instead. The `navigation-action` and `menu-sort`
   events in `setupMcpListeners()` are NOT menu events — they're MCP-specific and stay as-is

### Milestone 3: Sync accelerator labels for all menu commands

Extend the accelerator sync mechanism to cover all commands that have menu items (~25 commands):

1. Add a `HashMap<String, MenuItemRef>` to `MenuState` where `MenuItemRef` is an enum over `MenuItem` /
   `CheckMenuItem`. Existing named fields (`show_hidden_files`, `view_mode_full`, `view_mode_brief`, `view_submenu`,
   position indices, `pin_tab`, `context`) stay as-is — they need direct typed access for checked-state sync and
   dynamic label changes. The HashMap is for the ~20 regular `MenuItem`s that only need accelerator updates and
   enable/disable
2. Generalize `update_view_mode_accelerator` → a generic `update_menu_item_accelerator` that works with the HashMap
3. In `shortcuts-store.ts`, expand the `menuCommands` list to include all commands with menu items
4. On startup, `syncMenuAccelerators()` iterates all menu-bound commands and updates their accelerators from the
   persisted shortcut store

### Milestone 4: Context-aware enable/disable

1. Add a `set_menu_context` Tauri command that accepts a context string (`"explorer"` | `"other"`)
2. Define which menu items are active in which context:
   - `"explorer"`: all items enabled
   - `"other"`: file operations disabled (Copy, Move, Delete, Rename, New folder, View, Edit in editor, Show in
     Finder, Get info, Quick look, Copy path, Copy filename, selection commands), app-level items remain enabled
     (Settings, Quit, tabs, view modes, command palette)
3. Frontend calls `set_menu_context("other")` when Settings or file viewer gains focus, `set_menu_context("explorer")`
   when main explorer regains focus
4. Use the `MenuState` HashMap from Milestone 3 to iterate and enable/disable items

### Milestone 5: Align Linux menus

Update `build_menu_linux` to match the new menu structure:
- Add file operation items (Copy, Move, New folder, Delete, Delete permanently, View)
- Move tab items to a dedicated Tab submenu
- Add Select all / Deselect all to Edit
- Move Copy path / Copy filename to Edit
- Omit accelerators for commands where GTK interception is a known problem (Rename F2, others if discovered)
- Verify with `--check cfg-gate` at minimum; full test on Linux if possible

### Milestone 6: Testing and polish

- Manual testing: verify all menu items trigger the correct commands on macOS
- Verify Help → Search works on macOS (if not, implement fallback)
- Verify shortcut customization updates menu labels
- Verify context-aware disable/enable works across window focus changes
- Verify viewer window has clean menus
- Verify command palette still works for all commands
- Verify MCP integration unaffected
- Update `CLAUDE.md` files for menu.rs and shortcuts
- Run full `./scripts/check.sh`

### Milestone 7: Linux mnemonics (after 5 and 6)

Assign `&` mnemonic prefixes to all Linux menu items in a single pass. Mnemonics must be unique within each
submenu — for example, File can't have both `&Delete` and `&Delete permanently`. This is a targeted effort across
all menus at once so conflicts are caught holistically rather than piecemeal.

Guidelines:
- Prefer the first letter of the item name when available (for example, `&Open`, `&View`, `&Back`)
- When the first letter conflicts, pick the next most distinctive letter (for example, `&Delete` vs
  `Delete &permanently`, `&Copy...` vs `&Move...`)
- Match existing GTK conventions where applicable (for example, `&File`, `&Edit`, `&View`, `&Help` for top-level menus)
- The existing `build_menu_linux` mnemonics are a starting point but will need revision since the menu structure changed

## Task list

### Milestone 1: Build menus from scratch
- [x] Add new menu item ID constants (`FILE_COPY_ID`, `FILE_MOVE_ID`, etc.)
- [x] Rewrite `build_menu_macos` to build all menus from scratch (no `Menu::default()`)
- [x] Use `PredefinedMenuItem` for app menu items (Hide, Quit), Window items (Minimize, Zoom)
- [x] Add File menu with all file operations, including Delete permanently
- [x] Add Edit menu: Select all (⌘A), Deselect all (⌘⇧A), Copy path, Copy filename (no PredefinedMenuItems)
- [x] Add Tab submenu (New, Close, Next, Previous, Pin, Close others)
- [x] Add Sort by submenu in View (Name, Extension, Size, Modified, Created, Ascending, Descending)
- [x] Keep Help menu (objc2 `setHelpMenu:` registers it for macOS search field)
- [x] Rebuild `build_viewer_menu` from scratch
- [x] Wire new items in `on_menu_event` (using existing per-event pattern, before unification)
- [x] Strip system-injected Edit items (Writing Tools, AutoFill, Dictation, Emoji & Symbols) via objc2 cleanup
- [x] Route ⌘Q, ⌘H, ⌥⌘H through native PredefinedMenuItems (not JS dispatch)
- [x] Verify menu renders correctly, all items clickable
- [x] Run `./scripts/check.sh --check rustfmt --check clippy --check rust-tests`

### Milestone 2: Unify dispatch
- [x] Create `menu_id_to_command_id` mapping
- [x] Simplify `on_menu_event` to use mapping + single `"execute-command"` event
- [x] Add focus guard for file-scoped commands in `on_menu_event`
- [x] Keep CheckMenuItem special handling for show hidden files and view modes (avoid double-toggle)
- [x] Keep close-tab exception: close focused non-main window if one has focus, otherwise emit tab.close
- [x] Add `app.licenseKey` command to command-registry.ts and handleCommandExecute
- [x] Add `"execute-command"` listener in `+page.svelte` that calls `handleCommandExecute`
- [x] Remove redundant individual event listeners and unlisten variables
- [x] Update MCP event emitters to use `"execute-command"` where applicable
- [x] Verify all menu clicks still work (manual test)
- [x] Run `./scripts/check.sh --check rustfmt --check clippy --check rust-tests --svelte`

### Milestone 3: Sync accelerator labels (can parallel with 4 and 5)
- [x] Add `HashMap<String, MenuItemEntry>` to `MenuState` for generic menu item tracking
- [x] Generalize accelerator update function to work for any menu item type
- [x] Update `menuCommands` list in shortcuts-store.ts to cover all menu-bound commands
- [x] Add startup accelerator sync for all menu items
- [x] Verify: change a shortcut in Settings → menu label updates
- [x] Run `./scripts/check.sh --check rustfmt --check clippy --check rust-tests --svelte`

### Milestone 4: Context-aware enable/disable (can parallel with 3 and 5)
- [x] Add `set_menu_context` Tauri command
- [x] Define context → enabled items mapping in Rust
- [x] Call `set_menu_context` on window focus changes in frontend
- [ ] Verify: open Settings → file menu items grayed out, close → re-enabled
- [x] Run `./scripts/check.sh --check rustfmt --check clippy --check rust-tests --svelte`

### Milestone 5: Align Linux menus (can parallel with 3 and 4)
- [x] Update `build_menu_linux` to match new structure (file ops, Tab submenu, Edit reorg)
- [x] Omit accelerators for commands with known GTK interception issues
- [x] Use placeholder mnemonics (first letter) — final mnemonic pass is Milestone 7
- [x] Verify compilation with `--check cfg-gate`

### Milestone 6: Testing and polish
- [x] Full manual test pass of all menu items on macOS
- [x] Verify Help → Search works
- [ ] Verify viewer window menus are clean
- [x] Verify command palette still works
- [x] Verify MCP integration unaffected
- [x] Update CLAUDE.md files
- [x] Run full `./scripts/check.sh`

### Milestone 7: Linux mnemonics (after 5 and 6)
- [x] Audit all Linux menu items across all submenus for mnemonic conflicts
- [x] Assign unique `&` mnemonics per submenu, following GTK conventions where possible
- [x] Verify with `--check cfg-gate` and ideally manual test on Linux
