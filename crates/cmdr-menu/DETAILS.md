# cmdr-menu details

Pull-tier docs for `crates/cmdr-menu/`. Must-knows live in `CLAUDE.md`.

## Why a crate

**Decision**: the bar's definition (rows, ids, check groups, accelerators, label keys), its enabled-state fold, and the
click classification live here, Tauri-free; the desktop and the browser demo are two hosts of it.

**Why**: the demo (#302) needs a menu bar that says and does exactly what the native one does. Redrawing it from a TS
copy would drift on the first new item. As a crate, the demo reads the same `MENU_BAR` through WASM, and the desktop
gets a bar that's testable without a Tauri runtime (a real `muda::Menu` panics off the main thread), including the
enabled state of every row (`bar_states_test.rs`), which before the move was only reachable through `MenuState`.

## What each side owns

- **Here**: what the bar IS and DECIDES. `MENU_BAR` and its vocabulary, the ids and the command map, which row is
  enabled for a set of inputs (`enabled::entry_enabled`, plus the three per-handle verdicts the desktop applies), which
  Sort by row is checked (`sort::sort_check_states`), what a click asks for (`click::bar_click`), which catalog key a
  live label reads (`labels`), and the accelerator vocabulary (`accelerators`). The same for every right-click menu
  (`context`): its rows for a set of facts, and what a pick asks for (`context::click::context_click`).
- **The desktop** (`apps/desktop/src-tauri/src/menu/`): muda. Building the items, `MenuState` and its locks, the AppKit
  passes (icons, display accelerators, the Edit cleanup), the main/viewer swap, building each context menu from its rows
  (`context_rows.rs`) and filling its `Slot`s from the OS, `menu_t` resolution, and the parts of a click that need
  windows or the OS (is the main window focused, close the focused window, the native selector, a slot row's pick).
- **The demo** (`crates/cmdr-listing-wasm/src/menu.rs` + `apps/desktop/src/lib/demo/menu-bar.svelte.ts`): it stores the
  same pushes the desktop's menu-state commands store, asks this crate for the bar's state, and the page draws it. Its
  right-click menus (`context_menu.rs` + `context-menu.ts`) stash what each `show_*` command was opened over, as
  `MenuState` does, and draw the same rows.

## The enabled fold

`apply_menu_item_states` writes three kinds of handle, so the fold has three answers: `menu_item_enabled` for every
command item, tracked or not, `field_item_enabled` for the explorer items the desktop holds in their own fields (pin
tab, show hidden, the four view-mode checks), and `submenu_enabled` for the pane submenus and Sort by. `entry_enabled`
is that rule over one row, which is what the demo and `bar_states_test.rs` read.

**Decision**: tracking decides only whether a rebind can reach a row, never whether it greys. **Why**: macOS builds the
app menu's rows (About, Settings, the license, Acknowledgements) and the Zoom presets untracked, and while untracked
meant "never written" they stayed live with a dialog refusing their command, so a click did nothing; the same rows on
Linux, tracked, greyed. The desktop keeps their handles in `MenuState.untracked_items` for this alone.

## Accelerators

- `to_accelerator` / `to_menu_text`: the frontend's spelling (`⌘⇧P`, or `Ctrl+Shift+P` captured on Linux) into Tauri's,
  for a platform. The bar's door has two refusals (the modifier floor and muda's key vocabulary); the popup's has none.
  The rationale for both lives with the desktop's builder: `apps/desktop/src-tauri/src/menu/DETAILS.md` § "Display-only
  accelerators".
- `glyphs`: Tauri's spelling back into what a macOS menu draws, modifiers in AppKit's order (⌃⌥⇧⌘). Only the demo draws
  its own bar, so only it calls this; the desktop lets AppKit draw.

## Clicks

`bar_click` reads the row: a check item's role decides `settings-changed`, `view-mode-changed`, or `menu-sort`; Close
tab and the four edit items get their own lanes (both depend on which window is in front, which the host knows); every
other item is `execute-command` with its command and scope. The desktop's `handle_menu_event` takes its view-mode and
sort lanes from here, and its context-menu lanes from `context_click` (below); the viewer bar's ids are its own.

## Context menus

**Decision**: every right-click menu is rows here (`context::file::file_menu`, `context::popups::*`), built from the
facts of one right-click; the desktop turns rows into muda items (`context_rows.rs`) and the demo into HTML.

**Why**: the demo (#302) needs right-click menus that say and do exactly what the native ones do, and the desktop's
builders couldn't be tested (a `muda::Menu` panics off the main thread), so their order and gating were pinned only for
the one submenu that had been made data. Now `context_test.rs` snapshots every menu's rows on both platforms, and
`every_row_the_crate_builds_has_a_click` keeps a row from drawing with nothing behind it.

- **Rows**: `ContextRow` is an item, a check, a submenu, the file menu's `Header` line (words the frontend rendered, so
  it composes no plural or number), a separator, or a `Slot`. An item carries its id, its `ContextLabel` (a catalog key
  with tokens, or final text), whether it's enabled, and where its accelerator comes from (`Shortcut`).
- **Accelerators**: `Shortcut::resolve` reads the user's live map (`ContextMenuShortcuts`, pushed with every popup) by
  the row's own menu id, a borrowed one (Show in folder shows Go to parent's), or a command id (a queue row's), through
  `to_menu_text` with no floor: a popup's keys are display text, never registered, so a bare `Space` belongs there. The
  viewer's text menu is the one with fixed chords, shared with the viewer bar (`viewer_copy_accelerator`).
- **Slots**: a group only the OS can fill sits in the rows as a `Slot` at its position, macOS only: Open with, the tag
  colors, Share, Google Drive, iCloud eviction, the File Provider's actions, and Services. The desktop builds each from
  its slow facts (`file_context_menu.rs`'s `fill_slot`), placeholders the live menu fills in included; a slot brings its
  own separators, since only the host knows whether it shows. The demo has no OS and leaves every slot out. Drive's and
  iCloud's items are still single-sourced here (`DRIVE_ITEMS`, `EVICTION_ITEMS`).
- **Picks**: `context_click` reads the id. A plain command is `Command` (the bar's unified lane); the rest act on what
  the popup went up over (a path, a volume, a host row), which the host stashed at popup time, or on what the id itself
  carries (a tab action, a date-sort pane). A slot's rows (`open-with:…`, `tag-color:…`, `share-service:…`,
  `fp-action:…`) are the host's and never classify here. The lanes' wire words (`VolumeAction::wire`,
  `NetworkHostAction::wire`) are the frontend's event enums' kebab-case, pinned by the desktop's
  `context_click_wire_tests`.
- **Typed IPC shared with the desktop**: `ContextMenuShortcuts`, `QueueRowMenuEntry` / `QueueRowMenuAction`, `PaneSide`,
  and `ViewerEditActionKind` live here with `specta` derives (the app's exact `specta`, like `RequestedSort`), and the
  desktop re-exports them, so `bindings.ts` names one type each.
