//! Application menu configuration.

#[cfg(not(target_os = "macos"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use crate::ignore_poison::IgnorePoison;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{
    AppHandle, Runtime, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

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
        SEARCH_FILES_ID => Some(("search.open", CommandScope::FileScoped)),

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

        // Clipboard operations — cut/copy/paste are handled specially in on_menu_event
        // (native responder chain for non-main windows, execute-command for main window).
        // They're still listed here for command_id_to_menu_id reverse lookups.
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
        "search.open" => Some(SEARCH_FILES_ID),
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
        macos::build_menu_macos(app, show_hidden_files, view_mode, has_existing_license)
    }

    #[cfg(not(target_os = "macos"))]
    {
        linux::build_menu_linux(app, show_hidden_files, view_mode, has_existing_license)
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
    macos::cleanup_macos_menus();
}

/// Sets SF Symbol icons on menu items post-construction via native AppKit API.
///
/// Tauri's menu API doesn't support SF Symbols, so we walk the NSMenu hierarchy after
/// construction and call `NSImage(systemSymbolName:accessibilityDescription:)` + `setImage:`
/// on each item, matching by title within each known submenu.
#[cfg(target_os = "macos")]
pub fn set_macos_menu_icons() {
    macos::set_macos_menu_icons();
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

/// Builds a context menu for a specific file.
pub fn build_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    is_directory: bool,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    // Open / View / Edit group (files only)
    if !is_directory {
        let open_item = MenuItem::with_id(app, OPEN_ID, "Open", true, None::<&str>)?;
        let view_item = MenuItem::with_id(app, FILE_VIEW_ID, "View", true, Some("F3"))?;
        let edit_item = MenuItem::with_id(app, EDIT_ID, "Edit", true, Some("F4"))?;
        menu.append(&open_item)?;
        menu.append(&view_item)?;
        menu.append(&edit_item)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    // Copy / Move / Rename group
    let copy_item = MenuItem::with_id(app, FILE_COPY_ID, "Copy", true, Some("F5"))?;
    let move_item = MenuItem::with_id(app, FILE_MOVE_ID, "Move", true, Some("F6"))?;
    let rename_item = MenuItem::with_id(app, RENAME_ID, "Rename", true, Some("F2"))?;
    menu.append(&copy_item)?;
    menu.append(&move_item)?;
    menu.append(&rename_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // New folder
    let new_folder_item = MenuItem::with_id(app, FILE_NEW_FOLDER_ID, "New folder", true, Some("F7"))?;
    menu.append(&new_folder_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Delete
    let delete_item = MenuItem::with_id(app, FILE_DELETE_ID, "Delete", true, Some("F8"))?;
    menu.append(&delete_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Utility group: Show in Finder, Copy filename, Copy path
    let show_in_finder_item = MenuItem::with_id(
        app,
        SHOW_IN_FINDER_ID,
        show_in_file_manager_label(),
        true,
        Some(show_in_file_manager_accelerator()),
    )?;
    let copy_filename_item = MenuItem::with_id(
        app,
        COPY_FILENAME_ID,
        format!("Copy \"{}\"", filename),
        true,
        Some("Cmd+C"),
    )?;
    let copy_path_item = MenuItem::with_id(app, COPY_PATH_ID, "Copy path", true, Some(copy_path_accelerator()))?;
    menu.append(&show_in_finder_item)?;
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
            "search.open",
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
