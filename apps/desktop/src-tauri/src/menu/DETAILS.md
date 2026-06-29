# Menu system details

Pull-tier docs for `src-tauri/src/menu/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in [CLAUDE.md](CLAUDE.md).

Native menu bar for macOS and Linux. Builds platform-specific menus from scratch, handles menu
events, syncs accelerator labels with user-customized shortcuts, and enables/disables items based on
window focus context.

## File layout

- `mod.rs`: shared types (`MenuState`, `MenuItems`, `MenuItemEntry`, `MenuContext`,
  `NetworkHostMenuContext`, `CommandScope`, `ViewMode`), constants (all menu item IDs), the
  ID mapping functions (`menu_id_to_command`, `command_id_to_menu_id`), and re-exports of the
  public API exposed by the submodules below.
- `menu_items.rs`: small-piece builders and platform-aware helpers: `build_sort_submenu`,
  `build_zoom_submenu`, `register_item`, `truncate_for_menu_label`, the `copy_path_accelerator` /
  `show_in_file_manager_*` / `full_view_label` / `brief_view_label` platform helpers, and the
  `SortSubmenuItems` struct.
- `menu_structure.rs`: hierarchical assembly: the `build_menu` dispatcher, file context menu
  (`build_context_menu`), breadcrumb / tab / network-host / volume-selector-row context menus
  (`build_volume_row_context_menu`: favorite Rename/Remove or volume Eject), the viewer-window menu
  (`build_viewer_menu`), plus the `FileContextInfo` and `ContextMenuResult` types.
- `menu_handlers.rs`: event-handler and live-update helpers: `rebuild_view_mode_items`,
  `sync_view_mode_check_states`, `update_menu_item_accelerator`, `frontend_shortcut_to_accelerator`,
  and the macOS post-construction wrappers `cleanup_macos_menus` / `set_macos_menu_icons` (the
  actual objc2 FFI lives in `macos.rs`).
- `macos.rs`: `build_menu_macos` (full macOS menu bar), `cleanup_macos_menus` (removes
  system-injected Edit items, registers Help menu), `set_macos_menu_icons` (SF Symbol icons via
  objc2 FFI), and their helpers.
- `open_with.rs` (macOS): `build_open_with_submenu` for the file context menu's "Open with"
  submenu. Returns the submenu plus a `bundle_id → app_path` map that callers stash in
  `MenuState.context.open_with_apps` so `on_menu_event` can resolve dynamic `open-with:<bundle-id>`
  click targets.
- `linux.rs`: `build_menu_linux` (full Linux/GTK menu bar with mnemonics, no F-key accelerators).

## Key concepts

### Unified dispatch

Menu clicks route through a single `"execute-command"` Tauri event. `handle_menu_event` (in
`menu_handlers.rs`, wired into the Tauri builder via `.on_menu_event(menu::handle_menu_event)`)
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
  registered in the items HashMap, purely so user-customized accelerators flow through the
  generic update path. The on_menu_event special-case fires first, so the generic dispatch is
  never reached at click time.
- **Tab context menu**: emits specific tab action events with tab index payload
- **Open with** (macOS): items have dynamic IDs like `open-with:com.apple.Xcode` that can't be
  enumerated in `menu_id_to_command`. `on_menu_event` prefix-matches `open-with:` and calls
  `file_system::open_with::open_paths_with` directly, looking up the app URL via
  `MenuState.context.open_with_apps[bundle_id]` and the launch paths via
  `MenuState.context.paths`. The "Other..." entry shows an `NSOpenPanel` filtered to `.app`
  bundles and launches the chosen app the same way.
- **Finder tag colors** (macOS): the file context menu carries seven `IconMenuItem` circles
  (`menu_structure.rs::append_tag_color_group`, shown for files AND folders), IDs `tag-color:<1..=7>`,
  built with bitmaps from `menu/tag_icons.rs`. Like "Open with", they're prefix-routed
  (`on_menu_event` matches `tag-color:`) — NOT in `menu_id_to_command` — and call
  `file_system::tags::toggle_color` on the RIGHT-CLICKED selection (`MenuState.context.paths`),
  then `apply_tags_to_listing(MenuState.context.tags_listing_id, …)`. Acting on the right-clicked set
  is why they can't route through `execute-command`: a frontend command reads the *focused-pane*
  selection, which differs when the right-click lands on an unselected row. The xattr write runs on
  `spawn_blocking` (off the main/menu thread). The keyboard-assignable `tags.toggle*` commands cover
  the focused-selection case via the frontend (`pane-commands.ts::toggleTagOnFocusedSelection` →
  `toggle_tags` IPC); no default shortcut.
  - **Checked state = applied tag** (D7): muda's `IconMenuItem` has no native gutter checkmark (a fork
    would be a two-repo muda+Tauri patch), so the "applied" circle composites a white check INTO the
    bitmap. A color is "applied" when EVERY selected path already carries it
    (`FileContextInfo.applied_tag_colors`, computed from `tags::applied_colors` at menu-build time);
    `toggle_color` then removes it (all-have) or adds it (some/none have). Circles render at 36 px
    (2× the 18 pt logical menu-icon size) with a baked 1 px darkened-edge border so a pale fill
    (yellow) reads on light/dark menus; colors mirror the light-mode `--color-tag-*` tokens. The 14
    bitmaps (7 colors × {normal, checked}) are cached once in a `LazyLock`. macOS-only — Linux menus
    carry no icons.

### MenuState

Shared state managed via `tauri::State<MenuState<Wry>>`. Holds:
- Named `CheckMenuItem` references (`show_hidden_files`, plus four per-pane view-mode items:
  `view_mode_full_left/right` and `view_mode_brief_left/right`) for checked-state sync
- `pin_tab` MenuItem reference for dynamic label changes ("Pin tab" / "Unpin tab")
- `view_left_pane_submenu` / `view_right_pane_submenu`: the two pane-scoped submenus that hold
  the Full/Brief CheckMenuItems (Full at position 0, Brief at position 1). Used by
  `rebuild_view_mode_items` to remove/recreate/reinsert items when accelerators move on focus change.
- Cached view-mode state (`view_mode_active_pane`, `view_mode_left`, `view_mode_right`,
  `view_mode_full_accel`, `view_mode_brief_accel`) used by `rebuild_view_mode_items` to
  attach the keyboard accelerator only to the currently-active pane's pair
- `items: HashMap<String, MenuItemEntry>` for the ~20 regular MenuItems that need accelerator
  updates and enable/disable
- `context: MenuContext` for right-click context menu: `path` (primary right-clicked file),
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

### Per-window menu activation (`activate_window_menu`)

Each window's frontend focus handler calls `activate_window_menu(kind)` on focus-gain, with `kind`
one of `"main"` (main explorer), `"viewer"` (a file viewer), or `"other"` (Settings / Debug). The
command does two things: pick the right app menu (macOS), then set per-item enabled state.

On macOS there's a single app-level menu bar (no per-window menus, tauri-apps/tauri#5768), so the
menu is swapped wholesale via `app.set_menu()`:

- The **main menu** is cloned at startup (before `app.set_menu()`) and stored in `MenuState.main_menu`.
  The clone shares the same underlying items (Tauri's `Menu` is a reference-counted handle), so the
  item refs stored in `MenuState` keep mutating the live menu after a swap-back.
- The **viewer menu** is built once at startup (`build_viewer_menu`) and stored in
  `MenuState.viewer_menu`, with its `Word wrap` CheckMenuItem ref in `MenuState.viewer_word_wrap`.
  - Its **Edit** submenu carries the full predefined Cut/Copy/Paste/Select all, not Copy-only: predefined items route
    the native `cut:`/`copy:`/`paste:`/`selectAll:` selectors to the focused text field (the search box) through the
    responder chain, so don't trim it back — that's what left ⌘X/⌘V dead in the viewer search field. Predefined is fine
    here (unlike the main menu's custom Edit items above) because the viewer menu is a separate menu, never installed
    alongside the main one, so there's no item to conflict with.
- `MenuState.active_menu_kind` tracks which menu is installed, so a same-kind focus event (viewer →
  viewer, main → main) skips the swap entirely.
- `"main"` and `"other"` install the main menu; `"viewer"` installs the viewer menu. After any swap
  we re-run `cleanup_macos_menus` (macOS re-injects Edit items on every `set_menu`), and on a
  swap-back to the main menu we also re-apply `set_macos_menu_icons` (SF Symbols don't survive
  `app.set_menu()`). Both run on the main thread via `run_on_main_thread`, queued FIFO after Tauri's
  own main-thread menu install, so ordering is install → cleanup → icons.

On Linux `activate_window_menu` skips the swap (viewer windows carry their own per-window menu set by
`viewer_setup_menu` / `window.set_menu()`) and only does the enable/disable step.

The enable/disable step is the private `set_menu_context("explorer" | "other")` helper: it iterates
the `items` HashMap and sets `enabled` on each file-scoped item (`"main"` → explorer/enabled,
`"other"` → disabled). This is a visual hint reinforcing the focus guard in `on_menu_event`.

**Gotcha: `onFocusChanged` doesn't fire for a window's initial focus.** A window opens already
focused, so its frontend focus listener (registered in `onMount`) misses the first focus and only
sees later regains. The main window is fine (its menu is installed at startup) and Settings is fine
(opening it blurs main, whose `"other"` handler already greys the shared menu into the state Settings
wants). But the viewer needs its own menu swapped in, which no other window's handler does, so
`routes/viewer/+page.svelte` calls `activateWindowMenu("viewer")` explicitly on open in addition to
the focus listener. The ordering is race-free: a viewer's `onMount` only runs after its webview
loads, always after the main window's instant blur, so `"viewer"` wins; and macOS fires `resignKey`
before `becomeKey`, so the gaining window's handler runs last on a window-to-window switch too.

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

Context menus don't get SF Symbols for our own items because Tauri doesn't expose the raw `NSMenu`
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

Both platforms share: File, Edit, Select, View (with Sort by and Zoom submenus), Go, Tab, Help.

The **Select** submenu (between Edit and View) holds the four selection commands: `Select all` (⌘A), `Deselect all`
(⌘⇧A), `Select files…` (no menu accelerator), and `Deselect files…` (no menu accelerator). The two `…` items open the
Selection dialog (see `apps/desktop/src/lib/selection-dialog/CLAUDE.md`); their keystrokes (bare `+` / `-`) are bound in
`FilePane`'s keydown handler because macOS menu accelerators always carry the ⌘ modifier and bare `+` / `-` aren't
valid accelerator strings. The items are still registered in `MenuState.items` so a user-customized shortcut could flow
into the menu via the generic update path.

The **Go** submenu holds, in order: `Back` (⌘[), `Forward` (⌘]), separator, `Parent folder` (⌘↑), separator,
`Go to path…` (⌘G), `Go to latest download` (⌘J). The two jump items are `GO_TO_PATH_ID` (`"go_to_path"`) →
`nav.goToPath` and `GO_LATEST_DOWNLOAD_ID` (`"go_latest_download"`) → `downloads.goToLatest`, both `FileScoped` so they
grey out in the viewer/settings windows. `Go to path…` carries the macOS ellipsis (it opens the Go-to-path dialog);
`Go to latest download` has none (direct action). On macOS the SF Symbols are `arrow.right.to.line` (Go to path…) and
`arrow.down.circle` (Go to latest download); the symbol map matches by exact title string, so the `\u{2026}` ellipsis
must stay byte-identical between the `MenuItem` title and the map. On Linux the mnemonics are `Go &to path…` and
`Go to &latest download` (B/F/P are claimed by Back/Forward/Parent).

**Double-dispatch (⌘G / ⌘J).** A key combo matching a menu accelerator fires BOTH the native menu (`execute-command`)
AND the JS keydown dispatch on macOS (see `shortcuts/DETAILS.md` § "Modifier-key accelerators may fire twice"). This is
safe here without any suppression hack: ⌘G's dialog-open is idempotency-guarded in `+page.svelte`, and ⌘J's re-reveal is
naturally idempotent. Expect two `FE:user-action downloads.goToLatest` log lines on one ⌘J press — harmless.

The **Help** submenu holds, in order: `Keyboard shortcuts`, separator, `What's new`, `Send feedback…`,
`Send error report…` (Linux prepends `About cmdr` + a separator, since it has no app menu). `What's new`
(`HELP_WHATS_NEW_ID` (`"help_whats_new"`) → `help.whatsNew`, `App`-scoped) opens the post-update changelog popup (see
`apps/desktop/src/lib/whats-new/CLAUDE.md`); it has no default shortcut but is registered in `MenuState.items` so a
future custom binding still flows into the menu. Its macOS SF Symbol is `sparkles` (the symbol map matches by exact
title, so `What's new` must stay byte-identical); the Linux mnemonic is `&What's new` (`W` is free; `A`/`K`/`f`/`S` are
claimed by the other Help items).

The **Zoom** submenu (`build_zoom_submenu`) holds the text-size presets (75/100/125/150 %) plus Zoom in (`Cmd+Plus`) /
Zoom out (`Cmd+Minus`) / 100 % (`Cmd+0`). Items are `App`-scoped so the keyboard accelerators fire in any focused window.
Linux skips the in/out accelerators because GTK intercepts `Cmd+Plus` / `Cmd+Minus` at the toolkit level; the JS
shortcut dispatch path covers Linux.
macOS adds: cmdr (app menu), Window. See the menu item ID constants in `mod.rs` for the full item list.

Viewer windows get a minimal menu: File (Close), Edit (clipboard), View (Word wrap), and on macOS
also Window and Help. On Linux it's a per-window menu; on macOS it's installed app-level on viewer
focus-gain (see "Per-window menu activation" above).

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
**Why**: The previous single Full/Brief pair always targeted the active pane, but that scope was invisible in the menu, so testers were slow to figure out how to change the inactive pane's view. Nesting each pane's Full/Brief items inside its own submenu makes the scope obvious without cluttering the View root. The accelerator is attached only to the active pane's pair (and migrates on focus change via `rebuild_view_mode_items`) so the shortcut remains accurate: pressing ⌘1 always affects the active pane, and the visible binding sits next to the items it actually targets.

**Decision**: `Select all` and `Deselect all` live in the `Select` top-level menu, not in `Edit`.
**Why**: macOS convention puts them under `Edit`, but Cmdr's `selection.selectAll` operates on files, not on text. The
`Select` menu is the honest home for file-selection commands, and it groups them with the `Select files…` /
`Deselect files…` dialog openers. `Edit` retains the text-edit operations (Cut/Copy/Paste/Move here/Copy path/Copy
filename/Search files) plus Undo/Redo. Don't move them back without re-reading this entry — the file-vs-text-selection
distinction is the load-bearing reason.

**Decision**: SF Symbol icons only on the menu bar, not on context menus.
**Why**: Tauri doesn't support SF Symbols natively. For the menu bar, we walk `NSApplication.mainMenu()` post-construction via objc2 FFI and set SF Symbols directly on `NSMenuItem` objects, producing true template images that auto-tint correctly. Context menus don't get icons because Tauri doesn't expose the raw `NSMenu` pointer, and the alternative (rasterized bitmaps via `IconMenuItem`) produces visually poor results (no template tinting, wrong size/weight).

## Gotchas

- **No `Menu::default()`**: Both platforms build from scratch. The old approach inherited system
  defaults that added unwanted items.
- **Tab as accelerator**: Switch pane uses Tab, which could conflict with menu bar accessibility
  navigation. If issues arise, omit the accelerator and rely on JS dispatch.
- **Custom MenuItems for Cut/Copy/Paste/Select all**: The Edit menu uses custom MenuItems (not
  PredefinedMenuItems) for Cut, Copy, Paste, and Move here; the Select menu does the same for
  Select all. In `handle_menu_event`, these are handled specially: if the main window is focused,
  they route through `execute-command` so the frontend can decide between file and text semantics
  (via `document.activeElement` check). If a non-main window is focused (viewer, settings),
  `send_native_edit_action()` in `menu_handlers.rs` sends the native
  `copy:`/`cut:`/`paste:`/`selectAll:` selector through the responder chain via
  `NSApplication.sendAction:to:from:`, replicating what PredefinedMenuItems do internally. This
  ensures text clipboard and text select-all work natively in all windows. Undo and Redo remain
  PredefinedMenuItems since they only apply to text fields.
- **⌘A dual routing**: "Select all" uses ⌘A as a native menu accelerator (so it's visible in the
  Select menu — see § "Decision: Select all and Deselect all live in the new Select top-level menu"
  above). Since macOS intercepts it before the webview, the keystroke must be re-routed per focus:
  main window → `execute-command`, where the frontend's `handleCommandExecute` checks
  `document.activeElement` (input/textarea → `.select()` for text, otherwise select files);
  non-main window → native `selectAll:` via `send_native_edit_action()` (without this branch ⌘A is
  dead in settings text fields — the `FileScoped` focus guard would silently drop it). This avoids
  PredefinedMenuItem::select_all which would conflict with the custom MenuItem. Deselect all (⌘⇧A)
  stays on the plain `FileScoped` path: AppKit has no standard "deselect all" responder action for
  text fields, so there's nothing native to forward to.
- **Pin tab label**: `pin_tab` in MenuState is updated dynamically by the frontend to show
  "Pin tab" or "Unpin tab" based on the active tab's state.
- **Reopen closed tab item**: The Tab submenu includes "Reopen closed tab" (⌘⇧T on macOS) between
  Close tab and the Next/Previous tab pair. The item is created **disabled** and toggled live via
  `set_reopen_closed_tab_enabled(enabled: bool)`, using the same dynamic-state pattern as `pin_tab`'s label.
  `MenuState.reopen_closed_tab` holds the `MenuItem` reference. The frontend pushes enable state
  after every close, reopen, and focus change so the menu always reflects the focused pane's
  closed-tab stack.
