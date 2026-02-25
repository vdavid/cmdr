//! Application menu configuration.

use crate::ignore_poison::IgnorePoison;
use std::sync::Mutex;
use tauri::{
    AppHandle, Runtime, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu},
};

/// Menu item IDs for file actions.
pub const SHOW_HIDDEN_FILES_ID: &str = "show_hidden_files";
pub const VIEW_MODE_FULL_ID: &str = "view_mode_full";
pub const VIEW_MODE_BRIEF_ID: &str = "view_mode_brief";
pub const OPEN_ID: &str = "open";
pub const EDIT_ID: &str = "edit";
pub const SHOW_IN_FINDER_ID: &str = "show_in_finder";
pub const COPY_PATH_ID: &str = "copy_path";
pub const COPY_FILENAME_ID: &str = "copy_filename";
pub const GET_INFO_ID: &str = "get_info";
pub const QUICK_LOOK_ID: &str = "quick_look";
pub const RENAME_ID: &str = "rename";

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

/// Context for the current menu selection.
#[derive(Clone, Default)]
pub struct MenuContext {
    pub path: String,
    pub filename: String,
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

/// Builds the application menu with default macOS items plus a custom View and File submenu enhancements.
pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
    has_existing_license: bool,
) -> tauri::Result<MenuItems<R>> {
    // Start with the default menu (includes app menu with Quit, Hide, etc.)
    let menu = Menu::default(app)?;

    // Replace the default About item with our custom one that emits an event
    // The app menu is typically the first item
    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item {
            let text = submenu.text()?;
            if text == "cmdr" || text.to_lowercase().contains("cmdr") {
                // Find and remove the default About item, add our custom one
                let about_item = MenuItem::with_id(app, ABOUT_ID, "About cmdr", true, None::<&str>)?;

                // Get all items and recreate without the default about
                let items = submenu.items()?;
                for (i, sub_item) in items.iter().enumerate() {
                    if let MenuItemKind::Predefined(pred) = sub_item {
                        // Check if this is the About item by position (typically first)
                        if i == 0 {
                            submenu.remove(pred)?;
                            submenu.insert(&about_item, 0)?;

                            // Add license menu item after About - text depends on license status
                            let license_menu_text = if has_existing_license {
                                "See license details..."
                            } else {
                                "Enter license key..."
                            };
                            let enter_license_key_item =
                                MenuItem::with_id(app, ENTER_LICENSE_KEY_ID, license_menu_text, true, None::<&str>)?;
                            submenu.insert(&enter_license_key_item, 1)?;

                            // Add separator and Settings after license key
                            let separator = PredefinedMenuItem::separator(app)?;
                            submenu.insert(&separator, 2)?;
                            let settings_item =
                                MenuItem::with_id(app, SETTINGS_ID, "Settings...", true, Some("Cmd+,"))?;
                            submenu.insert(&settings_item, 3)?;
                            break;
                        }
                    }
                }
                break;
            }
        }
    }

    // Add File menu items
    let open_item = MenuItem::with_id(app, OPEN_ID, "Open", true, None::<&str>)?;
    let edit_item = MenuItem::with_id(app, EDIT_ID, "Edit", true, Some("F4"))?;
    let show_in_finder_item = MenuItem::with_id(app, SHOW_IN_FINDER_ID, "Show in Finder", true, Some("Opt+Cmd+O"))?;
    let copy_path_item = MenuItem::with_id(app, COPY_PATH_ID, "Copy path to clipboard", true, Some("Ctrl+Cmd+C"))?;
    let copy_filename_item = MenuItem::with_id(app, COPY_FILENAME_ID, "Copy filename", true, None::<&str>)?;
    let get_info_item = MenuItem::with_id(app, GET_INFO_ID, "Get info", true, Some("Cmd+I"))?;
    let quick_look_item = MenuItem::with_id(app, QUICK_LOOK_ID, "Quick look", true, Some("Space"))?;

    // Create tab menu items
    let new_tab_item = MenuItem::with_id(app, NEW_TAB_ID, "New tab", true, Some("Cmd+T"))?;
    let pin_tab_item = MenuItem::with_id(app, PIN_TAB_MENU_ID, "Pin tab", true, None::<&str>)?;
    let close_tab_item = MenuItem::with_id(app, CLOSE_TAB_ID, "Close tab", true, Some("Cmd+W"))?;

    // Find the existing File submenu and add our items to it
    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "File"
        {
            // Remove the predefined "Close Window" item (⌘W) — replaced by "Close tab"
            let items = submenu.items()?;
            for sub_item in &items {
                if let MenuItemKind::Predefined(pred) = sub_item {
                    // The predefined Close Window item is typically the last item in the File submenu
                    // We identify it by checking if it's a predefined (non-separator) item.
                    // On macOS, the default File submenu only has "Close Window" as a predefined item.
                    if pred.text().unwrap_or_default() == "Close Window" {
                        submenu.remove(pred)?;
                        break;
                    }
                }
            }

            submenu.prepend(&PredefinedMenuItem::separator(app)?)?;
            submenu.prepend(&quick_look_item)?;
            submenu.prepend(&get_info_item)?;
            submenu.prepend(&copy_filename_item)?;
            submenu.prepend(&copy_path_item)?;
            submenu.prepend(&show_in_finder_item)?;
            submenu.prepend(&edit_item)?;
            submenu.prepend(&open_item)?;

            // Append tab items at the end of the File submenu
            submenu.append(&PredefinedMenuItem::separator(app)?)?;
            submenu.append(&new_tab_item)?;
            submenu.append(&pin_tab_item)?;
            submenu.append(&close_tab_item)?;
            break;
        }
    }

    // Add Rename to the Edit submenu
    let rename_item = MenuItem::with_id(app, RENAME_ID, "Rename", true, Some("F2"))?;
    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "Edit"
        {
            submenu.append(&PredefinedMenuItem::separator(app)?)?;
            submenu.append(&rename_item)?;
            break;
        }
    }

    // Create our Show Hidden Files toggle
    let show_hidden_item = CheckMenuItem::with_id(
        app,
        SHOW_HIDDEN_FILES_ID,
        "Show hidden files",
        true, // enabled
        show_hidden_files,
        Some("Cmd+Shift+."),
    )?;

    // Create view mode menu items (radio-style: one checked at a time)
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

    // Find the existing View submenu and add our items to it
    // The default menu on macOS has: App, File, Edit, View, Window, Help
    let mut found_view_submenu: Option<Submenu<R>> = None;
    let mut view_full_pos: usize = 0;
    let mut view_brief_pos: usize = 0;

    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "View"
        {
            // Add separator then our items
            submenu.append(&PredefinedMenuItem::separator(app)?)?;

            // Track positions of view mode items (after the separator we just added)
            let base_count = submenu.items()?.len();
            view_full_pos = base_count;
            submenu.append(&view_mode_full_item)?;
            view_brief_pos = base_count + 1;
            submenu.append(&view_mode_brief_item)?;

            submenu.append(&PredefinedMenuItem::separator(app)?)?;
            submenu.append(&show_hidden_item)?;

            // Add Sort by submenu
            let sort_by_name = MenuItem::with_id(app, SORT_BY_NAME_ID, "Name", true, None::<&str>)?;
            let sort_by_ext = MenuItem::with_id(app, SORT_BY_EXTENSION_ID, "Extension", true, None::<&str>)?;
            let sort_by_size = MenuItem::with_id(app, SORT_BY_SIZE_ID, "Size", true, None::<&str>)?;
            let sort_by_modified = MenuItem::with_id(app, SORT_BY_MODIFIED_ID, "Date modified", true, None::<&str>)?;
            let sort_by_created = MenuItem::with_id(app, SORT_BY_CREATED_ID, "Date created", true, None::<&str>)?;
            let sort_asc = MenuItem::with_id(app, SORT_ASCENDING_ID, "Ascending", true, None::<&str>)?;
            let sort_desc = MenuItem::with_id(app, SORT_DESCENDING_ID, "Descending", true, None::<&str>)?;

            let sort_submenu = Submenu::with_items(
                app,
                "Sort by",
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
            )?;
            submenu.append(&sort_submenu)?;

            // Add command palette and switch pane after separator
            submenu.append(&PredefinedMenuItem::separator(app)?)?;
            let command_palette_item =
                MenuItem::with_id(app, COMMAND_PALETTE_ID, "Command palette...", true, Some("Cmd+Shift+P"))?;
            submenu.append(&command_palette_item)?;
            let switch_pane_item = MenuItem::with_id(app, SWITCH_PANE_ID, "Switch pane", true, Some("Tab"))?;
            submenu.append(&switch_pane_item)?;
            let swap_panes_item = MenuItem::with_id(app, SWAP_PANES_ID, "Swap panes", true, Some("Cmd+U"))?;
            submenu.append(&swap_panes_item)?;

            found_view_submenu = Some(submenu);
            break;
        }
    }

    // If View menu wasn't found (unlikely), create one
    let view_submenu = if let Some(submenu) = found_view_submenu {
        submenu
    } else {
        let view_menu = Submenu::with_items(
            app,
            "View",
            true,
            &[
                &view_mode_full_item,
                &view_mode_brief_item,
                &PredefinedMenuItem::separator(app)?,
                &show_hidden_item,
            ],
        )?;
        view_full_pos = 0;
        view_brief_pos = 1;
        menu.append(&view_menu)?;
        view_menu
    };

    // Create Go menu for navigation
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

    // Insert Go menu after View (before Window)
    // Find Window menu position and insert before it
    let mut inserted = false;
    let items = menu.items()?;
    for (i, item) in items.iter().enumerate() {
        if let MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "Window"
        {
            menu.insert(&go_menu, i)?;
            inserted = true;
            break;
        }
    }
    if !inserted {
        // Fallback: append at the end
        menu.append(&go_menu)?;
    }

    Ok(MenuItems {
        menu,
        show_hidden_files: show_hidden_item,
        view_mode_full: view_mode_full_item,
        view_mode_brief: view_mode_brief_item,
        view_submenu,
        view_mode_full_position: view_full_pos,
        view_mode_brief_position: view_brief_pos,
        pin_tab: pin_tab_item,
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
    let show_in_finder_item = MenuItem::with_id(app, SHOW_IN_FINDER_ID, "Show in Finder", true, Some("Opt+Cmd+O"))?;
    let copy_path_item = MenuItem::with_id(app, COPY_PATH_ID, "Copy path to clipboard", true, Some("Ctrl+Cmd+C"))?;
    let copy_filename_item = MenuItem::with_id(
        app,
        COPY_FILENAME_ID,
        format!("Copy \"{}\"", filename),
        true,
        Some("Cmd+C"),
    )?;
    let rename_item = MenuItem::with_id(app, RENAME_ID, "Rename", true, Some("F2"))?;
    let get_info_item = MenuItem::with_id(app, GET_INFO_ID, "Get info", true, Some("Cmd+I"))?;
    let quick_look_item = MenuItem::with_id(app, QUICK_LOOK_ID, "Quick look", true, None::<&str>)?;

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
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&get_info_item)?;
    menu.append(&quick_look_item)?;

    Ok(menu)
}

/// Builds a menu for viewer windows. Starts from the default macOS menu and adds a "Word wrap" toggle to the View submenu.
pub fn build_viewer_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;

    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "View"
        {
            submenu.append(&PredefinedMenuItem::separator(app)?)?;
            let word_wrap_item =
                CheckMenuItem::with_id(app, VIEWER_WORD_WRAP_ID, "Word wrap", true, false, None::<&str>)?;
            submenu.append(&word_wrap_item)?;
            break;
        }
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
            "Full view",
        )
    } else {
        (
            menu_state.view_mode_brief.lock_ignore_poison(),
            menu_state.view_mode_brief_position.lock_ignore_poison(),
            VIEW_MODE_BRIEF_ID,
            "Brief view",
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
}
