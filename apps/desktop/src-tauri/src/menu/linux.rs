use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::intl::menu_t;

use super::menu_items::{
    Mnemonics, ViewModeItems, build_sort_submenu, build_view_mode_items, build_zoom_submenu, copy_path_accelerator,
    register_item, register_sort_items, show_in_file_manager_accelerator, show_in_file_manager_label,
};
use super::{
    ABOUT_ID, ACKNOWLEDGEMENTS_ID, ASK_CMDR_ID, CHANGELOG_ID, CHECK_FOR_UPDATES_ID, CLOSE_OTHER_TABS_ID, CLOSE_TAB_ID,
    COMMAND_PALETTE_ID, COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID, DESELECT_FILES_ID, EDIT_COPY_ID, EDIT_CUT_ID,
    EDIT_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FAVORITES_ADD_ID, FILE_COMPRESS_ID, FILE_COPY_ID,
    FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_DUPLICATE_ID, FILE_MOVE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID,
    GET_INFO_ID, GO_BACK_ID, GO_FORWARD_ID, GO_HOME_ID, GO_LATEST_DOWNLOAD_ID, GO_PARENT_ID, GO_TO_PATH_ID,
    HELP_SEND_ERROR_REPORT_ID, HELP_SEND_FEEDBACK_ID, HELP_SHORTCUTS_ID, HELP_WHATS_NEW_ID, MenuItems, NEW_TAB_ID,
    NEXT_TAB_ID, OPEN_ID, OPERATION_LOG_ID, PIN_TAB_MENU_ID, PREV_TAB_ID, QUEUE_SHOW_ID, QUICK_LOOK_ID, RENAME_ID,
    REOPEN_CLOSED_TAB_ID, SEARCH_FILES_ID, SELECT_ALL_ID, SELECT_FILES_ID, SETTINGS_ID, SHOW_HIDDEN_FILES_ID,
    SHOW_IN_FINDER_ID, SUGGESTED_OPS_ID, SWAP_PANES_ID, SWITCH_PANE_ID, ViewMode,
};

/// Linux menu: builds all menus from scratch, matching the macOS menu structure.
/// Differences from macOS:
/// - No cmdr app menu (Settings and license go under Edit, About under Help)
/// - "Show in file manager" instead of "Show in Finder"
/// - Function-key accelerators (F2-F8, Shift+F8) omitted: GTK intercepts them before the webview,
///   and is_focused() fails on Linux, so JS dispatch handles these
/// - Tab and Space accelerators omitted (GTK accessibility conflicts)
/// - GTK `&` mnemonics, allocated per submenu by `Mnemonics` from the translated labels
pub(crate) fn build_menu_linux<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
) -> tauri::Result<MenuItems<R>> {
    let menu = Menu::new(app)?;

    // Mnemonic letters are unique per submenu, so each submenu gets its own
    // allocator and the menu bar's titles get one of their own. They're assigned
    // from the TRANSLATED labels in menu order (`Mnemonics`), because which
    // letters are free depends on the language.
    let mut bar = Mnemonics::new();

    // --- File menu ---
    let mut file = Mnemonics::new();
    let open_item = MenuItem::with_id(app, OPEN_ID, file.assign(&menu_t("menu.file.open")), true, None::<&str>)?;
    let file_view_item = MenuItem::with_id(
        app,
        FILE_VIEW_ID,
        file.assign(&menu_t("menu.file.view")),
        true,
        None::<&str>,
    )?;
    let edit_item = MenuItem::with_id(app, EDIT_ID, file.assign(&menu_t("menu.file.edit")), true, None::<&str>)?;
    let file_copy_item = MenuItem::with_id(
        app,
        FILE_COPY_ID,
        file.assign(&menu_t("menu.file.copy")),
        true,
        None::<&str>,
    )?;
    let file_move_item = MenuItem::with_id(
        app,
        FILE_MOVE_ID,
        file.assign(&menu_t("menu.file.move")),
        true,
        None::<&str>,
    )?;
    let file_duplicate_item = MenuItem::with_id(
        app,
        FILE_DUPLICATE_ID,
        file.assign(&menu_t("menu.file.duplicate")),
        true,
        None::<&str>,
    )?;
    let file_compress_item = MenuItem::with_id(
        app,
        FILE_COMPRESS_ID,
        file.assign(&menu_t("menu.file.compress")),
        true,
        None::<&str>,
    )?;
    let file_new_folder_item = MenuItem::with_id(
        app,
        FILE_NEW_FOLDER_ID,
        file.assign(&menu_t("menu.file.newFolder")),
        true,
        None::<&str>,
    )?;
    let file_delete_item = MenuItem::with_id(
        app,
        FILE_DELETE_ID,
        file.assign(&menu_t("menu.file.delete")),
        true,
        None::<&str>,
    )?;
    let file_delete_permanently_item = MenuItem::with_id(
        app,
        FILE_DELETE_PERMANENTLY_ID,
        file.assign(&menu_t("menu.file.deletePermanently")),
        true,
        None::<&str>,
    )?;
    let rename_item = MenuItem::with_id(
        app,
        RENAME_ID,
        file.assign(&menu_t("menu.file.rename")),
        true,
        None::<&str>,
    )?;
    let show_in_fm_item = MenuItem::with_id(
        app,
        SHOW_IN_FINDER_ID,
        file.assign(&show_in_file_manager_label()),
        true,
        Some(show_in_file_manager_accelerator()),
    )?;
    let get_info_item = MenuItem::with_id(
        app,
        GET_INFO_ID,
        file.assign(&menu_t("menu.file.getInfo")),
        true,
        Some("Cmd+I"),
    )?;
    let quick_look_item = MenuItem::with_id(
        app,
        QUICK_LOOK_ID,
        file.assign(&menu_t("menu.file.quickLook")),
        true,
        None::<&str>,
    )?;

    let file_menu = Submenu::with_items(
        app,
        bar.assign(&menu_t("menu.bar.file")),
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
            &show_in_fm_item,
            &get_info_item,
            &quick_look_item,
        ],
    )?;
    menu.append(&file_menu)?;

    // --- Edit menu ---
    let mut edit = Mnemonics::new();
    let edit_cut_item = MenuItem::with_id(
        app,
        EDIT_CUT_ID,
        edit.assign(&menu_t("menu.edit.cut")),
        true,
        Some("Ctrl+X"),
    )?;
    let edit_copy_item = MenuItem::with_id(
        app,
        EDIT_COPY_ID,
        edit.assign(&menu_t("menu.edit.copy")),
        true,
        Some("Ctrl+C"),
    )?;
    let edit_paste_item = MenuItem::with_id(
        app,
        EDIT_PASTE_ID,
        edit.assign(&menu_t("menu.edit.paste")),
        true,
        Some("Ctrl+V"),
    )?;
    let edit_paste_move_item = MenuItem::with_id(
        app,
        EDIT_PASTE_MOVE_ID,
        edit.assign(&menu_t("menu.edit.moveHere")),
        true,
        Some("Ctrl+Alt+V"),
    )?;
    let copy_path_item = MenuItem::with_id(
        app,
        COPY_PATH_ID,
        edit.assign(&menu_t("menu.edit.copyPath")),
        true,
        Some(copy_path_accelerator()),
    )?;
    let copy_filename_item = MenuItem::with_id(
        app,
        COPY_FILENAME_ID,
        edit.assign(&menu_t("menu.edit.copyFilename")),
        true,
        None::<&str>,
    )?;
    let search_files_item = MenuItem::with_id(
        app,
        SEARCH_FILES_ID,
        edit.assign(&menu_t("menu.edit.searchFiles")),
        true,
        Some("Cmd+F"),
    )?;
    let settings_item = MenuItem::with_id(
        app,
        SETTINGS_ID,
        edit.assign(&menu_t("menu.app.settings")),
        true,
        Some("Cmd+,"),
    )?;
    // Only one of these takes input, so only one gets the ellipsis: entering a key
    // asks for the key, seeing the details just shows them.
    let license_label = if has_existing_license {
        menu_t("menu.app.licenseDetails")
    } else {
        menu_t("menu.app.licenseEnter")
    };
    let license_item = MenuItem::with_id(
        app,
        ENTER_LICENSE_KEY_ID,
        edit.assign(&license_label),
        true,
        None::<&str>,
    )?;
    let check_for_updates_item = MenuItem::with_id(
        app,
        CHECK_FOR_UPDATES_ID,
        edit.assign(&menu_t("menu.app.checkForUpdates")),
        true,
        None::<&str>,
    )?;
    // Opens the "What's new" popup showing the latest releases (same command as Help > What's new).
    let changelog_item = MenuItem::with_id(
        app,
        CHANGELOG_ID,
        edit.assign(&menu_t("menu.app.changelog")),
        true,
        None::<&str>,
    )?;

    let edit_menu = Submenu::with_items(
        app,
        bar.assign(&menu_t("menu.bar.edit")),
        true,
        &[
            &edit_cut_item,
            &edit_copy_item,
            &edit_paste_item,
            &edit_paste_move_item,
            &PredefinedMenuItem::separator(app)?,
            &copy_path_item,
            &copy_filename_item,
            &PredefinedMenuItem::separator(app)?,
            &search_files_item,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &license_item,
            &check_for_updates_item,
            &changelog_item,
        ],
    )?;
    menu.append(&edit_menu)?;

    // --- Select menu ---
    // Lives between Edit and View, matching the macOS layout. Holds the four selection
    // commands. The two `…` dialog openers carry no accelerator: the keystroke binding
    // (bare `+` / `-`) lives in FilePane's keydown handler.
    let mut select = Mnemonics::new();
    let select_all_item = MenuItem::with_id(
        app,
        SELECT_ALL_ID,
        select.assign(&menu_t("menu.select.all")),
        true,
        Some("Cmd+A"),
    )?;
    let deselect_all_item = MenuItem::with_id(
        app,
        DESELECT_ALL_ID,
        select.assign(&menu_t("menu.select.deselectAll")),
        true,
        Some("Cmd+Shift+A"),
    )?;
    let select_files_item = MenuItem::with_id(
        app,
        SELECT_FILES_ID,
        select.assign(&menu_t("menu.select.files")),
        true,
        None::<&str>,
    )?;
    let deselect_files_item = MenuItem::with_id(
        app,
        DESELECT_FILES_ID,
        select.assign(&menu_t("menu.select.deselectFiles")),
        true,
        None::<&str>,
    )?;

    let select_menu = Submenu::with_items(
        app,
        bar.assign(&menu_t("menu.bar.select")),
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
    // accelerator, and it "follows" focus on Tab via `rebuild_view_mode_items`.
    // Initial build: left is the default active pane, both modes default to Brief.
    let mut view = Mnemonics::new();
    let view_mode_items = build_view_mode_items(
        app,
        view_mode,
        &view.assign(&menu_t("menu.view.leftPane")),
        &view.assign(&menu_t("menu.view.rightPane")),
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
        view.assign(&menu_t("menu.view.showHiddenFiles")),
        true,
        show_hidden_files,
        Some("Cmd+Shift+."),
    )?;
    // GTK intercepts F-row keys at the toolkit level, but Cmd+digit chords come
    // through fine. ⌘F3-⌘F6 alts go through JS dispatch only on Linux.
    //
    // ❗ These `Cmd+…` strings bind to SUPER here, not Ctrl: muda maps `"CMD"` to
    // `Modifiers::META`, which is Super on GTK, and only `CmdOrCtrl` resolves to
    // Ctrl off macOS. So the menu prints Super chords. Users still get Ctrl because
    // the frontend keydown layer accepts `metaKey || ctrlKey`; it's the LABEL that's
    // wrong. Switching to `CmdOrCtrl` also changes the macOS binding, so it needs a
    // check on both platforms rather than a blind sweep.
    // See `docs/notes/linux-gaps-2026-08-10.md`.
    let sort_items = build_sort_submenu(
        app,
        &view.assign(&menu_t("menu.view.sortBy")),
        Some("Cmd+3"),
        Some("Cmd+4"),
        Some("Cmd+5"),
        Some("Cmd+6"),
    )?;
    let sort_submenu = sort_items.submenu.clone();
    // GTK intercepts Cmd+Plus / Cmd+Minus at the toolkit level, so we don't
    // register them as native accelerators on Linux. The keyboard shortcuts
    // still work via the JS centralized dispatch path.
    let zoom_submenu = build_zoom_submenu(app, &view.assign(&menu_t("menu.view.zoom")), Some("Cmd+0"), None, None)?;
    let switch_pane_item = MenuItem::with_id(
        app,
        SWITCH_PANE_ID,
        view.assign(&menu_t("menu.view.switchPane")),
        true,
        None::<&str>,
    )?;
    let swap_panes_item = MenuItem::with_id(
        app,
        SWAP_PANES_ID,
        view.assign(&menu_t("menu.view.swapPanes")),
        true,
        Some("Cmd+U"),
    )?;
    let command_palette_item = MenuItem::with_id(
        app,
        COMMAND_PALETTE_ID,
        view.assign(&menu_t("menu.view.commandPalette")),
        true,
        Some("Cmd+Shift+P"),
    )?;
    // Default ⌘⌥Q, next to "Operation log" so the present-tense and past-tense views of the same
    // work read as a pair. `q` is the free mnemonic here (L, R, h, S, w, p, C, O, A are taken).
    // The accelerator syncs from the `queue.show` registry shortcut; this is the initial label.
    let queue_show_item = MenuItem::with_id(
        app,
        QUEUE_SHOW_ID,
        view.assign(&menu_t("menu.view.operationQueue")),
        true,
        Some("Cmd+Alt+Q"),
    )?;
    // Default ⌘⌥L (Cmd+Opt+L). ⌥⌘O — the plan's first choice — is taken by "Reveal in file manager".
    // The accelerator syncs from the `log.operationLog` registry shortcut; this is the initial label.
    let operation_log_item = MenuItem::with_id(
        app,
        OPERATION_LOG_ID,
        view.assign(&menu_t("menu.view.operationLog")),
        true,
        Some("Cmd+Alt+L"),
    )?;
    // Default ⌘⌥A. The accelerator syncs from the `askCmdr.toggle` registry shortcut.
    // No default accelerator: the status-corner indicator is the everyday way in, and a
    // suggestion waits indefinitely. A user who wants a key binds it.
    let suggested_ops_item = MenuItem::with_id(
        app,
        SUGGESTED_OPS_ID,
        view.assign(&menu_t("menu.view.suggestedOps")),
        true,
        None::<&str>,
    )?;
    let ask_cmdr_item = MenuItem::with_id(
        app,
        ASK_CMDR_ID,
        view.assign(&menu_t("menu.view.askCmdr")),
        true,
        Some("Cmd+Alt+A"),
    )?;

    let view_submenu = Submenu::with_items(
        app,
        bar.assign(&menu_t("menu.bar.view")),
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
    let mut go = Mnemonics::new();
    let go_back_item = MenuItem::with_id(app, GO_BACK_ID, go.assign(&menu_t("menu.go.back")), true, Some("Cmd+["))?;
    let go_forward_item = MenuItem::with_id(
        app,
        GO_FORWARD_ID,
        go.assign(&menu_t("menu.go.forward")),
        true,
        Some("Cmd+]"),
    )?;
    let go_parent_item = MenuItem::with_id(
        app,
        GO_PARENT_ID,
        go.assign(&menu_t("menu.go.parentFolder")),
        true,
        Some("Cmd+Up"),
    )?;
    let go_home_item = MenuItem::with_id(
        app,
        GO_HOME_ID,
        go.assign(&menu_t("menu.go.home")),
        true,
        Some("Shift+Cmd+H"),
    )?;
    let go_to_path_item = MenuItem::with_id(
        app,
        GO_TO_PATH_ID,
        go.assign(&menu_t("menu.go.goToPath")),
        true,
        Some("Cmd+G"),
    )?;
    let go_latest_download_item = MenuItem::with_id(
        app,
        GO_LATEST_DOWNLOAD_ID,
        go.assign(&menu_t("menu.go.goToLatestDownload")),
        true,
        Some("Cmd+J"),
    )?;
    // No default accelerator: `favorites.add` ships without a default shortcut (synced from any
    // user-assigned shortcut later).
    let favorites_add_item = MenuItem::with_id(
        app,
        FAVORITES_ADD_ID,
        go.assign(&menu_t("menu.go.addToFavorites")),
        true,
        None::<&str>,
    )?;

    let go_menu = Submenu::with_items(
        app,
        bar.assign(&menu_t("menu.bar.go")),
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
    let mut tab = Mnemonics::new();
    let new_tab_item = MenuItem::with_id(
        app,
        NEW_TAB_ID,
        tab.assign(&menu_t("menu.tab.newTab")),
        true,
        Some("Cmd+T"),
    )?;
    let close_tab_item = MenuItem::with_id(
        app,
        CLOSE_TAB_ID,
        tab.assign(&menu_t("menu.tab.closeTab")),
        true,
        Some("Cmd+W"),
    )?;
    // Disabled initially; frontend enables it after the first close.
    let reopen_closed_tab_item = MenuItem::with_id(
        app,
        REOPEN_CLOSED_TAB_ID,
        tab.assign(&menu_t("menu.tab.reopenClosedTab")),
        false,
        Some("Cmd+Shift+T"),
    )?;
    let next_tab_item = MenuItem::with_id(
        app,
        NEXT_TAB_ID,
        tab.assign(&menu_t("menu.tab.nextTab")),
        true,
        Some("Ctrl+Tab"),
    )?;
    let prev_tab_item = MenuItem::with_id(
        app,
        PREV_TAB_ID,
        tab.assign(&menu_t("menu.tab.previousTab")),
        true,
        Some("Ctrl+Shift+Tab"),
    )?;
    let pin_tab_item = MenuItem::with_id(
        app,
        PIN_TAB_MENU_ID,
        tab.assign(&menu_t("menu.tab.pinTab")),
        true,
        None::<&str>,
    )?;
    let close_other_tabs_item = MenuItem::with_id(
        app,
        CLOSE_OTHER_TABS_ID,
        tab.assign(&menu_t("menu.tab.closeOtherTabs")),
        true,
        None::<&str>,
    )?;

    let tab_menu = Submenu::with_items(
        app,
        bar.assign(&menu_t("menu.bar.tab")),
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

    // --- Help menu ---
    let mut help = Mnemonics::new();
    let about_item = MenuItem::with_id(
        app,
        ABOUT_ID,
        help.assign(&menu_t("menu.app.about")),
        true,
        None::<&str>,
    )?;
    // Linux has no app menu, so the third-party credits sit under Help next to About.
    let acknowledgements_item = MenuItem::with_id(
        app,
        ACKNOWLEDGEMENTS_ID,
        help.assign(&menu_t("menu.app.acknowledgements")),
        true,
        None::<&str>,
    )?;
    let shortcuts_item = MenuItem::with_id(
        app,
        HELP_SHORTCUTS_ID,
        help.assign(&menu_t("menu.help.keyboardShortcuts")),
        true,
        None::<&str>,
    )?;
    let whats_new_item = MenuItem::with_id(
        app,
        HELP_WHATS_NEW_ID,
        help.assign(&menu_t("menu.help.whatsNew")),
        true,
        None::<&str>,
    )?;
    let send_feedback_item = MenuItem::with_id(
        app,
        HELP_SEND_FEEDBACK_ID,
        help.assign(&menu_t("menu.help.sendFeedback")),
        true,
        None::<&str>,
    )?;
    let send_error_report_item = MenuItem::with_id(
        app,
        HELP_SEND_ERROR_REPORT_ID,
        help.assign(&menu_t("menu.help.sendErrorReport")),
        true,
        None::<&str>,
    )?;
    let help_menu = Submenu::with_items(
        app,
        bar.assign(&menu_t("menu.bar.help")),
        true,
        &[
            &about_item,
            &acknowledgements_item,
            &PredefinedMenuItem::separator(app)?,
            &shortcuts_item,
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
    // rename(12), sep(13), show_in_fm(14), get_info(15), quick_look(16)
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
    register_item(&mut items, SHOW_IN_FINDER_ID, &show_in_fm_item, &file_menu, 14);
    register_item(&mut items, GET_INFO_ID, &get_info_item, &file_menu, 15);
    register_item(&mut items, QUICK_LOOK_ID, &quick_look_item, &file_menu, 16);

    // Edit menu positions: cut(0), copy(1), paste(2), move_here(3), sep(4),
    // copy_path(5), copy_filename(6), sep(7), search_files(8), sep(9), settings(10),
    // license(11), check_for_updates(12), changelog(13)
    register_item(&mut items, EDIT_CUT_ID, &edit_cut_item, &edit_menu, 0);
    register_item(&mut items, EDIT_COPY_ID, &edit_copy_item, &edit_menu, 1);
    register_item(&mut items, EDIT_PASTE_ID, &edit_paste_item, &edit_menu, 2);
    register_item(&mut items, EDIT_PASTE_MOVE_ID, &edit_paste_move_item, &edit_menu, 3);
    register_item(&mut items, COPY_PATH_ID, &copy_path_item, &edit_menu, 5);
    register_item(&mut items, COPY_FILENAME_ID, &copy_filename_item, &edit_menu, 6);
    register_item(&mut items, SEARCH_FILES_ID, &search_files_item, &edit_menu, 8);
    register_item(&mut items, SETTINGS_ID, &settings_item, &edit_menu, 10);
    register_item(
        &mut items,
        CHECK_FOR_UPDATES_ID,
        &check_for_updates_item,
        &edit_menu,
        12,
    );
    register_item(&mut items, CHANGELOG_ID, &changelog_item, &edit_menu, 13);

    // Select menu positions: select_all(0), deselect_all(1), sep(2), select_files(3),
    // deselect_files(4). The two dialog openers carry no accelerator; bare `+` / `-` are
    // bound in FilePane's keydown handler. The items are still registered so a future
    // user-customized shortcut could flow into the menu via the generic update path.
    register_item(&mut items, SELECT_ALL_ID, &select_all_item, &select_menu, 0);
    register_item(&mut items, DESELECT_ALL_ID, &deselect_all_item, &select_menu, 1);
    register_item(&mut items, SELECT_FILES_ID, &select_files_item, &select_menu, 3);
    register_item(&mut items, DESELECT_FILES_ID, &deselect_files_item, &select_menu, 4);

    // View menu positions: left_pane_submenu(0), right_pane_submenu(1), sep(2), hidden(3),
    // sort(4), zoom(5), sep(6), switch(7), swap(8), sep(9), palette(10), queue(11),
    // operation_log(12), ask_cmdr(13)
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

    // Help menu: about(0), acknowledgements(1), separator(2), shortcuts(3),
    // whats_new(4), send_feedback(5), send_error_report(6)
    register_item(&mut items, ABOUT_ID, &about_item, &help_menu, 0);
    register_item(&mut items, HELP_SHORTCUTS_ID, &shortcuts_item, &help_menu, 3);
    register_item(&mut items, HELP_WHATS_NEW_ID, &whats_new_item, &help_menu, 4);
    register_item(&mut items, HELP_SEND_FEEDBACK_ID, &send_feedback_item, &help_menu, 5);
    register_item(
        &mut items,
        HELP_SEND_ERROR_REPORT_ID,
        &send_error_report_item,
        &help_menu,
        6,
    );

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
