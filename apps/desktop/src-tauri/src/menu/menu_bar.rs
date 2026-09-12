//! The native menu bar on both platforms: every menu, item, separator, and accelerator, in display
//! order. `menu_bar_builder.rs` builds it for the running platform, and `menu_spec.rs` defines the
//! words it's written in.
//!
//! A row's position in its submenu is the position the builder registers it at for accelerator
//! sync, so adding or moving a row can't desync the two. A new item is one row here.
//!
//! Where the platforms differ, the row says so:
//! - `macos_only` / `linux_only` rows, and `macos_menu` for the app and Window menus. Linux has no
//!   app menu, so About and the credits sit under Help, and Settings and the license under Edit.
//! - `macos(…)` accelerators, which Linux leaves out. For the F-keys, Tab, Space, and `Cmd+Plus` /
//!   `Cmd+Minus` that's because GTK intercepts them before the webview (and `is_focused()` fails
//!   there), so the frontend's keydown dispatch handles them.
//! - `split(…)` accelerators and `Label::PerPlatformKey` labels, spelled per platform.
//! - Top-level menu IDs, which reach macOS alone (`menu_spec::menu`).
//! - GTK `&` mnemonics on Linux, allocated per submenu from the translated labels
//!   (`mnemonics.rs`), so no row names one.
//!
//! ❗ The `Cmd+…` accelerators both platforms share bind to SUPER on Linux, not Ctrl: muda maps
//! `"CMD"` to `Modifiers::META`, which is Super on GTK, and only `CmdOrCtrl` resolves to Ctrl off
//! macOS. So the Linux menu prints Super chords. Users still get Ctrl because the frontend keydown
//! layer accepts `metaKey || ctrlKey`; it's the LABEL that's wrong. Switching to `CmdOrCtrl` also
//! changes the macOS binding, so it needs a check on both platforms rather than a blind sweep. See
//! `docs/notes/linux-gaps-2026-08-10.md`.

use super::menu_spec::{
    Accelerator, BarMenu, CheckRole, Entry, Label, NONE, Pane, PerPlatform, Predefined, SEPARATOR, SubmenuRole, both,
    check, item, labeled_item, linux_only, macos, macos_menu, macos_only, menu, predefined, split, submenu,
};
use super::{
    ABOUT_ID, ACKNOWLEDGEMENTS_ID, APP_MENU_ID, ASK_CMDR_ID, CHANGELOG_ID, CHECK_FOR_UPDATES_ID, CLOSE_OTHER_TABS_ID,
    CLOSE_TAB_ID, COMMAND_PALETTE_ID, COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID, DESELECT_FILES_ID, EDIT_COPY_ID,
    EDIT_CUT_ID, EDIT_ID, EDIT_MENU_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FAVORITES_ADD_ID,
    FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_DUPLICATE_ID, FILE_MENU_ID,
    FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GET_INFO_ID, GO_BACK_ID, GO_FORWARD_ID,
    GO_HOME_ID, GO_LATEST_DOWNLOAD_ID, GO_MENU_ID, GO_PARENT_ID, GO_TO_PATH_ID, HELP_MENU_ID,
    HELP_SEND_ERROR_REPORT_ID, HELP_SEND_FEEDBACK_ID, HELP_SHORTCUTS_ID, HELP_WHATS_NEW_ID, INVERT_SELECTION_ID,
    NEW_TAB_ID, NEXT_TAB_ID, OPEN_ID, OPEN_ONBOARDING_ID, OPEN_TERMINAL_HERE_ID, OPERATION_LOG_ID, PIN_TAB_MENU_ID,
    PREV_TAB_ID, QUEUE_SHOW_ID, QUICK_LOOK_ID, RENAME_ID, REOPEN_CLOSED_TAB_ID, SEARCH_FILES_ID, SELECT_ALL_ID,
    SELECT_FILES_ID, SELECT_MENU_ID, SERVERS_CONNECT_ID, SERVERS_MENU_ID, SERVERS_SHOW_ID, SETTINGS_ID,
    SHOW_HIDDEN_FILES_ID, SHOW_IN_FINDER_ID, SORT_ASCENDING_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID,
    SORT_BY_MENU_ID, SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID, SUGGESTED_OPS_ID,
    SWAP_PANES_ID, SWITCH_PANE_ID, TAB_MENU_ID, VIEW_MENU_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID,
    VIEW_MODE_FULL_LEFT_ID, VIEW_MODE_FULL_RIGHT_ID, VIEW_ZOOM_75_ID, VIEW_ZOOM_100_ID, VIEW_ZOOM_125_ID,
    VIEW_ZOOM_150_ID, VIEW_ZOOM_IN_ID, VIEW_ZOOM_OUT_ID, ViewMode, WINDOW_MENU_ID,
};

/// "Copy path", matching Finder's "Copy as Pathname". Must stay in sync with the `file.copyPath`
/// default in `src/lib/commands/sources/file-list.ts` (`⌘⌥C`), which is what the frontend
/// dispatcher listens for. The file context menu uses it too.
pub(crate) const COPY_PATH_ACCELERATOR: Accelerator = split("Cmd+Opt+C", "Ctrl+Alt+C");

/// "Show in Finder" / "Show in file manager", here and in the file context menu.
///
/// Two catalog keys rather than one with a platform token: "Finder" is Apple's app name and stays
/// English everywhere, while the Linux wording names a generic kind of program, so the two don't
/// have the same shape in every language.
pub(crate) const SHOW_IN_FILE_MANAGER_KEY: PerPlatform<&str> = PerPlatform {
    macos: "menu.file.showInFinder",
    linux: "menu.file.showInFileManager",
};
pub(crate) const SHOW_IN_FILE_MANAGER_ACCELERATOR: Accelerator = split("Opt+Cmd+O", "Alt+Ctrl+O");

// The items the macOS app menu holds, which Linux spreads over Edit and Help.
const ABOUT: Entry = item(ABOUT_ID, "menu.app.about", NONE);
/// Credits the open-source libraries Cmdr ships: app metadata, not a help topic, so on macOS it sits
/// next to About and the license, where macOS apps that ship one put it.
const ACKNOWLEDGEMENTS: Entry = item(ACKNOWLEDGEMENTS_ID, "menu.app.acknowledgements", NONE).untracked();
const LICENSE: Entry = labeled_item(ENTER_LICENSE_KEY_ID, Label::License, NONE).untracked();
const CHECK_FOR_UPDATES: Entry = item(CHECK_FOR_UPDATES_ID, "menu.app.checkForUpdates", NONE);
/// Opens the What's new popup with the latest releases, the same command as Help > What's new.
const CHANGELOG: Entry = item(CHANGELOG_ID, "menu.app.changelog", NONE);
const SETTINGS: Entry = item(SETTINGS_ID, "menu.app.settings", both("Cmd+,"));

pub(crate) const MENU_BAR: &[BarMenu] = &[
    macos_menu(
        APP_MENU_ID,
        Label::AppName,
        &[
            // Untracked here, tracked on Linux (under Help and Edit), so only Linux syncs a custom
            // shortcut onto About and Settings.
            ABOUT.untracked(),
            ACKNOWLEDGEMENTS,
            LICENSE,
            CHECK_FOR_UPDATES,
            CHANGELOG,
            // Re-entry to the onboarding wizard. Linux has no menu entry (palette only) by design:
            // `lib/onboarding/CLAUDE.md` § "Re-entry points".
            item(OPEN_ONBOARDING_ID, "menu.app.onboarding", NONE),
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
            SEPARATOR,
            labeled_item(
                SHOW_IN_FINDER_ID,
                Label::PerPlatformKey(SHOW_IN_FILE_MANAGER_KEY),
                SHOW_IN_FILE_MANAGER_ACCELERATOR,
            ),
            // Next to Show in Finder: the same gesture aimed at another app. macOS only, like the
            // launch module behind it. Built enabled; `set_open_terminal_here_enabled` greys it out
            // while the focused pane sits somewhere a shell can't `cd` into.
            macos_only(item(
                OPEN_TERMINAL_HERE_ID,
                "menu.file.openTerminalHere",
                macos("Alt+Cmd+T"),
            )),
            item(GET_INFO_ID, "menu.file.getInfo", both("Cmd+I")),
            // Shift+Space, not plain Space: AppKit consumes modifier accelerators before the webview
            // can, so the menu fires. Plain Space was dead, eaten by the webview's selection toggle
            // before AppKit's menu dispatcher saw it.
            item(QUICK_LOOK_ID, "menu.file.quickLook", macos("Shift+Space")),
        ],
    ),
    menu(
        EDIT_MENU_ID,
        "menu.bar.edit",
        &[
            macos_only(predefined(Predefined::Undo, "menu.edit.undo")),
            macos_only(predefined(Predefined::Redo, "menu.edit.redo")),
            macos_only(SEPARATOR),
            // Custom items rather than predefined ones, so ⌘X / ⌘C / ⌘V route through
            // `execute-command` and the frontend picks the text clipboard (an input has focus) or
            // the file clipboard. In other windows, `handle_menu_event` forwards the native selector.
            item(EDIT_CUT_ID, "menu.edit.cut", split("Cmd+X", "Ctrl+X")),
            item(EDIT_COPY_ID, "menu.edit.copy", split("Cmd+C", "Ctrl+C")),
            item(EDIT_PASTE_ID, "menu.edit.paste", split("Cmd+V", "Ctrl+V")),
            item(
                EDIT_PASTE_MOVE_ID,
                "menu.edit.moveHere",
                split("Alt+Cmd+V", "Ctrl+Alt+V"),
            ),
            SEPARATOR,
            item(COPY_PATH_ID, "menu.edit.copyPath", COPY_PATH_ACCELERATOR),
            item(COPY_FILENAME_ID, "menu.edit.copyFilename", NONE),
            SEPARATOR,
            item(SEARCH_FILES_ID, "menu.edit.searchFiles", both("Cmd+F")),
            linux_only(SEPARATOR),
            linux_only(SETTINGS),
            linux_only(LICENSE),
            linux_only(CHECK_FOR_UPDATES),
            linux_only(CHANGELOG),
        ],
    ),
    // Between Edit and View. The two dialog openers carry no accelerator: a macOS menu accelerator
    // always carries a modifier, so FilePane's keydown handler binds their bare `+` / `-`.
    menu(
        SELECT_MENU_ID,
        "menu.bar.select",
        &[
            item(SELECT_ALL_ID, "menu.select.all", both("Cmd+A")),
            item(DESELECT_ALL_ID, "menu.select.deselectAll", both("Cmd+Shift+A")),
            // No accelerator: `⇧8` carries no Cmd, and a bare one would swallow `*` in every text
            // field. FilePane's keydown handler binds it.
            item(INVERT_SELECTION_ID, "menu.select.invert", NONE),
            SEPARATOR,
            item(SELECT_FILES_ID, "menu.select.files", NONE),
            item(DESELECT_FILES_ID, "menu.select.deselectFiles", NONE),
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
                        both("Cmd+1"),
                        CheckRole::ViewMode(Pane::Left, ViewMode::Full),
                    ),
                    check(
                        VIEW_MODE_BRIEF_LEFT_ID,
                        "menu.view.briefView",
                        both("Cmd+2"),
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
                both("Cmd+Shift+."),
                CheckRole::ShowHiddenFiles,
            ),
            // GTK intercepts F-row keys but lets Cmd+digit chords through, so both platforms
            // register these; the ⌘F3–⌘F6 alternates go through JS dispatch alone. Date created and
            // the two order items have no shortcut to sync, so nothing tracks them.
            submenu(
                Some(SORT_BY_MENU_ID),
                "menu.view.sortBy",
                SubmenuRole::SortBy,
                &[
                    item(SORT_BY_NAME_ID, "menu.sort.name", both("Cmd+3")),
                    item(SORT_BY_EXTENSION_ID, "menu.sort.extension", both("Cmd+4")),
                    item(SORT_BY_MODIFIED_ID, "menu.sort.dateModified", both("Cmd+5")),
                    item(SORT_BY_SIZE_ID, "menu.sort.size", both("Cmd+6")),
                    item(SORT_BY_CREATED_ID, "menu.sort.dateCreated", NONE).untracked(),
                    SEPARATOR,
                    item(SORT_ASCENDING_ID, "menu.sort.ascending", NONE).untracked(),
                    item(SORT_DESCENDING_ID, "menu.sort.descending", NONE).untracked(),
                ],
            ),
            // Each preset writes `appearance.textSize` through the command-execute event, and Zoom
            // in / out move it by 10 percentage points. GTK intercepts `Cmd+Plus` / `Cmd+Minus`, so
            // Linux leaves those two to JS dispatch.
            submenu(
                None,
                "menu.view.zoom",
                SubmenuRole::Plain,
                &[
                    item(VIEW_ZOOM_75_ID, "menu.zoom.percent75", NONE).untracked(),
                    item(VIEW_ZOOM_100_ID, "menu.zoom.percent100", both("Cmd+0")).untracked(),
                    item(VIEW_ZOOM_125_ID, "menu.zoom.percent125", NONE).untracked(),
                    item(VIEW_ZOOM_150_ID, "menu.zoom.percent150", NONE).untracked(),
                    SEPARATOR,
                    item(VIEW_ZOOM_IN_ID, "menu.zoom.in", macos("Cmd+Plus")).untracked(),
                    item(VIEW_ZOOM_OUT_ID, "menu.zoom.out", macos("Cmd+Minus")).untracked(),
                ],
            ),
            SEPARATOR,
            // Tab conflicts with GTK's own keyboard navigation, so Linux leaves it to JS dispatch.
            item(SWITCH_PANE_ID, "menu.view.switchPane", macos("Tab")),
            item(SWAP_PANES_ID, "menu.view.swapPanes", both("Cmd+U")),
            SEPARATOR,
            item(COMMAND_PALETTE_ID, "menu.view.commandPalette", both("Cmd+Shift+P")),
            // The next four sync their accelerators from registry shortcuts (`queue.show`,
            // `log.operationLog`, `suggestedOps.show`, `askCmdr.toggle`), so these are the defaults.
            // The queue sits next to the log so the present-tense and past-tense views of the same
            // work read as a pair.
            item(QUEUE_SHOW_ID, "menu.view.operationQueue", both("Cmd+Alt+Q")),
            // ⌥⌘O, the first choice, is taken by Show in Finder.
            item(OPERATION_LOG_ID, "menu.view.operationLog", both("Cmd+Alt+L")),
            // No default: the status-corner indicator is the everyday way in, and a suggestion waits
            // indefinitely, so this isn't a key anyone reaches for mid-task. A user who wants one
            // binds it.
            item(SUGGESTED_OPS_ID, "menu.view.suggestedOps", NONE),
            item(ASK_CMDR_ID, "menu.view.askCmdr", both("Cmd+Alt+A")),
        ],
    ),
    menu(
        GO_MENU_ID,
        "menu.bar.go",
        &[
            item(GO_BACK_ID, "menu.go.back", both("Cmd+[")),
            item(GO_FORWARD_ID, "menu.go.forward", both("Cmd+]")),
            SEPARATOR,
            item(GO_PARENT_ID, "menu.go.parentFolder", both("Cmd+Up")),
            // Shift+Cmd+H, not Cmd+H: AppKit owns Cmd+H for Hide Cmdr and swallows it before the
            // webview ever sees a keydown.
            item(GO_HOME_ID, "menu.go.home", both("Shift+Cmd+H")),
            SEPARATOR,
            item(GO_TO_PATH_ID, "menu.go.goToPath", both("Cmd+G")),
            item(GO_LATEST_DOWNLOAD_ID, "menu.go.goToLatestDownload", both("Cmd+J")),
            SEPARATOR,
            // No default: `favorites.add` ships without a shortcut, and the sync picks up whatever
            // the user binds in Settings > Keyboard shortcuts.
            item(FAVORITES_ADD_ID, "menu.go.addToFavorites", NONE),
        ],
    ),
    menu(
        SERVERS_MENU_ID,
        "menu.bar.servers",
        &[
            item(SERVERS_CONNECT_ID, "menu.servers.connectToServer", both("Cmd+K")),
            // No default: `servers.show` ships without a shortcut.
            item(SERVERS_SHOW_ID, "menu.servers.showServers", NONE),
        ],
    ),
    menu(
        TAB_MENU_ID,
        "menu.bar.tab",
        &[
            item(NEW_TAB_ID, "menu.tab.newTab", both("Cmd+T")),
            item(CLOSE_TAB_ID, "menu.tab.closeTab", both("Cmd+W")),
            // Starts disabled; `set_reopen_closed_tab_enabled` turns it on after the first close.
            item(REOPEN_CLOSED_TAB_ID, "menu.tab.reopenClosedTab", both("Cmd+Shift+T")).disabled(),
            SEPARATOR,
            item(NEXT_TAB_ID, "menu.tab.nextTab", both("Ctrl+Tab")),
            item(PREV_TAB_ID, "menu.tab.previousTab", both("Ctrl+Shift+Tab")),
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
            linux_only(ABOUT),
            linux_only(ACKNOWLEDGEMENTS),
            linux_only(SEPARATOR),
            item(HELP_SHORTCUTS_ID, "menu.help.keyboardShortcuts", NONE),
            macos_only(SEPARATOR),
            item(HELP_WHATS_NEW_ID, "menu.help.whatsNew", NONE),
            item(HELP_SEND_FEEDBACK_ID, "menu.help.sendFeedback", NONE),
            item(HELP_SEND_ERROR_REPORT_ID, "menu.help.sendErrorReport", NONE),
        ],
    ),
];

#[cfg(test)]
#[path = "menu_bar_test.rs"]
mod menu_bar_test;
