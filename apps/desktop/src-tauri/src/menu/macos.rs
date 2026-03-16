use std::collections::HashMap;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSImage, NSMenuItem as NSMenuItemAppKit};
use objc2_foundation::NSString;
use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

use super::{
    ABOUT_ID, CLOSE_OTHER_TABS_ID, CLOSE_TAB_ID, COMMAND_PALETTE_ID, COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID,
    EDIT_COPY_ID, EDIT_CUT_ID, EDIT_ID, EDIT_PASTE_ID, EDIT_PASTE_MOVE_ID, ENTER_LICENSE_KEY_ID, FILE_COPY_ID,
    FILE_DELETE_ID, FILE_DELETE_PERMANENTLY_ID, FILE_MOVE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GET_INFO_ID,
    GO_BACK_ID, GO_FORWARD_ID, GO_PARENT_ID, MenuItems, NEW_TAB_ID, NEXT_TAB_ID, OPEN_ID, PIN_TAB_MENU_ID, PREV_TAB_ID,
    QUICK_LOOK_ID, RENAME_ID, SEARCH_FILES_ID, SELECT_ALL_ID, SETTINGS_ID, SHOW_HIDDEN_FILES_ID, SHOW_IN_FINDER_ID,
    SWAP_PANES_ID, SWITCH_PANE_ID, VIEW_MODE_BRIEF_ID, VIEW_MODE_FULL_ID, ViewMode, build_sort_submenu,
    copy_path_accelerator, register_item, show_in_file_manager_accelerator, show_in_file_manager_label,
};

pub(crate) fn build_menu_macos<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
) -> tauri::Result<MenuItems<R>> {
    let menu = Menu::new(app)?;

    // --- cmdr app menu ---
    let about_item = MenuItem::with_id(app, ABOUT_ID, "About cmdr", true, None::<&str>)?;
    let license_label = if has_existing_license {
        "See license details..."
    } else {
        "Enter license key..."
    };
    let license_item = MenuItem::with_id(app, ENTER_LICENSE_KEY_ID, license_label, true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, SETTINGS_ID, "Settings...", true, Some("Cmd+,"))?;

    let app_menu = Submenu::with_items(
        app,
        "cmdr",
        true,
        &[
            &about_item,
            &license_item,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    menu.append(&app_menu)?;

    // --- File menu ---
    let open_item = MenuItem::with_id(app, OPEN_ID, "Open", true, None::<&str>)?;
    let file_view_item = MenuItem::with_id(app, FILE_VIEW_ID, "View", true, Some("F3"))?;
    let edit_item = MenuItem::with_id(app, EDIT_ID, "Edit in editor", true, Some("F4"))?;
    let file_copy_item = MenuItem::with_id(app, FILE_COPY_ID, "Copy...", true, Some("F5"))?;
    let file_move_item = MenuItem::with_id(app, FILE_MOVE_ID, "Move...", true, Some("F6"))?;
    let file_new_folder_item = MenuItem::with_id(app, FILE_NEW_FOLDER_ID, "New folder", true, Some("F7"))?;
    let file_delete_item = MenuItem::with_id(app, FILE_DELETE_ID, "Delete", true, Some("F8"))?;
    let file_delete_permanently_item = MenuItem::with_id(
        app,
        FILE_DELETE_PERMANENTLY_ID,
        "Delete permanently",
        true,
        Some("Shift+F8"),
    )?;
    let rename_item = MenuItem::with_id(app, RENAME_ID, "Rename", true, Some("F2"))?;
    let show_in_finder_item = MenuItem::with_id(
        app,
        SHOW_IN_FINDER_ID,
        show_in_file_manager_label(),
        true,
        Some(show_in_file_manager_accelerator()),
    )?;
    let get_info_item = MenuItem::with_id(app, GET_INFO_ID, "Get info", true, Some("Cmd+I"))?;
    let quick_look_item = MenuItem::with_id(app, QUICK_LOOK_ID, "Quick look", true, Some("Space"))?;

    let file_menu = Submenu::with_items(
        app,
        "File",
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
    let edit_cut_item = MenuItem::with_id(app, EDIT_CUT_ID, "Cut", true, Some("Cmd+X"))?;
    let edit_copy_item = MenuItem::with_id(app, EDIT_COPY_ID, "Copy", true, Some("Cmd+C"))?;
    let edit_paste_item = MenuItem::with_id(app, EDIT_PASTE_ID, "Paste", true, Some("Cmd+V"))?;
    let edit_paste_move_item = MenuItem::with_id(app, EDIT_PASTE_MOVE_ID, "Move here", true, Some("Alt+Cmd+V"))?;
    let select_all_item = MenuItem::with_id(app, SELECT_ALL_ID, "Select all", true, Some("Cmd+A"))?;
    let deselect_all_item = MenuItem::with_id(app, DESELECT_ALL_ID, "Deselect all", true, Some("Cmd+Shift+A"))?;
    let copy_path_item = MenuItem::with_id(app, COPY_PATH_ID, "Copy path", true, Some(copy_path_accelerator()))?;
    let copy_filename_item = MenuItem::with_id(app, COPY_FILENAME_ID, "Copy filename", true, None::<&str>)?;
    let search_files_item = MenuItem::with_id(app, SEARCH_FILES_ID, "Search files", true, Some("Cmd+F"))?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
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
            &search_files_item,
        ],
    )?;
    menu.append(&edit_menu)?;

    // --- View menu ---
    let view_mode_full_item = CheckMenuItem::with_id(
        app,
        VIEW_MODE_FULL_ID,
        "Full view",
        true,
        view_mode == ViewMode::Full,
        Some("Cmd+1"),
    )?;
    let view_mode_brief_item = CheckMenuItem::with_id(
        app,
        VIEW_MODE_BRIEF_ID,
        "Brief view",
        true,
        view_mode == ViewMode::Brief,
        Some("Cmd+2"),
    )?;
    let show_hidden_item = CheckMenuItem::with_id(
        app,
        SHOW_HIDDEN_FILES_ID,
        "Show hidden files",
        true,
        show_hidden_files,
        Some("Cmd+Shift+."),
    )?;
    let sort_submenu = build_sort_submenu(app, "Sort by")?;
    let switch_pane_item = MenuItem::with_id(app, SWITCH_PANE_ID, "Switch pane", true, Some("Tab"))?;
    let swap_panes_item = MenuItem::with_id(app, SWAP_PANES_ID, "Swap panes", true, Some("Cmd+U"))?;
    let command_palette_item =
        MenuItem::with_id(app, COMMAND_PALETTE_ID, "Command palette...", true, Some("Cmd+Shift+P"))?;

    let view_submenu = Submenu::with_items(
        app,
        "View",
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
    let go_back_item = MenuItem::with_id(app, GO_BACK_ID, "Back", true, Some("Cmd+["))?;
    let go_forward_item = MenuItem::with_id(app, GO_FORWARD_ID, "Forward", true, Some("Cmd+]"))?;
    let go_parent_item = MenuItem::with_id(app, GO_PARENT_ID, "Parent folder", true, Some("Cmd+Up"))?;

    let go_menu = Submenu::with_items(
        app,
        "Go",
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
    let new_tab_item = MenuItem::with_id(app, NEW_TAB_ID, "New tab", true, Some("Cmd+T"))?;
    let close_tab_item = MenuItem::with_id(app, CLOSE_TAB_ID, "Close tab", true, Some("Cmd+W"))?;
    let next_tab_item = MenuItem::with_id(app, NEXT_TAB_ID, "Next tab", true, Some("Ctrl+Tab"))?;
    let prev_tab_item = MenuItem::with_id(app, PREV_TAB_ID, "Previous tab", true, Some("Ctrl+Shift+Tab"))?;
    let pin_tab_item = MenuItem::with_id(app, PIN_TAB_MENU_ID, "Pin tab", true, None::<&str>)?;
    let close_other_tabs_item = MenuItem::with_id(app, CLOSE_OTHER_TABS_ID, "Close other tabs", true, None::<&str>)?;

    let tab_menu = Submenu::with_items(
        app,
        "Tab",
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

    // --- Window menu ---
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    menu.append(&window_menu)?;

    // --- Help menu ---
    // macOS auto-adds a search field to any menu named "Help"
    let help_menu = Submenu::with_items(app, "Help", true, &[])?;
    menu.append(&help_menu)?;

    // --- Populate items HashMap for accelerator updates ---
    let mut items = HashMap::new();

    // File menu positions: open(0), view(1), edit(2), sep(3), copy(4), move(5),
    // new_folder(6), delete(7), delete_perm(8), sep(9), rename(10), sep(11),
    // show_in_finder(12), get_info(13), quick_look(14)
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
    register_item(&mut items, SHOW_IN_FINDER_ID, &show_in_finder_item, &file_menu, 12);
    register_item(&mut items, GET_INFO_ID, &get_info_item, &file_menu, 13);
    register_item(&mut items, QUICK_LOOK_ID, &quick_look_item, &file_menu, 14);

    // Edit menu positions: undo(0), redo(1), sep(2), cut(3), copy(4), paste(5), move_here(6),
    // sep(7), select_all(8), deselect_all(9), sep(10), copy_path(11), copy_filename(12),
    // sep(13), search_files(14)
    register_item(&mut items, EDIT_CUT_ID, &edit_cut_item, &edit_menu, 3);
    register_item(&mut items, EDIT_COPY_ID, &edit_copy_item, &edit_menu, 4);
    register_item(&mut items, EDIT_PASTE_ID, &edit_paste_item, &edit_menu, 5);
    register_item(&mut items, EDIT_PASTE_MOVE_ID, &edit_paste_move_item, &edit_menu, 6);
    register_item(&mut items, SELECT_ALL_ID, &select_all_item, &edit_menu, 8);
    register_item(&mut items, DESELECT_ALL_ID, &deselect_all_item, &edit_menu, 9);
    register_item(&mut items, COPY_PATH_ID, &copy_path_item, &edit_menu, 11);
    register_item(&mut items, COPY_FILENAME_ID, &copy_filename_item, &edit_menu, 12);
    register_item(&mut items, SEARCH_FILES_ID, &search_files_item, &edit_menu, 14);

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

pub(crate) fn cleanup_macos_menus() {
    // This runs during Tauri's setup() which is inside tao's `did_finish_launching`
    // — an `extern "C"` callback that aborts on panic. NSMenu operations can raise ObjC
    // exceptions (which are foreign exceptions that `catch_unwind` can't catch), so we
    // use `objc2::exception::catch` to absorb them gracefully.
    let result = objc2::exception::catch(cleanup_macos_menus_inner);
    if let Err(e) = result {
        log::warn!("Failed to clean up macOS menus: {e:?}");
    }
}

fn cleanup_macos_menus_inner() {
    // This runs during Tauri setup on the main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = app.mainMenu() else {
        return;
    };

    // Titles of system-injected items we want to remove from the Edit menu.
    let unwanted_titles: &[&str] = &[
        "Writing Tools",
        "AutoFill",
        "Start Dictation\u{2026}", // macOS uses Unicode ellipsis (U+2026)
        "Start Dictation...",
        "Emoji & Symbols",
    ];

    // Walk top-level menus looking for "Edit" and "Help"
    let count = main_menu.numberOfItems();
    for i in 0..count {
        let Some(top_item) = main_menu.itemAtIndex(i) else {
            continue;
        };
        let Some(submenu) = top_item.submenu() else {
            continue;
        };
        let title = submenu.title().to_string();

        if title == "Edit" {
            // Remove system-injected items by walking backwards. We use a manual index
            // instead of a range because each removal shifts indices — the loop must
            // re-check against the live count after every removal.
            let mut j = submenu.numberOfItems() - 1;
            while j >= 0 {
                if let Some(item) = submenu.itemAtIndex(j) {
                    let item_title = item.title().to_string();
                    if unwanted_titles.contains(&item_title.as_str()) {
                        submenu.removeItemAtIndex(j);
                        // Also remove a preceding separator if present
                        if j > 0
                            && let Some(prev) = submenu.itemAtIndex(j - 1)
                            && prev.isSeparatorItem()
                        {
                            submenu.removeItemAtIndex(j - 1);
                            j -= 1; // account for the extra removal
                        }
                    }
                }
                j -= 1;
            }

            // Clean up any trailing separator left at the bottom
            let final_count = submenu.numberOfItems();
            if final_count > 0
                && let Some(last) = submenu.itemAtIndex(final_count - 1)
                && last.isSeparatorItem()
            {
                submenu.removeItemAtIndex(final_count - 1);
            }
        } else if title == "Help" {
            // Register as the app's Help menu so macOS adds the search field
            app.setHelpMenu(Some(&submenu));
        }
    }
}

pub(crate) fn set_macos_menu_icons() {
    let result = objc2::exception::catch(set_macos_menu_icons_inner);
    if let Err(e) = result {
        log::warn!("Failed to set macOS menu icons: {e:?}");
    }
}

fn set_macos_menu_icons_inner() {
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    let Some(main_menu) = app.mainMenu() else {
        return;
    };

    let count = main_menu.numberOfItems();
    for i in 0..count {
        let Some(top_item) = main_menu.itemAtIndex(i) else {
            continue;
        };
        let Some(submenu) = top_item.submenu() else {
            continue;
        };
        let title = submenu.title().to_string();

        let mappings: &[(&str, &str)] = match title.as_str() {
            "cmdr" => &[
                ("Enter license key\u{2026}", "key"),
                ("See license details\u{2026}", "key"),
                ("Settings\u{2026}", "gearshape"),
            ],
            "File" => &[
                ("Open", "arrow.up.forward"),
                ("View", "document"),
                ("Edit in editor", "pencil"),
                ("Copy\u{2026}", "document.on.document"),
                ("Move\u{2026}", "folder"),
                ("New folder", "folder.badge.plus"),
                ("Delete", "trash"),
                ("Delete permanently", "trash.slash"),
                ("Rename", "character.cursor.ibeam"),
                ("Show in Finder", "arrow.forward.circle"),
                ("Get info", "info.circle"),
                ("Quick look", "eye"),
            ],
            "Edit" => &[
                ("Cut", "scissors"),
                ("Copy", "document.on.document"),
                ("Paste", "clipboard"),
                ("Move here", "document.on.clipboard"),
                ("Select all", "checkmark.circle"),
                ("Deselect all", "circle"),
                ("Copy path", "link"),
                ("Copy filename", "textformat"),
                ("Search files", "magnifyingglass"),
            ],
            "View" => {
                // Also apply icons to the "Sort by" submenu items
                apply_sf_symbols_to_nested_submenu(
                    &submenu,
                    "Sort by",
                    &[
                        ("Name", "textformat.alt"),
                        ("Extension", "character.textbox"),
                        ("Size", "ruler"),
                        ("Date modified", "clock"),
                        ("Date created", "calendar"),
                        ("Ascending", "chevron.up"),
                        ("Descending", "chevron.down"),
                    ],
                );

                &[
                    ("Switch pane", "rectangle.2.swap"),
                    ("Swap panes", "arrow.left.arrow.right"),
                    ("Command palette\u{2026}", "command"),
                ]
            }
            "Go" => &[
                ("Back", "chevron.left"),
                ("Forward", "chevron.right"),
                ("Parent folder", "arrow.up"),
            ],
            "Tab" => &[
                ("New tab", "plus"),
                ("Close tab", "xmark"),
                ("Next tab", "arrow.right"),
                ("Previous tab", "arrow.left"),
                ("Pin tab", "pin"),
                ("Close other tabs", "xmark.circle"),
            ],
            _ => continue,
        };

        apply_sf_symbols_to_submenu(&submenu, mappings);
    }
}

fn apply_sf_symbols_to_submenu(submenu: &objc2_app_kit::NSMenu, mappings: &[(&str, &str)]) {
    let item_count = submenu.numberOfItems();
    for j in 0..item_count {
        let Some(item) = submenu.itemAtIndex(j) else {
            continue;
        };
        if item.isSeparatorItem() {
            continue;
        }
        let item_title = item.title().to_string();
        for &(title, symbol_name) in mappings {
            if item_title == title {
                set_sf_symbol(&item, symbol_name);
                break;
            }
        }
    }
}

fn apply_sf_symbols_to_nested_submenu(parent: &objc2_app_kit::NSMenu, submenu_title: &str, mappings: &[(&str, &str)]) {
    let count = parent.numberOfItems();
    for i in 0..count {
        let Some(item) = parent.itemAtIndex(i) else {
            continue;
        };
        if let Some(child_menu) = item.submenu()
            && child_menu.title().to_string() == submenu_title
        {
            apply_sf_symbols_to_submenu(&child_menu, mappings);
            return;
        }
    }
}

fn set_sf_symbol(item: &NSMenuItemAppKit, symbol_name: &str) {
    let name = NSString::from_str(symbol_name);
    if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&name, None) {
        item.setImage(Some(&image));
    } else {
        log::warn!("SF Symbol not found: {symbol_name}");
    }
}
