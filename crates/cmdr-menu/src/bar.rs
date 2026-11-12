//! The menu bar on macOS, Windows, and Linux: every menu, item, separator, and accelerator, in
//! display order. The desktop's `menu_bar_builder.rs` builds it into native menus for the running
//! platform, the browser demo renders the macOS one, and `spec.rs` defines the words it's written
//! in. The Windows bar has the Linux bar's rows and keys until the Windows menu model is decided
//! (`DETAILS.md` § "Three platforms").
//!
//! A row's position in its submenu is the position the builder registers it at for accelerator
//! sync, so adding or moving a row can't desync the two. A new item is one row here.
//!
//! Each row's default accelerator equals the registry's first default key for its command on each
//! platform, or sits on `bar_parity_test.rs`'s reasoned exception list.
//!
//! Where the platforms differ, the row says so:
//! - `macos_only` / `windows_and_linux` rows, and `macos_menu` for the app and Window menus.
//!   Windows and Linux have no app menu, so About and the credits sit under Help, and Settings and
//!   the license under Edit.
//! - `macos(…)` accelerators, which Windows and Linux leave out. For the F-keys, Tab, Space, and
//!   `Cmd+Equal` / `Cmd+Minus` that's because GTK intercepts them before the webview (and
//!   `is_focused()` fails there), so the frontend's keydown dispatch handles them.
//! - `cmd_or_ctrl!(…)` accelerators: ⌘ on macOS, the same chord on Ctrl on Windows and Linux.
//! - `split(…)` accelerators, spelled per platform. Labels never are: a label that reads
//!   differently per OS is one catalog key whose value carries a branch per OS.
//! - Top-level menu IDs, which reach macOS alone (`spec::menu`).
//! - GTK `&` mnemonics on Linux, allocated per submenu from the translated labels (the desktop's
//!   `menu/mnemonics.rs`), so no row names one.
//!
//! ❗ Every accelerator here is a string muda parses, and a token it doesn't know is DISCARDED
//! rather than reported (`tauri::menu::MenuItem::new` is `.parse().ok()`), so the item comes up
//! with no key at all and nothing says a word. muda's modifier vocabulary is `OPTION`/`ALT`,
//! `CONTROL`/`CTRL`, `COMMAND`/`CMD`/`SUPER`, `SHIFT` (`muda-0.20.0/src/accelerator/mod.rs:249`): ❌
//! never `Opt`, which cost Copy path and Show in Finder their accelerators in every release until
//! 2026-09-16. The desktop's `menu/accelerators.rs` tests parse every string here with muda.
//!
//! ❗ A ⌘ chord is `cmd_or_ctrl!`, ❌ never a `Cmd+…` string on Windows or Linux: muda maps
//! `"CMD"` to `Modifiers::META`, which GTK binds and prints as the Super key (#284), and Win32 as
//! the Windows key. `same_on_all` and `split` refuse one at compile time, and `bar_test.rs` pins
//! it.

use crate::ids::{
    ABOUT_ID, ACKNOWLEDGEMENTS_ID, APP_MENU_ID, ASK_CMDR_ID, CALCULATE_FOLDER_SIZES_ID, CHANGELOG_ID,
    CHECK_FOR_UPDATES_ID, CLI_INSTALL_ID, CLONE_PANE_ID, CLOSE_OTHER_TABS_ID, CLOSE_TAB_ID, COMMAND_PALETTE_ID,
    COMPARE_DIRECTORIES_ID, COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID, DESELECT_FILES_ID, EDIT_COPY_ID,
    EDIT_CUT_ID, EDIT_ID, EDIT_MENU_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FAVORITES_ADD_ID,
    FAVORITES_OPEN_ID, FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_DUPLICATE_ID,
    FILE_MENU_ID, FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GET_INFO_ID, GO_BACK_ID,
    GO_FORWARD_ID, GO_HOME_ID, GO_LATEST_DOWNLOAD_ID, GO_MENU_ID, GO_PARENT_ID, GO_ROOT_ID, GO_TO_PATH_ID,
    HELP_ACCESSIBILITY_ID, HELP_DEBUG_LOG_ID, HELP_MENU_ID, HELP_SEND_ERROR_REPORT_ID, HELP_SEND_FEEDBACK_ID,
    HELP_SHORTCUTS_ID, HELP_WHATS_NEW_ID, INVERT_SELECTION_ID, MULTI_RENAME_ID, NEW_TAB_ID, NEXT_TAB_ID, OPEN_ID,
    OPEN_ONBOARDING_ID, OPEN_TERMINAL_HERE_ID, OPERATION_LOG_ID, PIN_TAB_MENU_ID, PREV_TAB_ID, QUEUE_SHOW_ID,
    QUICK_LOOK_ID, RENAME_ID, REOPEN_CLOSED_TAB_ID, SEARCH_FILES_ID, SELECT_ALL_ID, SELECT_FILES_ID, SELECT_MENU_ID,
    SELECT_SAME_KIND_ID, SERVERS_CONNECT_ID, SERVERS_SHOW_ID, SETTINGS_ID, SHOW_HIDDEN_FILES_ID, SHOW_IN_FINDER_ID,
    SORT_ASCENDING_ID, SORT_BY_ADDED_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MENU_ID,
    SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID, SUGGESTED_OPS_ID, SWAP_PANES_ID,
    SWITCH_PANE_ID, TAB_MENU_ID, UNDO_TRASH_ID, VIEW_MENU_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID,
    VIEW_MODE_FULL_LEFT_ID, VIEW_MODE_FULL_RIGHT_ID, VIEW_ZOOM_75_ID, VIEW_ZOOM_100_ID, VIEW_ZOOM_125_ID,
    VIEW_ZOOM_150_ID, VIEW_ZOOM_IN_ID, VIEW_ZOOM_OUT_ID, WINDOW_MENU_ID,
};
use crate::spec::SortCheck::{self, Column, Order};
use crate::spec::ViewMode;
use crate::spec::{
    Accelerator, BarMenu, CheckRole, Entry, EntryKind, Label, NONE, Pane, Platform, Predefined, SEPARATOR, SubmenuRole,
    SubmenuSpec, check, cmd_or_ctrl, displayed_item, item, labeled_item, macos, macos_menu, macos_only, menu,
    predefined, same_on_all, split, submenu, windows_and_linux,
};
use cmdr_sort::{SortColumn, SortOrder};

/// A View > Sort by row: a check item showing whether the focused pane asked for `role`.
const fn sort_check(id: &'static str, key: &'static str, accelerator: Accelerator, role: SortCheck) -> Entry {
    check(id, key, accelerator, CheckRole::Sort(role))
}

/// "Copy path", matching Finder's "Copy as Pathname". Must stay in sync with the `file.copyPath`
/// default in `src/lib/commands/sources/file-list.ts` (`⌘⌥C`), which is what the frontend
/// dispatcher listens for. The file context menu uses it too.
pub const COPY_PATH_ACCELERATOR: Accelerator = cmd_or_ctrl!("Alt+C");

/// "Show in Finder" / "Show in File Explorer" / "Show in file manager", here and in the file
/// context menu. One key: its catalog value names each OS's file manager, and the host picks the
/// words, so this crate never chooses text by `Platform`.
pub const SHOW_IN_FILE_MANAGER_KEY: &str = "menu.file.showInFileManager";
pub const SHOW_IN_FILE_MANAGER_ACCELERATOR: Accelerator = split("Alt+Cmd+O", "Alt+Ctrl+O", "Alt+Ctrl+O");

/// The display-only shortcut `MENU_BAR` gives `menu_id` on `platform`, if any.
///
/// The spec is the fallback answer, so it's what the desktop's `MenuState::set_item_label` composes
/// a rewritten label with while the user hasn't rebound the command. A host's record of a rebind
/// overrides it once they have; ❗ read that FIRST, or a rebind's glyph reverts on the next label
/// change.
pub fn spec_display_accelerator(menu_id: &str, platform: Platform) -> Option<&'static str> {
    match spec_entry(menu_id, platform)? {
        EntryKind::Item(item) => item.display_accelerator,
        _ => None,
    }
}

/// The accelerator `MENU_BAR` registers `menu_id` with on `platform`, if any. The desktop seeds the
/// view-mode pair's from it, so a rebuild can't put back a chord the bar never had.
pub fn spec_accelerator(menu_id: &str, platform: Platform) -> Option<&'static str> {
    match spec_entry(menu_id, platform)? {
        EntryKind::Item(item) => item.accelerator.on(platform),
        EntryKind::Check(check) => check.accelerator.on(platform),
        _ => None,
    }
}

/// The item or check `MENU_BAR` builds as `menu_id` on `platform`, nested ones included.
pub fn spec_entry(menu_id: &str, platform: Platform) -> Option<&'static EntryKind> {
    every_entry(platform).into_iter().find(|entry| match entry {
        EntryKind::Item(item) => item.id == menu_id,
        EntryKind::Check(check) => check.id == menu_id,
        _ => false,
    })
}

/// Every row `platform` builds, nested ones included, in build order: a submenu's row, then its
/// own rows, then the row after it.
pub fn every_entry(platform: Platform) -> Vec<&'static EntryKind> {
    fn walk(spec: &'static SubmenuSpec, platform: Platform, out: &mut Vec<&'static EntryKind>) {
        for entry in spec.entries_on(platform) {
            out.push(entry);
            if let EntryKind::Submenu(nested) = entry {
                walk(nested, platform, out);
            }
        }
    }
    let mut out = Vec::new();
    for bar_menu in menus_on(platform) {
        walk(&bar_menu.submenu, platform, &mut out);
    }
    out
}

/// Every accelerator `platform`'s bar registers, with the ID of the item it sits on.
pub fn registered_accelerators(platform: Platform) -> Vec<(&'static str, &'static str)> {
    every_entry(platform)
        .into_iter()
        .filter_map(|entry| match entry {
            EntryKind::Item(item) => Some((item.id, item.accelerator.on(platform)?)),
            EntryKind::Check(check) => Some((check.id, check.accelerator.on(platform)?)),
            _ => None,
        })
        .collect()
}

/// The top-level menus `platform`'s bar holds, in order.
pub fn menus_on(platform: Platform) -> impl Iterator<Item = &'static BarMenu> {
    MENU_BAR.iter().filter(move |bar_menu| bar_menu.is_on(platform))
}

/// The macOS app menu's title, and the viewer bar's.
///
/// Deliberately NOT a catalog key: macOS names the app menu after the application, and the
/// application is called `Cmdr` (the `productName` in `tauri.conf.json`, and the spelling the brand
/// uses everywhere). Translating it would make the one item every macOS user navigates by
/// unrecognizable, and it would earn a `sameAsSourceJustification` in every locale for nothing.
///
/// Only macOS has an app menu: Windows and Linux put About under Help and Settings under Edit.
pub const APP_MENU_TITLE: &str = "Cmdr";

// The items the macOS app menu holds, which Windows and Linux spread over Edit and Help.
const ABOUT: Entry = item(ABOUT_ID, "menu.app.about", NONE);
/// Credits the open-source libraries Cmdr ships: app metadata, not a help topic, so on macOS it sits
/// next to About and the license, where macOS apps that ship one put it.
const ACKNOWLEDGEMENTS: Entry = item(ACKNOWLEDGEMENTS_ID, "menu.app.acknowledgements", NONE).untracked();
const LICENSE: Entry = labeled_item(ENTER_LICENSE_KEY_ID, Label::License, NONE).untracked();
const CHECK_FOR_UPDATES: Entry = item(CHECK_FOR_UPDATES_ID, "menu.app.checkForUpdates", NONE);
/// Opens the What's new popup with the latest releases, the same command as Help > What's new.
const CHANGELOG: Entry = item(CHANGELOG_ID, "menu.app.changelog", NONE);
const SETTINGS: Entry = item(SETTINGS_ID, "menu.app.settings", cmd_or_ctrl!(","));

pub const MENU_BAR: &[BarMenu] = &[
    macos_menu(
        APP_MENU_ID,
        Label::AppName,
        &[
            // Untracked here, tracked on Windows and Linux (under Help and Edit), so only they sync a custom
            // shortcut onto About and Settings.
            ABOUT.untracked(),
            ACKNOWLEDGEMENTS,
            LICENSE,
            CHECK_FOR_UPDATES,
            CHANGELOG,
            // Re-entry to the onboarding wizard. Windows and Linux have no menu entry (palette only) by design:
            // `lib/onboarding/CLAUDE.md` § "Re-entry points".
            item(OPEN_ONBOARDING_ID, "menu.app.onboarding", NONE),
            // Its label follows the filesystem: Install, Uninstall, or Homebrew's. Built with the
            // Install wording; `cli::commands::refresh_menu_item` puts the real one on after every
            // build and on every focus-gain. macOS only, like the whole app menu.
            item(CLI_INSTALL_ID, "menu.app.cliCommandInstall", NONE),
            SEPARATOR,
            SETTINGS.untracked(),
            SEPARATOR,
            // AppKit fills it with Action extensions and other apps' services (Ghostty's "New tab
            // here", Nimble Commander's "Reveal", Quick Actions); muda wires
            // `NSApplication.servicesMenu` to it.
            predefined(Predefined::Services, "menu.app.services"),
            SEPARATOR,
            predefined(Predefined::Hide, "menu.app.hide"),
            predefined(Predefined::HideOthers, "menu.app.hideOthers"),
            predefined(Predefined::ShowAll, "menu.app.showAll"),
            SEPARATOR,
            predefined(Predefined::Quit, "menu.app.quit"),
        ],
    ),
    menu(
        FILE_MENU_ID,
        "menu.bar.file",
        &[
            item(OPEN_ID, "menu.file.open", NONE),
            item(FILE_VIEW_ID, "menu.file.view", macos("F3")),
            item(EDIT_ID, "menu.file.edit", macos("F4")),
            SEPARATOR,
            item(FILE_COPY_ID, "menu.file.copy", macos("F5")),
            item(FILE_MOVE_ID, "menu.file.move", macos("F6")),
            item(FILE_DUPLICATE_ID, "menu.file.duplicate", macos("Cmd+D")),
            item(FILE_COMPRESS_ID, "menu.file.compress", macos("Alt+F5")),
            item(FILE_NEW_FOLDER_ID, "menu.file.newFolder", macos("F7")),
            item(FILE_NEW_FILE_ID, "menu.file.newFile", macos("Shift+F4")),
            item(FILE_DELETE_ID, "menu.file.delete", macos("F8")),
            item(
                FILE_DELETE_PERMANENTLY_ID,
                "menu.file.deletePermanently",
                macos("Shift+F8"),
            ),
            SEPARATOR,
            item(RENAME_ID, "menu.file.rename", macos("F2")),
            item(MULTI_RENAME_ID, "menu.file.multiRename", same_on_all("Ctrl+M")),
            SEPARATOR,
            item(
                SHOW_IN_FINDER_ID,
                SHOW_IN_FILE_MANAGER_KEY,
                SHOW_IN_FILE_MANAGER_ACCELERATOR,
            ),
            // Next to Show in Finder: the same gesture aimed at another app. macOS only, like the
            // launch module behind it. Built enabled; `set_open_terminal_here_enabled` greys it out
            // while the focused pane sits somewhere a shell can't `cd` into.
            macos_only(item(OPEN_TERMINAL_HERE_ID, "menu.file.openTerminalHere", macos("F9"))),
            // Get info and Quick Look open macOS's own panels, so they're macOS only, like the
            // `get_info` and `quick_look` capabilities (the app's `menu/capability_rows_test.rs`
            // keeps the two in step).
            macos_only(item(GET_INFO_ID, "menu.file.getInfo", cmd_or_ctrl!("I"))),
            // Shift+Space, not plain Space: AppKit consumes modifier accelerators before the webview
            // can, so the menu fires. Plain Space was dead, eaten by the webview's selection toggle
            // before AppKit's menu dispatcher saw it.
            macos_only(item(QUICK_LOOK_ID, "menu.file.quickLook", macos("Shift+Space"))),
        ],
    ),
    menu(
        EDIT_MENU_ID,
        "menu.bar.edit",
        &[
            macos_only(predefined(Predefined::Undo, "menu.edit.undo")),
            macos_only(predefined(Predefined::Redo, "menu.edit.redo")),
            // Files, unlike the text Undo above: the trash toast's Undo, for keyboard users and
            // for after the toast has gone. Starts disabled; `update_undo_trash_menu` turns it on
            // and names what it would put back once there's a trash to undo.
            item(UNDO_TRASH_ID, "menu.edit.undoTrash", NONE).disabled(),
            SEPARATOR,
            // Custom items rather than predefined ones, so ⌘X / ⌘C / ⌘V route through
            // `execute-command` and the frontend picks the text clipboard (an input has focus) or
            // the file clipboard. In other windows, `handle_menu_event` forwards the native selector.
            item(EDIT_CUT_ID, "menu.edit.cut", cmd_or_ctrl!("X")),
            item(EDIT_COPY_ID, "menu.edit.copy", cmd_or_ctrl!("C")),
            item(EDIT_PASTE_ID, "menu.edit.paste", cmd_or_ctrl!("V")),
            item(
                EDIT_PASTE_MOVE_ID,
                "menu.edit.moveHere",
                split("Alt+Cmd+V", "Ctrl+Alt+V", "Ctrl+Alt+V"),
            ),
            SEPARATOR,
            item(COPY_PATH_ID, "menu.edit.copyPath", COPY_PATH_ACCELERATOR),
            item(COPY_FILENAME_ID, "menu.edit.copyFilename", NONE),
            SEPARATOR,
            item(SEARCH_FILES_ID, "menu.edit.searchFiles", cmd_or_ctrl!("F")),
            windows_and_linux(SEPARATOR),
            windows_and_linux(SETTINGS),
            windows_and_linux(LICENSE),
            windows_and_linux(CHECK_FOR_UPDATES),
            windows_and_linux(CHANGELOG),
        ],
    ),
    // Between Edit and View. The last three rows carry DISPLAY-only shortcuts: their keys have no
    // ⌘ / ⌃ / ⌥, so a real accelerator would fire app-wide and eat `*`, `+`, and `-` in every text
    // field. FilePane's keydown handler binds them; the menu only says what they are.
    menu(
        SELECT_MENU_ID,
        "menu.bar.select",
        &[
            item(SELECT_ALL_ID, "menu.select.all", cmd_or_ctrl!("A")),
            item(DESELECT_ALL_ID, "menu.select.deselectAll", cmd_or_ctrl!("Shift+A")),
            // Built with the neutral label; the frontend rewrites it to say what the cursor row
            // would select right now (`update_select_same_kind_menu`). `⌥⇧=` is the physical
            // spelling, the one honest on a layout where that key isn't `+`.
            displayed_item(SELECT_SAME_KIND_ID, "menu.select.sameKind", "⌥⇧="),
            // `*` means whichever key types `*` on the user's layout (`sources/file-list.ts`).
            displayed_item(INVERT_SELECTION_ID, "menu.select.invert", "*"),
            SEPARATOR,
            displayed_item(SELECT_FILES_ID, "menu.select.files", "+"),
            displayed_item(DESELECT_FILES_ID, "menu.select.deselectFiles", "-"),
            SEPARATOR,
            // Linux leaves F-keys to JS dispatch, like the File menu's ⇧F4.
            item(
                COMPARE_DIRECTORIES_ID,
                "menu.select.compareDirectories",
                macos("Shift+F2"),
            ),
        ],
    ),
    menu(
        VIEW_MENU_ID,
        "menu.bar.view",
        &[
            // Both panes' Full / Brief pairs always exist. Only the active pane's pair carries the
            // accelerator, and `rebuild_view_mode_items` moves it as focus moves, reinserting by
            // index: Full stays at 0 and Brief at 1. The build starts with the left pane active.
            submenu(
                None,
                "menu.view.leftPane",
                SubmenuRole::Pane(Pane::Left),
                &[
                    check(
                        VIEW_MODE_FULL_LEFT_ID,
                        "menu.view.fullView",
                        cmd_or_ctrl!("1"),
                        CheckRole::ViewMode(Pane::Left, ViewMode::Full),
                    ),
                    check(
                        VIEW_MODE_BRIEF_LEFT_ID,
                        "menu.view.briefView",
                        cmd_or_ctrl!("2"),
                        CheckRole::ViewMode(Pane::Left, ViewMode::Brief),
                    ),
                ],
            ),
            submenu(
                None,
                "menu.view.rightPane",
                SubmenuRole::Pane(Pane::Right),
                &[
                    check(
                        VIEW_MODE_FULL_RIGHT_ID,
                        "menu.view.fullView",
                        NONE,
                        CheckRole::ViewMode(Pane::Right, ViewMode::Full),
                    ),
                    check(
                        VIEW_MODE_BRIEF_RIGHT_ID,
                        "menu.view.briefView",
                        NONE,
                        CheckRole::ViewMode(Pane::Right, ViewMode::Brief),
                    ),
                ],
            ),
            SEPARATOR,
            check(
                SHOW_HIDDEN_FILES_ID,
                "menu.view.showHiddenFiles",
                cmd_or_ctrl!("Shift+."),
                CheckRole::ShowHiddenFiles,
            ),
            // GTK intercepts F-row keys but lets Cmd+digit chords through, so every platform
            // registers these; the ⌘F3–⌘F6 alternates go through JS dispatch alone. Date created, Date
            // added, and the two order items have no shortcut to sync, so nothing tracks them. Every
            // row checks what the focused pane asked for (`sort_checks.rs`).
            submenu(
                Some(SORT_BY_MENU_ID),
                "menu.view.sortBy",
                SubmenuRole::SortBy,
                &[
                    sort_check(
                        SORT_BY_NAME_ID,
                        "menu.sort.name",
                        cmd_or_ctrl!("3"),
                        Column(SortColumn::Name),
                    )
                    .tracked(),
                    sort_check(
                        SORT_BY_EXTENSION_ID,
                        "menu.sort.extension",
                        cmd_or_ctrl!("4"),
                        Column(SortColumn::Extension),
                    )
                    .tracked(),
                    sort_check(
                        SORT_BY_MODIFIED_ID,
                        "menu.sort.dateModified",
                        cmd_or_ctrl!("5"),
                        Column(SortColumn::Modified),
                    )
                    .tracked(),
                    sort_check(
                        SORT_BY_SIZE_ID,
                        "menu.sort.size",
                        cmd_or_ctrl!("6"),
                        Column(SortColumn::Size),
                    )
                    .tracked(),
                    sort_check(
                        SORT_BY_CREATED_ID,
                        "menu.sort.dateCreated",
                        NONE,
                        Column(SortColumn::Created),
                    ),
                    sort_check(SORT_BY_ADDED_ID, "menu.sort.dateAdded", NONE, Column(SortColumn::Added)),
                    SEPARATOR,
                    sort_check(
                        SORT_ASCENDING_ID,
                        "menu.sort.ascending",
                        NONE,
                        Order(SortOrder::Ascending),
                    ),
                    sort_check(
                        SORT_DESCENDING_ID,
                        "menu.sort.descending",
                        NONE,
                        Order(SortOrder::Descending),
                    ),
                ],
            ),
            // Each preset writes `appearance.textSize` through the command-execute event, and Zoom
            // in / out move it by 10 percentage points. GTK intercepts these two, so Linux leaves
            // them to JS dispatch.
            // ❗ Zoom in is `Cmd+Equal`, ❌ never `Cmd+Plus`: muda's key vocabulary has no `Plus`
            // (`accelerator.rs:151`'s `parse_code`), so the item came up with no key at all. `=` is
            // the physical key, and `view.zoom.in` already binds `⌘=` alongside `⌘+`.
            submenu(
                None,
                "menu.view.zoom",
                SubmenuRole::Plain,
                &[
                    item(VIEW_ZOOM_75_ID, "menu.zoom.percent75", NONE).untracked(),
                    item(VIEW_ZOOM_100_ID, "menu.zoom.percent100", cmd_or_ctrl!("0")).untracked(),
                    item(VIEW_ZOOM_125_ID, "menu.zoom.percent125", NONE).untracked(),
                    item(VIEW_ZOOM_150_ID, "menu.zoom.percent150", NONE).untracked(),
                    SEPARATOR,
                    item(VIEW_ZOOM_IN_ID, "menu.zoom.in", macos("Cmd+Equal")).untracked(),
                    item(VIEW_ZOOM_OUT_ID, "menu.zoom.out", macos("Cmd+Minus")).untracked(),
                ],
            ),
            SEPARATOR,
            // Tab conflicts with GTK's own keyboard navigation, so Linux leaves it to JS dispatch.
            item(SWITCH_PANE_ID, "menu.view.switchPane", macos("Tab")),
            item(SWAP_PANES_ID, "menu.view.swapPanes", cmd_or_ctrl!("U")),
            item(CLONE_PANE_ID, "menu.view.clonePane", cmd_or_ctrl!("Shift+C")),
            item(
                CALCULATE_FOLDER_SIZES_ID,
                "menu.view.calculateFolderSizes",
                same_on_all("Alt+Shift+Enter"),
            ),
            SEPARATOR,
            item(COMMAND_PALETTE_ID, "menu.view.commandPalette", cmd_or_ctrl!("Shift+P")),
            // The next four sync their accelerators from registry shortcuts (`queue.show`,
            // `log.operationLog`, `suggestedOps.show`, `askCmdr.toggle`), so these are the defaults.
            // The queue sits next to the log so the present-tense and past-tense views of the same
            // work read as a pair.
            item(QUEUE_SHOW_ID, "menu.view.operationQueue", cmd_or_ctrl!("Alt+Q")),
            // ⌥⌘O, the first choice, is taken by Show in Finder.
            item(OPERATION_LOG_ID, "menu.view.operationLog", cmd_or_ctrl!("Alt+L")),
            // No default: the status-corner indicator is the everyday way in, and a suggestion waits
            // indefinitely, so this isn't a key anyone reaches for mid-task. A user who wants one
            // binds it.
            item(SUGGESTED_OPS_ID, "menu.view.suggestedOps", NONE),
            item(ASK_CMDR_ID, "menu.view.askCmdr", cmd_or_ctrl!("Alt+A")),
        ],
    ),
    menu(
        GO_MENU_ID,
        "menu.bar.go",
        &[
            item(GO_BACK_ID, "menu.go.back", cmd_or_ctrl!("[")),
            item(GO_FORWARD_ID, "menu.go.forward", cmd_or_ctrl!("]")),
            SEPARATOR,
            item(GO_PARENT_ID, "menu.go.parentFolder", cmd_or_ctrl!("Up")),
            item(GO_ROOT_ID, "menu.go.rootFolder", cmd_or_ctrl!("/")),
            // Shift+Cmd+H, not Cmd+H: AppKit owns Cmd+H for Hide Cmdr and swallows it before the
            // webview ever sees a keydown.
            item(GO_HOME_ID, "menu.go.home", cmd_or_ctrl!("Shift+H")),
            SEPARATOR,
            item(GO_TO_PATH_ID, "menu.go.goToPath", cmd_or_ctrl!("G")),
            item(GO_LATEST_DOWNLOAD_ID, "menu.go.goToLatestDownload", cmd_or_ctrl!("J")),
            SEPARATOR,
            // No default: `favorites.add` ships without a shortcut, and the sync picks up whatever
            // the user binds in Settings > Keyboard shortcuts.
            item(FAVORITES_ADD_ID, "menu.go.addToFavorites", NONE),
            // ⌃D, matching the registry default and what Total Commander binds for the same list.
            item(FAVORITES_OPEN_ID, "menu.go.showFavorites", same_on_all("Ctrl+D")),
            SEPARATOR,
            // The servers pair. ⌘K is Finder's "Connect to Server…", and Finder keeps it in Go too.
            item(SERVERS_CONNECT_ID, "menu.go.connectToServer", cmd_or_ctrl!("K")),
            // No default: `servers.show` ships without a shortcut.
            item(SERVERS_SHOW_ID, "menu.go.showServers", NONE),
        ],
    ),
    menu(
        TAB_MENU_ID,
        "menu.bar.tab",
        &[
            item(NEW_TAB_ID, "menu.tab.newTab", cmd_or_ctrl!("T")),
            item(CLOSE_TAB_ID, "menu.tab.closeTab", cmd_or_ctrl!("W")),
            // Starts disabled; `set_reopen_closed_tab_enabled` turns it on after the first close.
            item(
                REOPEN_CLOSED_TAB_ID,
                "menu.tab.reopenClosedTab",
                cmd_or_ctrl!("Shift+T"),
            )
            .disabled(),
            SEPARATOR,
            item(NEXT_TAB_ID, "menu.tab.nextTab", same_on_all("Ctrl+Tab")),
            item(PREV_TAB_ID, "menu.tab.previousTab", same_on_all("Ctrl+Shift+Tab")),
            SEPARATOR,
            item(PIN_TAB_MENU_ID, "menu.tab.pinTab", NONE).pin_tab(),
            item(CLOSE_OTHER_TABS_ID, "menu.tab.closeOtherTabs", NONE),
        ],
    ),
    macos_menu(
        WINDOW_MENU_ID,
        Label::Key("menu.bar.window"),
        &[
            predefined(Predefined::Minimize, "menu.window.minimize"),
            predefined(Predefined::Maximize, "menu.window.zoom"),
        ],
    ),
    // macOS adds a search field to the menu `cleanup_macos_menus` registers as the Help menu.
    menu(
        HELP_MENU_ID,
        "menu.bar.help",
        &[
            windows_and_linux(ABOUT),
            windows_and_linux(ACKNOWLEDGEMENTS),
            windows_and_linux(SEPARATOR),
            item(HELP_SHORTCUTS_ID, "menu.help.keyboardShortcuts", NONE),
            item(HELP_ACCESSIBILITY_ID, "menu.help.accessibility", NONE),
            macos_only(SEPARATOR),
            item(HELP_WHATS_NEW_ID, "menu.help.whatsNew", NONE),
            item(HELP_SEND_FEEDBACK_ID, "menu.help.sendFeedback", NONE),
            item(HELP_DEBUG_LOG_ID, "menu.help.viewDebugLog", NONE),
            item(HELP_SEND_ERROR_REPORT_ID, "menu.help.sendErrorReport", NONE),
        ],
    ),
];

#[cfg(test)]
#[path = "bar_test.rs"]
mod bar_test;

#[cfg(test)]
#[path = "bar_parity_test.rs"]
mod bar_parity_test;
