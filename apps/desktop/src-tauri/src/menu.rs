//! Application menu configuration.

use std::sync::Mutex;
use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, Submenu},
};

/// Menu item IDs for file actions.
pub const SHOW_HIDDEN_FILES_ID: &str = "show_hidden_files";
pub const VIEW_MODE_FULL_ID: &str = "view_mode_full";
pub const VIEW_MODE_BRIEF_ID: &str = "view_mode_brief";
pub const OPEN_ID: &str = "open";
pub const SHOW_IN_FINDER_ID: &str = "show_in_finder";
pub const COPY_PATH_ID: &str = "copy_path";
pub const COPY_FILENAME_ID: &str = "copy_filename";
pub const GET_INFO_ID: &str = "get_info";
pub const QUICK_LOOK_ID: &str = "quick_look";

/// Menu item IDs for navigation (Go menu).
pub const GO_BACK_ID: &str = "go_back";
pub const GO_FORWARD_ID: &str = "go_forward";
pub const GO_PARENT_ID: &str = "go_parent";

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
}

impl<R: Runtime> Default for MenuState<R> {
    fn default() -> Self {
        Self {
            show_hidden_files: Mutex::new(None),
            view_mode_full: Mutex::new(None),
            view_mode_brief: Mutex::new(None),
            context: Mutex::new(MenuContext::default()),
        }
    }
}

/// Result struct for menu items that need to be stored.
pub struct MenuItems<R: Runtime> {
    pub menu: Menu<R>,
    pub show_hidden_files: CheckMenuItem<R>,
    pub view_mode_full: CheckMenuItem<R>,
    pub view_mode_brief: CheckMenuItem<R>,
}

/// View mode type that matches the frontend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    Full,
    #[default]
    Brief,
}

/// Builds the application menu with default macOS items plus a custom View and File submenu enhancements.
pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
    view_mode: ViewMode,
) -> tauri::Result<MenuItems<R>> {
    // Start with the default menu (includes app menu with Quit, Hide, etc.)
    let menu = Menu::default(app)?;

    // Add File menu items
    let open_item = tauri::menu::MenuItem::with_id(app, OPEN_ID, "Open", true, None::<&str>)?;
    let show_in_finder_item =
        tauri::menu::MenuItem::with_id(app, SHOW_IN_FINDER_ID, "Show in Finder", true, Some("Opt+Cmd+O"))?;
    let copy_path_item =
        tauri::menu::MenuItem::with_id(app, COPY_PATH_ID, "Copy path to clipboard", true, Some("Ctrl+Cmd+C"))?;
    let copy_filename_item =
        tauri::menu::MenuItem::with_id(app, COPY_FILENAME_ID, "Copy filename", true, None::<&str>)?;
    let get_info_item = tauri::menu::MenuItem::with_id(app, GET_INFO_ID, "Get info", true, Some("Cmd+I"))?;
    let quick_look_item = tauri::menu::MenuItem::with_id(app, QUICK_LOOK_ID, "Quick look", true, None::<&str>)?;

    // Find the existing File submenu and add our items to it
    for item in menu.items()? {
        if let tauri::menu::MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "File"
        {
            submenu.prepend(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
            submenu.prepend(&quick_look_item)?;
            submenu.prepend(&get_info_item)?;
            submenu.prepend(&copy_filename_item)?;
            submenu.prepend(&copy_path_item)?;
            submenu.prepend(&show_in_finder_item)?;
            submenu.prepend(&open_item)?;
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
    let mut found_view = false;
    for item in menu.items()? {
        if let tauri::menu::MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "View"
        {
            // Add separator then our items
            submenu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
            submenu.append(&view_mode_full_item)?;
            submenu.append(&view_mode_brief_item)?;
            submenu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
            submenu.append(&show_hidden_item)?;
            found_view = true;
            break;
        }
    }

    // If View menu wasn't found (unlikely), create one
    if !found_view {
        let view_menu = Submenu::with_items(
            app,
            "View",
            true,
            &[
                &view_mode_full_item,
                &view_mode_brief_item,
                &tauri::menu::PredefinedMenuItem::separator(app)?,
                &show_hidden_item,
            ],
        )?;
        menu.append(&view_menu)?;
    }

    // Create Go menu for navigation
    let go_back_item = tauri::menu::MenuItem::with_id(app, GO_BACK_ID, "Back", true, Some("Cmd+["))?;
    let go_forward_item = tauri::menu::MenuItem::with_id(app, GO_FORWARD_ID, "Forward", true, Some("Cmd+]"))?;
    let go_parent_item = tauri::menu::MenuItem::with_id(app, GO_PARENT_ID, "Parent folder", true, Some("Cmd+Up"))?;

    let go_menu = Submenu::with_items(
        app,
        "Go",
        true,
        &[
            &go_back_item,
            &go_forward_item,
            &tauri::menu::PredefinedMenuItem::separator(app)?,
            &go_parent_item,
        ],
    )?;

    // Insert Go menu after View (before Window)
    // Find Window menu position and insert before it
    let mut inserted = false;
    let items = menu.items()?;
    for (i, item) in items.iter().enumerate() {
        if let tauri::menu::MenuItemKind::Submenu(submenu) = item
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
    })
}

/// Builds a context menu for a specific file.
pub fn build_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    is_directory: bool,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    let open_item = tauri::menu::MenuItem::with_id(app, OPEN_ID, "Open", true, None::<&str>)?;
    let show_in_finder_item =
        tauri::menu::MenuItem::with_id(app, SHOW_IN_FINDER_ID, "Show in Finder", true, Some("Opt+Cmd+O"))?;
    let copy_path_item =
        tauri::menu::MenuItem::with_id(app, COPY_PATH_ID, "Copy path to clipboard", true, Some("Ctrl+Cmd+C"))?;
    let copy_filename_item = tauri::menu::MenuItem::with_id(
        app,
        COPY_FILENAME_ID,
        format!("Copy \"{}\"", filename),
        true,
        Some("Cmd+C"),
    )?;
    let get_info_item = tauri::menu::MenuItem::with_id(app, GET_INFO_ID, "Get info", true, Some("Cmd+I"))?;
    let quick_look_item = tauri::menu::MenuItem::with_id(app, QUICK_LOOK_ID, "Quick look", true, None::<&str>)?;

    // Add items to menu
    if !is_directory {
        menu.append(&open_item)?;
    }
    menu.append(&show_in_finder_item)?;
    menu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
    menu.append(&copy_filename_item)?;
    menu.append(&copy_path_item)?;
    menu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
    menu.append(&get_info_item)?;
    menu.append(&quick_look_item)?;

    Ok(menu)
}
