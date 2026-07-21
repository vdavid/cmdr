//! Hierarchical menu structure assembly.
//!
//! Builds the top-level application menu, context menus (file, breadcrumb,
//! tab, network host), and the viewer-window menu. Delegates the
//! per-platform menu bar shape to `menu::macos::build_menu_macos` and
//! `menu::linux::build_menu_linux`.

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

use tauri::{
    AppHandle, Runtime, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

#[cfg(target_os = "macos")]
use crate::file_system::open_with::OpenWithChoices;
#[cfg(target_os = "macos")]
use crate::file_system::sync_status::SyncStatus;

use super::menu_items::{
    COPY_FILENAME_MAX_CHARS, copy_path_accelerator, show_in_file_manager_accelerator, show_in_file_manager_label,
    truncate_for_menu_label,
};
#[cfg(target_os = "macos")]
use super::{CLOUD_MAKE_OFFLINE_ID, CLOUD_REMOVE_DOWNLOAD_ID, GET_INFO_ID, QUICK_LOOK_ID};
use super::{
    COPY_CURRENT_DIR_PATH_ID, COPY_FILENAME_ID, COPY_PATH_ID, EDIT_ID, EJECT_VOLUME_ID, FAVORITE_REMOVE_ID,
    FAVORITE_RENAME_ID, FAVORITES_ADD_CONTEXT_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_MOVE_ID, FILE_NEW_FOLDER_ID,
    FILE_VIEW_ID, ImageIndexMenuState, MenuItems, NETWORK_HOST_DISCONNECT_ID, NETWORK_HOST_FORGET_PASSWORD_ID,
    NETWORK_HOST_FORGET_SERVER_ID, OPEN_ID, RENAME_ID, SHOW_IN_FINDER_ID, TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID,
    TAB_PIN_ID, TOGGLE_SELECTION_ID, VIEWER_WORD_WRAP_ID, ViewMode, ViewerMenuItems, image_index_menu_items,
};

/// Per-file information needed to build a fully-populated context menu.
///
/// On non-macOS this is empty; on macOS it carries the cloud sync status (used to
/// decide between "Make available offline" and "Remove download"), whether the file
/// lives in any File Provider domain (gates cloud actions), and the precomputed
/// "Open with" candidate apps.
#[cfg(target_os = "macos")]
#[derive(Default)]
pub struct FileContextInfo {
    pub sync_status: SyncStatus,
    /// Whether this path is in iCloud Drive specifically. Gates the cloud action menu
    /// items. Eviction / download work via `FileManager` ubiquity APIs, which only
    /// support iCloud (not third-party File Providers). See `cloud_actions.rs` for why.
    pub is_icloud_drive: bool,
    pub open_with: OpenWithChoices,
    /// Which of the seven Finder color tags (index 1..=7) the selection already carries.
    /// "Applied" = EVERY selected path has a tag of that color, so the menu shows a
    /// checked (checkmark-composited) circle and the click toggles it off. Index 0 is
    /// unused (colorless). Computed by reading each path's tags once at menu-build time.
    pub applied_tag_colors: [bool; 8],
}

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub struct FileContextInfo;

/// Result of building a file context menu: the menu itself, plus (on macOS) a
/// `bundle_id → app_path` map that the caller stores in `MenuState.context.open_with_apps`
/// so `lib.rs::on_menu_event` can resolve `open-with:<bundle-id>` clicks back to an app URL.
pub struct ContextMenuResult<R: Runtime> {
    pub menu: Menu<R>,
    #[cfg(target_os = "macos")]
    pub open_with_apps: HashMap<String, PathBuf>,
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
        super::macos::build_menu_macos(app, show_hidden_files, view_mode, has_existing_license)
    }

    #[cfg(not(target_os = "macos"))]
    {
        super::linux::build_menu_linux(app, show_hidden_files, view_mode, has_existing_license)
    }
}

/// Builds a context menu for a specific file.
///
/// `restrict_destination_actions = true` is used by the search-results virtual
/// pane (`volumeId == "search-results"`, see `apps/desktop/src/lib/search/capabilities.ts`):
/// it suppresses Rename and New folder, which only make sense on a real directory.
/// Source-side actions (Open, Copy, Move, Delete, Show in Finder, Copy filename,
/// Copy path) stay because the underlying paths are real.
pub fn build_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    is_directory: bool,
    #[cfg_attr(
        not(target_os = "macos"),
        allow(unused_variables, reason = "all reads of `info` sit inside macOS-gated branches")
    )]
    info: &FileContextInfo,
    restrict_destination_actions: bool,
    // Media-index image-search facts about the right-clicked folder; `image_index_menu_items`
    // turns them into the folder-only chosen/exclusion items (empty when the master toggle
    // is off).
    image_index: ImageIndexMenuState,
) -> tauri::Result<ContextMenuResult<R>> {
    let menu = Menu::new(app)?;

    // Open / View / Edit group (files only)
    #[cfg(target_os = "macos")]
    let mut open_with_apps: HashMap<String, PathBuf> = HashMap::new();
    if !is_directory {
        let open_item = MenuItem::with_id(app, OPEN_ID, "Open", true, None::<&str>)?;
        let view_item = MenuItem::with_id(app, FILE_VIEW_ID, "View", true, Some("F3"))?;
        let edit_item = MenuItem::with_id(app, EDIT_ID, "Edit", true, Some("F4"))?;
        menu.append(&open_item)?;
        #[cfg(target_os = "macos")]
        {
            // Open with submenu: Finder convention, shown for files, not directories.
            let (submenu, map) = super::open_with::build_open_with_submenu(app, &info.open_with.candidates)?;
            menu.append(&submenu)?;
            open_with_apps = map;
        }
        menu.append(&view_item)?;
        menu.append(&edit_item)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    // Toggle selection (Space). No real accelerator registered — the JS handler in
    // FilePane.svelte owns the Space keydown; this Some("Space") string is purely
    // a visual hint for the context menu and never fires globally. Placing it in its
    // own group makes the Space shortcut discoverable without crowding the activation
    // (Open / View / Edit) or operations (Copy / Move / Rename) groups.
    let toggle_selection_item = MenuItem::with_id(app, TOGGLE_SELECTION_ID, "Toggle selection", true, Some("Space"))?;
    menu.append(&toggle_selection_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Finder tag colors (macOS): seven circles that toggle the system color tags on the
    // selection. Shown for files and folders (Finder tags both).
    #[cfg(target_os = "macos")]
    append_tag_color_group(app, &menu, info)?;

    // Copy / Move / Rename group. Rename is omitted on the search-results virtual
    // pane: the underlying file CAN be renamed, but doing it from the snapshot view
    // splits the file (snapshot keeps the old name, disk has the new) which is
    // confusing. The user can navigate to the real folder and rename there.
    let copy_item = MenuItem::with_id(app, FILE_COPY_ID, "Copy", true, Some("F5"))?;
    let move_item = MenuItem::with_id(app, FILE_MOVE_ID, "Move", true, Some("F6"))?;
    menu.append(&copy_item)?;
    menu.append(&move_item)?;
    if !restrict_destination_actions {
        let rename_item = MenuItem::with_id(app, RENAME_ID, "Rename", true, Some("F2"))?;
        menu.append(&rename_item)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // New folder — also omitted on search-results panes (no destination folder
    // to create into; the pane IS the snapshot, not a directory).
    if !restrict_destination_actions {
        let new_folder_item = MenuItem::with_id(app, FILE_NEW_FOLDER_ID, "New folder", true, Some("F7"))?;
        menu.append(&new_folder_item)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

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
        format!(
            "Copy \"{}\"",
            truncate_for_menu_label(filename, COPY_FILENAME_MAX_CHARS)
        ),
        true,
        Some("Cmd+C"),
    )?;
    let copy_path_item = MenuItem::with_id(app, COPY_PATH_ID, "Copy path", true, Some(copy_path_accelerator()))?;
    menu.append(&show_in_finder_item)?;
    menu.append(&copy_filename_item)?;
    menu.append(&copy_path_item)?;

    // Add to favorites — directories only (favorites are folders), and not on the search-results
    // snapshot pane (its rows aren't a stable folder to favorite). Favorites the right-clicked
    // folder's path, which `on_menu_event` reads from `MenuState.context.path`.
    if is_directory && !restrict_destination_actions {
        let add_favorite_item =
            MenuItem::with_id(app, FAVORITES_ADD_CONTEXT_ID, "Add to favorites", true, None::<&str>)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&add_favorite_item)?;
    }

    // Image-search group (media_index): folder-only, and only while image indexing is
    // enabled. `image_index_menu_items` decides the labels and which items are clickable.
    // Handled specially in `handle_menu_event` (they act on the right-clicked folder and
    // drive a FE persist path), never via `menu_id_to_command`.
    if is_directory {
        let items = image_index_menu_items(image_index);
        if !items.is_empty() {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            for item in items {
                let menu_item = MenuItem::with_id(app, item.id, item.label, item.enabled, None::<&str>)?;
                menu.append(&menu_item)?;
            }
        }
    }

    // Cloud actions (macOS File Provider): only show when the file is in a
    // cloud-managed folder, gated by sync status.
    #[cfg(target_os = "macos")]
    if info.is_icloud_drive {
        let cloud_item = match info.sync_status {
            SyncStatus::OnlineOnly => Some(MenuItem::with_id(
                app,
                CLOUD_MAKE_OFFLINE_ID,
                "Make available offline",
                true,
                None::<&str>,
            )?),
            SyncStatus::Synced => Some(MenuItem::with_id(
                app,
                CLOUD_REMOVE_DOWNLOAD_ID,
                "Remove download",
                true,
                None::<&str>,
            )?),
            // Uploading/Downloading: action already in flight, don't offer either.
            // Unknown: status query failed, hide to avoid confusion.
            _ => None,
        };
        if let Some(item) = cloud_item {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            menu.append(&item)?;
        }
    }

    // Quick Look and Get Info are macOS-only
    #[cfg(target_os = "macos")]
    {
        let get_info_item = MenuItem::with_id(app, GET_INFO_ID, "Get info", true, Some("Cmd+I"))?;
        let quick_look_item = MenuItem::with_id(app, QUICK_LOOK_ID, "Quick look", true, None::<&str>)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&get_info_item)?;
        menu.append(&quick_look_item)?;
    }

    Ok(ContextMenuResult {
        menu,
        #[cfg(target_os = "macos")]
        open_with_apps,
    })
}

/// Appends the seven Finder-tag color items (macOS) plus a trailing separator.
///
/// Each item is an `IconMenuItem` showing its color circle (open_with.rs pattern); the
/// "applied" colors (every selected file already carries them) get the checkmark-
/// composited variant. IDs are `tag-color:<index>`, prefix-routed in
/// `handle_menu_event`. Colors run in Finder's order (Red … Gray). Labels are the
/// English color names so the items stay accessible (screen readers read the text;
/// the circle is the icon). macOS-only — Linux menus carry no icons.
#[cfg(target_os = "macos")]
fn append_tag_color_group<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>, info: &FileContextInfo) -> tauri::Result<()> {
    use tauri::menu::IconMenuItem;

    // (color index, English system name), in Finder's color-row order.
    const COLORS: [(u8, &str); 7] = [
        (6, "Red"),
        (7, "Orange"),
        (5, "Yellow"),
        (2, "Green"),
        (4, "Blue"),
        (3, "Purple"),
        (1, "Gray"),
    ];

    for (color, name) in COLORS {
        let id = format!("{}{}", super::TAG_COLOR_ID_PREFIX, color);
        let checked = info.applied_tag_colors[color as usize];
        // `IconMenuItem` with `Some(image)` falls back to a text-only item if the image
        // build fails, so the menu still works without the circle.
        let icon = super::tag_icons::tag_circle_image(color, checked);
        let item = IconMenuItem::with_id(app, &id, name, true, icon, None::<&str>)?;
        menu.append(&item)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    Ok(())
}

/// Builds the minimal context menu for the `..` parent row: a single "Add to favorites" item that
/// favorites the parent directory. The full file context menu (Copy / Move / Delete, etc.) makes no
/// sense on `..`, so this is its own one-item menu. The caller stashes the parent dir in
/// `MenuState.context.path`; `on_menu_event` reads it back for the `FAVORITES_ADD_CONTEXT_ID` click.
pub fn build_parent_row_context_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let add_favorite_item = MenuItem::with_id(app, FAVORITES_ADD_CONTEXT_ID, "Add to favorites", true, None::<&str>)?;
    menu.append(&add_favorite_item)?;
    Ok(menu)
}

/// Builds a context menu for the breadcrumb path bar.
///
/// `accelerator` is the user's configured shortcut for the "Copy path" command (in
/// Tauri accelerator format, e.g. "Ctrl+Shift+C"), or empty if none is set.
/// `eject_volume_name`, when present, appends an `Eject ({name})` item that lets
/// the user eject the volume the breadcrumb represents. The caller is responsible
/// for stashing the matching `volume_id` in `MenuState.volume_eject_context` so
/// `on_menu_event` can dispatch the click.
///
/// When `eject_busy` is true, the item is rendered disabled with a ` (busy)`
/// suffix, so a volume with a write op reading from / writing to it can't be
/// ejected mid-transfer (mirrors the disabled eject button in the picker).
pub fn build_breadcrumb_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    accelerator: &str,
    eject_volume_name: Option<&str>,
    eject_busy: bool,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let accel: Option<&str> = if accelerator.is_empty() {
        None
    } else {
        Some(accelerator)
    };
    let copy_path_item = MenuItem::with_id(app, COPY_CURRENT_DIR_PATH_ID, "Copy path", true, accel)?;
    menu.append(&copy_path_item)?;
    if let Some(name) = eject_volume_name {
        let label = if eject_busy {
            format!("Eject ({}) (busy)", name)
        } else {
            format!("Eject ({})", name)
        };
        let eject_item = MenuItem::with_id(app, EJECT_VOLUME_ID, &label, !eject_busy, None::<&str>)?;
        menu.append(&eject_item)?;
    }
    Ok(menu)
}

/// Builds a menu for viewer windows (built from scratch on all platforms).
///
/// Returns the menu plus the `Word wrap` CheckMenuItem ref so the caller can flip its checked state
/// in O(1) (see `ViewerMenuItems`). On macOS the menu is installed app-level via `app.set_menu()`;
/// on Linux it's a per-window menu (`window.set_menu()`).
pub fn build_viewer_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<ViewerMenuItems<R>> {
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
    // Predefined items carry the native cut:/copy:/paste:/selectAll: selectors, which
    // macOS routes to the focused text field (the search box) through the responder
    // chain. All four are needed: without Cut/Paste, ⌘X/⌘V are dead in the viewer's
    // search input (the viewer menu is the active app menu while a viewer is focused).
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    menu.append(&edit_menu)?;

    // --- View menu ---
    // `word_wrap` is returned so the caller (and `viewer_set_word_wrap`) can flip its checked state
    // directly without a tree walk.
    let word_wrap = CheckMenuItem::with_id(app, VIEWER_WORD_WRAP_ID, "Word wrap", true, false, None::<&str>)?;
    let view_submenu = Submenu::with_items(app, "View", true, &[&word_wrap])?;
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

    Ok(ViewerMenuItems { menu, word_wrap })
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

/// Builds a context menu for a network host.
/// Always includes "Disconnect". Conditionally adds "Forget server" (manual hosts)
/// and "Forget saved password" (hosts with stored credentials).
pub fn build_network_host_context_menu(
    app: &AppHandle<Wry>,
    is_manual: bool,
    has_credentials: bool,
) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;

    // "Disconnect" is always shown. If nothing is mounted, the backend handles it gracefully.
    let disconnect = MenuItem::with_id(app, NETWORK_HOST_DISCONNECT_ID, "Disconnect", true, None::<&str>)?;
    menu.append(&disconnect)?;

    if is_manual {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        let forget_server = MenuItem::with_id(app, NETWORK_HOST_FORGET_SERVER_ID, "Forget server", true, None::<&str>)?;
        menu.append(&forget_server)?;
    }

    if has_credentials {
        if !is_manual {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        let forget_password = MenuItem::with_id(
            app,
            NETWORK_HOST_FORGET_PASSWORD_ID,
            "Forget saved password",
            true,
            None::<&str>,
        )?;
        menu.append(&forget_password)?;
    }

    Ok(menu)
}

/// Builds the context menu for a row in the volume-selector dropdown.
///
/// Favorites get `Rename` + `Remove`; an ejectable volume gets an `Eject ({name})`
/// item (disabled with a ` (busy)` suffix while a write op touches it, mirroring the
/// breadcrumb menu and the inline eject button). The caller stashes the target id +
/// name in `MenuState.volume_row_context` so `on_menu_event` can dispatch the click.
pub fn build_volume_row_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    is_favorite: bool,
    eject_volume_name: Option<&str>,
    eject_busy: bool,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    if is_favorite {
        let rename_item = MenuItem::with_id(app, FAVORITE_RENAME_ID, "Rename", true, None::<&str>)?;
        menu.append(&rename_item)?;
        let remove_item = MenuItem::with_id(app, FAVORITE_REMOVE_ID, "Remove", true, None::<&str>)?;
        menu.append(&remove_item)?;
    } else if let Some(name) = eject_volume_name {
        let label = if eject_busy {
            format!("Eject ({}) (busy)", name)
        } else {
            format!("Eject ({})", name)
        };
        let eject_item = MenuItem::with_id(app, EJECT_VOLUME_ID, &label, !eject_busy, None::<&str>)?;
        menu.append(&eject_item)?;
    }

    Ok(menu)
}
