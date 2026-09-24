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
- `menu_bar.rs`: `MENU_BAR`, the menu bar for both platforms as data, one row per item with every platform difference
  marked on its row, plus `SHOW_IN_FILE_MANAGER_KEY`, the per-platform LABEL key the file context menu shares
  ("Show in Finder" / "Show in file manager"). `menu_bar_test.rs` pins both bars as text. Why it's data: the menu-bar
  Decision under "Key decisions".
- `menu_spec.rs`: the words `MENU_BAR` is written in (`item`, `check`, `submenu`, `macos_only`, `both`, `macos`,
  `split`, …) and `Platform` / `PerPlatform`, where a platform difference resolves. No `cfg`, so both bars exist on
  every host.
- `menu_bar_builder.rs`: `build_menu`, which builds `MENU_BAR` for `Platform::current()`, allocates the mnemonics,
  registers the tracked items, and hands back `MenuItems`.
- `display_accelerators.rs` (macOS): `set_display_accelerators`, the pass that draws a shortcut the bar can't register
  as a right-aligned, dimmed run on an item's attributed title. See "Display-only accelerators" below.
- `menu_items.rs`: small shared pieces: `APP_MENU_TITLE`, `pin_tab_label`, `truncate_for_menu_label`, the
  `SameKindTarget` payload and `same_kind_menu_label` behind "Select all of the same kind"'s live wording, plus
  `DetachWord` / `detach_label`, which decide whether a row's leave-this-volume item reads `Eject ({name})` or
  `Disconnect` (a phone gets the second: `adb` has no per-client detach, so nothing is made safe to unplug).
- `file_context_menu.rs`: the file context menu (`build_context_menu`) and the facts it is built from
  (`FileContextInfo`, `ContextMenuPaneFacts`, `ContextMenuResult`), plus `append_tag_color_group`. Its own file because
  it is by far the biggest menu here, and the only one whose shape depends on the row, the pane, the cloud provider,
  and the OS at once. ❗ `context_menu_icons_test.rs`'s guard test `include_str!`s THIS file to check every icon names an
  item the menu actually builds; a builder moving out of it has to take that `include_str!` along.
- `menu_structure.rs`: the smaller context menus — breadcrumb (with the `detach_label` item) / parent-row / tab /
  network-host / function-key-bar — the viewer-window menu (`build_viewer_menu`), and the `ContextMenuShortcuts` /
  `context_item` vocabulary every popup here shares. A volume switcher row's, a favorite's, and a servers-hub place's
  actions are NOT here: they're the in-app `Menu`'s, one list in
  `apps/desktop/src/lib/file-explorer/navigation/row-menu.ts`.
- `selection_submenu.rs`: the file context menu's `Selection >` submenu, its rows held as data (`SELECTION_ROWS`) so a
  unit test can pin their order without building a `muda::Menu`.
- `install.rs`: `at_startup`, the single call `lib.rs` makes in `setup`: pin the UI language, build the bar,
  run the macOS AppKit passes, and place the `MenuState` everything else mutates. Order inside is load-bearing.
- `item_states.rs`: `apply_menu_item_states` (the one writer of every menu item's enabled state, derived from stored
  inputs through the pure `menu_item_enabled` plus `viewer_text_edit_enabled` for the viewer bar's Cut / Paste),
  `note_viewer_search_focus` (which viewer's search box holds focus), `set_menu_context` (records which window's menu
  the explorer items answer to), and the macOS-only `swap_to_main_menu` / `swap_to_viewer_menu` (the app-level menu-bar
  swap). They're called from `commands::menu_state::activate_window_menu` and the other menu-state IPC commands.
- `menu_handlers.rs`: `handle_menu_event`, the `.on_menu_event` dispatcher, plus the macOS
  post-construction wrappers `cleanup_macos_menus` / `set_macos_menu_icons` and
  `send_native_edit_action` over the pure `native_edit_selector_for` (the actual objc2 FFI lives in `macos_appkit.rs`)
  and `focused_viewer_label` / `viewer_edit_action_for`, which route the viewer bar's own items to the viewer in front.
- `accelerators.rs`: `frontend_shortcut_to_accelerator` (frontend glyphs to Tauri accelerator
  strings) and `update_menu_item_accelerator` (swapping one on a live item).
- `view_mode_items.rs`: `rebuild_view_mode_items` (full remove/recreate/reinsert when the active pane
  or a shortcut moved) and `sync_view_mode_check_states` (the cheap check-state-only path).
- `macos_appkit.rs`: the two passes that reach past Tauri into AppKit once the bar is built:
  `cleanup_macos_menus` (removes system-injected Edit items, registers the Help menu) and
  `set_macos_menu_icons` (SF Symbol icons via objc2 FFI) with its `MENU_BAR_ICONS` table.
- `services_context.rs` (macOS): `Services` in the file context menu. `append_services_submenu` puts
  the (empty) item there; `lend_services_menu` borrows AppKit's own Services menu onto it for as long
  as the menu is up, and points it at the right-clicked rows. See "Services in the right-click menu".
- `context_menu_facts.rs` (macOS): the file context menu's slow facts, asked off the main thread on the menu's own
  `framework_pool` and collected under the 100 ms `GRACE`; `file_context_info` turns what answered into the
  `FileContextInfo` the builder reads, `Slow::Pending` for the rest. See "Slow facts and the live menu".
- `context_menu_live.rs` (macOS): `lend_live_menu`, which lands every fact that missed the grace period on the menu
  while it's open (`LateTargets`, `Placeheld` submenus, `SlotGroup` rows). Same section.
- `open_with.rs` (macOS): `build_open_with_submenu` for the file context menu's "Open with"
  submenu (`OPEN_WITH_SUBMENU_ID`). Returns the submenu plus a `bundle_id → app_path` map that callers stash in
  `MenuState.context.open_with_apps` so `on_menu_event` can resolve dynamic `open-with:<bundle-id>`
  click targets. `build_pending_open_with_submenu` / `fill_open_with_submenu` are the late pair.
- `share_submenu.rs` (macOS): `build_share_submenu` for the file context menu's `Share` (`SHARE_SUBMENU_ID`), one
  plain item per service in `FileContextInfo::share_services`, plus the `share-service:<index>` id
  pair (`share_service_id` / `share_service_index`). `build_pending_share_submenu` / `fill_share_submenu` are the late
  pair. The services themselves and the click side live in `file_system/share.rs`.
- `file_provider_items.rs` (macOS): the file context menu's File Provider group, `append_file_provider_group` over a
  `ProviderOffer`, plus the `fp-action:<index>` id pair (`file_provider_action_id` / `file_provider_action_index`),
  and `append_pending_file_provider_group`, its `PENDING_SLOTS` hidden slots for a late offer. The offer itself and the
  click side live in `file_system/file_provider_actions/`.
- `context_menu_icons.rs` (macOS): every image the file context menu carries, through `lend_context_menu_icons` and the
  pure `image_runs`: the `FILE_CONTEXT_ICONS` SF Symbols, the provider logo on each provider action, the app icons in
  "Open with", each service's icon in `Share`, and the tag circles. See "Images on a CONTEXT menu".
- `provider_logos.rs` (macOS): `PROVIDER_LOGOS`, which provider's logo is which, matched by app bundle ID, embedding the
  SVGs in `provider_logos/`. See "Provider logos on a CONTEXT menu".
- `context_menu_header.rs`: the file context menu's first line, naming what the menu will act on.
  `append_context_menu_header` (the disabled item plus its separator, both platforms), the `ContextMenuTargetFacts` the
  builder takes, the `ContextMenuTarget` the IPC command deserializes (the field's rationale is this feature's, so it
  lives with the feature rather than beside `PaneContextMenuFacts`), and — macOS only, in a nested `macos` module —
  `lend_context_menu_header`, which restyles it as a header on the tracking notification. See "The context menu's header
  line".
- `tag_row/` (macOS): the file context menu's Finder tag colors as one row of circles. `model.rs` holds the portable
  rules (the `SWATCHES` color table, layout, hit-testing, glyphs, captions, `find_tag_run`), `view.rs` the `NSView` that
  draws them, and `loan.rs` `lend_tag_row`, which installs the row on the tracking notification. See "The tag row".
- `tag_icons.rs` (macOS): the circle PNGs for the seven plain tag items, the look the row falls back to.
- `media_index_items.rs`: `image_index_menu_items`, which decides the image-search group's labels and which of them are
  clickable.
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
- **Cmdr's capitalization wins over Apple's, including on the standard items.** `menu.app.showAll` is `Show all` and
  `menu.app.hideOthers` is `Hide others`, though Apple writes both Title Case. This menu bar is sentence case
  throughout (`Go to path…` against Apple's `Go to Folder…`), the palette and shortcuts list name the same commands
  through `commands.app*.label`, and, per the bullet above, these are our own `NSMenuItem`s sitting next to no
  system-supplied sibling to match. ❌ Don't "restore" Apple's casing on those two: it reopens the split where one
  command had two names.

### Rebuilding on a language change

`rebuild_menu_bar` builds the whole bar again rather than retitling it in place. Every label moves at once, the Linux
mnemonics have to be reallocated from the new words (below), and `PredefinedMenuItem`s carry text of their own, so a
walk that retitled each item would be the build, minus the guarantee that it covered everything. Two callers: the `set_ui_language`
command (the user picked a language in Settings) and `intl/live_locale.rs`'s emit site (the OS moved under a `'system'`
setting). Both first ask `refresh_active_locale` whether the answer actually MOVED, because a rebuild is a visible
flicker plus a round of frontend re-pushes.

What survives is what Rust knows: the show-hidden tick (read off the live item), the per-pane view modes and their
accelerator (`rebuild_view_mode_items` runs again afterwards), the licence wording (cached in
`MenuState.has_existing_license`, since the licence lookup isn't generic over the runtime), and which of the two macOS
bars is installed (`active_menu_kind`, so a focused viewer isn't yanked back to the main bar). Then `cleanup_macos_menus`
and `set_macos_menu_icons` run exactly as the focus-swap path does.

What does NOT survive is everything the frontend had pushed onto the old items: custom accelerators, the pin/unpin
label, the "Reopen closed tab" enabled flag, the "Select all of the same kind" label, and the file-scoped
enable/disable state. The `menu-bar-rebuilt` event carries that news to `DualPaneExplorer.svelte`, which re-pushes all
five. Re-pushing everything beats tracking what
moved: the event is rare, and a missed re-push is invisible until a user reaches for a shortcut. "Open terminal here"
needs no re-push of its own: its verdict is stored in `MenuState` and re-applied by the `activate_window_menu('main')`
that handler already makes.

### Linux mnemonics are allocated, not authored

A GTK mnemonic has to be unique within its submenu, and which letters are free depends on the words in the menu — so it
depends on the LANGUAGE. A hand-picked English set couldn't survive nine translations, and a translator can't be handed
a per-submenu uniqueness puzzle on top of translating. So `mnemonics::Mnemonics` allocates them at build time from the
translated labels, in menu order: word-initial letters first (what people scan for), then any other letter or digit,
then no marker at all if everything is taken (which costs one keystroke and nothing else). The menu bar's own titles get
one allocator; each submenu gets its own. The builder runs every label through it on both platforms (a no-op on macOS),
so no row in `menu_bar.rs` names a mnemonic, and a new item can't get one on one platform and not the other.

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
- **Function key bar context menu**: one item (`FUNCTION_KEY_BAR_HIDE_ID`), emitting the payload-less
  `FunctionKeyBarHideRequested` to the main window. Nothing is stashed in `MenuState.context`, because there's no
  right-clicked target to remember: the frontend owns both the setting write and the toast that offers the way back
  (`src/lib/file-explorer/pane/function-key-bar-hide.ts`)
- **Open with** (macOS): items have dynamic IDs like `open-with:com.apple.Xcode` that can't be
  enumerated in `menu_id_to_command`. `on_menu_event` prefix-matches `open-with:` and calls
  `file_system::open_with::open_paths_with` directly, looking up the app URL via
  `MenuState.context.open_with_apps[bundle_id]` and the launch paths via
  `MenuState.context.paths`. The "Other…" entry shows an `NSOpenPanel` filtered to `.app`
  bundles and launches the chosen app the same way.
- **Finder tag colors** (macOS): the file context menu carries seven circle items
  (`file_context_menu.rs::append_tag_color_group`, shown for files AND folders), IDs `tag-color:<1..=7>`,
  which `tag_row/` draws as Finder's one row of circles once the menu tracks ("The tag row"). Like "Open with", they're
  prefix-routed
  (`on_menu_event` matches `tag-color:`) — NOT in `menu_id_to_command` — and call
  `file_system::tags::toggle_color` on the RIGHT-CLICKED selection (`MenuState.context.paths`),
  then `apply_tags_to_listing(MenuState.context.tags_listing_id, …)`. Acting on the right-clicked set
  is why they can't route through `execute-command`: a frontend command reads the *focused-pane*
  selection, which differs when the right-click lands on an unselected row. The xattr write runs on
  `spawn_blocking` (off the main/menu thread). The keyboard-assignable `tags.toggle*` commands cover
  the focused-selection case via the frontend (`pane-commands.ts::toggleTagOnFocusedSelection` →
  `toggle_tags` IPC); no default shortcut.
  - **Applied = every selected path carries the color** (`FileContextInfo.applied_tag_colors`, computed from
    `tags::applied_colors` at menu-build time); `toggle_color` then removes it (all-have) or adds it (some/none have).
    The row's hover caption says which of the two a click does, and nothing else can happen.
  - **The plain items' look is the fallback**: an applied color's circle composites a white check INTO it rather than
    leaning on a gutter checkmark (`tag_icons.rs`, 36 px shown at 18 pt, in the row's light-mode colors with its
    darkened ring baked in). The 14 PNGs are cached once in a `LazyLock`, and `context_menu_icons.rs` puts them on the
    items like every other context-menu image. macOS-only — Linux menus carry no icons.
- **Share** (macOS): a submenu of one item per service macOS offers for the RIGHT-CLICKED rows, built by
  `share_submenu.rs` from the enumeration `show_file_context_menu` made. Ids are
  `share-service:<index>` into that offer, prefix-routed in `handle_menu_event` rather than listed in
  `menu_id_to_command` — the same reason `open-with:` and the tag colors are. Why it's hand-built,
  what the empty case is, and where the offer lives: `file_system/DETAILS.md` § "The Share submenu",
  plus `src/lib/file-explorer/pane/DETAILS.md` § "Sharing a row" for the pane gate.

  **The click is deferred one main-thread turn** (`run_on_main_thread`) rather than performed inline.
  `on_menu_event` runs while muda's own menu-tracking loop is still unwinding, and a service usually
  puts a window or sheet up, which that unwind can dismiss.
- **Provider actions** (macOS): one flat line per File Provider action the rows' provider offers,
  below Cmdr's own cloud items, in the provider's own words and, for a known provider, with its logo
  (`file_provider_items.rs`; the logos: "Provider logos on a CONTEXT menu"). Ids are `fp-action:<index>` into the offer
  `MenuContext.file_provider_offer` keeps, prefix-routed in `handle_menu_event` like `share-service:`,
  since the items exist only once File Provider vouched for the rows, which a palette entry or shortcut
  couldn't check first. The click runs `file_provider_actions::perform` off the main thread; the
  provider shows its own windows. `file_system/DETAILS.md` § "File Provider actions".
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
- `items: HashMap<String, MenuItemEntry>` for every `Tracking::Tracked` row in `menu_bar.rs`, the items
  that need accelerator updates and enable/disable
- `context: MenuContext` for right-click context menu: `path` (primary right-clicked file),
  `filename`, `paths` (full selection if the right-clicked file is part of it, else `[path]`),
  and (macOS) `open_with_apps` (`bundle_id → app_path` map populated when "Open with" submenu
  is built, consumed by `on_menu_event` on click)

### Accelerator sync

Menu accelerators must match user-customized shortcuts. An update removes the old item, creates a new
one carrying the new accelerator, and reinserts it at the same position.
`update_menu_item_accelerator()` handles regular items via the
HashMap; `rebuild_view_mode_items()` handles the four per-pane view-mode CheckMenuItems together
because they share a single accelerator pair (⌘1 / ⌘2 by default) that "follows" the active pane.

⚠️ **This design is stated everywhere as "Tauri has no `set_accelerator()`", and that is false.** Tauri 2.11.5 exposes
one on all three item kinds (`tauri-2.11.5/src/menu/normal.rs:110`, `check.rs:112`, `icon.rs:191`), over muda 0.19.3's
own (`muda-0.19.3/src/items/normal.rs:104`) — read from the vendored sources, 2026-09-16. So the machinery this section
describes is probably reducible to one call per item, and an in-place set would ALSO keep the item's SF Symbol and its
attributed title instead of destroying and restoring them (see "Display-only accelerators"). Nobody has tried it; it's
its own change, gated on the `menu_bar_test.rs` snapshot plus a live rebind on both platforms. ❗ Whatever replaces
this must keep parsing what it emits: `set_accelerator` swallows a string muda can't parse exactly the way
`MenuItem::new` does (`accelerator.and_then(|s| s.as_ref().parse().ok())`).

The frontend triggers regular-item updates via `invoke('update_menu_accelerator')` from
`shortcuts-store.ts`, and triggers view-mode rebuilds via `invoke('update_view_mode_menu')` from
`DualPaneExplorer.svelte` on focus change, swap, and any view-mode toggle.

`menu_bar_builder.rs` registers every `Tracking::Tracked` row in `MenuState.items` at its own index among
the rows its submenu shows on this platform, nested submenus (Sort by) included. There's no hand-typed
position anywhere: the index stored IS the item's index in the rows the submenu is built from, so adding,
moving, or platform-tagging a row can't desync one from the other.

### Display-only accelerators

Three Select-menu rows run on `*`, `+`, and `-`. None of those keys carries ⌘, ⌃, or ⌥, and a menu-bar accelerator with
no such modifier is fired by AppKit app-wide, ahead of the webview: registering `⇧8` would stop `*` reaching every text
field in the app. So the file pane's keydown handler owns the keys and the menu only says what they are.

"Select all of the same kind" is a fourth row on the same path for a different reason. `⌥⇧=` WOULD clear the floor, and
`frontend_shortcut_to_accelerator` really does answer `Some("Alt+Shift+Equal")` for it — but David wants `⌥+` free for
typing, so the row is built with `displayed_item`, which pairs the glyph with `NONE` by construction. Nothing pushes a
default binding through `update_menu_accelerator` (only CUSTOM shortcuts are re-pushed, `shortcuts-store.ts`), so the
default stays display-only; a user who deliberately rebinds the command to a ⌥ combo gets a real accelerator, which is
what "a rebind stays honest" means below.

❌ **No display rule prettifies the glyph** (`⌥⇧=` → `⌥+`, `⇧8` → `*`). The menu shows the physical combo because that
is true on every layout, and David types on a custom mixed English/Hungarian one where the "friendly" spelling is wrong.
The numpad spelling is the command's SECOND default (`['⌥⇧=', '⌥+']`), not a display alias.

**The modifier floor** is where that rule lives. `frontend_shortcut_to_accelerator` answers `None` for any combo
without one of the three (Shift alone doesn't clear it, since `⇧8` IS `*`), so a bare key can't reach a menu item by
accident — including through a rebind in Settings > Keyboard shortcuts, which is how one used to. The unfloored
conversion is `frontend_shortcut_to_menu_text`, and it belongs to popups alone (above).

**The second refusal is a key muda can't name** (`is_codeable_key`). muda parses an accelerator into a `Code`, a
PHYSICAL key, so every shifted character falls in a hole: `+`, `*`, `(`, `_` have no `Code` of their own, only the
unshifted key they sit on. `view.zoom.in` ships `⌘+`, so this is a combo the app really produces, and a non-US layout's
letters (`ö`) land in the same hole. ❌ Don't paper over it by emitting `Cmd+Shift+Equal`: which physical key types `+`
is a layout question, and guessing wrong binds something else. It routes to the display path instead, which is honest —
the glyph shows and the frontend's keydown dispatch runs the command.

❗ **Both refusals exist because the failure they prevent is SILENT.** `tauri::menu::MenuItem::new` is
`accelerator.and_then(|s| s.as_ref().parse().ok())` (`tauri-2.11.5/src/menu/normal.rs:65`): a string muda rejects is
discarded and the item is built with no accelerator at all — no error, no panic, no log line, indistinguishable from a
command nobody bound. `Opt` (muda accepts `OPTION` and `ALT` only, `muda-0.19.3/src/accelerator.rs:534`) cost Copy path,
Show in Finder and every rebound ⌥ combo their menu keys for the app's whole life, and `Cmd+Plus` cost Zoom in its own,
while green unit tests compared the output to strings we had made up. So `accelerators.rs` now PARSES what it emits, and
a second test parses every accelerator `MENU_BAR` hardcodes. ❗ Keep both: a string comparison cannot see this class of
bug.

**Two ways the glyph reaches the user.** `ItemSpec::display_accelerator` carries ONE string for both platforms, set by
`menu_spec::displayed_item`, which pairs it with `NONE` so a row can't register and display a shortcut at once
(`menu_bar_test.rs` pins that too). `display_accelerator_label` is where they part:

- **Linux** spells it into the label (`Invert selection (⇧8)`). GTK offers no other hook. The mnemonic is allocated on
  the bare label first, so no underline lands inside the parens.
- **macOS** leaves the label alone and `display_accelerators.rs` sets an attributed title shaped `"{label}\t{glyph}"`:
  a right `NSTextTab` puts the glyph in the key-equivalent column, and `secondaryLabelColor` on that run alone dims it
  while the label keeps its own color. The font attribute is load-bearing — an attributed title with no
  `NSFontAttributeName` falls back to the system face, not the menu font.

The two blocks in the `menu_bar_test.rs` snapshot diverge for this submenu because of this, and that's correct.

**Placing the tab stop.** AppKit lays a menu out as `widest label + gap + widest key equivalent` and right-aligns the
key equivalents against the content's right edge. `tab_stop_for` reproduces that, measuring the labels (from muda's own
text, which is always plain) and every sibling's key equivalent (reconstructed from `keyEquivalent` and its modifier
mask). Only the gap is a constant, `KEY_EQUIVALENT_COLUMN_GAP`, because AppKit exposes no metric for it and the laid-out
width isn't known until the menu opens. Nothing here may be a fixed column: a translated label can be half again as long
as the English one.

**❗ Re-apply it wherever a fresh `NSMenuItem` can appear**, which is every place `set_macos_menu_icons` runs: the
startup install, the main↔viewer swap, `rebuild_menu_bar`, and after `update_menu_item_accelerator`'s remove/recreate.
Miss one and the glyph vanishes after a rebind or an app-switch, with nothing but a log line to say so.

**❗ `setAttributedTitle:` also rewrites `title`.** A styled item reads back as `"Invert selection\t⇧8"`, so
`find_ns_item` matches only up to the first tab. Without that, the SF Symbol pass stopped finding those three rows and
their icons went missing on the first menu-bar swap (seen in the running app on macOS 26, 2026-09-16). No label of ours
holds a tab for any other reason.

**A rebind stays honest.** `update_menu_accelerator` computes both halves: a combo that clears the floor becomes a real
accelerator, anything else becomes the displayed one, never both. It records the verdict in
`MenuState.display_accelerators` so a later menu swap redraws what the user bound rather than what `MENU_BAR` was built
with, and `MenuItemEntry::label` keeps the label without a shortcut spelled into it so a Linux rebind can't stack a
second `(⇧8)` on the end. The glyph is the frontend's own spelling (`⇧8`), ❌ not the Tauri one (`Shift+8`): it is drawn,
never parsed. A word key shows as `Backspace` where AppKit would draw `⌫`; nobody has hit that yet, and the fix would be
a display-spelling table Rust doesn't have today.

### The one item whose LABEL changes: "Select all of the same kind"

Two items in the bar say something different depending on state. `Pin tab` flips between two whole catalog keys
(`Label::License` does the same for the licence row, and both are resolved at BUILD time). "Select all of the same
kind" is the third and the only one rewritten AFTER the build, from data only the frontend has: what the focused pane's
cursor row is. It reads "Select all folders", "Select all with extension *.pdf", "Select all files with no extension",
or the neutral wording when the row implies no kind.

**The words are rendered in Rust**, by `menu_items::same_kind_menu_label` from a typed `SameKindTarget` (wire-identical
to the frontend's `pane/select-same-kind.ts` type, pinned by a deserialization test). ❌ Never a string the frontend
composes: the bar speaks the language `crate::intl` resolves, which is not necessarily the webview's.

**`MenuState::set_item_label` is how a tracked item's label moves**, and it is not a bare `set_text`, because two other
things hang off the label:

- Linux spells the display-only shortcut INTO the label, so the new one has to be recomposed with it. The effective
  shortcut is `MenuState.display_accelerators` first (what a rebind put there), then `menu_bar::spec_display_accelerator`
  as the fallback — ❗ in that order, or a rebind's glyph reverts on the next cursor move.
- `MenuItemEntry::label` is what `update_menu_item_accelerator` rebuilds the item from, so leaving it stale would mean a
  later rebind silently restored the neutral wording.

❗ **On macOS the caller must follow with `set_display_accelerators`.** `set_text` replaces the attributed title with a
plain one, and the dimmed `⌥⇧=` goes with it. `update_select_same_kind_menu` does exactly that, and the pass is cheap
enough to re-run per push (it walks the spec, finds only the Select menu wants glyphs, and re-measures one tab stop).

**The push is debounced 200 ms and skipped when the words wouldn't change**, since holding an arrow key is a burst whose
only interesting value is the last one. The store behind it (`pane/same-kind-target.svelte.ts`) is the single source
every surface's label comes from: who publishes into it, who pulls versus who is pushed to, and why there may be only
one publisher are all in `src/lib/file-explorer/pane/DETAILS.md` § Select all of the same kind.

A language rebuild throws the item away, so `DualPaneExplorer`'s `menu-bar-rebuilt` handler calls `resyncSameKindMenu()`
alongside the other frontend-only re-pushes.

### Where a CONTEXT menu's accelerator comes from

All of that is the menu BAR's problem. A popup menu is rebuilt from scratch on every right-click, so it needs no
tracking, no remove/reinsert, and no sync: it just reads the registry as it stands at that instant.

The frontend sends it. `showFileContextMenu` / `showBreadcrumbContextMenu` (`lib/tauri-commands/file-actions.ts`) put a
`shortcuts` map in the payload — every command that has a binding right now, as `commandId → combo`, in the frontend's
canonical spelling. It arrives as `ContextMenuShortcuts`, and each item asks it by MENU id
(`ContextMenuShortcuts::for_menu_item`), which resolves through `menu_id_to_command`, so an item's label can't come to
describe a different command than the item runs. `context_item()` takes that id once and uses it for both.

Two things to keep straight:

- ❌ **Never `frontend_shortcut_to_accelerator` here — `frontend_shortcut_to_menu_text` is the popup's door.** A popup's
  key equivalents are never registered with the app, so its accelerators are pure display text and a bare `Space` (the
  only place the toggle-selection key is discoverable) or `F5` is fine. The bar's modifier floor would silently drop
  nine of the file menu's fifteen labels.
- ❌ **Never a literal accelerator string on a context-menu item.** A literal starts lying the moment someone rebinds
  that command, and nothing catches it. An item whose command has nothing bound shows nothing, which is the honest
  answer; `None::<&str>` stays right for an item with no command behind it at all (`favorites_add_context`, the Drive
  and cloud items, Quick Look).

⚠️ **Known limitation: a shifted-character combo still draws no key here.** `frontend_shortcut_to_menu_text("⌘+")`
answers `Cmd++`, which muda can't parse, so `MenuItem::with_id` discards it and the row comes up bare. It costs nothing
today (the same row drew nothing before the popup read the registry at all), and it only bites a user who rebinds a
context-menu command to a combo whose key is a shifted character. The real fix is a display-spelling table that renders
`⌘+` as `⌘+` rather than round-tripping through Tauri's accelerator syntax; ❌ don't reach for
`frontend_shortcut_to_accelerator`'s `is_codeable_key` refusal here, which would drop the label entirely instead of
drawing a wrong one.

### The `Selection >` submenu

The file context menu's selection group is a submenu (`selection_submenu.rs`), keyed `menu.context.selection`, holding
everything the Select menu bar has plus Toggle selection at the head. Order: Toggle selection, Select all, Deselect all,
Select all of the same kind, Invert selection, separator, Select files…, Deselect files….

The rows are DATA (`SELECTION_ROWS`) rather than a straight-line builder, for one reason: a real `muda::Menu` panics off
the main thread, so nothing can build this submenu in a unit test and read it back. Keeping the rows as a const means
their order and their command mapping are both pinned by tests that don't touch AppKit at all.

Its "same kind" row is composed at popup time, from the target the frontend computes for the RIGHT-CLICKED row
(`pane-pointer.ts`, `sameKindTargetFor(entry)`) and sends in the `show_file_context_menu` payload. ❗ ❌ Never the menu
bar's debounced value: a right-click (and `⌃⏎`) is exactly the moment a label a frame behind would be read as truth, and
`⌃⏎` removed the mouse trip that used to hide the staleness. Two callers send no target and get the neutral label: the
Search DIALOG's row menu (its items act on the pane behind the dialog, so the row's own kind would be a lie) and any
surface that doesn't answer. The search-results snapshot pane does send one, since it runs the command over those rows.

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
  `MenuState.viewer_menu`, with its `Word wrap` CheckMenuItem ref in `MenuState.viewer_word_wrap` and its Edit >
  Cut / Paste refs in `MenuState.viewer_edit_cut` / `viewer_edit_paste`.
  - Its **Edit** submenu is a deliberate mix, because the two halves act on different things.
    - **Cut and Paste belong to the search box**, the only editable field in a viewer window. On macOS they're Custom
      items (`VIEWER_EDIT_CUT_ID` / `VIEWER_EDIT_PASTE_ID`) that forward the native `cut:` / `paste:` selectors down
      the responder chain themselves, through the same `send_native_edit_action` the main bar's Edit items use outside
      the main window. ❗ Everywhere else they stay `PredefinedMenuItem`s: the responder chain is macOS-only, so a
      Custom item off macOS would leave ⌘X / ⌘V dead in the viewer's search field, which is exactly what trimming them
      did once. ❌ Whatever else changes about these two, don't break that: `native_edit_selector_for` pins the
      mapping.
    - **Decision/Why they're Custom at all:** so `apply_menu_item_states` can grey them out while the search box
      doesn't have focus (§ "Dialog refusals, and the one writer of an item's enabled state"). Always-live items that
      do nothing in the content area read as broken. The greying is chrome and can't cost the search box its chords:
      a disabled item's key equivalent either still fires (and `cut:` reaches a first responder with nothing to cut) or
      falls through to the webview, whose own handling does the same job, and while the box HAS focus the items are
      enabled anyway.
    - **Copy and Select all are Custom items** (`VIEWER_EDIT_COPY_ID` / `VIEWER_SELECT_ALL_ID`) routed to the focused
      viewer's FRONTEND as a typed `ViewerEditAction`. **Decision/Why:** their native selectors act on the DOM
      selection, and the viewed file isn't in its reach — `.file-content` is `user-select: none` because the viewer
      owns an offset-based selection model, and `.status-bar` deliberately opts back in (`user-select: text`, so people
      can copy the filename). The footer is therefore the only thing a native `selectAll:` can land on, and a Copy after
      it copies the footer. The frontend runs the same two functions ⌘A / ⌘C already run
      (`apps/desktop/src/routes/viewer/viewer-menu-actions.ts`), so it doesn't matter whether a ⌘-chord reaches the webview before the
      menu's key equivalent.
    - ❗ All four ids are viewer-specific, NOT the main bar's `EDIT_CUT_ID` / `EDIT_COPY_ID` / `EDIT_PASTE_ID` /
      `SELECT_ALL_ID`. Both bars share `EDIT_MENU_ID`, and `handle_menu_event` tells the two lanes apart by item id
      alone; reusing them would make one click mean two things. The viewer's items had no ids at all while they were
      Predefined, which is why the collision never existed before.
- `MenuState.active_menu_kind` tracks which menu is installed, so a same-kind focus event (viewer →
  viewer, main → main) skips the swap entirely.
- `"main"` and `"other"` install the main menu; `"viewer"` installs the viewer menu. After any swap
  we re-run `cleanup_macos_menus` (macOS re-injects Edit items on every `set_menu`), and on a
  swap-back to the main menu we also re-apply `set_macos_menu_icons` (SF Symbols don't survive
  `app.set_menu()`). Both run on the main thread via `run_on_main_thread`, queued FIFO after Tauri's
  own main-thread menu install, so ordering is install → cleanup → icons.

On Linux `activate_window_menu` skips the swap (viewer windows carry their own per-window menu set by
`viewer_setup_menu` / `window.set_menu()`) and only does the enable/disable step.

The enable/disable step is the private `set_menu_context("explorer" | "other")` helper: it stores whether the explorer
owns the menu and recomputes every item (`"main"` → file-scoped items enabled, `"other"` → greyed). This is a visual
hint reinforcing the focus guard in `on_menu_event`.

## Dialog refusals, and the one writer of an item's enabled state

Every main-menu item's enabled state comes from `apply_menu_item_states` in `item_states.rs`, which derives it from
stored inputs through the pure `menu_item_enabled`: whether the explorer owns the menu, `file_operations_blocked`, the
"Open terminal here" and "Reopen closed tab" verdicts, and `commands_refused_over_dialog`. Each input's writer stores it
and calls the recompute. **Decision/Why:** when each input wrote its items directly, the last writer won, so a focus
round-trip through Settings once re-offered Copy with a dialog still up, and every later input had to be "re-applied
LAST". One derivation makes the order irrelevant.

`commands_refused_over_dialog` is the frontend dialog gate's answer (`routes/(main)/DETAILS.md` § The dialog gate): every
`BLOCKED_BY_DIALOGS` command while a dialog, an explorer overlay, or the command palette is up in the main window, pushed
by `menu-dialog-gate.svelte.ts` through `set_commands_refused_over_dialog`. It greys an item out whichever window is in
front, since a click from Settings still lands in the main window's refusing dispatch core. Close tab is the one
exception: it greys out only while the main window is in front, because anywhere else ⌘W closes the focused window.

- **Regular items: chrome.** A disabled item's accelerator still fires, and the dispatch core refuses the command itself.
- **The two check items: the refusal itself.** Show hidden files and the per-pane view modes toggle themselves before
  `handle_menu_event` runs, and they emit their own events (`settings-changed`, `view-mode-changed`), so the frontend
  can't untoggle them and show hidden never passes the dispatch core. `handle_menu_event` checks
  `MenuState::refuses_over_dialog` and puts the check back. Their commands are named by the `*_COMMAND_ID` consts in
  `command_map.rs`, pinned by `rust-command-id-drift.test.ts`.
- **Lock order:** the recompute copies the refused set before taking any item lock, because `handle_menu_event` holds a
  check item's lock while it reads the set. The viewer block reads `viewer_search_focus` and drops that lock before
  taking the two item locks, for the same reason.

The one verdict here that isn't the main bar's is the **viewer bar's Edit > Cut / Paste**, live only while a viewer's
search box holds keyboard focus (`viewer_text_edit_enabled`). It's here because the rule is the rule: one writer of
`set_enabled`. Its input is `MenuState.viewer_search_focus`, the LABEL of the viewer whose search box has focus, which
each viewer pushes through `viewer_set_search_input_focused` on the input's focus and blur, on the search bar closing
(removing a focused input from the DOM fires no `blur`), and on its window's focus-gain.

- **Decision/Why a label rather than a bool:** the viewer bar is shared, and clicking from viewer A to viewer B crosses
  two pushes in flight (A's input blurs, B re-pushes on focus-gain). `note_viewer_search_focus` only lets a `false`
  from the viewer that last claimed focus clear it, so both orderings land on B. A last-writer-wins bool could leave B
  greyed while the user types in it.
- A language rebuild builds fresh items, so `install` in `rebuild.rs` re-runs the recompute: this verdict's input lives
  in Rust, and unlike the frontend-owned ones nobody re-pushes it after `MenuBarRebuilt`.

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

A live title is still only a title, so it can collide with another item's. When a pass needs a GROUP of items, it
matches the whole group as one contiguous run rather than each title on its own: the tag row's `find_tag_run` takes
the seven color titles in order or nothing, because the header line carries the bare filename and a folder named `Red`
puts a `Red` item above the real one.

This is why every top-level menu in `menu_bar.rs` carries an ID from `command_map.rs`, which the macOS
bar builds it with (Linux looks nothing up, so it builds its menus without one). Two IDs are shared
with the viewer menu bar (`menu_structure.rs`): `EDIT_MENU_ID` and `HELP_MENU_ID`, because
`cleanup_macos_menus` runs against whichever bar is installed. Only one bar is ever installed at a time,
so the shared IDs never collide. Sort by carries `SORT_BY_MENU_ID` on both platforms.

### macOS cleanup (objc2)

`cleanup_macos_menus(app)` runs post-construction via objc2 FFI:
1. Hands the Help menu (found by `HELP_MENU_ID`) to `NSApplication.setHelpMenu:` so macOS adds the
   search field. Tauri's `Submenu::set_as_help_menu_for_nsapp` resolves the live `NSMenu` itself, so
   this half needs no title at all.
2. Hangs AppKit's own services menu off the Services item in the installed app menu
   (`adopt_installed_services_menu`), which is what makes `Cmdr > Services` list the file services
   Finder lists. muda registers one `NSMenu` at build time and the installed bar carries a different,
   empty one, so AppKit fills a menu nobody can see; `NSApplication.setServicesMenu:` is ignored once
   AppKit owns one, so this goes the other way and re-parents the managed menu (detaching it from
   muda's first, or AppKit raises). Measurements and the rest of the mechanism:
   `../services_menu/DETAILS.md`. It sits in its own `objc2::exception::catch`, because re-parenting
   an `NSMenu` is the one step here that can raise and an escaping exception takes step 3 with it.
3. Finds the Edit menu by `EDIT_MENU_ID` and removes the items AppKit injects into it (Writing
   Tools, AutoFill, Dictation, Emoji & Symbols), plus the separators they leave behind.

The injected items in step 3 carry none of our IDs (AppKit adds them after we build the menu), so
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

### Services in the right-click menu

The file context menu ends with `Services`, the same list `Cmdr > Services` shows, acting on the row
that was right-clicked. `services_context.rs` owns it, and two facts shape all of it.

**AppKit owns exactly ONE Services menu, and the menu bar already has it.**
`NSApplication.servicesMenu` is the only `NSMenu` AppKit fills, and an `NSMenu` has at most one
supermenu — a second parent raises `NSInternalInconsistencyException`. So the context menu BORROWS
it: `ServicesLoan` takes it off the menu bar's Services item, hangs it on the context menu's, and
hands it back when it drops. ❗ The loan must outlive `popup()`, which runs AppKit's whole tracking
loop, so the binding in `commands/menu.rs` is `let _services_loan = …` and never `let _ = …`.
`detach_from_supermenu` in `macos_appkit.rs` is the single place a detach happens, shared with
`adopt_installed_services_menu`.

**Tauri hands out no `NSMenu` for a context menu.** muda has `ContextMenu::ns_menu()`, but Tauri
reaches it only through `pub(crate) mod sealed`, and `tauri::menu::Menu` exposes no equivalent. So
there is nothing to attach the borrowed menu to before `popup()`. The handle comes from AppKit
instead: `NSMenuDidBeginTrackingNotification` carries the `NSMenu` about to be tracked, so the swap
happens in that observer, on whichever tracking menu holds an item titled with the live `Services`
label. The observer registers on the first right-click and lives for the process, like the
accent-color one.

What was rejected:

- **`NSApplication.setServicesMenu:` pointed at a menu of ours.** Ignored once AppKit owns one, the
  same wall `adopt_installed_services_menu` hit (`../services_menu/DETAILS.md`).
- **Copying the managed menu** into our own submenu. Its contents are computed per open: AppKit
  writes the selection to a scratch pasteboard once per candidate service while the menu opens, and
  filters on each service's `NSRequiredContext`. A copy is a snapshot of a different selection —
  and often of nothing at all, since on a fresh launch the menu still holds 0 items at the moment
  it is lent (logged, macOS 26.6.2, 2026-09-09) and fills only when it opens.
- **Installing the context menu as the main menu bar for an instant** to read `NSApp.mainMenu`. A
  menu-bar swap re-runs AppKit's app-menu discovery, and keeping the Services registration alive
  through that is the fight this feature already won once; not worth re-opening for a handle.

**The submenu acts on the right-clicked row, not the pane selection.** That's Finder's rule and the
one the rest of the context menu follows, and it genuinely differs: right-clicking a row OUTSIDE the
selection acts on that row alone. `services_menu::selection`'s context target carries the override
for the loan's lifetime; the reasoning and what it costs are in `../services_menu/DETAILS.md`.

The item shows whenever the pane's rows are real OS paths (`ContextMenuPaneFacts::can_share`, the
same fact `Share` rides on: both hand file URLs to something outside Cmdr).

### Slow facts and the live menu

The file context menu opens within the grace period, whatever the disk does. Six things it shows depend on asking the
rows' storage: the tag checks (`getxattr` per row), the Google Drive items (a Drive xattr and two `stat`s), the File
Provider actions, "Open with" (LaunchServices), `Share` (ShareKit reads each file's attributes), and the iCloud eviction
pair (sync status). On a network share each is a round trip. A sample of the running app on an SMB share over Tailscale
(2026-09-23, five right-clicks, 1 ms interval) spent 3.4 s of main-thread time in the Drive read, 2.1 s in `Share`,
1.0 s in the tag reads, and 0.2 s in "Open with": about 1.3 s per menu, once 10 s, with the whole app frozen meanwhile,
since `show_file_context_menu` is a sync command and runs on the main thread.

- **Gather** (`context_menu_facts.rs`): `start` submits one job per fact the menu can use (`FactsRequest::kinds`) to its
  own `framework_pool` instance, so a wedged provider can't starve sync status or the reverse. The command does its
  cheap work meanwhile, then `collect` waits until every fact answered or `GRACE` (100 ms from the command's start)
  passed, whichever is first. The job-side routing flips under one lock, so each answer lands in exactly one place:
  the build, or the late route.
- **Build**: `file_context_info` marks each fact still out `Slow::Pending`, and the builder gives it a part that can be
  filled in while the menu is open (`LateTargets`, the next bullet). A fact nobody asked about is `Ready` with its empty
  answer, so the builder needs no third state.
- **Late** (`context_menu_live.rs`): each late answer goes to the main queue (`late_sink`), tagged with its menu's
  generation. The tracking observer hands the live menu its `NSMenu`, hides the pending row groups, and applies what
  answered in between; after that each answer applies on arrival. The loan's `Drop` forgets the menu and cancels the
  gathering, so a late answer for a closed menu goes nowhere and a job still queued behind a stuck mount skips its work.

What a late answer may change is set by three limits, all verified with a standalone muda 0.19.3 probe on macOS 27.0
(2026-09-24, screenshots while the menu was open):

- **❗ muda holds the context menu itself borrowed for the whole popup** (`Menu::show_context_menu_for_nsview` takes
  `borrow_mut()` around the blocking `popUpMenuPositioningItem`). `Menu::append`, `insert`, and even `ns_menu()` panic
  with "RefCell already borrowed" while it's up. Submenus and single items have their own cells and change freely, and
  AppKit redraws both live, the open submenu included. So ❌ no late code touches the top-level `Menu`.
- **AppKit keeps the highlight at the same row index**, so a row inserted above the highlighted one moves the highlight
  onto a different command (the probe highlighted `Share`, inserted a row above, and `Copy` came out highlighted). So a
  late answer never inserts into the context menu: "Open with" and `Share` are `Placeheld` submenus whose disabled line
  ("Finding apps…", "Finding share options…") the answer replaces; the tag row checks its circles in place
  (`tag_row::set_applied`, pre-sized for both caption wordings so it never grows); and the Drive, iCloud, and File
  Provider groups are `SlotGroup`s, built whole with their final IDs, hidden as the menu starts tracking, and revealed
  when their answer lands. They sit near the bottom, so revealing moves only the rows below them.
- **The late route is the main dispatch queue**, which AppKit drains while a menu tracks. Tauri's `run_on_main_thread`
  from another thread goes through tao's event-loop proxy instead, and every Tauri menu call blocks its caller on the
  answer. From the main thread Tauri runs a menu call inline (`send_user_message` compares thread IDs), which is what
  makes the late calls safe.

Smaller rules:

- **A late empty `Share` says "No share options"** inside the submenu instead of removing it: the item is in the context
  menu, which can't change while it's up, and hiding it would move every row below it.
- **The File Provider group has `PENDING_SLOTS` (12) slots**, titled with an invisible U+2063 plus their ID so the run
  can be found; an offer past that shows its first 12 and logs the rest. Its budget is 1 s
  (`FILE_PROVIDER_ACTIONS_BUDGET` in the facts module), since the menu doesn't wait on it.
- **Images for late items** come from the `land_*` functions in `context_menu_icons.rs`, which read titles off the item
  and submenu handles the live menu holds, never off the context `Menu`.
- **The `Share` offer is armed where the click reads it**, on the main thread (`share::arm_offer`): at build time for an
  offer that answered, `None` otherwise, and again when the late one lands. `ShareOffer` crosses from the worker to the
  main thread once; its `Send` impl carries the evidence (Main Thread Checker clean off the main thread).

### SF Symbol icons (macOS only)

`set_macos_menu_icons(app)` runs post-construction via objc2 FFI, walking the `MENU_BAR_ICONS` table
in `macos_appkit.rs` and calling `NSImage(systemSymbolName:)` + `setImage:` on each `NSMenuItem`. This
produces true template images that auto-tint on selection highlighting.

The table is `(menu ID, [(menu item ID, SF Symbol name)], nested)`, resolved to titles as described
above. `nested` recurses one level for View > Sort by; the recursion is uniform, so a second nesting
level would need no new code. Anything that fails to resolve logs a warning naming the ID and the
symbol, because the failure is otherwise invisible: the menu builds fine, just without an icon.
`menu_icon_ids_are_built_by_the_menu_bar` (in `macos_appkit.rs`) is the guard: each icon's item has to
sit directly inside the menu its group names in `MENU_BAR` on macOS, which is where the pass looks.

Every group names a MAIN-bar menu, so the viewer's bar carries no icons at all. That's the table being
short rather than a pass that fails: the viewer's items have their own `VIEWER_*` ids and no group
mentions them.

**Gotcha**: an accelerator update replaces the menu item (see "Accelerator sync"), and the fresh
`NSMenuItem` carries no image, so `update_menu_accelerator` re-applies the icons afterwards.

**Below macOS 11 there are no SF Symbols at all**, and the bundle's floor is 10.15, so `set_sf_symbol`
returns early on `crate::platform::macos_at_least(11, 0)`. Catalina gets menu items with no icons,
which is the feature degrading; calling `imageWithSystemSymbolName:accessibilityDescription:` there
would raise an unrecognized-selector exception and abort the process instead. The
`allowed-newer-selector` marker on the call is what tells `desktop-rust-macos-availability` the gate
exists, since it reads lines rather than control flow.

**On macOS 27 a menu hides the images it was given, unless each item opts in.** `NSMenuItem` gained
`preferredImageVisibility` there, defaulting to `Automatic`, under which AppKit hides an item's SYMBOL
image and leaves a non-symbol one alone (and hides a bitmap too under the macOS 27 SDK, below).
`keep_menu_image_visible` sets `Visible` through `msg_send!` (`objc2-app-kit` 0.3.2 binds no such
property) above a `macos_at_least(27, 0)` gate.

**Every image on every Cmdr menu item goes through ONE door, `set_menu_item_image`** in
`macos_appkit.rs`, which sets the image and then opts the item in. The menu bar's symbols, the Dock
menu's (`set_sf_symbol`), and every context-menu image (`context_menu_icons.rs`) all end there, and
`clear_menu_item_image` is the only other `setImage:` in the tree. **Clippy enforces it**: the root
`clippy.toml` refuses `NSMenuItem::setImage` and muda's `IconMenuItem` (its type, its builder, and
the `MenuBuilder` / `SubmenuBuilder` `icon` methods), and the two `#[expect]`s in the door are the only
exemptions. So a new image route can't skip the opt-in by accident: it makes an `NSImage` and hands it
to the door.

The failure it prevents is fully silent, which is why the opt-in lives in the door rather than at the
call sites: `imageWithSystemSymbolName:` answers a valid image, `setImage:` takes it, `item.image()`
reads back non-nil, and nothing draws. That silence once blanked three icon sets under dev builds (the
"Open with" app icons, the `Share` icons, and the tag circles), all `IconMenuItem`s whose `NSMenuItem`
nothing of ours could reach to opt in, which is why clippy refuses the type instead of a doc asking
nicely. Measured on macOS 27.0
(26A428), `NSMenu.size` offscreen, 2026-09-21: a titled item carrying a symbol image lays out at
72 pt, the same as an item with no image at all, and at 91 pt once `Visible` is set. Under the hood an
`NSSymbolImageRep` reports `size` 15 × 17 but `pixelsWide` × `pixelsHigh` of 0 × 0, while any
pixel-backed rep measures normally. Apple's guidance is to carry an image where an item names an
object or a concept rather than an action, so ❗ this stays an opt-in per item; a future icon set worth
having is worth asking for explicitly.

**Gotcha**: the hiding is keyed to the SDK the binary links against as well as the OS it runs on, and
an SDK 27 build hides EVERY menu-item image that hasn't opted in. Measured with a COMPILED ObjC probe
(`LC_BUILD_VERSION` sdk 27.0) on macOS 27.0, `NSMenu.size` offscreen, 2026-09-21: a titled item is
72 pt wide with no image, 72 pt with a bitmap, 72 pt with a symbol, and 91 pt with a symbol opted in;
an icon-only item (an image with an empty title) is 16 pt with or without a bitmap, so it vanishes
outright. ❗ Measure this with a compiled binary and nothing else: a `swift` script is read by an
interpreter whose OWN linked SDK is what AppKit consults, and it answers that bitmaps are unaffected.

Cmdr SHIPS at SDK 26.5 (`otool -l | grep -A3 LC_BUILD_VERSION` on the bundle). ❗ A dev build is NOT
on the shipped SDK; it links whatever the installed Command Line Tools carry (27.0 since 2026-09-09 on
David's Mac), so a dev build is where an image that skipped the opt-in shows up blank first. With every
image behind the door, nothing in the menus depends on the SDK any more, which is the precondition for
moving the release runner to an Xcode 27 image. Nothing pins that today: `release.yml` builds on
`macos-latest`, which moves on GitHub's schedule with no commit of ours.

#### Images on a CONTEXT menu

`context_menu_icons.rs` puts EVERY image on the file context menu: the Drive items' SF Symbols, the
provider logos, the app icons in "Open with", each service's own icon in `Share`, and the tag items'
fallback circles. Every one of those items is a plain `MenuItem`. It needs its own mechanism because
Tauri hands out no `NSMenu` for a context menu (the same wall the Services loan hit, above).

❌ **`IconMenuItem` is not the answer, for any image**, and clippy refuses it. muda sets its image on an
`NSMenuItem` nothing of ours can reach, so no door can opt it into macOS 27's visibility, and under the
macOS 27 SDK it draws nothing. It also can't render a template image (muda hands `NSImage` a PNG and
never calls `setTemplate:`), so a monochrome glyph would vanish in one appearance and stay dark on a
highlighted row.

- **What goes where is pure data.** `image_runs(&FileContextInfo)` lists, by item ID, what each item
  shows (`ItemImage`: a symbol, a logo, an app's RGBA, a tag circle, or a share-service index), grouped
  into RUNS: items that sit next to each other in one menu, the context menu itself or a submenu found
  by its ID (`OPEN_WITH_SUBMENU_ID`, `SHARE_SUBMENU_ID`). Unit-tested without AppKit.
- **Arming** (`lend_context_menu_icons`, before `popup()`) resolves each run's IDs to live titles (a
  title is translated text, so it's read off the item rather than written down) and makes the
  `NSImage`s. A run the menu didn't build, or built only part of, is dropped whole. ❗ The returned
  `IconLoan` must outlive `popup()`, same as `ServicesLoan`; its `Drop` disarms so a later menu can't
  inherit stale titles.
- **Landing**: the `NSMenuDidBeginTrackingNotification` observer acts on the ROOT menu only (no
  supermenu). AppKit posts it before laying the menu out, which is why an image set there still gets its
  gutter, and the submenus already hang off the root by then, so "Open with" and `Share` get their
  images before either opens. Each run is found as ONE contiguous stretch of titles (`find_title_run`,
  compared up to the first TAB like `find_ns_item`), because a single title can collide: two apps or two
  share extensions can share a name (pairing by position inside the run handles that), and the header
  line carries the bare filename, so a folder named `Mail` would otherwise take the Mail service's icon.
  A Drive item stays a run of one, where that collision is still possible and costs a stray icon at worst.
- **An item a view draws gets no image**: the tag row's item would still claim the image column and push
  every title right. `tag_row` clears it on install, and this pass skips it, so the two observers can run
  in either order.
- **Sizes**: 16 × 16 pt for logos, app icons (32 px, so 2× on Retina), and share icons, the box the
  neighbouring symbols take; 18 pt for the tag circles (36 px). A share icon is macOS's own `NSImage`,
  copied before sizing because the system shares it.

`macos_appkit.rs` owns `observe_menu_tracking`, `tracking_menu`, `find_ns_item`, `find_ns_submenu`, and
the door (`set_menu_item_image`, `sf_symbol_image`, `set_sf_symbol`), shared with the other consumers.

`set_sf_symbol` and `set_menu_item_image` are `pub(crate)`, for a consumer outside this module:
`../dock/menu/native.rs` hand-builds the Dock tile's `NSMenu` (Tauri exposes no `NSMenu` and a Dock
menu never enters the menu bar, so none of the resolution machinery above transfers) and puts symbols
on its bare `NSMenuItem`s directly — no arming, no tracking observer, because it owns the items rather
than borrowing Tauri's. That menu's own rules: `../dock/menu/CLAUDE.md`.

Today the table is the three Google Drive items: `arrow.up.forward.app` for "Open in Google Drive"
(distinct from the menu bar's plain `arrow.up.forward` on `Open`), `link` for "Copy Google Drive
link" — deliberately the same symbol the menu bar's `Copy path` carries, since `Copy` already shares
`document.on.document` across two menus — and `sparkles` for "Ask Gemini", the glyph Apple and Google
both spell AI with, shared with `Ask Cmdr` in the menu bar. All verified present with
`NSImage(systemSymbolName:)` on macOS 26.6.2, 2026-09-09. Provider actions get no symbol: their labels
are the provider's, and a glyph Cmdr picked would claim to know what each one does. They carry their
provider's logo instead, which only says whose action a line is (next section).

#### Provider logos on a CONTEXT menu

Each line of a File Provider's own actions (`file_provider_items.rs`) carries that provider's logo, the way Finder shows
them: Dropbox, Google Drive, MacDroid, OneDrive, and Box (`PROVIDER_LOGOS` in `provider_logos.rs`). A provider missing
from the table shows its lines without one. Cmdr's own three Drive items keep their SF Symbols: they're Cmdr's actions,
built from Drive's links.

- **Same pass as every other context-menu image.** `image_runs` turns the menu's `ProviderOffer` into one run of
  `ItemImage::Logo` over its `fp-action:<index>` lines, so one tracking observer and one loan cover them all.
- **Matched by the APP's bundle ID, on a dot boundary.** The offer's `provider_id` is File Provider's `providerID`, the
  extension's bundle ID, and macOS makes an extension's ID start with its app's plus a dot. So `com.box.desktop` claims
  `com.box.desktop.boxfileprovider`, and `com.microsoft.OneDrive` leaves `com.microsoft.OneDrive-mac.FileProvider` to
  the App Store build's own entry.
- **Colored on purpose, ❌ no `setTemplate:`.** The brand's colors are the point, so a highlighted row keeps them, at
  lower contrast on the accent fill, which is accepted.
- **An `NSImage` from the SVG bytes (`initWithData:`), sized 16 × 16 pt**, the box the neighbouring symbols take (`link`
  17 × 17, `sparkles` 15 × 17, `arrow.up.forward.app` 15 × 14 at the 13 pt menu font; verified on macOS 26.6,
  `NSImage.size`, 2026-09-12). It stays a vector, where `IconMenuItem` would need RGBA rasterized at a guessed scale. All
  five load as `_NSSVGImageRep` on macOS 26.6 (Swift probe, 2026-09-12); older releases are unverified. No version gate:
  the selector is as old as `NSImage`, so an OS that can't read SVG answers nil, logged at debug, and the line shows no
  logo.
- **The SVGs are normalized for CoreSVG** (`provider_logos/`): explicit hex fills (no `currentColor`, no CSS `<style>`),
  no fixed `width` / `height`, and a square `viewBox`, so sizing to a square can't skew one (Box's wordmark is padded
  equally above and below). Gradients render as they are. `every_logo_is_a_square_svg_with_explicit_colors` and
  `every_svg_on_disk_is_a_logo_in_the_table` guard it.
- **Sources and licenses:** Google Drive and OneDrive are selfh.st icons (CC BY 4.0, so the attribution comment stays in
  each file), Dropbox is `mdi:dropbox` (Material Design Icons by Pictogrammers, Apache 2.0), MacDroid is
  `material-symbols:android` (Material Symbols by Google, Apache 2.0), and Box is Box's own mark. The three icon sets are
  credited in `scripts/check/checks/third-party-vendored.json`, which feeds the Acknowledgements dialog and
  `THIRD-PARTY-NOTICES.md`: a new, renamed, or re-sourced logo updates its credit there in the same commit
  (`third-party-notices` fails on a credited file that's gone).

### The context menu's header line

`context_menu_header.rs`. The file context menu's first item, above a separator, naming what the menu is about to act
on: `photo.jpg · 2.1 MB` for one row, `3 items · 3.2 MB` for several, dropping the size half when there isn't one
(`photo folder`, `3 items`). The separator between the parts is U+00B7 between spaces, which reads the same in every
language we ship and so needs no catalog entry.

**Why it exists at all**: Cmdr follows Finder — a right-click INSIDE the current selection acts on the whole selection,
a right-click outside it acts on that one row — and nothing on screen used to say which. Select `a.jpg`, `b.jpg`,
`c.jpg`, right-click `e.jpg`, and every item below applied to `e.jpg` alone with no way to tell. This is not
decoration.

**Portable half**: a DISABLED `MenuItem`, which is what makes it read as a label rather than a command, then a
`PredefinedMenuItem::separator`. No AppKit, so Linux gets it too. Its `CONTEXT_MENU_TARGET_ID` maps to nothing in
`menu_id_to_command`, so even a click that somehow arrived is a no-op (`the_header_maps_to_no_command`).

**macOS half**: an attributed title (the small menu font, `secondaryLabelColor`) so it also LOOKS like a header rather
than a greyed-out command. It rides the same `NSMenuDidBeginTrackingNotification` as the SF Symbols above, for the same
reason — Tauri hands out no `NSMenu` for a context menu — with the same `HeaderLoan` discipline: ❗ hold it until
`popup()` returns, ❌ never `let _ =`. It cannot panic and has no `.unwrap()`: if the attributed pass doesn't happen,
the plain disabled item is already correct.

❌ **Not `+[NSMenuItem sectionHeaderWithTitle:]`.** It's macOS 14 (the bundle floor is 10.15) and it CREATES an item, so
it can't restyle the one muda already made; swapping items inside a menu muda owns would desync muda's own child
bookkeeping. The attributed title gets the look without touching menu structure.

**Decision**: every number in the header crosses IPC ALREADY RENDERED by the frontend — `ContextMenuTarget::count_text`
as well as `size_text`. This module composes the label's SHAPE and formats nothing.
**Why**: all three things a reader would notice are locale-dependent and none of them is expressible here. A size needs
`appearance.fileSizeFormat` (binary vs SI) and `listing.sizeUnit` (dynamic or fixed), which only
`src/lib/units/byte-size.ts` honours. A count needs the active locale's grouping separator (`12,345` against `12.345`)
and its plural category, and `menu_t` deliberately has neither — it is a table lookup with no ICU and no number
formatting, and "a label that seems to need a plural is a label to reshape" (see "Labels come from the message
catalog"). Composing either half here would be a second, worse implementation drifting from the pane's own status bar
and size column, with both on screen at once; and the plural gap is not hypothetical, since "two or more needs no
distinct form" happens to hold for the 11 locales shipping today and breaks the moment a Slavic one lands (Polish and
Russian both need a separate paucal form for 2–4). The frontend already computes this summary for the status bar, so it
reuses that path: `apps/desktop/src/lib/file-explorer/selection/context-menu-target.ts`, which also holds the "no honest
size, no size" rule.

❗ What stays HERE is the COUNT ITSELF, `context_paths.len()`. The number of targets decides filename-vs-count and it
has to be the number the menu's actions will use, so `count_text` is the wording for that branch and never the reason to
take it (`one_row_shows_its_name_even_when_a_count_text_arrives`). With no wording for a multi-row menu the label falls
back to the bare number (`3 · 3.2 MB`): a defensive branch, judged on "never wrong" rather than "pretty", keeping the
one fact the line exists for. ❌ Not the primary filename, which would restate the ambiguity the header removes; ❌ not
the size alone, which would drop the count silently.

**Decision**: the header carries no file KIND ("PNG image", which Finder shows).
**Why**: `UTType` is the API for it, and `Cargo.toml` deliberately keeps `UniformTypeIdentifiers.framework` out of the
binary to hold the app's 10.15 floor (see the `objc2-quick-look-ui` comment there; `desktop-macos-framework-floor` fails
the build if it comes back). The alternative, an `NSURL` resource read, means disk I/O on every single right-click and
can hang on a dead mount, which "immediate feedback" forbids. Name and size answer the question the header exists for.

**i18n**: no `menu.*` key at all. The one string the header needs beyond a filename is
`fileExplorer.contextMenu.itemCount`, an ICU plural in the FRONTEND catalog, resolved there and sent as text — which is
what keeps the native catalog free of the count-plus-noun shape `menu_t` can't render properly.

### The tag row

`tag_row/`. Finder shows its seven tag colors as one line of circles with a caption under it. Cmdr builds seven plain
`tag-color:<index>` items (see "Unified dispatch"), and on macOS the row replaces their look while keeping every bit of
their behavior.

- **Look**, Finder's own, measured off @2x screenshots of its row (dark mode, macOS 27.0, 2026-09-15): 14 pt circles on
  a 24 pt pitch, a 1 pt ring darker than the fill, a white check on an applied color. A hovered circle grows to 20 pt
  and shows a plus (the click adds) or a cross (it removes), with no animation and no highlight band. The caption reads
  `Tags`, or `Add "Green"` / `Remove "Green"` over a circle, in the menu font and `secondaryLabelColor`. The fills are
  Cmdr's `--color-tag-*` tokens in both modes, picked per draw from the view's `effectiveAppearance`. `model.rs::SWATCHES`
  mirrors them by hand, with a pointer comment on each side: a Rust test embedding `app.css` would make every CSS edit
  re-run the Rust lanes.
- **Install**: `lend_tag_row` reads the seven live titles and applied flags and resolves the three captions
  (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`, the color's name as `{color}`). On
  `NSMenuDidBeginTrackingNotification` the observer finds the seven as ONE contiguous run of top-level titles
  (`find_tag_run`), clears the first item's bitmap, sets the row as that item's view, and hides the other six. No run,
  no change, and the plain items are a correct menu. ❗ Hold `TagRowLoan` past `popup()` like the other loans.
- **Click**: `mouseUp:` on a circle, or VoiceOver's press, reads that item's `action` and `target` and sends them through
  `NSApplication.sendAction:to:from:` synchronously, then `cancelTracking`. `handle_menu_event` toggles exactly as it
  does for the plain item. The row holds its items WEAKLY (each item retains its view), and the loan's `Drop` empties
  that list, so a press arriving after `popup()` returns does nothing.
- **Alignment**: the caption and the first circle line up with the menu's title column, which AppKit doesn't expose.
  `TITLE_COLUMN_X` is an estimate to tune by eye; when any visible item outside the tag run has an image, titles move
  right by `IMAGE_COLUMN_WIDTH` (measured) and the row follows. ❗ The row asks the live menu at every draw, hit-test,
  and accessibility-frame refresh (`TagRowView::title_x`, over the pure `menu_shows_images`), never once at install:
  `context_menu_icons.rs` sets every image (the tag circles included) from its own observer of the same notification, and
  `NSNotificationCenter` promises no order between observers.
- **Accessibility**: the row is an `AXGroup` named like its idle caption, holding seven `AXCheckBox` elements, each named
  after its color, valued 1 when applied, with a press that clicks. No keyboard path: views in menu items get no key
  events, and Finder's row has none either.
- **What's verified where**: a test can't drive a real menu. The drawing, the menu size before and after the swap, the
  click path against muda-shaped items, and the accessibility tree were checked offscreen (macOS 27.0, a throwaway
  AppKit harness rendering `view.rs`, 2026-09-16). Hover inside real tracking, the click-through, VoiceOver, and the
  title inset need a live look.

**Decision**: one `NSView` installed over the seven plain items at tracking time, whose clicks fire those items.
**Why**: Tauri has no custom-view menu item, and a hand-built one would sit outside muda's bookkeeping and
`handle_menu_event`'s routing. Keeping the items keeps the IDs, the handler, the right-clicked-selection semantics, and
a working fallback; the row changes only the look. The click reads `action` and `target` at click time because the
selector is muda's (`fireMenuItemAction:` in 0.19.3, `customAction:` in 0.20), and sends synchronously because 0.19.3's
item ivar points into a `MenuChild` freed when `show_file_context_menu` returns.

## Platform differences

Every difference is marked on its row in `menu_bar.rs`, and `menu_bar_test.rs` spells out each bar in full.

- **App menu**: macOS has a dedicated "cmdr" menu with About, License, and Settings. Linux has none: About sits under
  Help, Settings and License under Edit.
- **Predefined items**: macOS has Hide, Hide Others, Show All, Quit, the Window items, and Undo/Redo. Linux has none (GTK
  has no equivalent).
- **Accelerators**: macOS has the full set. Linux omits F2 (Rename) and the others GTK intercepts.
- **Mnemonics**: macOS doesn't use them. Linux gives `&` prefixes for GTK keyboard navigation, unique per submenu.
- **Help search**: macOS has the native NSMenu search field via `setHelpMenu:`. Linux has none.
- **System cleanup**: on macOS, objc2 strips the injected Edit items. Linux needs none.
- **Menu icons**: macOS sets every image via objc2 through `set_menu_item_image` (SF Symbols on the bar and context
  menus; logos, app icons, share icons, and tag circles on context menus). Linux doesn't support menu icons.
- **Tag colors**: macOS only, drawn as one row by a custom `NSView` over seven plain items. Linux has no tags.

## Menu structure

Both platforms share: File, Edit, Select, View (with Sort by and Zoom submenus), Go, Tab, Help.

The **file context menu opens with the target header** (`append_context_menu_header`, above everything else), then a
separator, then the Open / View / Edit group. See "The context menu's header line".

The **File** submenu's transfer group runs `Copy…` (F5), `Move…` (F6), `Duplicate` (⌘D), `Compress…` (⌥F5). `Duplicate`
carries no ellipsis because it picks nothing: it copies the selection into the folder it already sits in, and the
backend resolves the self-collision per item. The context menu (`file_context_menu.rs`) offers it only when
`restrict_destination_actions` is false, so it is absent on the search-results virtual pane alongside `Rename`: each
selected item would have to land in its own real folder, which one transfer can't express. Its macOS SF Symbol is
`plus.square.on.square`, the Linux mnemonic is `D&uplicate`. What the command does once dispatched:
`apps/desktop/src/lib/file-explorer/pane/DETAILS.md`.

The **creation** group is `New folder…` (F7) then `New file…` (⇧F4), in the File menu and the file context menu alike:
both create into the active pane's folder, so they read as a pair and stay adjacent. Like `Duplicate` and `Rename`, both
sit behind `restrict_destination_actions` and vanish on the search-results virtual pane, which has no destination folder
of its own. macOS SF Symbols are `folder.badge.plus` and `document.badge.plus`.

The file context menu's **hand-it-elsewhere group** runs `Show in Finder`, `Open terminal here`, `Share`, then the
clipboard pair `Copy "name"` / `Copy path`. All three of the first ones give the selection to something outside Cmdr, so
they read as one row of choices. `Share` is macOS-only and carries no ellipsis, being a submenu. It is ABSENT rather than
greyed on two counts: `can_share` is false (a greyed item would have to explain "this row isn't a file yet", which no
label does better than its absence), or macOS offers no service for the selection at all.

The submenu closes with a separator and `Edit extensions…`, which opens System Settings' Extensions pane
(`x-apple.systempreferences:com.apple.ExtensionsPreferences`, the page macOS titles `Login Items & Extensions`). It earns
its place from the absence rule above: the list holds only what macOS currently offers, and the whole submenu vanishes
when that's nothing, so without this item there is no route from Cmdr to the place a missing service is switched back
on. Its click is handled in `menu_handlers.rs` beside the `share-service:` routing rather than through
`menu_id_to_command`, because it isn't a file command: nothing to bind a shortcut to, nothing for the palette to offer,
and the submenu is backend-owned end to end anyway. It reuses `permissions::open_system_settings_url`, the house helper
for these deep links (the Tauri opener plugin's default allowlist drops the `x-apple.systempreferences:` scheme
silently, so a hand-rolled `NSWorkspace` call or a plugin call would both be wrong). Its ID, `share-edit-extensions`,
deliberately sits OUTSIDE the `share-service:` family, since `handle_menu_event` walks that flat ID space by prefix and a
near-miss would hand `perform_offered` an index the offer doesn't have
(`edit_extensions_sits_outside_the_share_service_id_space`).

The file context menu's **cloud group** (macOS) is provider-aware: a concatenation of what each provider can actually
do, rather than one iCloud-shaped block.

- **Google Drive** contributes `Open in Google Drive` and `Copy Google Drive link`, shown whenever
  `FileContextInfo.google_drive_links` is `Some`, plus `Ask Gemini` when that value's `gemini_url` is also `Some`
  (files only). That gate is a resolved Drive item ID, NOT a path prefix, because Drive's mirror mode keeps real files
  outside `~/Library/CloudStorage` (`file_system/google_drive/`). No label takes an ellipsis: each acts on the item it
  was invoked on and picks nothing.
- **iCloud Drive** contributes the eviction pair, `Make available offline` / `Remove download`, exactly one of them,
  keyed on `SyncStatus`. ❌ Don't widen it to other providers: the `FileManager` ubiquity APIs behind it reject
  everything but iCloud, and a provider's own pin/unpin is a File Provider custom action reserved for the app that
  bundles the extension. `CloudProvider::supports_eviction` is where that limit is stated.
- **The provider's own actions** come last, as one flat group (Dropbox, Google Drive, MacDroid, any provider),
  evaluated the way Finder evaluates them, minus Drive's three that duplicate the Drive items above. See **Provider
  actions** above.

All three Drive items also reach the command palette, re-resolving the links from the path so the palette and the menu
agree.

The **Select** submenu (between Edit and View) holds the six selection commands: `Select all` (⌘A), `Deselect all`
(⌘⇧A), `Select all of the same kind` (⌥⇧=), `Invert selection` (⇧8), `Select files…` (+), and `Deselect files…` (-).
Only the first two carry a REGISTERED accelerator; the other four show a dimmed, display-only glyph and are run by
`FilePane`'s keydown handler, each for a reason set out under "Display-only accelerators" above. The two `…` items open
the Selection dialog (see `apps/desktop/src/lib/selection-dialog/CLAUDE.md`). All six are registered in
`MenuState.items`, so a user-customized shortcut flows into the menu through the generic update path — and becomes a
real accelerator when the rebind clears the modifier floor.

The **Go** submenu holds, in order: `Back` (⌘[), `Forward` (⌘]), separator, `Parent folder` (⌘↑), `Home` (⇧⌘H),
separator, `Go to path…` (⌘G), `Go to latest download` (⌘J), separator, `Add to favorites`, `Show favorites` (⌃D),
`Show servers`. The two jump items are `GO_TO_PATH_ID` (`"go_to_path"`) →
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

The **server items** have no menu of their own: they close Go as a pair below a separator, `Connect to server…` (⌘K,
`SERVERS_CONNECT_ID` → `servers.connect`, key `menu.go.connectToServer`) then `Show servers` (no default shortcut,
`SERVERS_SHOW_ID` → `servers.show`, key `menu.go.showServers`). Go is where Finder keeps ⌘K "Connect to Server…", so a
Mac user looks there first. Two items don't earn a top-level menu, and the menu bar is the discoverable door the `⌥F1`
volume selector's bottom row alone wasn't. Both are menu faces of existing palette commands, `FileScoped` because each
acts in the main window: the first opens the add-server sheet, the second takes the focused pane to the servers hub.
macOS SF Symbols are `network` and `server.rack`, both in the Go icon list. What the commands do:
`apps/desktop/src/routes/(main)/command-handlers/servers-handlers.ts`.

The **Help** submenu holds, in order: `Keyboard shortcuts`, separator, `What's new`, `Send feedback…`,
`Send error report…` (Linux, which has no app menu, starts with `About`, `Acknowledgements`, and a separator, and has
no separator after `Keyboard shortcuts`). `What's new`
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

The **Zoom** submenu holds the text-size presets (75/100/125/150 %) plus Zoom in (`Cmd+Plus`) /
Zoom out (`Cmd+Minus`) / 100 % (`Cmd+0`). Items are `App`-scoped so the keyboard accelerators fire in any focused window.
Linux skips the in/out accelerators because GTK intercepts `Cmd+Plus` / `Cmd+Minus` at the toolkit level; the JS
shortcut dispatch path covers Linux.
macOS adds: cmdr (app menu), Window. `menu_bar.rs` has the full item list.

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
**Why**: it is what the code does, and `MenuState` tracks both the `Submenu` reference and the positional index for every updatable item to make it possible. ⚠️ The reason recorded for it — that Tauri's menu API has no `set_accelerator()` — is false, and the simplification it blocks is described under "Accelerator sync". Read that before extending this machinery.

**Decision**: one menu bar for both platforms, written as data (`menu_bar.rs`) and built by one generic builder (`menu_bar_builder.rs`), with every platform difference marked on the row it changes: `macos_only` / `linux_only` rows and menus, `macos(…)` / `split(…)` accelerators, per-platform label keys, and top-level IDs that reach macOS alone.
**Why**: per-platform builders write each item's ID, label, and accelerator twice, so a new item is two edits in lockstep and a drift between them goes unnoticed. As data, a new item is one row, its registration position is its index among the rows its submenu shows, and the table carries no `cfg`, so `menu_bar_test.rs` pins BOTH bars as text on any host. That's the only way to check the Linux bar on a Mac (a real `muda::Menu` panics off the main thread), and the snapshot doubles as the review surface: a menu change reads as a diff of the bar. Sort by, Zoom, and the pane view-mode submenus are rows too, so nothing about the bar lives in a helper.

A few asymmetries look accidental, and each is one row edit away: macOS keeps About and Settings out of `MenuState.items` while Linux tracks them, the Help menu's separator sits after `Keyboard shortcuts` on macOS but after the About group on Linux, and Duplicate's `Cmd+D` is macOS-only.

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
**Why**: Apple's own phrasing ("requires further input") doesn't decide Cmdr's cases, because both of our big confirmations arrive pre-filled and are usually dismissed with Return. The copy/move dialog takes a destination that is genuinely steerable (it's the focused control, and confirm is blocked while the path is invalid), so the destination pane is a suggestion, not the command. The delete dialog can't retarget anything: the file set is fixed, and its trash-vs-permanent switch only picks between two commands that already exist as two menu items (`Delete` / `Delete permanently`), so flipping it is switching command, not steering this one. Hence `Copy…` / `Move…` / `Compress…` / `New folder…` / `New file…` / `Search files…` / `Go to path…` / `Select files…`, and bare `Share` (a submenu opens no dialog and changes nothing about what the command acts on), `Delete`, `Rename` (inline edit, no dialog), `Add to favorites`, `Operation log`, `What's new`, `Acknowledgements`, `Get info`. The looser reading ("a dialog appears") was rejected: nearly every destructive command in Cmdr shows something, so under it the mark lands on almost everything in the File menu and stops carrying information. There are two deliberate exceptions, both OS conventions: `Check for updates…`, because Sparkle-style updaters have made that exact label near-universal on macOS and dropping the ellipsis reads as a typo, and `Edit extensions…`, whose own Decision is below.

**Decision**: `Edit extensions…` (the `Share` submenu's last item) KEEPS its ellipsis, the second deliberate exception alongside `Check for updates…`.
**Why**: The rule above, read literally, says bare: opening System Settings changes nothing about what any Cmdr command acts on. OS parity wins here anyway, because this item's neighbours are the comparison a user actually makes. Both menus macOS puts next to ours end in an ellipsis on a settings-opening item: AppKit's Services menu ends with `Services Settings…`, ShareKit's share menu with `More…` (verified on macOS 26.6.2, 2026-09-09, by reading `AppKit.framework/…/Services.loctable` and `ShareKit.framework/…/ShareKit.loctable`). Cmdr's `Share` submenu sits inches from both, so a bare label there reads as a missing character rather than as a considered distinction — the same reasoning that earned `Check for updates…` its exception, applied where the convention is the *shape* of the label rather than its exact string. Two exceptions, both OS conventions at a boundary the user crosses into the system, still leave the mark informative everywhere else.
Note that Finder has no `Edit Extensions…` item to copy on current macOS: that string appears in no `.loctable`, `.strings`, or `.nib` under `/System/Library` or `/System/Applications` (same verification), so the `@key.description` sends translators to the `Login Items & Extensions` settings page for the noun, and tells them to keep the `…`.

**Decision**: every menu image goes on a real `NSMenuItem` through objc2 and ONE door (`set_menu_item_image`), ❌ never
through `IconMenuItem`, pixels included.
**Why**: macOS 27 hides a menu item's image unless the item opts in, and muda's `IconMenuItem` sets its image on an
`NSMenuItem` we can't reach to opt in, so it's invisible under the macOS 27 SDK. It also rasterizes without
`setTemplate:`, so a monochrome glyph draws as literal pixels: it disappears in the appearance it wasn't baked for and
stays dark on a highlighted row. `NSMenuItem.setImage:` with a real symbol image gets AppKit's tinting for light, dark,
and highlight. The menu BAR is reached by walking `NSApplication.mainMenu()` post-construction (`set_macos_menu_icons`);
a CONTEXT menu has no `NSMenu` to walk, so it goes through `NSMenuDidBeginTrackingNotification` instead — see "Images on
a CONTEXT menu", and "The context menu's header line", which crosses the same boundary for its attributed title. The
rejected alternative was a sweep on that notification opting in whatever item already carried an image: it keeps
`IconMenuItem` but has to tell Cmdr's menus from AppKit's (the borrowed Services submenu, the menu bar) with nothing to
key on, and it leaves a second image route open for the next icon set to take. One door plus a clippy ban closes the
class of bug.

## Gotchas

- **No `Menu::default()`**: Both platforms build from scratch. The old approach inherited system
  defaults that added unwanted items.
- **Tab as accelerator**: Switch pane uses Tab, which could conflict with menu bar accessibility
  navigation. If issues arise, omit the accelerator and rely on JS dispatch.
- **Custom MenuItems for Cut/Copy/Paste/Select all**: The Edit menu uses custom MenuItems (not
  PredefinedMenuItems) for Cut, Copy, Paste, and Move here; the Select menu does the same for
  Select all. In `handle_menu_event`, these are handled specially: if the main window is focused,
  they route through `execute-command` so the frontend can decide between file and text semantics
  (via `document.activeElement` check). If a non-main window is focused,
  `send_native_edit_action()` in `menu_handlers.rs` sends the native
  `copy:`/`cut:`/`paste:`/`selectAll:` selector through the responder chain via
  `NSApplication.sendAction:to:from:`, replicating what PredefinedMenuItems do internally. This
  ensures text clipboard and text select-all work natively in all windows. Undo and Redo remain
  PredefinedMenuItems since they only apply to text fields. ❗ In practice the native branch serves Settings and the
  other main-bar windows: a focused VIEWER has swapped the viewer bar in, and all four of its items carry their own ids
  and their own lanes (§ "Per-window menu activation"). The viewer's Cut / Paste land in the same
  `send_native_edit_action`, off their own branch and unconditionally: `native_edit_selector_for` maps both bars' ids,
  and the main-bar branch asks a question ("is the MAIN window focused?") that a viewer click would answer right by
  accident rather than by rule.
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
- **An item carrying a view still claims the image column.** AppKit reserves image space for a view item's `image`, so
  `tag_row/loan.rs` clears the first tag item's bitmap before `setView:`, or every title in the menu moves 24 pt right,
  and `context_menu_icons.rs` never puts one on an item that carries a view, whichever observer runs first. Hidden items
  don't count. (macOS 27.0, `NSMenu.size` offscreen, 2026-09-16.)
- **Pin tab label**: `pin_tab` in MenuState is updated dynamically by the frontend to show
  "Pin tab" or "Unpin tab" based on the active tab's state.
- **Reopen closed tab item**: The Tab submenu includes "Reopen closed tab" (⌘⇧T on macOS) between
  Close tab and the Next/Previous tab pair. The item is created **disabled**. The frontend pushes the verdict through
  `set_reopen_closed_tab_enabled(enabled: bool)` after every close, reopen, and focus change, which stores it in
  `MenuState.reopen_closed_tab_enabled` and recomputes the items (§ Dialog refusals), so the menu always reflects the
  focused pane's closed-tab stack.
- **Open terminal here item** (`OPEN_TERMINAL_HERE_ID` → `file.openTerminalHere`, macOS only): sits in the File menu
  right after Show in Finder (⌥⌘T), and in the file context menu next to it. It's the one item whose enabled state
  follows the FOCUSED PANE rather than the window: a pane on MTP or ADB has no path a shell can `cd` into. The verdict
  arrives through `set_open_terminal_here_enabled` and is STORED in `MenuState.open_terminal_here_enabled`, not just
  applied, because every recompute derives the item from all its inputs at once (§ Dialog refusals). That is also what
  restores it after a menu-bar rebuild, since the frontend's `menu-bar-rebuilt` handler calls
  `activate_window_menu('main')`. The context-menu copy needs no channel: it's built
  per right-click, so `show_file_context_menu` carries the answer in `PaneContextMenuFacts.can_open_terminal_here`.
  ⚠️ Greying is CHROME: a disabled item's accelerator still fires, and the palette has no disabled state at all, so
  the real refusal is the frontend handler (`apps/desktop/src/lib/open-terminal/CLAUDE.md`).
