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
  live label reads (`labels`), and the accelerator vocabulary (`accelerators`).
- **The desktop** (`apps/desktop/src-tauri/src/menu/`): muda. Building the items, `MenuState` and its locks, the AppKit
  passes (icons, display accelerators, the Edit cleanup), the main/viewer swap, every context menu, `menu_t` resolution,
  and the parts of a click that need windows (is the main window focused, close the focused window, the native
  selector).
- **The demo** (`crates/cmdr-listing-wasm/src/menu.rs` + `apps/desktop/src/lib/demo/menu-bar.ts`): it stores the same
  pushes the desktop's menu-state commands store, asks this crate for the bar's state, and the page draws it.

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
sort lanes from here; its other lanes (context menus, the viewer bar) have ids this bar doesn't build.
