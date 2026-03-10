use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

use super::{
    ABOUT_ID, CLOSE_OTHER_TABS_ID, CLOSE_TAB_ID, COMMAND_PALETTE_ID, COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID,
    EDIT_COPY_ID, EDIT_CUT_ID, EDIT_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FILE_COPY_ID,
    FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_MOVE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GET_INFO_ID,
    GO_BACK_ID, GO_FORWARD_ID, GO_PARENT_ID, MenuItems, NEW_TAB_ID, NEXT_TAB_ID, OPEN_ID, PIN_TAB_MENU_ID, PREV_TAB_ID,
    QUICK_LOOK_ID, RENAME_ID, SELECT_ALL_ID, SETTINGS_ID, SHOW_HIDDEN_FILES_ID, SHOW_IN_FINDER_ID, SWAP_PANES_ID,
    SWITCH_PANE_ID, VIEW_MODE_BRIEF_ID, VIEW_MODE_FULL_ID, ViewMode, build_sort_submenu, copy_path_accelerator,
    register_item, show_in_file_manager_accelerator, show_in_file_manager_label,
};

/// Linux menu: builds all menus from scratch, matching the macOS menu structure.
/// Differences from macOS:
/// - No cmdr app menu (Settings and license go under Edit, About under Help)
/// - "Show in file manager" instead of "Show in Finder"
/// - Function-key accelerators (F2-F8, Shift+F8) omitted — GTK intercepts them
///   before the webview, and is_focused() fails on Linux, so JS dispatch handles these
/// - Tab and Space accelerators omitted (GTK accessibility conflicts)
/// - Placeholder `&` mnemonics (first letter) — final mnemonic pass is Milestone 7
pub(crate) fn build_menu_linux<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
) -> tauri::Result<MenuItems<R>> {
    let menu = Menu::new(app)?;

    // --- File menu ---
    let open_item = MenuItem::with_id(app, OPEN_ID, "&Open", true, None::<&str>)?;
    let file_view_item = MenuItem::with_id(app, FILE_VIEW_ID, "&View", true, None::<&str>)?;
    let edit_item = MenuItem::with_id(app, EDIT_ID, "Edit in &editor", true, None::<&str>)?;
    let file_copy_item = MenuItem::with_id(app, FILE_COPY_ID, "&Copy...", true, None::<&str>)?;
    let file_move_item = MenuItem::with_id(app, FILE_MOVE_ID, "&Move...", true, None::<&str>)?;
    let file_new_folder_item = MenuItem::with_id(app, FILE_NEW_FOLDER_ID, "&New folder", true, None::<&str>)?;
    let file_delete_item = MenuItem::with_id(app, FILE_DELETE_ID, "&Delete", true, None::<&str>)?;
    let file_delete_permanently_item = MenuItem::with_id(
        app,
        FILE_DELETE_PERMANENTLY_ID,
        "Delete &permanently",
        true,
        None::<&str>,
    )?;
    let rename_item = MenuItem::with_id(app, RENAME_ID, "Re&name", true, None::<&str>)?;
    let show_in_fm_item = MenuItem::with_id(
        app,
        SHOW_IN_FINDER_ID,
        show_in_file_manager_label(),
        true,
        Some(show_in_file_manager_accelerator()),
    )?;
    let get_info_item = MenuItem::with_id(app, GET_INFO_ID, "Get &info", true, Some("Cmd+I"))?;
    let quick_look_item = MenuItem::with_id(app, QUICK_LOOK_ID, "&Quick look", true, None::<&str>)?;

    let file_menu = Submenu::with_items(
        app,
        "&File",
        true,
        &[
            &open_item,
            &file_view_item,
            &edit_item,
            &PredefinedMenuItem::separator(app)?,
            &file_copy_item,
            &file_move_item,
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
    let edit_cut_item = MenuItem::with_id(app, EDIT_CUT_ID, "Cu&t", true, Some("Ctrl+X"))?;
    let edit_copy_item = MenuItem::with_id(app, EDIT_COPY_ID, "&Copy", true, Some("Ctrl+C"))?;
    let edit_paste_item = MenuItem::with_id(app, EDIT_PASTE_ID, "&Paste", true, Some("Ctrl+V"))?;
    let edit_paste_move_item = MenuItem::with_id(app, EDIT_PASTE_MOVE_ID, "&Move here", true, Some("Ctrl+Alt+V"))?;
    let select_all_item = MenuItem::with_id(app, SELECT_ALL_ID, "Select &all", true, Some("Cmd+A"))?;
    let deselect_all_item = MenuItem::with_id(app, DESELECT_ALL_ID, "D&eselect all", true, Some("Cmd+Shift+A"))?;
    let copy_path_item = MenuItem::with_id(app, COPY_PATH_ID, "Cop&y path", true, Some(copy_path_accelerator()))?;
    let copy_filename_item = MenuItem::with_id(app, COPY_FILENAME_ID, "Copy file&name", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, SETTINGS_ID, "&Settings...", true, Some("Cmd+,"))?;
    let license_label = if has_existing_license {
        "See &license details..."
    } else {
        "Enter &license key..."
    };
    let license_item = MenuItem::with_id(app, ENTER_LICENSE_KEY_ID, license_label, true, None::<&str>)?;

    let edit_menu = Submenu::with_items(
        app,
        "&Edit",
        true,
        &[
            &edit_cut_item,
            &edit_copy_item,
            &edit_paste_item,
            &edit_paste_move_item,
            &PredefinedMenuItem::separator(app)?,
            &select_all_item,
            &deselect_all_item,
            &PredefinedMenuItem::separator(app)?,
            &copy_path_item,
            &copy_filename_item,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &license_item,
        ],
    )?;
    menu.append(&edit_menu)?;

    // --- View menu ---
    let view_mode_full_item = CheckMenuItem::with_id(
        app,
        VIEW_MODE_FULL_ID,
        "&Full view",
        true,
        view_mode == ViewMode::Full,
        Some("Cmd+1"),
    )?;
    let view_mode_brief_item = CheckMenuItem::with_id(
        app,
        VIEW_MODE_BRIEF_ID,
        "&Brief view",
        true,
        view_mode == ViewMode::Brief,
        Some("Cmd+2"),
    )?;
    let show_hidden_item = CheckMenuItem::with_id(
        app,
        SHOW_HIDDEN_FILES_ID,
        "Show &hidden files",
        true,
        show_hidden_files,
        Some("Cmd+Shift+."),
    )?;
    let sort_submenu = build_sort_submenu(app, "&Sort by")?;
    let switch_pane_item = MenuItem::with_id(app, SWITCH_PANE_ID, "S&witch pane", true, None::<&str>)?;
    let swap_panes_item = MenuItem::with_id(app, SWAP_PANES_ID, "Swa&p panes", true, Some("Cmd+U"))?;
    let command_palette_item = MenuItem::with_id(
        app,
        COMMAND_PALETTE_ID,
        "&Command palette...",
        true,
        Some("Cmd+Shift+P"),
    )?;

    let view_submenu = Submenu::with_items(
        app,
        "&View",
        true,
        &[
            &view_mode_full_item,
            &view_mode_brief_item,
            &PredefinedMenuItem::separator(app)?,
            &show_hidden_item,
            &sort_submenu,
            &PredefinedMenuItem::separator(app)?,
            &switch_pane_item,
            &swap_panes_item,
            &PredefinedMenuItem::separator(app)?,
            &command_palette_item,
        ],
    )?;
    menu.append(&view_submenu)?;

    // View mode items are at positions 0 and 1 in our freshly built View submenu
    let view_full_pos: usize = 0;
    let view_brief_pos: usize = 1;

    // --- Go menu ---
    let go_back_item = MenuItem::with_id(app, GO_BACK_ID, "&Back", true, Some("Cmd+["))?;
    let go_forward_item = MenuItem::with_id(app, GO_FORWARD_ID, "&Forward", true, Some("Cmd+]"))?;
    let go_parent_item = MenuItem::with_id(app, GO_PARENT_ID, "&Parent folder", true, Some("Cmd+Up"))?;

    let go_menu = Submenu::with_items(
        app,
        "&Go",
        true,
        &[
            &go_back_item,
            &go_forward_item,
            &PredefinedMenuItem::separator(app)?,
            &go_parent_item,
        ],
    )?;
    menu.append(&go_menu)?;

    // --- Tab menu ---
    let new_tab_item = MenuItem::with_id(app, NEW_TAB_ID, "&New tab", true, Some("Cmd+T"))?;
    let close_tab_item = MenuItem::with_id(app, CLOSE_TAB_ID, "&Close tab", true, Some("Cmd+W"))?;
    let next_tab_item = MenuItem::with_id(app, NEXT_TAB_ID, "Ne&xt tab", true, Some("Ctrl+Tab"))?;
    let prev_tab_item = MenuItem::with_id(app, PREV_TAB_ID, "&Previous tab", true, Some("Ctrl+Shift+Tab"))?;
    let pin_tab_item = MenuItem::with_id(app, PIN_TAB_MENU_ID, "P&in tab", true, None::<&str>)?;
    let close_other_tabs_item = MenuItem::with_id(app, CLOSE_OTHER_TABS_ID, "Close &other tabs", true, None::<&str>)?;

    let tab_menu = Submenu::with_items(
        app,
        "&Tab",
        true,
        &[
            &new_tab_item,
            &close_tab_item,
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
    let about_item = MenuItem::with_id(app, ABOUT_ID, "&About cmdr", true, None::<&str>)?;
    let help_menu = Submenu::with_items(app, "&Help", true, &[&about_item])?;
    menu.append(&help_menu)?;

    // --- Populate items HashMap for accelerator updates ---
    let mut items = HashMap::new();

    // File menu positions: open(0), view(1), edit(2), sep(3), copy(4), move(5),
    // new_folder(6), delete(7), delete_perm(8), sep(9), rename(10), sep(11),
    // show_in_fm(12), get_info(13), quick_look(14)
    register_item(&mut items, OPEN_ID, &open_item, &file_menu, 0);
    register_item(&mut items, FILE_VIEW_ID, &file_view_item, &file_menu, 1);
    register_item(&mut items, EDIT_ID, &edit_item, &file_menu, 2);
    register_item(&mut items, FILE_COPY_ID, &file_copy_item, &file_menu, 4);
    register_item(&mut items, FILE_MOVE_ID, &file_move_item, &file_menu, 5);
    register_item(&mut items, FILE_NEW_FOLDER_ID, &file_new_folder_item, &file_menu, 6);
    register_item(&mut items, FILE_DELETE_ID, &file_delete_item, &file_menu, 7);
    register_item(
        &mut items,
        FILE_DELETE_PERMANENTLY_ID,
        &file_delete_permanently_item,
        &file_menu,
        8,
    );
    register_item(&mut items, RENAME_ID, &rename_item, &file_menu, 10);
    register_item(&mut items, SHOW_IN_FINDER_ID, &show_in_fm_item, &file_menu, 12);
    register_item(&mut items, GET_INFO_ID, &get_info_item, &file_menu, 13);
    register_item(&mut items, QUICK_LOOK_ID, &quick_look_item, &file_menu, 14);

    // Edit menu positions: cut(0), copy(1), paste(2), move_here(3), sep(4),
    // select_all(5), deselect_all(6), sep(7), copy_path(8), copy_filename(9),
    // sep(10), settings(11), license(12)
    register_item(&mut items, EDIT_CUT_ID, &edit_cut_item, &edit_menu, 0);
    register_item(&mut items, EDIT_COPY_ID, &edit_copy_item, &edit_menu, 1);
    register_item(&mut items, EDIT_PASTE_ID, &edit_paste_item, &edit_menu, 2);
    register_item(&mut items, EDIT_PASTE_MOVE_ID, &edit_paste_move_item, &edit_menu, 3);
    register_item(&mut items, SELECT_ALL_ID, &select_all_item, &edit_menu, 5);
    register_item(&mut items, DESELECT_ALL_ID, &deselect_all_item, &edit_menu, 6);
    register_item(&mut items, COPY_PATH_ID, &copy_path_item, &edit_menu, 8);
    register_item(&mut items, COPY_FILENAME_ID, &copy_filename_item, &edit_menu, 9);
    register_item(&mut items, SETTINGS_ID, &settings_item, &edit_menu, 11);

    // View menu positions: full(0), brief(1), sep(2), hidden(3), sort(4), sep(5),
    // switch(6), swap(7), sep(8), palette(9)
    register_item(&mut items, SWITCH_PANE_ID, &switch_pane_item, &view_submenu, 6);
    register_item(&mut items, SWAP_PANES_ID, &swap_panes_item, &view_submenu, 7);
    register_item(&mut items, COMMAND_PALETTE_ID, &command_palette_item, &view_submenu, 9);

    // Go menu positions: back(0), forward(1), sep(2), parent(3)
    register_item(&mut items, GO_BACK_ID, &go_back_item, &go_menu, 0);
    register_item(&mut items, GO_FORWARD_ID, &go_forward_item, &go_menu, 1);
    register_item(&mut items, GO_PARENT_ID, &go_parent_item, &go_menu, 3);

    // Tab menu positions: new(0), close(1), sep(2), next(3), prev(4), sep(5), pin(6), close_others(7)
    register_item(&mut items, NEW_TAB_ID, &new_tab_item, &tab_menu, 0);
    register_item(&mut items, CLOSE_TAB_ID, &close_tab_item, &tab_menu, 1);
    register_item(&mut items, NEXT_TAB_ID, &next_tab_item, &tab_menu, 3);
    register_item(&mut items, PREV_TAB_ID, &prev_tab_item, &tab_menu, 4);
    register_item(&mut items, CLOSE_OTHER_TABS_ID, &close_other_tabs_item, &tab_menu, 7);

    // Help menu: about(0)
    register_item(&mut items, ABOUT_ID, &about_item, &help_menu, 0);

    Ok(MenuItems {
        menu,
        show_hidden_files: show_hidden_item,
        view_mode_full: view_mode_full_item,
        view_mode_brief: view_mode_brief_item,
        view_submenu,
        view_mode_full_position: view_full_pos,
        view_mode_brief_position: view_brief_pos,
        pin_tab: pin_tab_item,
        items,
        sort_submenu,
    })
}
