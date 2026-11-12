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

## Three platforms

`Platform` is `MacOs`, `Windows`, and `Linux`; `PerPlatform` carries one arm each, and a row's `on` says which bars it
sits in (`EVERY_PLATFORM`, `MACOS_ONLY`, `WINDOWS_AND_LINUX`). Nothing is a leftover: a helper names its rule
(`same_on_all`, `cmd_or_ctrl!`, `macos`, `split(macos, windows, linux)`, `macos_only`, `windows_and_linux`).

**Decision**: the Windows bar has the Linux bar's rows, accelerators, and label-spelled display shortcuts
(`WINDOWS_MENU_BAR` in `bar_test.rs` equals `LINUX_MENU_BAR` today). **Why**: Cmdr doesn't run on Windows yet, and the
Windows menu model (whether a Win32 accelerator fires before WebView2, what `Alt+<letter>` does with a bar present) is
task 63 of `docs/specs/windows-port-plan.md`. Windows is its own value so that decision is a row edit with its own
snapshot diff.

Two `windows_and_linux` rows stand in for what AppKit gives macOS: File > Quit / Exit Cmdr (`QUIT_ID`, one label key
with a per-OS branch, and no key shown or registered, since `Alt+F4` closes the window through the quit gate there) and
View > Full screen (`VIEW_FULL_SCREEN_ID`, `F11` shown only, as GTK takes bare F-keys before the webview). macOS's Quit
is the app menu's predefined item, and AppKit adds its own full-screen item to the View menu.

## Parity with the registry

**Decision**: the bar keeps its own accelerators (a reviewed snapshot, buildable standalone for the demo), and
`bar_parity_test.rs` ties them to the frontend registry: on each platform, every row with a command
(`menu_id_to_command`, plus the left pane's view-mode pair, which holds the active pane's keys at build) registers and
shows exactly what a push of its first default key would (`rebound_row_key`, the door `update_menu_accelerator` takes),
compared as modifier set plus key. **Why**: the bar is a second copy of the default keys, and nothing else noticed when
the two drifted; generating the bar from the registry would move registry data into Rust and lose the deliberate
per-platform omissions.

- The registry side is `src/registry_default_keys.tsv` (`platform`, command id, first default key, one line per
  `menuCommands` entry the OS offers under its capability fixture). The frontend's `lib/commands/default-keymap.test.ts`
  writes it with `toMatchFileSnapshot`, so a stale file fails there; the Svelte lane's inputs include it.
- `EXCEPTIONS` lists each row that differs on purpose with its reason: rows whose first default can't be an accelerator
  so the bar uses the second (`⌘+`, `Backspace`), and Windows's view-mode rows, which register Linux's `Ctrl+1` /
  `Ctrl+2` where Windows puts TC's `Ctrl+F2` / `Ctrl+F1` first (the bar leaves F-key chords to keydown dispatch). An
  exception that stops differing, or names a row the bar no longer builds, fails `every_exception_still_differs`.
- macOS's bare F-keys, `Tab`, and `Shift+Space`, Select same kind's display-only `⌥⇧=` / `Alt+Shift+=`, and every key a
  Windows or Linux row only shows (the F-keys GTK takes first, `Tab`, the two zoom chords, Go to parent's `Alt+↑`, Move
  here's `Ctrl+Shift+V`) need no exception: a push of a row's own key comes back as the bar built it (`rebound_row_key`;
  `apps/desktop/src-tauri/src/menu/DETAILS.md` § "Display-only accelerators"). A row that registers nothing on a
  platform still shows its key there: a display-only key that went unshown was the F-key rows' state before.

## Keys the page owns

**Decision**: the bar never registers a key the page must get, past a row's own built key. `src/page_owned_keys.tsv`
(`platform`, frontend combo, why), written by `lib/commands/default-keymap.test.ts` like the registry file, lists them:
each platform's native text keys (`routes/(main)/native-text-keys.ts`) and, on Windows and Linux, every key of the
windows that wear the main bar (the operation queue, About). `rebound_row_key` keeps a pushed one display-only
(`page_owns`), and `bar_parity_test.rs` fails a built accelerator on the list unless `PAGE_OWNED_BY_DESIGN` names it
(Edit > Copy, whose handler copies a field's text; Select all, the queue's own road to its ⌘A). **Why**: a bar
accelerator fires before the page. A shadowed `⌘[` hands Go back its last key, `⌥←`, and registered, it fired Go back
from every text field instead of word-left; Go to parent's `Alt+Up` would sit on the queue window's Move earlier.
(Verified in the Linux lane with real X keys on 2026-11-12: with `Alt+Up` registered on the Go row, the queue window
still received `Alt+↑`, so GTK didn't take it there; the row only shows it now regardless, and
`menu-mnemonic-mask.spec.ts` keeps pressing it.)

## Accelerators

- `to_accelerator` / `to_menu_text`: the frontend's spelling into Tauri's, for a platform. The frontend sends each
  platform its own spelling (`⌘⇧P` on macOS, `Ctrl+Shift+P` on Windows and Linux), and each modifier token names one
  physical key on every platform: ⌘ and `Super` are the Meta key (`Cmd` on macOS, `Super` elsewhere), ⌃ and `Ctrl` are
  Control. ❌ No platform conversion here (a ⌘ is never rewritten to Ctrl): the registry holds a key list per platform.
  The browser demo's macOS bar reads a Windows visitor's `Ctrl+F` as `⌃F`, the key they hold (`menu_test.rs` pins it).
- The bar's door has two refusals (the modifier floor and muda's key vocabulary); the popup's has none. The rationale
  for both lives with the desktop's builder: `apps/desktop/src-tauri/src/menu/DETAILS.md` § "Display-only accelerators".
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
the one submenu that had been made data. Now `context_test.rs` snapshots every menu's rows on every platform, and
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
