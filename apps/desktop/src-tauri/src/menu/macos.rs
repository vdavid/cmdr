//! The macOS menu bar's shape.
//!
//! Building only. The two passes that reach into AppKit afterwards live in `macos_appkit.rs`.

use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
};

use crate::intl::menu_t;

use super::menu_items::{
    APP_MENU_TITLE, MenuSlot, ViewModeItems, build_registered_submenu, build_sort_submenu, build_view_mode_items,
    build_zoom_submenu, copy_path_accelerator, register_sort_items, show_in_file_manager_accelerator,
    show_in_file_manager_label,
};
use super::{
    ABOUT_ID, ACKNOWLEDGEMENTS_ID, APP_MENU_ID, ASK_CMDR_ID, CHANGELOG_ID, CHECK_FOR_UPDATES_ID, CLOSE_OTHER_TABS_ID,
    CLOSE_TAB_ID, COMMAND_PALETTE_ID, COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID, DESELECT_FILES_ID, EDIT_COPY_ID,
    EDIT_CUT_ID, EDIT_ID, EDIT_MENU_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FAVORITES_ADD_ID,
    FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_DUPLICATE_ID, FILE_MENU_ID,
    FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GET_INFO_ID, GO_BACK_ID, GO_FORWARD_ID,
    GO_HOME_ID, GO_LATEST_DOWNLOAD_ID, GO_MENU_ID, GO_PARENT_ID, GO_TO_PATH_ID, HELP_MENU_ID,
    HELP_SEND_ERROR_REPORT_ID, HELP_SEND_FEEDBACK_ID, HELP_SHORTCUTS_ID, HELP_WHATS_NEW_ID, INVERT_SELECTION_ID,
    MenuItems, NEW_TAB_ID, NEXT_TAB_ID, OPEN_ID, OPEN_ONBOARDING_ID, OPEN_TERMINAL_HERE_ID, OPERATION_LOG_ID,
    PIN_TAB_MENU_ID, PREV_TAB_ID, QUEUE_SHOW_ID, QUICK_LOOK_ID, RENAME_ID, REOPEN_CLOSED_TAB_ID, SEARCH_FILES_ID,
    SELECT_ALL_ID, SELECT_FILES_ID, SELECT_MENU_ID, SERVERS_CONNECT_ID, SERVERS_MENU_ID, SERVERS_SHOW_ID, SETTINGS_ID,
    SHOW_HIDDEN_FILES_ID, SHOW_IN_FINDER_ID, SUGGESTED_OPS_ID, SWAP_PANES_ID, SWITCH_PANE_ID, TAB_MENU_ID,
    VIEW_MENU_ID, ViewMode, WINDOW_MENU_ID,
};

pub(crate) fn build_menu_macos<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
) -> tauri::Result<MenuItems<R>> {
    let menu = Menu::new(app)?;
    // Registrations land here as each submenu below is built, at whatever index the item actually
    // holds in that submenu's `MenuSlot` array: see `menu_items::build_registered_submenu`.
    let mut items = HashMap::new();

    // --- cmdr app menu ---
    let about_item = MenuItem::with_id(app, ABOUT_ID, menu_t("menu.app.about"), true, None::<&str>)?;
    // Credits the open-source libraries Cmdr ships. Sits next to About and the
    // license: it's app metadata, not a help topic, which is also where macOS
    // apps that ship one put it.
    let acknowledgements_item = MenuItem::with_id(
        app,
        ACKNOWLEDGEMENTS_ID,
        menu_t("menu.app.acknowledgements"),
        true,
        None::<&str>,
    )?;
    // Only one of these takes input, so only one gets the ellipsis: entering a key
    // asks for the key, seeing the details just shows them.
    let license_label = if has_existing_license {
        menu_t("menu.app.licenseDetails")
    } else {
        menu_t("menu.app.licenseEnter")
    };
    let license_item = MenuItem::with_id(app, ENTER_LICENSE_KEY_ID, license_label, true, None::<&str>)?;
    let check_for_updates_item = MenuItem::with_id(
        app,
        CHECK_FOR_UPDATES_ID,
        menu_t("menu.app.checkForUpdates"),
        true,
        None::<&str>,
    )?;
    // Opens the "What's new" popup showing the latest releases (same command as Help > What's new).
    let changelog_item = MenuItem::with_id(app, CHANGELOG_ID, menu_t("menu.app.changelog"), true, None::<&str>)?;
    // Re-entry to the onboarding wizard. Placed under "Check for updates…".
    // Linux gets no menu entry (palette-only) by design — see
    // `lib/onboarding/CLAUDE.md` § "Re-entry points".
    let open_onboarding_item = MenuItem::with_id(
        app,
        OPEN_ONBOARDING_ID,
        menu_t("menu.app.onboarding"),
        true,
        None::<&str>,
    )?;
    let settings_item = MenuItem::with_id(app, SETTINGS_ID, menu_t("menu.app.settings"), true, Some("Cmd+,"))?;

    let app_menu = build_registered_submenu(
        app,
        Some(APP_MENU_ID),
        APP_MENU_TITLE,
        &[
            MenuSlot::Plain(&about_item),
            MenuSlot::Plain(&acknowledgements_item),
            MenuSlot::Plain(&license_item),
            MenuSlot::Reg(CHECK_FOR_UPDATES_ID, &check_for_updates_item),
            MenuSlot::Reg(CHANGELOG_ID, &changelog_item),
            MenuSlot::Reg(OPEN_ONBOARDING_ID, &open_onboarding_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Plain(&settings_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            // System Services menu, populated by AppKit with Action extensions and other apps'
            // services (Ghostty's "New tab here", Nimble Commander's "Reveal", Quick Actions, etc.).
            // muda's PredefinedMenuItem::services wires `NSApplication.servicesMenu` for us.
            MenuSlot::Plain(&PredefinedMenuItem::services(app, Some(&menu_t("menu.app.services")))?),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Plain(&PredefinedMenuItem::hide(app, Some(&menu_t("menu.app.hide")))?),
            MenuSlot::Plain(&PredefinedMenuItem::hide_others(
                app,
                Some(&menu_t("menu.app.hideOthers")),
            )?),
            MenuSlot::Plain(&PredefinedMenuItem::show_all(app, Some(&menu_t("menu.app.showAll")))?),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Plain(&PredefinedMenuItem::quit(app, Some(&menu_t("menu.app.quit")))?),
        ],
        &mut items,
    )?;
    menu.append(&app_menu)?;

    // --- File menu ---
    let open_item = MenuItem::with_id(app, OPEN_ID, menu_t("menu.file.open"), true, None::<&str>)?;
    let file_view_item = MenuItem::with_id(app, FILE_VIEW_ID, menu_t("menu.file.view"), true, Some("F3"))?;
    let edit_item = MenuItem::with_id(app, EDIT_ID, menu_t("menu.file.edit"), true, Some("F4"))?;
    let file_copy_item = MenuItem::with_id(app, FILE_COPY_ID, menu_t("menu.file.copy"), true, Some("F5"))?;
    let file_move_item = MenuItem::with_id(app, FILE_MOVE_ID, menu_t("menu.file.move"), true, Some("F6"))?;
    let file_duplicate_item = MenuItem::with_id(
        app,
        FILE_DUPLICATE_ID,
        menu_t("menu.file.duplicate"),
        true,
        Some("Cmd+D"),
    )?;
    let file_compress_item = MenuItem::with_id(
        app,
        FILE_COMPRESS_ID,
        menu_t("menu.file.compress"),
        true,
        Some("Alt+F5"),
    )?;
    let file_new_folder_item =
        MenuItem::with_id(app, FILE_NEW_FOLDER_ID, menu_t("menu.file.newFolder"), true, Some("F7"))?;
    let file_new_file_item = MenuItem::with_id(
        app,
        FILE_NEW_FILE_ID,
        menu_t("menu.file.newFile"),
        true,
        Some("Shift+F4"),
    )?;
    let file_delete_item = MenuItem::with_id(app, FILE_DELETE_ID, menu_t("menu.file.delete"), true, Some("F8"))?;
    let file_delete_permanently_item = MenuItem::with_id(
        app,
        FILE_DELETE_PERMANENTLY_ID,
        menu_t("menu.file.deletePermanently"),
        true,
        Some("Shift+F8"),
    )?;
    let rename_item = MenuItem::with_id(app, RENAME_ID, menu_t("menu.file.rename"), true, Some("F2"))?;
    let show_in_finder_item = MenuItem::with_id(
        app,
        SHOW_IN_FINDER_ID,
        show_in_file_manager_label(),
        true,
        Some(show_in_file_manager_accelerator()),
    )?;
    // Sits next to Show in Finder because it's the same gesture aimed elsewhere:
    // "take me to this folder in that other app". Built enabled; the frontend greys
    // it out through `set_open_terminal_here_enabled` while the focused pane sits
    // somewhere a shell can't `cd` into.
    let open_terminal_here_item = MenuItem::with_id(
        app,
        OPEN_TERMINAL_HERE_ID,
        menu_t("menu.file.openTerminalHere"),
        true,
        Some("Alt+Cmd+T"),
    )?;
    let get_info_item = MenuItem::with_id(app, GET_INFO_ID, menu_t("menu.file.getInfo"), true, Some("Cmd+I"))?;
    // Shift+Space rather than plain Space: AppKit consumes modifier
    // accelerators before the webview can capture them, so the menu actually
    // fires. Plain Space was dead — the webview's Tier-2 selection-toggle
    // handler ate the keydown before AppKit's menu dispatcher saw it.
    let quick_look_item = MenuItem::with_id(
        app,
        QUICK_LOOK_ID,
        menu_t("menu.file.quickLook"),
        true,
        Some("Shift+Space"),
    )?;

    let file_menu = build_registered_submenu(
        app,
        Some(FILE_MENU_ID),
        &menu_t("menu.bar.file"),
        &[
            MenuSlot::Reg(OPEN_ID, &open_item),
            MenuSlot::Reg(FILE_VIEW_ID, &file_view_item),
            MenuSlot::Reg(EDIT_ID, &edit_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(FILE_COPY_ID, &file_copy_item),
            MenuSlot::Reg(FILE_MOVE_ID, &file_move_item),
            MenuSlot::Reg(FILE_DUPLICATE_ID, &file_duplicate_item),
            MenuSlot::Reg(FILE_COMPRESS_ID, &file_compress_item),
            MenuSlot::Reg(FILE_NEW_FOLDER_ID, &file_new_folder_item),
            MenuSlot::Reg(FILE_NEW_FILE_ID, &file_new_file_item),
            MenuSlot::Reg(FILE_DELETE_ID, &file_delete_item),
            MenuSlot::Reg(FILE_DELETE_PERMANENTLY_ID, &file_delete_permanently_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(RENAME_ID, &rename_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(SHOW_IN_FINDER_ID, &show_in_finder_item),
            MenuSlot::Reg(OPEN_TERMINAL_HERE_ID, &open_terminal_here_item),
            MenuSlot::Reg(GET_INFO_ID, &get_info_item),
            MenuSlot::Reg(QUICK_LOOK_ID, &quick_look_item),
        ],
        &mut items,
    )?;
    menu.append(&file_menu)?;

    // --- Edit menu ---
    // Custom MenuItems for Cut/Copy/Paste replace PredefinedMenuItems. This routes ⌘C/⌘V/⌘X
    // through execute-command dispatch so the frontend can decide between text clipboard (when
    // an input is focused) and file clipboard (when the file list has focus). Text clipboard is
    // handled via document.execCommand / navigator.clipboard API in the frontend handler.
    let edit_cut_item = MenuItem::with_id(app, EDIT_CUT_ID, menu_t("menu.edit.cut"), true, Some("Cmd+X"))?;
    let edit_copy_item = MenuItem::with_id(app, EDIT_COPY_ID, menu_t("menu.edit.copy"), true, Some("Cmd+C"))?;
    let edit_paste_item = MenuItem::with_id(app, EDIT_PASTE_ID, menu_t("menu.edit.paste"), true, Some("Cmd+V"))?;
    let edit_paste_move_item = MenuItem::with_id(
        app,
        EDIT_PASTE_MOVE_ID,
        menu_t("menu.edit.moveHere"),
        true,
        Some("Alt+Cmd+V"),
    )?;
    let copy_path_item = MenuItem::with_id(
        app,
        COPY_PATH_ID,
        menu_t("menu.edit.copyPath"),
        true,
        Some(copy_path_accelerator()),
    )?;
    let copy_filename_item = MenuItem::with_id(
        app,
        COPY_FILENAME_ID,
        menu_t("menu.edit.copyFilename"),
        true,
        None::<&str>,
    )?;
    let search_files_item = MenuItem::with_id(
        app,
        SEARCH_FILES_ID,
        menu_t("menu.edit.searchFiles"),
        true,
        Some("Cmd+F"),
    )?;

    let edit_menu = build_registered_submenu(
        app,
        Some(EDIT_MENU_ID),
        &menu_t("menu.bar.edit"),
        &[
            MenuSlot::Plain(&PredefinedMenuItem::undo(app, Some(&menu_t("menu.edit.undo")))?),
            MenuSlot::Plain(&PredefinedMenuItem::redo(app, Some(&menu_t("menu.edit.redo")))?),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(EDIT_CUT_ID, &edit_cut_item),
            MenuSlot::Reg(EDIT_COPY_ID, &edit_copy_item),
            MenuSlot::Reg(EDIT_PASTE_ID, &edit_paste_item),
            MenuSlot::Reg(EDIT_PASTE_MOVE_ID, &edit_paste_move_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(COPY_PATH_ID, &copy_path_item),
            MenuSlot::Reg(COPY_FILENAME_ID, &copy_filename_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(SEARCH_FILES_ID, &search_files_item),
        ],
        &mut items,
    )?;
    menu.append(&edit_menu)?;

    // --- Select menu ---
    // Lives between Edit and View. Holds the selection commands: Select all / Deselect all
    // (formerly in Edit), and the two new pattern-based dialog openers.
    // The dialog openers carry no menu accelerator: macOS menu accelerators always carry
    // a modifier (Cmd), and the bare `+` / `-` keystrokes are bound in FilePane's keydown
    // handler instead. The labels show no accelerator badge as a result.
    let select_all_item = MenuItem::with_id(app, SELECT_ALL_ID, menu_t("menu.select.all"), true, Some("Cmd+A"))?;
    let deselect_all_item = MenuItem::with_id(
        app,
        DESELECT_ALL_ID,
        menu_t("menu.select.deselectAll"),
        true,
        Some("Cmd+Shift+A"),
    )?;
    // No accelerator: `⇧8` carries no Cmd, and a bare menu accelerator would swallow the
    // `*` keystroke in every text field. FilePane's keydown handler binds it instead.
    let invert_selection_item = MenuItem::with_id(
        app,
        INVERT_SELECTION_ID,
        menu_t("menu.select.invert"),
        true,
        None::<&str>,
    )?;
    let select_files_item = MenuItem::with_id(app, SELECT_FILES_ID, menu_t("menu.select.files"), true, None::<&str>)?;
    let deselect_files_item = MenuItem::with_id(
        app,
        DESELECT_FILES_ID,
        menu_t("menu.select.deselectFiles"),
        true,
        None::<&str>,
    )?;

    let select_menu = build_registered_submenu(
        app,
        Some(SELECT_MENU_ID),
        &menu_t("menu.bar.select"),
        &[
            MenuSlot::Reg(SELECT_ALL_ID, &select_all_item),
            MenuSlot::Reg(DESELECT_ALL_ID, &deselect_all_item),
            MenuSlot::Reg(INVERT_SELECTION_ID, &invert_selection_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(SELECT_FILES_ID, &select_files_item),
            MenuSlot::Reg(DESELECT_FILES_ID, &deselect_files_item),
        ],
        &mut items,
    )?;
    menu.append(&select_menu)?;

    // --- View menu ---
    // View > Left pane > {Full, Brief} and View > Right pane > {Full, Brief}.
    // Both pairs always exist; only the active pane's pair carries the keyboard
    // accelerator (⌘1/⌘2 by default), and it "follows" focus on Tab via
    // `rebuild_view_mode_items`. Initial build: left is the default active pane,
    // both modes default to Brief.
    let view_mode_items = build_view_mode_items(
        app,
        view_mode,
        &menu_t("menu.view.leftPane"),
        &menu_t("menu.view.rightPane"),
    )?;
    let ViewModeItems {
        full_left: view_mode_full_left_item,
        brief_left: view_mode_brief_left_item,
        full_right: view_mode_full_right_item,
        brief_right: view_mode_brief_right_item,
        left_submenu: view_left_pane_submenu,
        right_submenu: view_right_pane_submenu,
    } = view_mode_items;

    let show_hidden_item = CheckMenuItem::with_id(
        app,
        SHOW_HIDDEN_FILES_ID,
        menu_t("menu.view.showHiddenFiles"),
        true,
        show_hidden_files,
        Some("Cmd+Shift+."),
    )?;
    let sort_items = build_sort_submenu(
        app,
        &menu_t("menu.view.sortBy"),
        Some("Cmd+3"),
        Some("Cmd+4"),
        Some("Cmd+5"),
        Some("Cmd+6"),
    )?;
    let sort_submenu = sort_items.submenu.clone();
    let zoom_submenu = build_zoom_submenu(
        app,
        &menu_t("menu.view.zoom"),
        Some("Cmd+0"),
        Some("Cmd+Plus"),
        Some("Cmd+Minus"),
    )?;
    let switch_pane_item = MenuItem::with_id(app, SWITCH_PANE_ID, menu_t("menu.view.switchPane"), true, Some("Tab"))?;
    let swap_panes_item = MenuItem::with_id(app, SWAP_PANES_ID, menu_t("menu.view.swapPanes"), true, Some("Cmd+U"))?;
    let command_palette_item = MenuItem::with_id(
        app,
        COMMAND_PALETTE_ID,
        menu_t("menu.view.commandPalette"),
        true,
        Some("Cmd+Shift+P"),
    )?;
    // Default ⌘⌥Q (rendered ⌥⌘Q by macOS). Sits next to "Operation log" so the present-tense
    // and past-tense views of the same work read as a pair. The accelerator syncs from the
    // `queue.show` registry shortcut; this is the initial label.
    let queue_show_item = MenuItem::with_id(
        app,
        QUEUE_SHOW_ID,
        menu_t("menu.view.operationQueue"),
        true,
        Some("Cmd+Alt+Q"),
    )?;
    // Default ⌘⌥L (Cmd+Opt+L). ⌥⌘O — the plan's first choice — is taken by "Show in Finder".
    // The accelerator syncs from the `log.operationLog` registry shortcut; this is the initial label.
    let operation_log_item = MenuItem::with_id(
        app,
        OPERATION_LOG_ID,
        menu_t("menu.view.operationLog"),
        true,
        Some("Cmd+Alt+L"),
    )?;
    // No default accelerator: the status-corner indicator is the everyday way in, and a
    // suggestion waits indefinitely, so this isn't a key anyone reaches for mid-task. A user
    // who wants one binds it, and the accelerator then syncs from the `suggestedOps.show`
    // registry shortcut like every other item here.
    let suggested_ops_item = MenuItem::with_id(
        app,
        SUGGESTED_OPS_ID,
        menu_t("menu.view.suggestedOps"),
        true,
        None::<&str>,
    )?;
    // Default ⌘⌥A (rendered ⌥⌘A by macOS). The accelerator syncs from the `askCmdr.toggle`
    // registry shortcut; this is the initial label.
    let ask_cmdr_item = MenuItem::with_id(app, ASK_CMDR_ID, menu_t("menu.view.askCmdr"), true, Some("Cmd+Alt+A"))?;

    let view_submenu = build_registered_submenu(
        app,
        Some(VIEW_MENU_ID),
        &menu_t("menu.bar.view"),
        &[
            MenuSlot::Plain(&view_left_pane_submenu),
            MenuSlot::Plain(&view_right_pane_submenu),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Plain(&show_hidden_item),
            MenuSlot::Plain(&sort_submenu),
            MenuSlot::Plain(&zoom_submenu),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(SWITCH_PANE_ID, &switch_pane_item),
            MenuSlot::Reg(SWAP_PANES_ID, &swap_panes_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(COMMAND_PALETTE_ID, &command_palette_item),
            MenuSlot::Reg(QUEUE_SHOW_ID, &queue_show_item),
            MenuSlot::Reg(OPERATION_LOG_ID, &operation_log_item),
            MenuSlot::Reg(SUGGESTED_OPS_ID, &suggested_ops_item),
            MenuSlot::Reg(ASK_CMDR_ID, &ask_cmdr_item),
        ],
        &mut items,
    )?;
    menu.append(&view_submenu)?;

    // --- Go menu ---
    let go_back_item = MenuItem::with_id(app, GO_BACK_ID, menu_t("menu.go.back"), true, Some("Cmd+["))?;
    let go_forward_item = MenuItem::with_id(app, GO_FORWARD_ID, menu_t("menu.go.forward"), true, Some("Cmd+]"))?;
    let go_parent_item = MenuItem::with_id(app, GO_PARENT_ID, menu_t("menu.go.parentFolder"), true, Some("Cmd+Up"))?;
    // The ellipsis marks the dialog opener; "Go to latest download" is a direct action (none).
    let go_to_path_item = MenuItem::with_id(app, GO_TO_PATH_ID, menu_t("menu.go.goToPath"), true, Some("Cmd+G"))?;
    let go_latest_download_item = MenuItem::with_id(
        app,
        GO_LATEST_DOWNLOAD_ID,
        menu_t("menu.go.goToLatestDownload"),
        true,
        Some("Cmd+J"),
    )?;
    // Shift+Cmd+H, not Cmd+H: AppKit owns Cmd+H for "Hide Cmdr" and would swallow it
    // before the webview ever sees a keydown.
    let go_home_item = MenuItem::with_id(app, GO_HOME_ID, menu_t("menu.go.home"), true, Some("Shift+Cmd+H"))?;
    // No default accelerator: `favorites.add` ships without a default shortcut. The
    // accelerator-sync pass picks up whatever the user later binds in Settings > Keyboard shortcuts.
    let favorites_add_item = MenuItem::with_id(
        app,
        FAVORITES_ADD_ID,
        menu_t("menu.go.addToFavorites"),
        true,
        None::<&str>,
    )?;

    let go_menu = build_registered_submenu(
        app,
        Some(GO_MENU_ID),
        &menu_t("menu.bar.go"),
        &[
            MenuSlot::Reg(GO_BACK_ID, &go_back_item),
            MenuSlot::Reg(GO_FORWARD_ID, &go_forward_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(GO_PARENT_ID, &go_parent_item),
            MenuSlot::Reg(GO_HOME_ID, &go_home_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(GO_TO_PATH_ID, &go_to_path_item),
            MenuSlot::Reg(GO_LATEST_DOWNLOAD_ID, &go_latest_download_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(FAVORITES_ADD_ID, &favorites_add_item),
        ],
        &mut items,
    )?;
    menu.append(&go_menu)?;

    // --- Servers menu ---
    // The ellipsis marks the sheet, where the user picks which server to connect to.
    let servers_connect_item = MenuItem::with_id(
        app,
        SERVERS_CONNECT_ID,
        menu_t("menu.servers.connectToServer"),
        true,
        Some("Cmd+K"),
    )?;
    // No default accelerator: `servers.show` ships without a default shortcut.
    let servers_show_item = MenuItem::with_id(
        app,
        SERVERS_SHOW_ID,
        menu_t("menu.servers.showServers"),
        true,
        None::<&str>,
    )?;

    let servers_menu = build_registered_submenu(
        app,
        Some(SERVERS_MENU_ID),
        &menu_t("menu.bar.servers"),
        &[
            MenuSlot::Reg(SERVERS_CONNECT_ID, &servers_connect_item),
            MenuSlot::Reg(SERVERS_SHOW_ID, &servers_show_item),
        ],
        &mut items,
    )?;
    menu.append(&servers_menu)?;

    // --- Tab menu ---
    let new_tab_item = MenuItem::with_id(app, NEW_TAB_ID, menu_t("menu.tab.newTab"), true, Some("Cmd+T"))?;
    let close_tab_item = MenuItem::with_id(app, CLOSE_TAB_ID, menu_t("menu.tab.closeTab"), true, Some("Cmd+W"))?;
    // Disabled initially; frontend enables it after the first close via
    // `set_reopen_closed_tab_enabled`.
    let reopen_closed_tab_item = MenuItem::with_id(
        app,
        REOPEN_CLOSED_TAB_ID,
        menu_t("menu.tab.reopenClosedTab"),
        false,
        Some("Cmd+Shift+T"),
    )?;
    let next_tab_item = MenuItem::with_id(app, NEXT_TAB_ID, menu_t("menu.tab.nextTab"), true, Some("Ctrl+Tab"))?;
    let prev_tab_item = MenuItem::with_id(
        app,
        PREV_TAB_ID,
        menu_t("menu.tab.previousTab"),
        true,
        Some("Ctrl+Shift+Tab"),
    )?;
    let pin_tab_item = MenuItem::with_id(app, PIN_TAB_MENU_ID, menu_t("menu.tab.pinTab"), true, None::<&str>)?;
    let close_other_tabs_item = MenuItem::with_id(
        app,
        CLOSE_OTHER_TABS_ID,
        menu_t("menu.tab.closeOtherTabs"),
        true,
        None::<&str>,
    )?;

    let tab_menu = build_registered_submenu(
        app,
        Some(TAB_MENU_ID),
        &menu_t("menu.bar.tab"),
        &[
            MenuSlot::Reg(NEW_TAB_ID, &new_tab_item),
            MenuSlot::Reg(CLOSE_TAB_ID, &close_tab_item),
            MenuSlot::Reg(REOPEN_CLOSED_TAB_ID, &reopen_closed_tab_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(NEXT_TAB_ID, &next_tab_item),
            MenuSlot::Reg(PREV_TAB_ID, &prev_tab_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Plain(&pin_tab_item),
            MenuSlot::Reg(CLOSE_OTHER_TABS_ID, &close_other_tabs_item),
        ],
        &mut items,
    )?;
    menu.append(&tab_menu)?;

    // --- Window menu ---
    let window_menu = build_registered_submenu(
        app,
        Some(WINDOW_MENU_ID),
        &menu_t("menu.bar.window"),
        &[
            MenuSlot::Plain(&PredefinedMenuItem::minimize(
                app,
                Some(&menu_t("menu.window.minimize")),
            )?),
            MenuSlot::Plain(&PredefinedMenuItem::maximize(app, Some(&menu_t("menu.window.zoom")))?),
        ],
        &mut items,
    )?;
    menu.append(&window_menu)?;

    // --- Help menu ---
    // macOS auto-adds a search field to any menu named "Help"
    let shortcuts_item = MenuItem::with_id(
        app,
        HELP_SHORTCUTS_ID,
        menu_t("menu.help.keyboardShortcuts"),
        true,
        None::<&str>,
    )?;
    let whats_new_item = MenuItem::with_id(app, HELP_WHATS_NEW_ID, menu_t("menu.help.whatsNew"), true, None::<&str>)?;
    let send_feedback_item = MenuItem::with_id(
        app,
        HELP_SEND_FEEDBACK_ID,
        menu_t("menu.help.sendFeedback"),
        true,
        None::<&str>,
    )?;
    let send_error_report_item = MenuItem::with_id(
        app,
        HELP_SEND_ERROR_REPORT_ID,
        menu_t("menu.help.sendErrorReport"),
        true,
        None::<&str>,
    )?;
    let help_menu = build_registered_submenu(
        app,
        Some(HELP_MENU_ID),
        &menu_t("menu.bar.help"),
        &[
            MenuSlot::Reg(HELP_SHORTCUTS_ID, &shortcuts_item),
            MenuSlot::Plain(&PredefinedMenuItem::separator(app)?),
            MenuSlot::Reg(HELP_WHATS_NEW_ID, &whats_new_item),
            MenuSlot::Reg(HELP_SEND_FEEDBACK_ID, &send_feedback_item),
            MenuSlot::Reg(HELP_SEND_ERROR_REPORT_ID, &send_error_report_item),
        ],
        &mut items,
    )?;
    menu.append(&help_menu)?;

    // Sort by: the positions live with the layout in `menu_items::register_sort_items`.
    register_sort_items(&mut items, &sort_items);

    Ok(MenuItems {
        menu,
        show_hidden_files: show_hidden_item,
        view_mode_full_left: view_mode_full_left_item,
        view_mode_brief_left: view_mode_brief_left_item,
        view_mode_full_right: view_mode_full_right_item,
        view_mode_brief_right: view_mode_brief_right_item,
        view_left_pane_submenu,
        view_right_pane_submenu,
        pin_tab: pin_tab_item,
        items,
        sort_submenu,
    })
}
