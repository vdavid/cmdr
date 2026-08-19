# Menu system details

Pull-tier docs for `src-tauri/src/menu/`: architecture, flows, and decision rationale. Must-know invariants and
gotchas live in `CLAUDE.md`.

Native menu bar for macOS and Linux. Builds platform-specific menus from scratch, handles menu
events, syncs accelerator labels with user-customized shortcuts, and enables/disables items based on
window focus context.

## File layout

- `mod.rs`: shared types (`MenuState`, `MenuItems`, `MenuItemEntry`, `MenuContext`,
  `NetworkHostMenuContext`, `CommandScope`, `ViewMode`), re-exports of the public API exposed by the
  submodules below, plus a glob re-export of `command_map` so the menu IDs and mapping functions
  stay reachable at `crate::menu::…`.
- `command_map.rs`: the menu item ID constants (all `*_ID`) and the ID mapping functions
  (`menu_id_to_command`, `command_id_to_menu_id`).
- `menu_items.rs`: small-piece builders and platform-aware helpers: `build_sort_submenu`,
  `build_zoom_submenu`, `register_item`, `register_sort_items`, `truncate_for_menu_label`, the `copy_path_accelerator` /
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
- `macos.rs`: `build_menu_macos`, the full macOS menu bar. Building only.
- `macos_appkit.rs`: the two passes that reach past Tauri into AppKit once the bar is built:
  `cleanup_macos_menus` (removes system-injected Edit items, registers the Help menu) and
  `set_macos_menu_icons` (SF Symbol icons via objc2 FFI) with its `MENU_BAR_ICONS` table.
- `open_with.rs` (macOS): `build_open_with_submenu` for the file context menu's "Open with"
  submenu. Returns the submenu plus a `bundle_id → app_path` map that callers stash in
  `MenuState.context.open_with_apps` so `on_menu_event` can resolve dynamic `open-with:<bundle-id>`
  click targets.
- `linux.rs`: `build_menu_linux` (full Linux/GTK menu bar with mnemonics, no F-key accelerators).
- `rebuild.rs`: `rebuild_menu_bar`, which throws the bar away and builds a new one in the current UI language.
- `mnemonics.rs`: `Mnemonics`, the per-submenu allocator for the Linux underline letter.

## Labels come from the message catalog

Every user-facing string in this module is a `menu.*` key resolved through `crate::intl::menu_t`, which reads the
`native_strings.gen.rs` table (generated from `messages/<locale>/menu.json` by `pnpm intl:native-strings`) plus the
active UI locale. Why the lookup lives in Rust rather than being handed over IPC: `src-tauri/src/intl/DETAILS.md`.

Three shapes are worth knowing here:

- **`menu_t_with` for the three labels that name what they act on** (`Copy "photo.jpg"`, `Eject (Backup)`, and the
  `(busy)` variant, plus `Open with`'s `{app} (default)`). It's a literal `{token}` replacement, the same raw pipeline
  the `errors.*` family uses on the frontend, NOT ICU: there is no ICU engine in the app process and importing one for
  four labels would be a bad trade. ❌ Don't add a fifth without asking whether the label can be reshaped instead.
- **`APP_MENU_TITLE` stays the literal `cmdr`.** macOS names the app menu after the application, so translating it would
  make the one item every macOS user navigates by unrecognizable, and it would earn a `sameAsSourceJustification` in
  every locale for nothing.
- **Every `PredefinedMenuItem` gets explicit text.** muda hardcodes English titles (`"Undo"`, `"Hide Others"`,
  `"Show All"`, `"Zoom"`, …) and only interpolates the app name into `About`/`Hide`/`Quit`; macOS does NOT localize them
  for us, because these are plain `NSMenuItem`s muda creates, not system-provided ones. (Verified by reading muda
  0.19.3's predefined-item source, 2026-08-19.) So a `None` text argument ships an English label into a translated menu.

### Rebuilding on a language change

A label can't be translated in place (muda has no `set_text` for a `Submenu` title, and the AppKit passes resolve items
through the live bar anyway), so `rebuild_menu_bar` builds the whole thing again. Two callers: the `set_ui_language`
command (the user picked a language in Settings) and `intl/live_locale.rs`'s emit site (the OS moved under a `'system'`
setting). Both first ask `refresh_active_locale` whether the answer actually MOVED, because a rebuild is a visible
flicker plus a round of frontend re-pushes.

What survives is what Rust knows: the show-hidden tick (read off the live item), the per-pane view modes and their
accelerator (`rebuild_view_mode_items` runs again afterwards), the licence wording (cached in
`MenuState.has_existing_license`, since the licence lookup isn't generic over the runtime), and which of the two macOS
bars is installed (`active_menu_kind`, so a focused viewer isn't yanked back to the main bar). Then `cleanup_macos_menus`
and `set_macos_menu_icons` run exactly as the focus-swap path does.

What does NOT survive is everything the frontend had pushed onto the old items: custom accelerators, the pin/unpin
label, the "Reopen closed tab" enabled flag, and the file-scoped enable/disable state. The `menu-bar-rebuilt` event
carries that news to `DualPaneExplorer.svelte`, which re-pushes all four. Re-pushing everything beats tracking what
moved: the event is rare, and a missed re-push is invisible until a user reaches for a shortcut.

### Linux mnemonics are allocated, not authored

A GTK mnemonic has to be unique within its submenu, and which letters are free depends on the words in the menu — so it
depends on the LANGUAGE. A hand-picked English set couldn't survive nine translations, and a translator can't be handed
a per-submenu uniqueness puzzle on top of translating. So `mnemonics::Mnemonics` allocates them at build time from the
translated labels, in menu order: word-initial letters first (what people scan for), then any other letter or digit,
then no marker at all if everything is taken (which costs one keystroke and nothing else). The menu bar's own titles get
one allocator; each submenu gets its own. It's a no-op on macOS, but the call sites are identical on both platforms so a
new item can't be given a mnemonic on one and not the other.

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
  `MenuState.context.paths`. The "Other…" entry shows an `NSOpenPanel` filtered to `.app`
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
- **Image-search group** (media_index): a folder's context menu carries TWO items, shown only while
  image indexing is enabled: chosen-folder membership ("Add to indexed folders" / "Remove from indexed
  folders", `media_index_{add,remove}_folder`) and the privacy veto ("Don't index images in this
  folder" / "Index images here again", `media_index_{exclude,include}_folder`). `show_file_context_menu`
  reads the four live facts into an `ImageIndexMenuState` (`gate::is_enabled` plus
  `network::config::{is_excluded, is_chosen_folder, is_covered_by_parent_folder}`) and the pure
  `media_index_items::image_index_menu_items` turns them into labels + enabled flags.

  **Decision: an add that would do nothing is DISABLED, never silently accepted.** The veto beats
  membership backend-side, and an ancestor entry already covers a child, so in both cases the add item
  shows disabled with the reason in its label; the un-exclude item sits right below it as the way out.
  Removal stays enabled even under the veto (it takes a real entry off the list). Why: a click that
  persists a list entry which indexes nothing is the kind of inferred model wave 1 removed.

  Like tag colors, both items act on the RIGHT-CLICKED folder (`MenuState.context.path`), so they're
  special-cased in `handle_menu_event` (NOT in `menu_id_to_command`) — but instead of doing the work in
  Rust each emits an event (`MediaIndexFolderExclusion` / `MediaIndexFolderChoice`) to the FE, which
  persists `mediaIndex.excludedFolders` / `mediaIndex.alwaysIndexFolders` through the SAME helpers the
  Settings list uses (`excluded-folders.ts` / `always-index-folders.ts`), so menu and Settings can't
  drift (the native menu can't write the FE settings store). Full backend flow:
  `media_index/DETAILS.md` § Per-folder photo-search exclude.

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

Because the position is a bare magic number, inserting or removing one item silently shifts every
`register_item` index after it, and the damage only shows up the first time a user rebinds that
shortcut: a DIFFERENT item gets removed and reinserted. `register_item_positions_match_submenu_order`
in `menu_items.rs` guards it by reading `macos.rs` / `linux.rs` with `include_str!` and checking each
registered index against the item's real slot in that submenu's `Submenu::with_items` /
`Submenu::with_id_and_items` array. Source
parsing is the only option available: building a real menu needs AppKit on the main thread. Submenus
assembled by a helper (`build_zoom_submenu`, `build_sort_submenu`) have no literal array in those
files, so their registrations are skipped. That is why the Sort by registrations live in
`menu_items::register_sort_items`, beside the builder that fixes their order: nothing over in the
platform files could have checked them anyway.

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

### Finding a menu from AppKit

Both macOS post-construction passes have the same problem: they work on `NSMenu` objects, and AppKit
indexes menus and items by title and nothing else. Tauri IDs don't reach that layer (muda sets no
`identifier` and no `tag` on the `NSMenuItem` it creates), and a title is user-facing text that
translation moves.

**Decision**: keep the ID as the key everywhere we own, and resolve it to a title at the last
moment. Each pass takes the `AppHandle`, reads `app.menu()` (whichever menu bar is installed right
now), finds the menu or item by ID there, and asks that object what title it currently carries. The
title then locates the `NSMenu` / `NSMenuItem`. The string being matched comes from the same object
AppKit drew, so a translated label matches itself.

**Why**: keying off English titles is a hard-rule violation (never classify by string-matching) and
breaks the moment a title is translated, silently in both cases: icons vanish, and AppKit's injected
Edit items come back.

This is why every menu-bar submenu is built with `Submenu::with_id_and_items` and an ID from
`command_map.rs`. Two IDs are shared with the viewer menu bar (`menu_structure.rs`): `EDIT_MENU_ID`
and `HELP_MENU_ID`, because `cleanup_macos_menus` runs against whichever bar is installed. Only one
bar is ever installed at a time, so the shared IDs never collide. `menu_items.rs` owns
`SORT_BY_MENU_ID` (its `build_sort_submenu` serves both platforms).

### macOS cleanup (objc2)

`cleanup_macos_menus(app)` runs post-construction via objc2 FFI:
1. Hands the Help menu (found by `HELP_MENU_ID`) to `NSApplication.setHelpMenu:` so macOS adds the
   search field. Tauri's `Submenu::set_as_help_menu_for_nsapp` resolves the live `NSMenu` itself, so
   this half needs no title at all.
2. Finds the Edit menu by `EDIT_MENU_ID` and removes the items AppKit injects into it (Writing
   Tools, AutoFill, Dictation, Emoji & Symbols), plus the separators they leave behind.

The injected items in step 2 carry none of our IDs (AppKit adds them after we build the menu), so
they're matched on `NSMenuItem.identifier` — AppKit's own API identity, listed in
`APPKIT_INJECTED_EDIT_ITEM_IDS`. Their TITLES would be the obvious key and are the wrong one: macOS
localizes them to the system language, so an English title match strips nothing on a Swedish Mac and
every injected item survives. Two of the four identifiers are private (`_NS…`), which is the price.

Measured on macOS 26.5.2 (2026-08-19), reading every Edit item at startup: our own items carry
AppKit's default identifier, the action selector name (`fireMenuItemAction:` for muda items, `undo:`
/ `redo:` for the predefined pair), so nothing of ours collides. macOS injects duplicates (two "Start
Dictation…", three "Emoji & Symbols"), which is why the removal loop takes every match rather than
the first.

Uses `objc2::exception::catch` because NSMenu operations can raise ObjC exceptions inside Tauri's
`did_finish_launching` callback, which aborts on panic.

### SF Symbol icons (macOS only)

`set_macos_menu_icons(app)` runs post-construction via objc2 FFI, walking the `MENU_BAR_ICONS` table
in `macos.rs` and calling `NSImage(systemSymbolName:)` + `setImage:` on each `NSMenuItem`. This
produces true template images that auto-tint on selection highlighting.

The table is `(menu ID, [(menu item ID, SF Symbol name)], nested)`, resolved to titles as described
above. `nested` recurses one level for View > Sort by; the recursion is uniform, so a second nesting
level would need no new code. Anything that fails to resolve logs a warning naming the ID and the
symbol, because the failure is otherwise invisible: the menu builds fine, just without an icon.
`menu_icon_ids_are_built_by_the_menu_bar` (in `macos.rs`) is the compile-time-ish guard, parsing
`macos.rs` and `menu_items.rs` for the IDs the menu bar actually constructs.

**Gotcha**: an accelerator update replaces the menu item (see "Accelerator sync"), and the fresh
`NSMenuItem` carries no image, so `update_menu_accelerator` re-applies the icons afterwards.

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

The **File** submenu's transfer group runs `Copy…` (F5), `Move…` (F6), `Duplicate` (⌘D), `Compress…` (⌥F5). `Duplicate`
carries no ellipsis because it picks nothing: it copies the selection into the folder it already sits in, and the
backend resolves the self-collision per item. The context menu (`menu_structure.rs`) offers it only when
`restrict_destination_actions` is false, so it is absent on the search-results virtual pane alongside `Rename`: each
selected item would have to land in its own real folder, which one transfer can't express. Its macOS SF Symbol is
`plus.square.on.square`, the Linux mnemonic is `D&uplicate`. What the command does once dispatched:
`apps/desktop/src/lib/file-explorer/pane/DETAILS.md`.

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
`arrow.down.circle` (Go to latest download), keyed by item ID in `MENU_BAR_ICONS`. On Linux the mnemonics are
`Go &to path…` and
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

A **second** entry point opens the same popup: `Changelog…` (`CHANGELOG_ID` (`"changelog"`)), placed directly below
`Check for updates…` (macOS cmdr menu; Linux: bottom of the Edit menu). It maps to the same `help.whatsNew` command, so
both menu items open the identical latest-five slice. Its macOS SF Symbol is `list.bullet.rectangle`. Because two menu
IDs share one command, `command_id_to_menu_id("help.whatsNew")` resolves only to `HELP_WHATS_NEW_ID`; that's fine since
neither carries a default shortcut (a future binding would just track the Help item).

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

**Decision**: `macos.rs` and `linux.rs` each keep their own `register_item` block, even though roughly 70 lines of it are identical.

**Why**: `register_item_positions_match_submenu_order` is a source-parsing test. It reads `macos.rs` and `linux.rs` with `include_str!`, pairs every `register_item(…, &submenu, N)` call against the literal `Submenu::with_items(…, &[…])` array in the SAME file, and fails when `N` doesn't point at that item. It's the only guard there is: building a real menu needs AppKit on the main thread, so a wrong index is otherwise invisible until a user edits a shortcut and a different item moves. The test explicitly skips any submenu assembled by a helper, because a helper's array isn't in the file being parsed — so lifting the shared registrations into one would hand back the duplication and take the guard with it. A submenu built by a helper is the one exception, and `register_sort_items` takes it: the guard already skips those submenus, so keeping their indices in the platform files bought no coverage while letting a reorder inside `build_sort_submenu` desync two hardcoded copies. Positions belong next to the array that sets them.

The wider version of this question (five of the seven menus have identical structure and differ only in labels and accelerators, so a per-platform data table could build them all) is a real option and would collapse both files, but it replaces this test rather than keeping it, and it reshapes a menu bar David reviews by eye. Not something to do as a side effect of a duplication pass.

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

**Decision**: A menu label ends with `…` when the dialog it opens can change WHAT the command acts on, not merely whether it runs.
**Why**: Apple's own phrasing ("requires further input") doesn't decide Cmdr's cases, because both of our big confirmations arrive pre-filled and are usually dismissed with Return. The copy/move dialog takes a destination that is genuinely steerable (it's the focused control, and confirm is blocked while the path is invalid), so the destination pane is a suggestion, not the command. The delete dialog can't retarget anything: the file set is fixed, and its trash-vs-permanent switch only picks between two commands that already exist as two menu items (`Delete` / `Delete permanently`), so flipping it is switching command, not steering this one. Hence `Copy…` / `Move…` / `Compress…` / `New folder…` / `Search files…` / `Go to path…` / `Select files…`, and bare `Delete`, `Rename` (inline edit, no dialog), `Add to favorites`, `Operation log`, `What's new`, `Acknowledgements`, `Get info`. The looser reading ("a dialog appears") was rejected: nearly every destructive command in Cmdr shows something, so under it the mark lands on almost everything in the File menu and stops carrying information. `Check for updates…` is the one deliberate exception to the rule, kept because Sparkle-style updaters have made that exact label near-universal on macOS and dropping the ellipsis reads as a typo.

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
