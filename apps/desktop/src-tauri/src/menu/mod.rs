//! Application menu configuration.

use crate::ignore_poison::IgnorePoison;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{
    AppHandle, Runtime, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::NSApplication;

/// Menu item IDs for file actions.
pub const SHOW_HIDDEN_FILES_ID: &str = "show_hidden_files";
pub const VIEW_MODE_FULL_ID: &str = "view_mode_full";
pub const VIEW_MODE_BRIEF_ID: &str = "view_mode_brief";
pub const OPEN_ID: &str = "open";
pub const EDIT_ID: &str = "edit";
pub const FILE_VIEW_ID: &str = "file_view";
pub const FILE_COPY_ID: &str = "file_copy";
pub const FILE_MOVE_ID: &str = "file_move";
pub const FILE_NEW_FOLDER_ID: &str = "file_new_folder";
pub const FILE_DELETE_ID: &str = "file_delete";
pub const FILE_DELETE_PERMANENTLY_ID: &str = "file_delete_permanently";
pub const SHOW_IN_FINDER_ID: &str = "show_in_finder";
pub const COPY_PATH_ID: &str = "copy_path";
pub const COPY_FILENAME_ID: &str = "copy_filename";
pub const GET_INFO_ID: &str = "get_info";
pub const QUICK_LOOK_ID: &str = "quick_look";
pub const RENAME_ID: &str = "rename";
pub const SELECT_ALL_ID: &str = "select_all_files";
pub const DESELECT_ALL_ID: &str = "deselect_all";

/// Menu item IDs for clipboard operations (Edit menu).
pub const EDIT_CUT_ID: &str = "edit_cut";
pub const EDIT_COPY_ID: &str = "edit_copy";
pub const EDIT_PASTE_ID: &str = "edit_paste";
pub const EDIT_PASTE_MOVE_ID: &str = "edit_paste_move";

/// Menu item ID for command palette.
pub const COMMAND_PALETTE_ID: &str = "command_palette";

/// Menu item ID for Switch Pane.
pub const SWITCH_PANE_ID: &str = "switch_pane";

/// Menu item ID for Swap Panes.
pub const SWAP_PANES_ID: &str = "swap_panes";

/// Menu item IDs for navigation (Go menu).
pub const GO_BACK_ID: &str = "go_back";
pub const GO_FORWARD_ID: &str = "go_forward";
pub const GO_PARENT_ID: &str = "go_parent";

/// Menu item IDs for sorting.
pub const SORT_BY_NAME_ID: &str = "sort_by_name";
pub const SORT_BY_EXTENSION_ID: &str = "sort_by_extension";
pub const SORT_BY_SIZE_ID: &str = "sort_by_size";
pub const SORT_BY_MODIFIED_ID: &str = "sort_by_modified";
pub const SORT_BY_CREATED_ID: &str = "sort_by_created";
pub const SORT_ASCENDING_ID: &str = "sort_ascending";
pub const SORT_DESCENDING_ID: &str = "sort_descending";

/// Whether a command requires the main window to be focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandScope {
    /// Always emit regardless of window focus (Settings, About, Command palette, etc.)
    App,
    /// Only emit when the main window is focused (file operations, navigation, etc.)
    FileScoped,
}

/// Maps a menu item ID to its command registry ID and scope.
/// Returns `None` for items handled specially (CheckMenuItems, close-tab, viewer word wrap,
/// tab context menu, context menu file actions, sort items).
pub fn menu_id_to_command(menu_id: &str) -> Option<(&'static str, CommandScope)> {
    match menu_id {
        // App-level commands (always emit)
        ABOUT_ID => Some(("app.about", CommandScope::App)),
        ENTER_LICENSE_KEY_ID => Some(("app.licenseKey", CommandScope::App)),
        SETTINGS_ID => Some(("app.settings", CommandScope::App)),
        COMMAND_PALETTE_ID => Some(("app.commandPalette", CommandScope::FileScoped)),

        // Pane commands (file-scoped)
        SWITCH_PANE_ID => Some(("pane.switch", CommandScope::FileScoped)),
        SWAP_PANES_ID => Some(("pane.swap", CommandScope::FileScoped)),

        // Navigation commands (file-scoped)
        GO_BACK_ID => Some(("nav.back", CommandScope::FileScoped)),
        GO_FORWARD_ID => Some(("nav.forward", CommandScope::FileScoped)),
        GO_PARENT_ID => Some(("nav.parent", CommandScope::FileScoped)),

        // Tab commands (file-scoped)
        NEW_TAB_ID => Some(("tab.new", CommandScope::FileScoped)),
        CLOSE_TAB_ID => Some(("tab.close", CommandScope::FileScoped)),
        NEXT_TAB_ID => Some(("tab.next", CommandScope::FileScoped)),
        PREV_TAB_ID => Some(("tab.prev", CommandScope::FileScoped)),
        PIN_TAB_MENU_ID => Some(("tab.togglePin", CommandScope::FileScoped)),
        CLOSE_OTHER_TABS_ID => Some(("tab.closeOthers", CommandScope::FileScoped)),

        // Clipboard operations (App scope — text clipboard must work in all windows;
        // the frontend's activeElement check routes between text and file clipboard)
        EDIT_CUT_ID => Some(("edit.cut", CommandScope::App)),
        EDIT_COPY_ID => Some(("edit.copy", CommandScope::App)),
        EDIT_PASTE_ID => Some(("edit.paste", CommandScope::App)),
        EDIT_PASTE_MOVE_ID => Some(("edit.pasteAsMove", CommandScope::App)),

        // File operations (file-scoped)
        OPEN_ID => Some(("nav.open", CommandScope::FileScoped)),
        RENAME_ID => Some(("file.rename", CommandScope::FileScoped)),
        EDIT_ID => Some(("file.edit", CommandScope::FileScoped)),
        FILE_VIEW_ID => Some(("file.view", CommandScope::FileScoped)),
        FILE_COPY_ID => Some(("file.copy", CommandScope::FileScoped)),
        FILE_MOVE_ID => Some(("file.move", CommandScope::FileScoped)),
        FILE_NEW_FOLDER_ID => Some(("file.newFolder", CommandScope::FileScoped)),
        FILE_DELETE_ID => Some(("file.delete", CommandScope::FileScoped)),
        FILE_DELETE_PERMANENTLY_ID => Some(("file.deletePermanently", CommandScope::FileScoped)),
        SHOW_IN_FINDER_ID => Some(("file.showInFinder", CommandScope::FileScoped)),
        COPY_PATH_ID => Some(("file.copyPath", CommandScope::FileScoped)),
        COPY_FILENAME_ID => Some(("file.copyFilename", CommandScope::FileScoped)),
        GET_INFO_ID => Some(("file.getInfo", CommandScope::FileScoped)),
        QUICK_LOOK_ID => Some(("file.quickLook", CommandScope::FileScoped)),
        SELECT_ALL_ID => Some(("selection.selectAll", CommandScope::FileScoped)),
        DESELECT_ALL_ID => Some(("selection.deselectAll", CommandScope::FileScoped)),

        // Not mapped: CheckMenuItems (show_hidden_files, view modes), close-tab (special logic),
        // viewer word wrap, tab context menu actions, sort items
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
        "pane.switch" => Some(SWITCH_PANE_ID),
        "pane.swap" => Some(SWAP_PANES_ID),
        "nav.back" => Some(GO_BACK_ID),
        "nav.forward" => Some(GO_FORWARD_ID),
        "nav.parent" => Some(GO_PARENT_ID),
        "tab.new" => Some(NEW_TAB_ID),
        "tab.close" => Some(CLOSE_TAB_ID),
        "tab.next" => Some(NEXT_TAB_ID),
        "tab.prev" => Some(PREV_TAB_ID),
        "tab.togglePin" => Some(PIN_TAB_MENU_ID),
        "tab.closeOthers" => Some(CLOSE_OTHER_TABS_ID),
        "file.rename" => Some(RENAME_ID),
        "file.edit" => Some(EDIT_ID),
        "file.view" => Some(FILE_VIEW_ID),
        "file.copy" => Some(FILE_COPY_ID),
        "file.move" => Some(FILE_MOVE_ID),
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
        "edit.cut" => Some(EDIT_CUT_ID),
        "edit.copy" => Some(EDIT_COPY_ID),
        "edit.paste" => Some(EDIT_PASTE_ID),
        "edit.pasteAsMove" => Some(EDIT_PASTE_MOVE_ID),
        _ => None,
    }
}

/// Context for the current menu selection.
#[derive(Clone, Default)]
pub struct MenuContext {
    pub path: String,
    pub filename: String,
}

/// A menu item tracked for accelerator updates, with its parent submenu and position.
pub struct MenuItemEntry<R: Runtime> {
    pub item: MenuItem<R>,
    pub submenu: Submenu<R>,
    pub position: usize,
}

/// Stores references to menu items and current context.
pub struct MenuState<R: Runtime> {
    pub show_hidden_files: Mutex<Option<CheckMenuItem<R>>>,
    pub view_mode_full: Mutex<Option<CheckMenuItem<R>>>,
    pub view_mode_brief: Mutex<Option<CheckMenuItem<R>>>,
    pub context: Mutex<MenuContext>,
    /// Reference to the View submenu for accelerator updates
    pub view_submenu: Mutex<Option<Submenu<R>>>,
    /// Positions of items in View submenu (for reinsertion after accelerator updates)
    pub view_mode_full_position: Mutex<usize>,
    pub view_mode_brief_position: Mutex<usize>,
    /// Pin/unpin tab menu item (label toggles based on active tab state)
    pub pin_tab: Mutex<Option<MenuItem<R>>>,
    /// Generic menu items keyed by menu item ID, for accelerator and enable/disable updates.
    pub items: Mutex<HashMap<String, MenuItemEntry<R>>>,
    /// Sort by submenu (disabled when not in explorer context)
    pub sort_submenu: Mutex<Option<Submenu<R>>>,
}

impl<R: Runtime> Default for MenuState<R> {
    fn default() -> Self {
        Self {
            show_hidden_files: Mutex::new(None),
            view_mode_full: Mutex::new(None),
            view_mode_brief: Mutex::new(None),
            context: Mutex::new(MenuContext::default()),
            view_submenu: Mutex::new(None),
            view_mode_full_position: Mutex::new(0),
            view_mode_brief_position: Mutex::new(0),
            pin_tab: Mutex::new(None),
            items: Mutex::new(HashMap::new()),
            sort_submenu: Mutex::new(None),
        }
    }
}

/// Result struct for menu items that need to be stored.
pub struct MenuItems<R: Runtime> {
    pub menu: Menu<R>,
    pub show_hidden_files: CheckMenuItem<R>,
    pub view_mode_full: CheckMenuItem<R>,
    pub view_mode_brief: CheckMenuItem<R>,
    /// Reference to View submenu for accelerator updates
    pub view_submenu: Submenu<R>,
    /// Position of Full view item in View submenu
    pub view_mode_full_position: usize,
    /// Position of Brief view item in View submenu
    pub view_mode_brief_position: usize,
    /// Pin/unpin tab menu item (label updated dynamically by frontend)
    pub pin_tab: MenuItem<R>,
    /// Generic menu items for accelerator updates, keyed by menu item ID.
    pub items: HashMap<String, MenuItemEntry<R>>,
    /// Sort by submenu (disabled when not in explorer context)
    pub sort_submenu: Submenu<R>,
}

/// View mode type that matches the frontend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    Full,
    #[default]
    Brief,
}

/// Menu item ID for viewer word wrap toggle.
pub const VIEWER_WORD_WRAP_ID: &str = "viewer_word_wrap";

/// Menu item IDs for tab actions (app menu).
pub const NEW_TAB_ID: &str = "new_tab";
pub const PIN_TAB_MENU_ID: &str = "pin_tab_menu";
pub const CLOSE_TAB_ID: &str = "close_tab";
pub const NEXT_TAB_ID: &str = "next_tab";
pub const PREV_TAB_ID: &str = "prev_tab";
pub const CLOSE_OTHER_TABS_ID: &str = "close_other_tabs";

/// Menu item IDs for tab context menu.
pub const TAB_PIN_ID: &str = "tab_pin";
pub const TAB_CLOSE_OTHERS_ID: &str = "tab_close_others";
pub const TAB_CLOSE_ID: &str = "tab_close";

/// Menu item ID for About window.
pub const ABOUT_ID: &str = "about";

/// Menu item ID for Enter License Key.
pub const ENTER_LICENSE_KEY_ID: &str = "enter_license_key";

/// Menu item ID for Settings.
pub const SETTINGS_ID: &str = "settings";

/// Platform-aware accelerator for "Copy path to clipboard".
/// On macOS: Ctrl+Cmd+C. On Linux: Ctrl+Shift+C (Ctrl+Cmd+C becomes Ctrl+Ctrl+C which is broken).
#[cfg(target_os = "macos")]
fn copy_path_accelerator() -> &'static str {
    "Ctrl+Cmd+C"
}

#[cfg(not(target_os = "macos"))]
fn copy_path_accelerator() -> &'static str {
    "Ctrl+Shift+C"
}

/// Platform-aware accelerator for "Show in Finder / file manager".
#[cfg(target_os = "macos")]
fn show_in_file_manager_accelerator() -> &'static str {
    "Opt+Cmd+O"
}

#[cfg(not(target_os = "macos"))]
fn show_in_file_manager_accelerator() -> &'static str {
    "Alt+Ctrl+O"
}

/// Platform-aware label for the "Show in Finder" / "Show in file manager" action.
#[cfg(target_os = "macos")]
fn show_in_file_manager_label() -> &'static str {
    "Show in Finder"
}

#[cfg(not(target_os = "macos"))]
fn show_in_file_manager_label() -> &'static str {
    "Show in &file manager"
}

/// Builds the application menu for the current platform.
pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
) -> tauri::Result<MenuItems<R>> {
    #[cfg(target_os = "macos")]
    {
        build_menu_macos(app, show_hidden_files, view_mode, has_existing_license)
    }

    #[cfg(not(target_os = "macos"))]
    {
        build_menu_linux(app, show_hidden_files, view_mode, has_existing_license)
    }
}

/// Removes macOS system-injected items from the Edit menu and registers the Help menu.
///
/// macOS AppKit automatically injects "Writing Tools", "AutoFill", "Start Dictation...",
/// and "Emoji & Symbols" into any menu named "Edit". It also only shows the Help menu
/// search field when a menu is registered via `NSApplication.setHelpMenu:`. Both of these
/// happen at the AppKit level regardless of how the menu is constructed — so we fix them
/// post-construction via native API calls.
#[cfg(target_os = "macos")]
pub fn cleanup_macos_menus() {
    // This runs during Tauri's setup() which is inside tao's `did_finish_launching`
    // — an `extern "C"` callback that aborts on panic. NSMenu operations can raise ObjC
    // exceptions (which are foreign exceptions that `catch_unwind` can't catch), so we
    // use `objc2::exception::catch` to absorb them gracefully.
    let result = objc2::exception::catch(cleanup_macos_menus_inner);
    if let Err(e) = result {
        log::warn!("Failed to clean up macOS menus: {e:?}");
    }
}

#[cfg(target_os = "macos")]
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

/// Builds the Sort by submenu (shared between macOS and Linux).
fn build_sort_submenu<R: Runtime>(app: &AppHandle<R>, label: &str) -> tauri::Result<Submenu<R>> {
    let sort_by_name = MenuItem::with_id(app, SORT_BY_NAME_ID, "Name", true, None::<&str>)?;
    let sort_by_ext = MenuItem::with_id(app, SORT_BY_EXTENSION_ID, "Extension", true, None::<&str>)?;
    let sort_by_size = MenuItem::with_id(app, SORT_BY_SIZE_ID, "Size", true, None::<&str>)?;
    let sort_by_modified = MenuItem::with_id(app, SORT_BY_MODIFIED_ID, "Date modified", true, None::<&str>)?;
    let sort_by_created = MenuItem::with_id(app, SORT_BY_CREATED_ID, "Date created", true, None::<&str>)?;
    let sort_asc = MenuItem::with_id(app, SORT_ASCENDING_ID, "Ascending", true, None::<&str>)?;
    let sort_desc = MenuItem::with_id(app, SORT_DESCENDING_ID, "Descending", true, None::<&str>)?;

    Submenu::with_items(
        app,
        label,
        true,
        &[
            &sort_by_name,
            &sort_by_ext,
            &sort_by_size,
            &sort_by_modified,
            &sort_by_created,
            &PredefinedMenuItem::separator(app)?,
            &sort_asc,
            &sort_desc,
        ],
    )
}

/// Registers a regular MenuItem in the items HashMap for accelerator updates.
fn register_item<R: Runtime>(
    items: &mut HashMap<String, MenuItemEntry<R>>,
    id: &str,
    item: &MenuItem<R>,
    submenu: &Submenu<R>,
    position: usize,
) {
    items.insert(
        id.to_string(),
        MenuItemEntry {
            item: item.clone(),
            submenu: submenu.clone(),
            position,
        },
    );
}

/// macOS menu: builds all menus from scratch (no `Menu::default()` patching).
#[cfg(target_os = "macos")]
fn build_menu_macos<R: Runtime>(
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
    // sep(7), select_all(8), deselect_all(9), sep(10), copy_path(11), copy_filename(12)
    register_item(&mut items, EDIT_CUT_ID, &edit_cut_item, &edit_menu, 3);
    register_item(&mut items, EDIT_COPY_ID, &edit_copy_item, &edit_menu, 4);
    register_item(&mut items, EDIT_PASTE_ID, &edit_paste_item, &edit_menu, 5);
    register_item(&mut items, EDIT_PASTE_MOVE_ID, &edit_paste_move_item, &edit_menu, 6);
    register_item(&mut items, SELECT_ALL_ID, &select_all_item, &edit_menu, 8);
    register_item(&mut items, DESELECT_ALL_ID, &deselect_all_item, &edit_menu, 9);
    register_item(&mut items, COPY_PATH_ID, &copy_path_item, &edit_menu, 11);
    register_item(&mut items, COPY_FILENAME_ID, &copy_filename_item, &edit_menu, 12);

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

/// Linux menu: builds all menus from scratch, matching the macOS menu structure.
/// Differences from macOS:
/// - No cmdr app menu (Settings and license go under Edit, About under Help)
/// - "Show in file manager" instead of "Show in Finder"
/// - Function-key accelerators (F2–F8, Shift+F8) omitted — GTK intercepts them
///   before the webview, and is_focused() fails on Linux, so JS dispatch handles these
/// - Tab and Space accelerators omitted (GTK accessibility conflicts)
/// - Placeholder `&` mnemonics (first letter) — final mnemonic pass is Milestone 7
#[cfg(not(target_os = "macos"))]
fn build_menu_linux<R: Runtime>(
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

/// Builds a context menu for a specific file.
pub fn build_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    is_directory: bool,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    let open_item = MenuItem::with_id(app, OPEN_ID, "Open", true, None::<&str>)?;
    let edit_item = MenuItem::with_id(app, EDIT_ID, "Edit", true, Some("F4"))?;
    let show_in_finder_item = MenuItem::with_id(
        app,
        SHOW_IN_FINDER_ID,
        show_in_file_manager_label(),
        true,
        Some(show_in_file_manager_accelerator()),
    )?;
    let copy_path_item = MenuItem::with_id(
        app,
        COPY_PATH_ID,
        "Copy path to clipboard",
        true,
        Some(copy_path_accelerator()),
    )?;
    let copy_filename_item = MenuItem::with_id(
        app,
        COPY_FILENAME_ID,
        format!("Copy \"{}\"", filename),
        true,
        Some("Cmd+C"),
    )?;
    let rename_item = MenuItem::with_id(app, RENAME_ID, "Rename", true, Some("F2"))?;

    // Add items to menu
    if !is_directory {
        menu.append(&open_item)?;
        menu.append(&edit_item)?;
    }
    menu.append(&show_in_finder_item)?;
    menu.append(&rename_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&copy_filename_item)?;
    menu.append(&copy_path_item)?;

    // Quick Look and Get Info are macOS-only
    #[cfg(target_os = "macos")]
    {
        let get_info_item = MenuItem::with_id(app, GET_INFO_ID, "Get info", true, Some("Cmd+I"))?;
        let quick_look_item = MenuItem::with_id(app, QUICK_LOOK_ID, "Quick look", true, None::<&str>)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&get_info_item)?;
        menu.append(&quick_look_item)?;
    }

    Ok(menu)
}

/// Builds a menu for viewer windows (built from scratch on all platforms).
pub fn build_viewer_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    #[cfg(target_os = "macos")]
    {
        // --- cmdr app menu (minimal for viewer) ---
        let viewer_app_menu = Submenu::with_items(
            app,
            "cmdr",
            true,
            &[
                &PredefinedMenuItem::hide(app, None)?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::show_all(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, None)?,
            ],
        )?;
        menu.append(&viewer_app_menu)?;
    }

    // --- File menu ---
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[&PredefinedMenuItem::close_window(app, Some("Close"))?],
    )?;
    menu.append(&file_menu)?;

    // --- Edit menu ---
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    menu.append(&edit_menu)?;

    // --- View menu ---
    let word_wrap_item = CheckMenuItem::with_id(app, VIEWER_WORD_WRAP_ID, "Word wrap", true, false, None::<&str>)?;
    let view_submenu = Submenu::with_items(app, "View", true, &[&word_wrap_item])?;
    menu.append(&view_submenu)?;

    #[cfg(target_os = "macos")]
    {
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
        let help_menu = Submenu::with_items(app, "Help", true, &[])?;
        menu.append(&help_menu)?;
    }

    Ok(menu)
}

/// Convert frontend shortcut format (⌘2) to Tauri accelerator format (Cmd+2).
/// Returns None if the shortcut is empty or invalid.
pub fn frontend_shortcut_to_accelerator(shortcut: &str) -> Option<String> {
    if shortcut.is_empty() {
        return None;
    }

    let mut result = String::new();
    let mut chars = shortcut.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '⌘' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Cmd");
            }
            '⌃' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Ctrl");
            }
            '⌥' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Opt");
            }
            '⇧' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Shift");
            }
            '↑' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Up");
            }
            '↓' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Down");
            }
            '←' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Left");
            }
            '→' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Right");
            }
            _ => {
                // Regular character (letter, number, etc.)
                if !result.is_empty() {
                    result.push('+');
                }
                // Handle special key names
                let remaining: String = std::iter::once(c).chain(chars.by_ref()).collect();
                if remaining.eq_ignore_ascii_case("enter") {
                    result.push_str("Enter");
                } else if remaining.eq_ignore_ascii_case("space") {
                    result.push_str("Space");
                } else if remaining.eq_ignore_ascii_case("tab") {
                    result.push_str("Tab");
                } else if remaining.eq_ignore_ascii_case("escape") {
                    result.push_str("Escape");
                } else if remaining.eq_ignore_ascii_case("backspace") {
                    result.push_str("Backspace");
                } else if remaining.starts_with('F') || remaining.starts_with('f') {
                    // Function keys like F1, F4
                    result.push_str(&remaining.to_uppercase());
                } else if remaining.eq_ignore_ascii_case("pageup") {
                    result.push_str("PageUp");
                } else if remaining.eq_ignore_ascii_case("pagedown") {
                    result.push_str("PageDown");
                } else if remaining.eq_ignore_ascii_case("home") {
                    result.push_str("Home");
                } else if remaining.eq_ignore_ascii_case("end") {
                    result.push_str("End");
                } else {
                    // Single character or unknown - use as-is (uppercase for letters)
                    result.push_str(&remaining.to_uppercase());
                }
                break;
            }
        }
    }

    if result.is_empty() { None } else { Some(result) }
}

/// Update the accelerator for a view mode menu item.
/// Returns the new CheckMenuItem reference if successful.
pub fn update_view_mode_accelerator<R: Runtime>(
    app: &AppHandle<R>,
    menu_state: &MenuState<R>,
    is_full_mode: bool,
    new_accelerator: Option<&str>,
    is_checked: bool,
) -> tauri::Result<CheckMenuItem<R>> {
    let view_submenu_guard = menu_state.view_submenu.lock_ignore_poison();
    let view_submenu = view_submenu_guard
        .as_ref()
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;

    let (menu_item_guard, position_guard, menu_id, label) = if is_full_mode {
        (
            menu_state.view_mode_full.lock_ignore_poison(),
            menu_state.view_mode_full_position.lock_ignore_poison(),
            VIEW_MODE_FULL_ID,
            "&Full view",
        )
    } else {
        (
            menu_state.view_mode_brief.lock_ignore_poison(),
            menu_state.view_mode_brief_position.lock_ignore_poison(),
            VIEW_MODE_BRIEF_ID,
            "&Brief view",
        )
    };

    let old_item = menu_item_guard
        .as_ref()
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;
    let position = *position_guard;

    // Remove the old item
    view_submenu.remove(old_item)?;

    // Create new item with new accelerator
    let new_item = CheckMenuItem::with_id(app, menu_id, label, true, is_checked, new_accelerator)?;

    // Insert at same position
    view_submenu.insert(&new_item, position)?;

    Ok(new_item)
}

/// Update the accelerator for any menu item tracked in the items HashMap.
/// Removes the old item, creates a new one with the same ID/label/enabled state
/// but a new accelerator, and reinserts at the same position.
pub fn update_menu_item_accelerator<R: Runtime>(
    app: &AppHandle<R>,
    menu_state: &MenuState<R>,
    menu_item_id: &str,
    new_accelerator: Option<&str>,
) -> tauri::Result<()> {
    let mut items_guard = menu_state.items.lock_ignore_poison();
    let entry = items_guard
        .get(menu_item_id)
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;

    let label = entry.item.text()?;
    let enabled = entry.item.is_enabled()?;
    let submenu = entry.submenu.clone();
    let position = entry.position;

    // Remove old item, create replacement with new accelerator, reinsert
    submenu.remove(&entry.item)?;
    let new_item = MenuItem::with_id(app, menu_item_id, &label, enabled, new_accelerator)?;
    submenu.insert(&new_item, position)?;

    // Update the HashMap entry
    items_guard.insert(
        menu_item_id.to_string(),
        MenuItemEntry {
            item: new_item,
            submenu,
            position,
        },
    );

    Ok(())
}

/// Builds a context menu for a tab.
pub fn build_tab_context_menu(
    app: &AppHandle<Wry>,
    is_pinned: bool,
    can_close: bool,
    has_other_unpinned_tabs: bool,
) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;

    let pin_label = if is_pinned { "Unpin tab" } else { "Pin tab" };
    let pin_item = MenuItem::with_id(app, TAB_PIN_ID, pin_label, true, None::<&str>)?;
    let close_others_item = MenuItem::with_id(
        app,
        TAB_CLOSE_OTHERS_ID,
        "Close other tabs",
        has_other_unpinned_tabs,
        None::<&str>,
    )?;
    let close_item = MenuItem::with_id(app, TAB_CLOSE_ID, "Close tab", can_close, None::<&str>)?;

    menu.append(&pin_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&close_others_item)?;
    menu.append(&close_item)?;

    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontend_shortcut_to_accelerator_simple() {
        // Basic modifier + key combinations
        assert_eq!(frontend_shortcut_to_accelerator("⌘1"), Some("Cmd+1".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘2"), Some("Cmd+2".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘⇧P"), Some("Cmd+Shift+P".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌥⌘O"), Some("Opt+Cmd+O".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌃⌘C"), Some("Ctrl+Cmd+C".to_string()));
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_arrows() {
        assert_eq!(frontend_shortcut_to_accelerator("⌘↑"), Some("Cmd+Up".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘↓"), Some("Cmd+Down".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘["), Some("Cmd+[".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘]"), Some("Cmd+]".to_string()));
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_special_keys() {
        assert_eq!(frontend_shortcut_to_accelerator("Tab"), Some("Tab".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("Enter"), Some("Enter".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("Space"), Some("Space".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("F4"), Some("F4".to_string()));
        assert_eq!(
            frontend_shortcut_to_accelerator("Backspace"),
            Some("Backspace".to_string())
        );
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_empty() {
        assert_eq!(frontend_shortcut_to_accelerator(""), None);
    }

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
        // Command palette is FileScoped — disabled when Settings/viewer has focus
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
            menu_id_to_command(GO_BACK_ID),
            Some(("nav.back", CommandScope::FileScoped))
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
        assert_eq!(menu_id_to_command(VIEW_MODE_FULL_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_BRIEF_ID), None);
        assert_eq!(menu_id_to_command(VIEWER_WORD_WRAP_ID), None);
        assert_eq!(menu_id_to_command(SORT_BY_NAME_ID), None);
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
            "tab.new",
            "tab.close",
            "tab.next",
            "tab.prev",
            "tab.togglePin",
            "tab.closeOthers",
            "file.rename",
            "file.edit",
            "file.view",
            "file.copy",
            "file.move",
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
