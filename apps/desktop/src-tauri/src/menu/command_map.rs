//! Menu ID ↔ command-registry bridge.
//!
//! Holds every `*_ID` menu item string constant and the two exhaustive mapping functions between
//! menu item IDs and command-registry IDs (`menu_id_to_command` and `command_id_to_menu_id`). These
//! are re-exported from `mod.rs` (`pub use command_map::*;`), so every existing `crate::menu::…` /
//! `super::…` import path stays valid. Both maps are kept in sync manually; the
//! `rust-command-id-drift.test.ts` Vitest test parses `menu_id_to_command` as the drift backstop.

use super::CommandScope;

/// Menu item IDs for file actions.
pub const SHOW_HIDDEN_FILES_ID: &str = "show_hidden_files";
/// View mode CheckMenuItems, one pair per pane, nested under per-pane submenus
/// (View > Left pane > Full view / Brief view, and the same for Right pane).
/// The keyboard shortcut (⌘1/⌘2 by default) only attaches to the items belonging
/// to the active pane, and "moves" to the other pane on focus change.
/// See `rebuild_view_mode_items`.
pub const VIEW_MODE_FULL_LEFT_ID: &str = "view_mode_full_left";
pub const VIEW_MODE_BRIEF_LEFT_ID: &str = "view_mode_brief_left";
pub const VIEW_MODE_FULL_RIGHT_ID: &str = "view_mode_full_right";
pub const VIEW_MODE_BRIEF_RIGHT_ID: &str = "view_mode_brief_right";

/// Zoom (text-size) submenu under View. Each preset writes
/// `appearance.textSize`; in/out adjust by 25 percentage points.
pub const VIEW_ZOOM_75_ID: &str = "view_zoom_75";
pub const VIEW_ZOOM_100_ID: &str = "view_zoom_100";
pub const VIEW_ZOOM_125_ID: &str = "view_zoom_125";
pub const VIEW_ZOOM_150_ID: &str = "view_zoom_150";
pub const VIEW_ZOOM_IN_ID: &str = "view_zoom_in";
pub const VIEW_ZOOM_OUT_ID: &str = "view_zoom_out";
pub const OPEN_ID: &str = "open";
pub const EDIT_ID: &str = "edit";
pub const FILE_VIEW_ID: &str = "file_view";
pub const FILE_COPY_ID: &str = "file_copy";
pub const FILE_MOVE_ID: &str = "file_move";
pub const FILE_COMPRESS_ID: &str = "file_compress";
pub const FILE_NEW_FOLDER_ID: &str = "file_new_folder";
pub const FILE_DELETE_ID: &str = "file_delete";
pub const FILE_DELETE_PERMANENTLY_ID: &str = "file_delete_permanently";
pub const SHOW_IN_FINDER_ID: &str = "show_in_finder";
pub const COPY_PATH_ID: &str = "copy_path";
pub const COPY_CURRENT_DIR_PATH_ID: &str = "copy_current_dir_path";
pub const COPY_FILENAME_ID: &str = "copy_filename";
pub const GET_INFO_ID: &str = "get_info";
pub const QUICK_LOOK_ID: &str = "quick_look";
pub const RENAME_ID: &str = "rename";
pub const SELECT_ALL_ID: &str = "select_all_files";
pub const DESELECT_ALL_ID: &str = "deselect_all";
pub const SELECT_FILES_ID: &str = "select_files";
pub const DESELECT_FILES_ID: &str = "deselect_files";
pub const TOGGLE_SELECTION_ID: &str = "toggle_selection";

/// Menu item IDs for cloud actions (macOS File Provider).
pub const CLOUD_MAKE_OFFLINE_ID: &str = "cloud_make_offline";
pub const CLOUD_REMOVE_DOWNLOAD_ID: &str = "cloud_remove_download";

/// Menu item ID prefix for the seven Finder-tag color items in the file context menu
/// (macOS). Followed by the color index (1..=7), e.g. `tag-color:6`. Prefix-routed in
/// `handle_menu_event` (like `open-with:`) straight to the tag write, NOT through
/// `menu_id_to_command` — the click acts on the right-clicked selection in
/// `MenuState.context`, not the focused-pane selection a command would use.
#[cfg(target_os = "macos")]
pub const TAG_COLOR_ID_PREFIX: &str = "tag-color:";

/// Menu item IDs for clipboard operations (Edit menu).
pub const EDIT_CUT_ID: &str = "edit_cut";
pub const EDIT_COPY_ID: &str = "edit_copy";
pub const EDIT_PASTE_ID: &str = "edit_paste";
pub const EDIT_PASTE_MOVE_ID: &str = "edit_paste_move";

/// Menu item ID for command palette.
pub const COMMAND_PALETTE_ID: &str = "command_palette";

/// Menu item ID for Search files.
pub const SEARCH_FILES_ID: &str = "search_files";

/// Menu item ID for Switch Pane.
pub const SWITCH_PANE_ID: &str = "switch_pane";

/// Menu item ID for Swap Panes.
pub const SWAP_PANES_ID: &str = "swap_panes";

/// Menu item IDs for navigation (Go menu).
pub const GO_BACK_ID: &str = "go_back";
pub const GO_FORWARD_ID: &str = "go_forward";
pub const GO_PARENT_ID: &str = "go_parent";
/// "Go to path…" (⌘G): opens the Go-to-path dialog (dialog-open is idempotency-guarded).
pub const GO_TO_PATH_ID: &str = "go_to_path";
/// "Go to latest download" (⌘J): jumps the focused pane to the most recent download.
pub const GO_LATEST_DOWNLOAD_ID: &str = "go_latest_download";

/// "Add to favorites", menu bar + palette: maps to the `favorites.add` command, which favorites the
/// focused pane's current folder. Ships with NO default shortcut (adding a favorite is infrequent);
/// the menu item's accelerator stays empty until the user binds one in Settings > Keyboard shortcuts,
/// at which point the accelerator-sync pass fills it in.
pub const FAVORITES_ADD_ID: &str = "favorites_add";

/// "Add to favorites", folder-row + parent-row CONTEXT menus: favorites `MenuState.context.path`
/// directly in `on_menu_event` (the right-clicked folder, or the parent dir for `..`). A separate id
/// from `FAVORITES_ADD_ID` so the menu-bar item (focused-pane dir) and the context item
/// (right-clicked path) can't be confused; intercepted before the unified `menu_id_to_command`
/// lookup, so it never routes through a command.
pub const FAVORITES_ADD_CONTEXT_ID: &str = "favorites_add_context";

/// Menu item IDs for sorting.
pub const SORT_BY_NAME_ID: &str = "sort_by_name";
pub const SORT_BY_EXTENSION_ID: &str = "sort_by_extension";
pub const SORT_BY_SIZE_ID: &str = "sort_by_size";
pub const SORT_BY_MODIFIED_ID: &str = "sort_by_modified";
pub const SORT_BY_CREATED_ID: &str = "sort_by_created";
pub const SORT_ASCENDING_ID: &str = "sort_ascending";
pub const SORT_DESCENDING_ID: &str = "sort_descending";

/// Menu item ID for viewer word wrap toggle.
pub const VIEWER_WORD_WRAP_ID: &str = "viewer_word_wrap";

/// Menu item IDs for tab actions (app menu).
pub const NEW_TAB_ID: &str = "new_tab";
pub const PIN_TAB_MENU_ID: &str = "pin_tab_menu";
pub const CLOSE_TAB_ID: &str = "close_tab";
pub const REOPEN_CLOSED_TAB_ID: &str = "reopen_closed_tab";
pub const NEXT_TAB_ID: &str = "next_tab";
pub const PREV_TAB_ID: &str = "prev_tab";
pub const CLOSE_OTHER_TABS_ID: &str = "close_other_tabs";

/// Menu item IDs for tab context menu.
pub const TAB_PIN_ID: &str = "tab_pin";
pub const TAB_CLOSE_OTHERS_ID: &str = "tab_close_others";
pub const TAB_CLOSE_ID: &str = "tab_close";

/// Menu item IDs for network host context menu.
pub const NETWORK_HOST_FORGET_SERVER_ID: &str = "network_host_forget_server";
pub const NETWORK_HOST_FORGET_PASSWORD_ID: &str = "network_host_forget_password";
pub const NETWORK_HOST_DISCONNECT_ID: &str = "network_host_disconnect";

/// Menu item ID for "Eject (name)" in the breadcrumb / volume context menus.
pub const EJECT_VOLUME_ID: &str = "eject_volume";

/// Menu item IDs for the favorite row context menu (volume selector dropdown).
pub const FAVORITE_RENAME_ID: &str = "favorite_rename";
pub const FAVORITE_REMOVE_ID: &str = "favorite_remove";

/// Menu item ID for About window.
pub const ABOUT_ID: &str = "about";

/// Menu item ID for Enter License Key.
pub const ENTER_LICENSE_KEY_ID: &str = "enter_license_key";

/// Menu item ID for Settings.
pub const SETTINGS_ID: &str = "settings";

/// Menu item ID for "Send error report…" (under the Help menu).
pub const HELP_SEND_ERROR_REPORT_ID: &str = "help_send_error_report";

/// Menu item ID for "Send feedback…" (under the Help menu).
pub const HELP_SEND_FEEDBACK_ID: &str = "help_send_feedback";

/// Menu item ID for "Keyboard shortcuts" (opens the read-only shortcuts help window, under the Help menu).
pub const HELP_SHORTCUTS_ID: &str = "help_shortcuts";

/// Menu item ID for "Show transfer queue" (opens the transfer-queue window, under the Help menu).
pub const QUEUE_SHOW_ID: &str = "queue_show";

/// Menu item ID for "What's new" (opens the changelog popup, under the Help menu).
pub const HELP_WHATS_NEW_ID: &str = "help_whats_new";

/// Menu item ID for "Operation log" (opens the alpha operation-log dialog, under the View menu).
pub const OPERATION_LOG_ID: &str = "operation_log";

/// Menu item ID for "Ask Cmdr" (toggles the alpha chat rail, under the View menu).
pub const ASK_CMDR_ID: &str = "ask_cmdr";

/// Menu item ID for "Check for updates…" (under the Cmdr / Help menu).
pub const CHECK_FOR_UPDATES_ID: &str = "check_for_updates";

/// Menu item ID for "Changelog…" (under the Cmdr app menu, below "Check for updates…").
/// A second entry point to the "What's new" popup: it maps to the same `help.whatsNew`
/// command as the Help-menu "What's new" item, so both open the latest-five changelog slice.
pub const CHANGELOG_ID: &str = "changelog";

/// Menu item ID for "Onboarding…" (under the Cmdr app menu, macOS only).
/// Linux re-entry to the onboarding wizard is palette-only by design (see
/// `apps/desktop/src/lib/onboarding/CLAUDE.md` § "Re-entry points").
pub const OPEN_ONBOARDING_ID: &str = "open_onboarding";

/// Maps a menu item ID to its command registry ID and scope.
/// Returns `None` for items handled specially (CheckMenuItems, close-tab, viewer word wrap,
/// tab context menu, context menu file actions, sort items).
///
/// Each `Some(("…", …))` command id here is emitted across IPC and must exist in the
/// frontend `COMMAND_IDS` tuple (`src/lib/commands/command-ids.ts`). The
/// `rust-command-id-drift.test.ts` Vitest test parses this function and asserts that
/// — the IPC boundary is un-typed, so that test is the backstop for drift.
pub fn menu_id_to_command(menu_id: &str) -> Option<(&'static str, CommandScope)> {
    match menu_id {
        // App-level commands (always emit)
        ABOUT_ID => Some(("app.about", CommandScope::App)),
        ENTER_LICENSE_KEY_ID => Some(("app.licenseKey", CommandScope::App)),
        SETTINGS_ID => Some(("app.settings", CommandScope::App)),
        COMMAND_PALETTE_ID => Some(("app.commandPalette", CommandScope::FileScoped)),
        SEARCH_FILES_ID => Some(("search.open", CommandScope::FileScoped)),
        HELP_SHORTCUTS_ID => Some(("help.openShortcuts", CommandScope::App)),
        QUEUE_SHOW_ID => Some(("queue.show", CommandScope::App)),
        HELP_WHATS_NEW_ID => Some(("help.whatsNew", CommandScope::App)),
        // Second entry point (Cmdr menu) to the same "What's new" popup. Deliberately maps
        // to `help.whatsNew`; the reverse `command_id_to_menu_id` keeps pointing at the Help
        // item, which is fine (neither carries a default shortcut).
        CHANGELOG_ID => Some(("help.whatsNew", CommandScope::App)),
        OPERATION_LOG_ID => Some(("log.operationLog", CommandScope::App)),
        ASK_CMDR_ID => Some(("askCmdr.toggle", CommandScope::App)),
        HELP_SEND_ERROR_REPORT_ID => Some(("help.sendErrorReport", CommandScope::App)),
        HELP_SEND_FEEDBACK_ID => Some(("feedback.send", CommandScope::App)),
        CHECK_FOR_UPDATES_ID => Some(("app.checkForUpdates", CommandScope::App)),
        OPEN_ONBOARDING_ID => Some(("cmdr.openOnboarding", CommandScope::App)),

        // Pane commands (file-scoped)
        SWITCH_PANE_ID => Some(("pane.switch", CommandScope::FileScoped)),
        SWAP_PANES_ID => Some(("pane.swap", CommandScope::FileScoped)),

        // Navigation commands (file-scoped)
        GO_BACK_ID => Some(("nav.back", CommandScope::FileScoped)),
        GO_FORWARD_ID => Some(("nav.forward", CommandScope::FileScoped)),
        GO_PARENT_ID => Some(("nav.parent", CommandScope::FileScoped)),
        GO_TO_PATH_ID => Some(("nav.goToPath", CommandScope::FileScoped)),
        GO_LATEST_DOWNLOAD_ID => Some(("downloads.goToLatest", CommandScope::FileScoped)),
        FAVORITES_ADD_ID => Some(("favorites.add", CommandScope::FileScoped)),

        // Tab commands (file-scoped)
        NEW_TAB_ID => Some(("tab.new", CommandScope::FileScoped)),
        CLOSE_TAB_ID => Some(("tab.close", CommandScope::FileScoped)),
        REOPEN_CLOSED_TAB_ID => Some(("tab.reopen", CommandScope::FileScoped)),
        NEXT_TAB_ID => Some(("tab.next", CommandScope::FileScoped)),
        PREV_TAB_ID => Some(("tab.prev", CommandScope::FileScoped)),
        PIN_TAB_MENU_ID => Some(("tab.togglePin", CommandScope::FileScoped)),
        CLOSE_OTHER_TABS_ID => Some(("tab.closeOthers", CommandScope::FileScoped)),

        // Edit actions: cut/copy/paste (and select_all_files below) are handled specially in
        // on_menu_event (native responder chain for non-main windows, execute-command for the
        // main window). They're still listed here for command_id_to_menu_id reverse lookups.
        EDIT_CUT_ID => Some(("edit.cut", CommandScope::App)),
        EDIT_COPY_ID => Some(("edit.copy", CommandScope::App)),
        EDIT_PASTE_ID => Some(("edit.paste", CommandScope::App)),
        EDIT_PASTE_MOVE_ID => Some(("edit.pasteAsMove", CommandScope::FileScoped)),

        // File operations (file-scoped)
        OPEN_ID => Some(("nav.open", CommandScope::FileScoped)),
        RENAME_ID => Some(("file.rename", CommandScope::FileScoped)),
        EDIT_ID => Some(("file.edit", CommandScope::FileScoped)),
        FILE_VIEW_ID => Some(("file.view", CommandScope::FileScoped)),
        FILE_COPY_ID => Some(("file.copy", CommandScope::FileScoped)),
        FILE_MOVE_ID => Some(("file.move", CommandScope::FileScoped)),
        FILE_COMPRESS_ID => Some(("file.compress", CommandScope::FileScoped)),
        FILE_NEW_FOLDER_ID => Some(("file.newFolder", CommandScope::FileScoped)),
        FILE_DELETE_ID => Some(("file.delete", CommandScope::FileScoped)),
        FILE_DELETE_PERMANENTLY_ID => Some(("file.deletePermanently", CommandScope::FileScoped)),
        SHOW_IN_FINDER_ID => Some(("file.showInFinder", CommandScope::FileScoped)),
        COPY_PATH_ID => Some(("file.copyPath", CommandScope::FileScoped)),
        COPY_CURRENT_DIR_PATH_ID => Some(("file.copyCurrentDirectoryPath", CommandScope::FileScoped)),
        COPY_FILENAME_ID => Some(("file.copyFilename", CommandScope::FileScoped)),
        GET_INFO_ID => Some(("file.getInfo", CommandScope::FileScoped)),
        QUICK_LOOK_ID => Some(("file.quickLook", CommandScope::FileScoped)),
        // Intercepted by on_menu_event before this lookup (like cut/copy/paste): main window →
        // execute-command, non-main → native selectAll: so ⌘A still works in text fields there.
        SELECT_ALL_ID => Some(("selection.selectAll", CommandScope::FileScoped)),
        DESELECT_ALL_ID => Some(("selection.deselectAll", CommandScope::FileScoped)),
        SELECT_FILES_ID => Some(("selection.selectFiles", CommandScope::FileScoped)),
        DESELECT_FILES_ID => Some(("selection.deselectFiles", CommandScope::FileScoped)),
        TOGGLE_SELECTION_ID => Some(("selection.toggle", CommandScope::FileScoped)),

        // Cloud actions (macOS File Provider)
        CLOUD_MAKE_OFFLINE_ID => Some(("cloud.makeOffline", CommandScope::FileScoped)),
        CLOUD_REMOVE_DOWNLOAD_ID => Some(("cloud.removeDownload", CommandScope::FileScoped)),

        // Zoom (text size): App scope so ⌘0/⌘+/⌘- work in any focused window.
        VIEW_ZOOM_75_ID => Some(("view.zoom.set75", CommandScope::App)),
        VIEW_ZOOM_100_ID => Some(("view.zoom.set100", CommandScope::App)),
        VIEW_ZOOM_125_ID => Some(("view.zoom.set125", CommandScope::App)),
        VIEW_ZOOM_150_ID => Some(("view.zoom.set150", CommandScope::App)),
        VIEW_ZOOM_IN_ID => Some(("view.zoom.in", CommandScope::App)),
        VIEW_ZOOM_OUT_ID => Some(("view.zoom.out", CommandScope::App)),

        // Sort items: mapped so user-customized accelerators can flow into the menu via the
        // generic update path. At runtime, `on_menu_event` intercepts these IDs *before* this
        // lookup and emits `menu-sort` instead of `execute-command`, so this mapping never
        // routes a click. It exists purely as the source of truth for the reverse map.
        SORT_BY_NAME_ID => Some(("sort.byName", CommandScope::FileScoped)),
        SORT_BY_EXTENSION_ID => Some(("sort.byExtension", CommandScope::FileScoped)),
        SORT_BY_MODIFIED_ID => Some(("sort.byModified", CommandScope::FileScoped)),
        SORT_BY_SIZE_ID => Some(("sort.bySize", CommandScope::FileScoped)),

        // Not mapped: CheckMenuItems (show_hidden_files, view modes), close-tab (special logic),
        // viewer word wrap, tab context menu actions, sort order items (ascending/descending/
        // date-created), "open-with:*" (prefix-routed before this lookup in
        // `lib.rs::on_menu_event`).
        _ => None,
    }
}

/// Maps a command registry ID to its menu item ID.
/// Returns `None` for commands that don't have menu items, or that use CheckMenuItems
/// (view modes) which have their own specific update path.
pub fn command_id_to_menu_id(command_id: &str) -> Option<&'static str> {
    match command_id {
        "app.about" => Some(ABOUT_ID),
        "app.licenseKey" => Some(ENTER_LICENSE_KEY_ID),
        "app.settings" => Some(SETTINGS_ID),
        "app.commandPalette" => Some(COMMAND_PALETTE_ID),
        "search.open" => Some(SEARCH_FILES_ID),
        "help.openShortcuts" => Some(HELP_SHORTCUTS_ID),
        "queue.show" => Some(QUEUE_SHOW_ID),
        "help.whatsNew" => Some(HELP_WHATS_NEW_ID),
        "log.operationLog" => Some(OPERATION_LOG_ID),
        "askCmdr.toggle" => Some(ASK_CMDR_ID),
        "help.sendErrorReport" => Some(HELP_SEND_ERROR_REPORT_ID),
        "feedback.send" => Some(HELP_SEND_FEEDBACK_ID),
        "app.checkForUpdates" => Some(CHECK_FOR_UPDATES_ID),
        "cmdr.openOnboarding" => Some(OPEN_ONBOARDING_ID),
        "pane.switch" => Some(SWITCH_PANE_ID),
        "pane.swap" => Some(SWAP_PANES_ID),
        "nav.back" => Some(GO_BACK_ID),
        "nav.forward" => Some(GO_FORWARD_ID),
        "nav.parent" => Some(GO_PARENT_ID),
        "nav.goToPath" => Some(GO_TO_PATH_ID),
        "downloads.goToLatest" => Some(GO_LATEST_DOWNLOAD_ID),
        "favorites.add" => Some(FAVORITES_ADD_ID),
        "tab.new" => Some(NEW_TAB_ID),
        "tab.close" => Some(CLOSE_TAB_ID),
        "tab.reopen" => Some(REOPEN_CLOSED_TAB_ID),
        "tab.next" => Some(NEXT_TAB_ID),
        "tab.prev" => Some(PREV_TAB_ID),
        "tab.togglePin" => Some(PIN_TAB_MENU_ID),
        "tab.closeOthers" => Some(CLOSE_OTHER_TABS_ID),
        "file.rename" => Some(RENAME_ID),
        "file.edit" => Some(EDIT_ID),
        "file.view" => Some(FILE_VIEW_ID),
        "file.copy" => Some(FILE_COPY_ID),
        "file.move" => Some(FILE_MOVE_ID),
        "file.compress" => Some(FILE_COMPRESS_ID),
        "file.newFolder" => Some(FILE_NEW_FOLDER_ID),
        "file.delete" => Some(FILE_DELETE_ID),
        "file.deletePermanently" => Some(FILE_DELETE_PERMANENTLY_ID),
        "file.showInFinder" => Some(SHOW_IN_FINDER_ID),
        "file.copyPath" => Some(COPY_PATH_ID),
        "file.copyFilename" => Some(COPY_FILENAME_ID),
        "file.getInfo" => Some(GET_INFO_ID),
        "file.quickLook" => Some(QUICK_LOOK_ID),
        "selection.selectAll" => Some(SELECT_ALL_ID),
        "selection.deselectAll" => Some(DESELECT_ALL_ID),
        "selection.selectFiles" => Some(SELECT_FILES_ID),
        "selection.deselectFiles" => Some(DESELECT_FILES_ID),
        "selection.toggle" => Some(TOGGLE_SELECTION_ID),
        "view.zoom.set75" => Some(VIEW_ZOOM_75_ID),
        "view.zoom.set100" => Some(VIEW_ZOOM_100_ID),
        "view.zoom.set125" => Some(VIEW_ZOOM_125_ID),
        "view.zoom.set150" => Some(VIEW_ZOOM_150_ID),
        "view.zoom.in" => Some(VIEW_ZOOM_IN_ID),
        "view.zoom.out" => Some(VIEW_ZOOM_OUT_ID),
        "edit.cut" => Some(EDIT_CUT_ID),
        "edit.copy" => Some(EDIT_COPY_ID),
        "edit.paste" => Some(EDIT_PASTE_ID),
        "edit.pasteAsMove" => Some(EDIT_PASTE_MOVE_ID),
        "cloud.makeOffline" => Some(CLOUD_MAKE_OFFLINE_ID),
        "cloud.removeDownload" => Some(CLOUD_REMOVE_DOWNLOAD_ID),
        "sort.byName" => Some(SORT_BY_NAME_ID),
        "sort.byExtension" => Some(SORT_BY_EXTENSION_ID),
        "sort.byModified" => Some(SORT_BY_MODIFIED_ID),
        "sort.bySize" => Some(SORT_BY_SIZE_ID),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_id_to_command_app_scoped() {
        // App-level commands should always emit
        assert_eq!(menu_id_to_command(ABOUT_ID), Some(("app.about", CommandScope::App)));
        assert_eq!(
            menu_id_to_command(SETTINGS_ID),
            Some(("app.settings", CommandScope::App))
        );
        assert_eq!(
            menu_id_to_command(ENTER_LICENSE_KEY_ID),
            Some(("app.licenseKey", CommandScope::App))
        );
        // Command palette is FileScoped: disabled when Settings/viewer has focus
        assert_eq!(
            menu_id_to_command(COMMAND_PALETTE_ID),
            Some(("app.commandPalette", CommandScope::FileScoped))
        );
    }

    #[test]
    fn test_menu_id_to_command_file_scoped() {
        // File-scoped commands require main window focus
        assert_eq!(
            menu_id_to_command(SWITCH_PANE_ID),
            Some(("pane.switch", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(FILE_COPY_ID),
            Some(("file.copy", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(FILE_COMPRESS_ID),
            Some(("file.compress", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(GO_BACK_ID),
            Some(("nav.back", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(GO_TO_PATH_ID),
            Some(("nav.goToPath", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(GO_LATEST_DOWNLOAD_ID),
            Some(("downloads.goToLatest", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(NEW_TAB_ID),
            Some(("tab.new", CommandScope::FileScoped))
        );
        // Context menu items also routed through unified dispatch
        assert_eq!(
            menu_id_to_command(OPEN_ID),
            Some(("nav.open", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(EDIT_ID),
            Some(("file.edit", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(SHOW_IN_FINDER_ID),
            Some(("file.showInFinder", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(COPY_PATH_ID),
            Some(("file.copyPath", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(COPY_FILENAME_ID),
            Some(("file.copyFilename", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(GET_INFO_ID),
            Some(("file.getInfo", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(QUICK_LOOK_ID),
            Some(("file.quickLook", CommandScope::FileScoped))
        );
    }

    #[test]
    fn test_menu_id_to_command_unmapped() {
        // Items with special handling return None
        assert_eq!(menu_id_to_command(SHOW_HIDDEN_FILES_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_FULL_LEFT_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_BRIEF_LEFT_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_FULL_RIGHT_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_BRIEF_RIGHT_ID), None);
        assert_eq!(menu_id_to_command(VIEWER_WORD_WRAP_ID), None);
        // Sort order items (ascending/descending) and date-created use the menu-sort
        // event path and are not mapped. Only the four shortcut-bound columns are.
        assert_eq!(menu_id_to_command(SORT_ASCENDING_ID), None);
        assert_eq!(menu_id_to_command(SORT_DESCENDING_ID), None);
        assert_eq!(menu_id_to_command(SORT_BY_CREATED_ID), None);
        assert_eq!(menu_id_to_command("unknown_id"), None);
    }

    #[test]
    fn test_command_id_to_menu_id_roundtrip() {
        // Every entry in command_id_to_menu_id should map back correctly via menu_id_to_command
        let command_ids = [
            "app.about",
            "app.licenseKey",
            "app.settings",
            "app.commandPalette",
            "pane.switch",
            "pane.swap",
            "nav.back",
            "nav.forward",
            "nav.parent",
            "nav.goToPath",
            "downloads.goToLatest",
            "favorites.add",
            "tab.new",
            "tab.close",
            "tab.reopen",
            "tab.next",
            "tab.prev",
            "tab.togglePin",
            "tab.closeOthers",
            "search.open",
            "file.rename",
            "file.edit",
            "file.view",
            "file.copy",
            "file.move",
            "file.compress",
            "file.newFolder",
            "file.delete",
            "file.deletePermanently",
            "file.showInFinder",
            "file.copyPath",
            "file.copyFilename",
            "file.getInfo",
            "file.quickLook",
            "selection.selectAll",
            "selection.deselectAll",
            "selection.selectFiles",
            "selection.deselectFiles",
            "help.openShortcuts",
            "queue.show",
            "help.whatsNew",
            "log.operationLog",
            "help.sendErrorReport",
            "feedback.send",
            "app.checkForUpdates",
            "cmdr.openOnboarding",
            "cloud.makeOffline",
            "cloud.removeDownload",
            "sort.byName",
            "sort.byExtension",
            "sort.byModified",
            "sort.bySize",
        ];

        for command_id in &command_ids {
            let menu_id = command_id_to_menu_id(command_id);
            assert!(menu_id.is_some(), "command_id_to_menu_id missing: {command_id}");
            let (back, _scope) = menu_id_to_command(menu_id.unwrap())
                .unwrap_or_else(|| panic!("menu_id_to_command missing for menu_id from command {command_id}"));
            assert_eq!(back, *command_id, "roundtrip mismatch for {command_id}");
        }
    }

    #[test]
    fn test_command_id_to_menu_id_unmapped() {
        // Commands without menu items return None
        assert_eq!(command_id_to_menu_id("view.fullMode"), None);
        assert_eq!(command_id_to_menu_id("view.briefMode"), None);
        assert_eq!(command_id_to_menu_id("view.showHidden"), None);
        assert_eq!(command_id_to_menu_id("unknown"), None);
    }
}
