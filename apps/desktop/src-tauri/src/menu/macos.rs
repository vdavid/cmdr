//! The macOS menu bar's shape.
//!
//! Building only. The two passes that reach into AppKit afterwards live in `macos_appkit.rs`.

use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::intl::menu_t;

use super::menu_items::{
    APP_MENU_TITLE, ViewModeItems, build_sort_submenu, build_view_mode_items, build_zoom_submenu,
    copy_path_accelerator, register_item, register_sort_items, show_in_file_manager_accelerator,
    show_in_file_manager_label,
};
use super::{
    ABOUT_ID, ACKNOWLEDGEMENTS_ID, APP_MENU_ID, ASK_CMDR_ID, CHANGELOG_ID, CHECK_FOR_UPDATES_ID, CLOSE_OTHER_TABS_ID,
    CLOSE_TAB_ID, COMMAND_PALETTE_ID, COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID, DESELECT_FILES_ID, EDIT_COPY_ID,
    EDIT_CUT_ID, EDIT_ID, EDIT_MENU_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FAVORITES_ADD_ID,
    FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_DUPLICATE_ID, FILE_MENU_ID,
    FILE_MOVE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GET_INFO_ID, GO_BACK_ID, GO_FORWARD_ID, GO_HOME_ID,
    GO_LATEST_DOWNLOAD_ID, GO_MENU_ID, GO_PARENT_ID, GO_TO_PATH_ID, HELP_MENU_ID, HELP_SEND_ERROR_REPORT_ID,
    HELP_SEND_FEEDBACK_ID, HELP_SHORTCUTS_ID, HELP_WHATS_NEW_ID, MenuItems, NEW_TAB_ID, NEXT_TAB_ID, OPEN_ID,
    OPEN_ONBOARDING_ID, OPERATION_LOG_ID, PIN_TAB_MENU_ID, PREV_TAB_ID, QUEUE_SHOW_ID, QUICK_LOOK_ID, RENAME_ID,
    REOPEN_CLOSED_TAB_ID, SEARCH_FILES_ID, SELECT_ALL_ID, SELECT_FILES_ID, SELECT_MENU_ID, SETTINGS_ID,
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

    let app_menu = Submenu::with_id_and_items(
        app,
        APP_MENU_ID,
        APP_MENU_TITLE,
        true,
        &[
            &about_item,
            &acknowledgements_item,
            &license_item,
            &check_for_updates_item,
            &changelog_item,
            &open_onboarding_item,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            // System Services menu, populated by AppKit with Action extensions and other apps'
            // services (Ghostty's "New tab here", Nimble Commander's "Reveal", Quick Actions, etc.).
            // muda's PredefinedMenuItem::services wires `NSApplication.servicesMenu` for us.
            &PredefinedMenuItem::services(app, Some(&menu_t("menu.app.services")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(&menu_t("menu.app.hide")))?,
            &PredefinedMenuItem::hide_others(app, Some(&menu_t("menu.app.hideOthers")))?,
            &PredefinedMenuItem::show_all(app, Some(&menu_t("menu.app.showAll")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(&menu_t("menu.app.quit")))?,
        ],
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

    let file_menu = Submenu::with_id_and_items(
        app,
        FILE_MENU_ID,
        menu_t("menu.bar.file"),
        true,
        &[
            &open_item,
            &file_view_item,
            &edit_item,
            &PredefinedMenuItem::separator(app)?,
            &file_copy_item,
            &file_move_item,
            &file_duplicate_item,
            &file_compress_item,
            &file_new_folder_item,
            &file_delete_item,
            &file_delete_permanently_item,
            &PredefinedMenuItem::separator(app)?,
            &rename_item,
            &PredefinedMenuItem::separator(app)?,
            &show_in_finder_item,
            &get_info_item,
            &quick_look_item,
        ],
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

    let edit_menu = Submenu::with_id_and_items(
        app,
        EDIT_MENU_ID,
        menu_t("menu.bar.edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&menu_t("menu.edit.undo")))?,
            &PredefinedMenuItem::redo(app, Some(&menu_t("menu.edit.redo")))?,
            &PredefinedMenuItem::separator(app)?,
            &edit_cut_item,
            &edit_copy_item,
            &edit_paste_item,
            &edit_paste_move_item,
            &PredefinedMenuItem::separator(app)?,
            &copy_path_item,
            &copy_filename_item,
            &PredefinedMenuItem::separator(app)?,
            &search_files_item,
        ],
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
    let select_files_item = MenuItem::with_id(app, SELECT_FILES_ID, menu_t("menu.select.files"), true, None::<&str>)?;
    let deselect_files_item = MenuItem::with_id(
        app,
        DESELECT_FILES_ID,
        menu_t("menu.select.deselectFiles"),
        true,
        None::<&str>,
    )?;

    let select_menu = Submenu::with_id_and_items(
        app,
        SELECT_MENU_ID,
        menu_t("menu.bar.select"),
        true,
        &[
            &select_all_item,
            &deselect_all_item,
            &PredefinedMenuItem::separator(app)?,
            &select_files_item,
            &deselect_files_item,
        ],
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

    let view_submenu = Submenu::with_id_and_items(
        app,
        VIEW_MENU_ID,
        menu_t("menu.bar.view"),
        true,
        &[
            &view_left_pane_submenu,
            &view_right_pane_submenu,
            &PredefinedMenuItem::separator(app)?,
            &show_hidden_item,
            &sort_submenu,
            &zoom_submenu,
            &PredefinedMenuItem::separator(app)?,
            &switch_pane_item,
            &swap_panes_item,
            &PredefinedMenuItem::separator(app)?,
            &command_palette_item,
            &queue_show_item,
            &operation_log_item,
            &suggested_ops_item,
            &ask_cmdr_item,
        ],
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

    let go_menu = Submenu::with_id_and_items(
        app,
        GO_MENU_ID,
        menu_t("menu.bar.go"),
        true,
        &[
            &go_back_item,
            &go_forward_item,
            &PredefinedMenuItem::separator(app)?,
            &go_parent_item,
            &go_home_item,
            &PredefinedMenuItem::separator(app)?,
            &go_to_path_item,
            &go_latest_download_item,
            &PredefinedMenuItem::separator(app)?,
            &favorites_add_item,
        ],
    )?;
    menu.append(&go_menu)?;

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

    let tab_menu = Submenu::with_id_and_items(
        app,
        TAB_MENU_ID,
        menu_t("menu.bar.tab"),
        true,
        &[
            &new_tab_item,
            &close_tab_item,
            &reopen_closed_tab_item,
            &PredefinedMenuItem::separator(app)?,
            &next_tab_item,
            &prev_tab_item,
            &PredefinedMenuItem::separator(app)?,
            &pin_tab_item,
            &close_other_tabs_item,
        ],
    )?;
    menu.append(&tab_menu)?;

    // --- Window menu ---
    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_MENU_ID,
        menu_t("menu.bar.window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(&menu_t("menu.window.minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(&menu_t("menu.window.zoom")))?,
        ],
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
    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_MENU_ID,
        menu_t("menu.bar.help"),
        true,
        &[
            &shortcuts_item,
            &PredefinedMenuItem::separator(app)?,
            &whats_new_item,
            &send_feedback_item,
            &send_error_report_item,
        ],
    )?;
    menu.append(&help_menu)?;

    // --- Populate items HashMap for accelerator updates ---
    let mut items = HashMap::new();

    // File menu positions: open(0), view(1), edit(2), sep(3), copy(4), move(5),
    // duplicate(6), compress(7), new_folder(8), delete(9), delete_perm(10), sep(11),
    // rename(12), sep(13), show_in_finder(14), get_info(15), quick_look(16)
    register_item(&mut items, OPEN_ID, &open_item, &file_menu, 0);
    register_item(&mut items, FILE_VIEW_ID, &file_view_item, &file_menu, 1);
    register_item(&mut items, EDIT_ID, &edit_item, &file_menu, 2);
    register_item(&mut items, FILE_COPY_ID, &file_copy_item, &file_menu, 4);
    register_item(&mut items, FILE_MOVE_ID, &file_move_item, &file_menu, 5);
    register_item(&mut items, FILE_DUPLICATE_ID, &file_duplicate_item, &file_menu, 6);
    register_item(&mut items, FILE_COMPRESS_ID, &file_compress_item, &file_menu, 7);
    register_item(&mut items, FILE_NEW_FOLDER_ID, &file_new_folder_item, &file_menu, 8);
    register_item(&mut items, FILE_DELETE_ID, &file_delete_item, &file_menu, 9);
    register_item(
        &mut items,
        FILE_DELETE_PERMANENTLY_ID,
        &file_delete_permanently_item,
        &file_menu,
        10,
    );
    register_item(&mut items, RENAME_ID, &rename_item, &file_menu, 12);
    register_item(&mut items, SHOW_IN_FINDER_ID, &show_in_finder_item, &file_menu, 14);
    register_item(&mut items, GET_INFO_ID, &get_info_item, &file_menu, 15);
    register_item(&mut items, QUICK_LOOK_ID, &quick_look_item, &file_menu, 16);

    // Edit menu positions: undo(0), redo(1), sep(2), cut(3), copy(4), paste(5), move_here(6),
    // sep(7), copy_path(8), copy_filename(9), sep(10), search_files(11)
    register_item(&mut items, EDIT_CUT_ID, &edit_cut_item, &edit_menu, 3);
    register_item(&mut items, EDIT_COPY_ID, &edit_copy_item, &edit_menu, 4);
    register_item(&mut items, EDIT_PASTE_ID, &edit_paste_item, &edit_menu, 5);
    register_item(&mut items, EDIT_PASTE_MOVE_ID, &edit_paste_move_item, &edit_menu, 6);
    register_item(&mut items, COPY_PATH_ID, &copy_path_item, &edit_menu, 8);
    register_item(&mut items, COPY_FILENAME_ID, &copy_filename_item, &edit_menu, 9);
    register_item(&mut items, SEARCH_FILES_ID, &search_files_item, &edit_menu, 11);

    // Select menu positions: select_all(0), deselect_all(1), sep(2), select_files(3),
    // deselect_files(4). The two `…` items carry no accelerator: bare `+`/`-` aren't valid
    // macOS menu accelerators (those always carry Cmd), so the keystroke binding lives in
    // FilePane's keydown handler. The items are still registered so a future user-customized
    // shortcut could flow into the menu via the generic update path.
    register_item(&mut items, SELECT_ALL_ID, &select_all_item, &select_menu, 0);
    register_item(&mut items, DESELECT_ALL_ID, &deselect_all_item, &select_menu, 1);
    register_item(&mut items, SELECT_FILES_ID, &select_files_item, &select_menu, 3);
    register_item(&mut items, DESELECT_FILES_ID, &deselect_files_item, &select_menu, 4);

    // View menu positions: full(0), brief(1), sep(2), hidden(3), sort(4), zoom(5), sep(6),
    // switch(7), swap(8), sep(9), command(10), queue(11), operation_log(12),
    // suggested_ops(13), ask_cmdr(14)
    register_item(&mut items, SWITCH_PANE_ID, &switch_pane_item, &view_submenu, 7);
    register_item(&mut items, SWAP_PANES_ID, &swap_panes_item, &view_submenu, 8);
    register_item(&mut items, COMMAND_PALETTE_ID, &command_palette_item, &view_submenu, 10);
    register_item(&mut items, QUEUE_SHOW_ID, &queue_show_item, &view_submenu, 11);
    register_item(&mut items, OPERATION_LOG_ID, &operation_log_item, &view_submenu, 12);
    register_item(&mut items, SUGGESTED_OPS_ID, &suggested_ops_item, &view_submenu, 13);
    register_item(&mut items, ASK_CMDR_ID, &ask_cmdr_item, &view_submenu, 14);

    // Sort by: the positions live with the layout in `menu_items::register_sort_items`.
    register_sort_items(&mut items, &sort_items);

    // Go menu positions: back(0), forward(1), sep(2), parent(3), home(4), sep(5), go_to_path(6),
    // go_latest_download(7), sep(8), favorites_add(9)
    register_item(&mut items, GO_BACK_ID, &go_back_item, &go_menu, 0);
    register_item(&mut items, GO_FORWARD_ID, &go_forward_item, &go_menu, 1);
    register_item(&mut items, GO_PARENT_ID, &go_parent_item, &go_menu, 3);
    register_item(&mut items, GO_HOME_ID, &go_home_item, &go_menu, 4);
    register_item(&mut items, GO_TO_PATH_ID, &go_to_path_item, &go_menu, 6);
    register_item(&mut items, GO_LATEST_DOWNLOAD_ID, &go_latest_download_item, &go_menu, 7);
    register_item(&mut items, FAVORITES_ADD_ID, &favorites_add_item, &go_menu, 9);

    // Tab menu positions: new(0), close(1), reopen(2), sep(3), next(4), prev(5), sep(6), pin(7),
    // close_others(8)
    register_item(&mut items, NEW_TAB_ID, &new_tab_item, &tab_menu, 0);
    register_item(&mut items, CLOSE_TAB_ID, &close_tab_item, &tab_menu, 1);
    register_item(&mut items, REOPEN_CLOSED_TAB_ID, &reopen_closed_tab_item, &tab_menu, 2);
    register_item(&mut items, NEXT_TAB_ID, &next_tab_item, &tab_menu, 4);
    register_item(&mut items, PREV_TAB_ID, &prev_tab_item, &tab_menu, 5);
    register_item(&mut items, CLOSE_OTHER_TABS_ID, &close_other_tabs_item, &tab_menu, 8);

    // Help menu positions: shortcuts(0), sep(1), whats_new(2), send_feedback(3), send_error_report(4)
    register_item(&mut items, HELP_SHORTCUTS_ID, &shortcuts_item, &help_menu, 0);
    register_item(&mut items, HELP_WHATS_NEW_ID, &whats_new_item, &help_menu, 2);
    register_item(&mut items, HELP_SEND_FEEDBACK_ID, &send_feedback_item, &help_menu, 3);
    register_item(
        &mut items,
        HELP_SEND_ERROR_REPORT_ID,
        &send_error_report_item,
        &help_menu,
        4,
    );

    // cmdr menu positions: about(0), acknowledgements(1), license(2),
    // check_for_updates(3), changelog(4), open_onboarding(5), sep(6), settings(7),
    // sep(8), services(9), sep(10), hide(11), hide_others(12), show_all(13),
    // sep(14), quit(15)
    register_item(&mut items, CHECK_FOR_UPDATES_ID, &check_for_updates_item, &app_menu, 3);
    register_item(&mut items, CHANGELOG_ID, &changelog_item, &app_menu, 4);
    register_item(&mut items, OPEN_ONBOARDING_ID, &open_onboarding_item, &app_menu, 5);

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
        reopen_closed_tab: reopen_closed_tab_item,
        items,
        sort_submenu,
    })
}
