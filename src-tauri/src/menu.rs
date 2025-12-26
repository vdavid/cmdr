//! Application menu configuration.

use std::sync::Mutex;
use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, Submenu},
};

/// Menu item ID for the "Show Hidden Files" toggle.
pub const SHOW_HIDDEN_FILES_ID: &str = "show_hidden_files";

/// Stores references to menu items that need to be accessed later.
pub struct MenuState<R: Runtime> {
    pub show_hidden_files: Mutex<Option<CheckMenuItem<R>>>,
}

impl<R: Runtime> Default for MenuState<R> {
    fn default() -> Self {
        Self {
            show_hidden_files: Mutex::new(None),
        }
    }
}

/// Builds the application menu with default macOS items plus a custom View submenu.
///
/// This preserves the standard macOS app menu (About, Hide, Quit, etc.) and adds
/// our Show Hidden Files item to the existing View menu.
///
/// # Arguments
/// * `app` - The Tauri app handle
/// * `show_hidden_files` - Initial checked state for the "Show Hidden Files" item
///
/// # Returns
/// A tuple of (Menu, CheckMenuItem) so the caller can store the CheckMenuItem reference
pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    show_hidden_files: bool,
) -> tauri::Result<(Menu<R>, CheckMenuItem<R>)> {
    // Start with the default menu (includes app menu with Quit, Hide, etc.)
    let menu = Menu::default(app)?;

    // Create our Show Hidden Files toggle
    let show_hidden_item = CheckMenuItem::with_id(
        app,
        SHOW_HIDDEN_FILES_ID,
        "Show Hidden Files",
        true, // enabled
        show_hidden_files,
        Some("Cmd+Shift+."),
    )?;

    // Find the existing View submenu and add our item to it
    // The default menu on macOS has: App, File, Edit, View, Window, Help
    let mut found_view = false;
    for item in menu.items()? {
        if let tauri::menu::MenuItemKind::Submenu(submenu) = item
            && submenu.text()? == "View"
        {
            // Add separator then our item
            submenu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
            submenu.append(&show_hidden_item)?;
            found_view = true;
            break;
        }
    }

    // If View menu wasn't found (unlikely), create one
    if !found_view {
        let view_menu = Submenu::with_items(app, "View", true, &[&show_hidden_item])?;
        menu.append(&view_menu)?;
    }

    Ok((menu, show_hidden_item))
}
