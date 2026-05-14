//! Application menu configuration.

#[cfg(not(target_os = "macos"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub mod open_with;

use crate::ignore_poison::IgnorePoison;
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{
    AppHandle, Runtime, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

#[cfg(target_os = "macos")]
use crate::file_system::open_with::OpenWithChoices;
#[cfg(target_os = "macos")]
use crate::file_system::sync_status::SyncStatus;

/// Menu item IDs for file actions.
pub const SHOW_HIDDEN_FILES_ID: &str = "show_hidden_files";
/// View mode CheckMenuItems — one pair per pane, nested under per-pane submenus
/// (View > Left pane > Full view / Brief view, and the same for Right pane).
/// The keyboard shortcut (⌘1/⌘2 by default) only attaches to the items belonging
/// to the active pane, and "moves" to the other pane on focus change.
/// See `rebuild_view_mode_items`.
pub const VIEW_MODE_FULL_LEFT_ID: &str = "view_mode_full_left";
pub const VIEW_MODE_BRIEF_LEFT_ID: &str = "view_mode_brief_left";
pub const VIEW_MODE_FULL_RIGHT_ID: &str = "view_mode_full_right";
pub const VIEW_MODE_BRIEF_RIGHT_ID: &str = "view_mode_brief_right";

/// Zoom (text-size) submenu under View. Each preset writes
/// `appearance.textSize`; in/out adjust by 25 percentage points.
pub const VIEW_ZOOM_75_ID: &str = "view_zoom_75";
pub const VIEW_ZOOM_100_ID: &str = "view_zoom_100";
pub const VIEW_ZOOM_125_ID: &str = "view_zoom_125";
pub const VIEW_ZOOM_150_ID: &str = "view_zoom_150";
pub const VIEW_ZOOM_IN_ID: &str = "view_zoom_in";
pub const VIEW_ZOOM_OUT_ID: &str = "view_zoom_out";
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
pub const COPY_CURRENT_DIR_PATH_ID: &str = "copy_current_dir_path";
pub const COPY_FILENAME_ID: &str = "copy_filename";
pub const GET_INFO_ID: &str = "get_info";
pub const QUICK_LOOK_ID: &str = "quick_look";
pub const RENAME_ID: &str = "rename";
pub const SELECT_ALL_ID: &str = "select_all_files";
pub const DESELECT_ALL_ID: &str = "deselect_all";

/// Menu item IDs for cloud actions (macOS File Provider).
pub const CLOUD_MAKE_OFFLINE_ID: &str = "cloud_make_offline";
pub const CLOUD_REMOVE_DOWNLOAD_ID: &str = "cloud_remove_download";

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
        HELP_SEND_ERROR_REPORT_ID => Some(("help.sendErrorReport", CommandScope::App)),
        CHECK_FOR_UPDATES_ID => Some(("app.checkForUpdates", CommandScope::App)),

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
        REOPEN_CLOSED_TAB_ID => Some(("tab.reopen", CommandScope::FileScoped)),
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
        COPY_CURRENT_DIR_PATH_ID => Some(("file.copyCurrentDirectoryPath", CommandScope::FileScoped)),
        COPY_FILENAME_ID => Some(("file.copyFilename", CommandScope::FileScoped)),
        GET_INFO_ID => Some(("file.getInfo", CommandScope::FileScoped)),
        QUICK_LOOK_ID => Some(("file.quickLook", CommandScope::FileScoped)),
        SELECT_ALL_ID => Some(("selection.selectAll", CommandScope::FileScoped)),
        DESELECT_ALL_ID => Some(("selection.deselectAll", CommandScope::FileScoped)),

        // Cloud actions (macOS File Provider)
        CLOUD_MAKE_OFFLINE_ID => Some(("cloud.makeOffline", CommandScope::FileScoped)),
        CLOUD_REMOVE_DOWNLOAD_ID => Some(("cloud.removeDownload", CommandScope::FileScoped)),

        // Zoom (text size) — App scope so ⌘0/⌘+/⌘- work in any focused window.
        VIEW_ZOOM_75_ID => Some(("view.zoom.set75", CommandScope::App)),
        VIEW_ZOOM_100_ID => Some(("view.zoom.set100", CommandScope::App)),
        VIEW_ZOOM_125_ID => Some(("view.zoom.set125", CommandScope::App)),
        VIEW_ZOOM_150_ID => Some(("view.zoom.set150", CommandScope::App)),
        VIEW_ZOOM_IN_ID => Some(("view.zoom.in", CommandScope::App)),
        VIEW_ZOOM_OUT_ID => Some(("view.zoom.out", CommandScope::App)),

        // Sort items: mapped so user-customized accelerators can flow into the menu via the
        // generic update path. At runtime, `on_menu_event` intercepts these IDs *before* this
        // lookup and emits `menu-sort` instead of `execute-command` — so this mapping never
        // routes a click. It exists purely as the source of truth for the reverse map.
        SORT_BY_NAME_ID => Some(("sort.byName", CommandScope::FileScoped)),
        SORT_BY_EXTENSION_ID => Some(("sort.byExtension", CommandScope::FileScoped)),
        SORT_BY_MODIFIED_ID => Some(("sort.byModified", CommandScope::FileScoped)),
        SORT_BY_SIZE_ID => Some(("sort.bySize", CommandScope::FileScoped)),

        // Not mapped: CheckMenuItems (show_hidden_files, view modes), close-tab (special logic),
        // viewer word wrap, tab context menu actions, sort order items (ascending/descending/
        // date-created), "open-with:*" (prefix-routed before this lookup in
        // `lib.rs::on_menu_event`).
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
        "help.sendErrorReport" => Some(HELP_SEND_ERROR_REPORT_ID),
        "app.checkForUpdates" => Some(CHECK_FOR_UPDATES_ID),
        "pane.switch" => Some(SWITCH_PANE_ID),
        "pane.swap" => Some(SWAP_PANES_ID),
        "nav.back" => Some(GO_BACK_ID),
        "nav.forward" => Some(GO_FORWARD_ID),
        "nav.parent" => Some(GO_PARENT_ID),
        "tab.new" => Some(NEW_TAB_ID),
        "tab.close" => Some(CLOSE_TAB_ID),
        "tab.reopen" => Some(REOPEN_CLOSED_TAB_ID),
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
        "view.zoom.set75" => Some(VIEW_ZOOM_75_ID),
        "view.zoom.set100" => Some(VIEW_ZOOM_100_ID),
        "view.zoom.set125" => Some(VIEW_ZOOM_125_ID),
        "view.zoom.set150" => Some(VIEW_ZOOM_150_ID),
        "view.zoom.in" => Some(VIEW_ZOOM_IN_ID),
        "view.zoom.out" => Some(VIEW_ZOOM_OUT_ID),
        "edit.cut" => Some(EDIT_CUT_ID),
        "edit.copy" => Some(EDIT_COPY_ID),
        "edit.paste" => Some(EDIT_PASTE_ID),
        "edit.pasteAsMove" => Some(EDIT_PASTE_MOVE_ID),
        "cloud.makeOffline" => Some(CLOUD_MAKE_OFFLINE_ID),
        "cloud.removeDownload" => Some(CLOUD_REMOVE_DOWNLOAD_ID),
        "sort.byName" => Some(SORT_BY_NAME_ID),
        "sort.byExtension" => Some(SORT_BY_EXTENSION_ID),
        "sort.byModified" => Some(SORT_BY_MODIFIED_ID),
        "sort.bySize" => Some(SORT_BY_SIZE_ID),
        _ => None,
    }
}

/// Context for the current menu selection.
#[derive(Clone, Default)]
pub struct MenuContext {
    /// The right-clicked file's path (the "primary" file, used for single-file actions
    /// like "Copy 'filename'", Get info, Quick look).
    pub path: String,
    pub filename: String,
    /// All paths the menu's actions should affect. For a right-click on a non-selected
    /// file, this is just `[path]`. For a right-click on a file that's part of a
    /// multi-selection, this is the full selection. Used by "Open with" launches and
    /// by cloud actions when the user wants the action to apply across all selected
    /// files.
    pub paths: Vec<String>,
    /// Map of bundle ID → app path for the most-recent "Open with" submenu. Populated
    /// when the context menu is built; consumed when the user clicks an
    /// `open-with:<bundle-id>` item.
    #[cfg(target_os = "macos")]
    pub open_with_apps: HashMap<String, PathBuf>,
}

/// Context for the network host context menu (stored so on_menu_event can emit it).
#[derive(Clone, Default)]
pub struct NetworkHostMenuContext {
    pub host_id: String,
    pub host_name: String,
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
    /// Per-pane view mode CheckMenuItems. Both pairs always exist; only the active
    /// pane's pair carries keyboard accelerators. See `rebuild_view_mode_items`.
    pub view_mode_full_left: Mutex<Option<CheckMenuItem<R>>>,
    pub view_mode_brief_left: Mutex<Option<CheckMenuItem<R>>>,
    pub view_mode_full_right: Mutex<Option<CheckMenuItem<R>>>,
    pub view_mode_brief_right: Mutex<Option<CheckMenuItem<R>>>,
    pub context: Mutex<MenuContext>,
    /// Per-pane submenus that hold the Full/Brief CheckMenuItems. The View submenu
    /// itself just nests these two (`Left pane >` and `Right pane >`).
    /// Each pane's Full item is at position 0, Brief at position 1.
    pub view_left_pane_submenu: Mutex<Option<Submenu<R>>>,
    pub view_right_pane_submenu: Mutex<Option<Submenu<R>>>,
    /// Cached state used by `rebuild_view_mode_items` to know which side gets the accelerator
    /// and what each side's checked state should be. Frontend pushes updates via
    /// `update_view_mode_menu`. Defaults: active = left, both modes = brief.
    pub view_mode_active_pane: Mutex<String>,
    pub view_mode_left: Mutex<ViewMode>,
    pub view_mode_right: Mutex<ViewMode>,
    /// Cached view-mode shortcuts. Frontend pushes updates via `update_menu_accelerator`.
    /// Defaults match the labels created in `build_menu_*` (Cmd+1 / Cmd+2).
    pub view_mode_full_accel: Mutex<Option<String>>,
    pub view_mode_brief_accel: Mutex<Option<String>>,
    /// Pin/unpin tab menu item (label toggles based on active tab state)
    pub pin_tab: Mutex<Option<MenuItem<R>>>,
    /// Reopen closed tab menu item (enabled when the focused pane's closed-tab stack is non-empty)
    pub reopen_closed_tab: Mutex<Option<MenuItem<R>>>,
    /// Generic menu items keyed by menu item ID, for accelerator and enable/disable updates.
    pub items: Mutex<HashMap<String, MenuItemEntry<R>>>,
    /// Sort by submenu (disabled when not in explorer context)
    pub sort_submenu: Mutex<Option<Submenu<R>>>,
    /// Context for the most recent network host context menu (host_id + host_name)
    pub network_host_context: Mutex<NetworkHostMenuContext>,
}

impl<R: Runtime> Default for MenuState<R> {
    fn default() -> Self {
        Self {
            show_hidden_files: Mutex::new(None),
            view_mode_full_left: Mutex::new(None),
            view_mode_brief_left: Mutex::new(None),
            view_mode_full_right: Mutex::new(None),
            view_mode_brief_right: Mutex::new(None),
            context: Mutex::new(MenuContext::default()),
            view_left_pane_submenu: Mutex::new(None),
            view_right_pane_submenu: Mutex::new(None),
            view_mode_active_pane: Mutex::new("left".to_string()),
            view_mode_left: Mutex::new(ViewMode::Brief),
            view_mode_right: Mutex::new(ViewMode::Brief),
            view_mode_full_accel: Mutex::new(Some("Cmd+1".to_string())),
            view_mode_brief_accel: Mutex::new(Some("Cmd+2".to_string())),
            pin_tab: Mutex::new(None),
            reopen_closed_tab: Mutex::new(None),
            items: Mutex::new(HashMap::new()),
            sort_submenu: Mutex::new(None),
            network_host_context: Mutex::new(NetworkHostMenuContext::default()),
        }
    }
}

/// Result struct for menu items that need to be stored.
pub struct MenuItems<R: Runtime> {
    pub menu: Menu<R>,
    pub show_hidden_files: CheckMenuItem<R>,
    /// Per-pane view-mode CheckMenuItems (only the left pair carries the
    /// accelerator at construction time; the right pair gets it after the
    /// frontend's first `update_view_mode_menu` call if right is the saved
    /// active pane).
    pub view_mode_full_left: CheckMenuItem<R>,
    pub view_mode_brief_left: CheckMenuItem<R>,
    pub view_mode_full_right: CheckMenuItem<R>,
    pub view_mode_brief_right: CheckMenuItem<R>,
    /// Per-pane submenus (Full at position 0, Brief at position 1) — used by
    /// `rebuild_view_mode_items` to reinsert items after accelerator changes.
    pub view_left_pane_submenu: Submenu<R>,
    pub view_right_pane_submenu: Submenu<R>,
    /// Pin/unpin tab menu item (label updated dynamically by frontend)
    pub pin_tab: MenuItem<R>,
    /// Reopen closed tab menu item (enable state synced from frontend)
    pub reopen_closed_tab: MenuItem<R>,
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
pub const REOPEN_CLOSED_TAB_ID: &str = "reopen_closed_tab";
pub const NEXT_TAB_ID: &str = "next_tab";
pub const PREV_TAB_ID: &str = "prev_tab";
pub const CLOSE_OTHER_TABS_ID: &str = "close_other_tabs";

/// Menu item IDs for tab context menu.
pub const TAB_PIN_ID: &str = "tab_pin";
pub const TAB_CLOSE_OTHERS_ID: &str = "tab_close_others";
pub const TAB_CLOSE_ID: &str = "tab_close";

/// Menu item IDs for network host context menu.
pub const NETWORK_HOST_FORGET_SERVER_ID: &str = "network_host_forget_server";
pub const NETWORK_HOST_FORGET_PASSWORD_ID: &str = "network_host_forget_password";
pub const NETWORK_HOST_DISCONNECT_ID: &str = "network_host_disconnect";

/// Menu item ID for About window.
pub const ABOUT_ID: &str = "about";

/// Menu item ID for Enter License Key.
pub const ENTER_LICENSE_KEY_ID: &str = "enter_license_key";

/// Menu item ID for Settings.
pub const SETTINGS_ID: &str = "settings";

/// Menu item ID for "Send error report…" (under the Help menu).
pub const HELP_SEND_ERROR_REPORT_ID: &str = "help_send_error_report";

/// Menu item ID for "Check for updates…" (under the Cmdr / Help menu).
pub const CHECK_FOR_UPDATES_ID: &str = "check_for_updates";

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

/// Items returned from `build_sort_submenu` so callers can register the sort items
/// in the items HashMap for accelerator updates.
pub(crate) struct SortSubmenuItems<R: Runtime> {
    pub submenu: Submenu<R>,
    pub by_name: MenuItem<R>,
    pub by_extension: MenuItem<R>,
    pub by_modified: MenuItem<R>,
    pub by_size: MenuItem<R>,
}

/// Builds the Sort by submenu (shared between macOS and Linux).
///
/// Accelerators for Name/Extension/Date modified/Size are caller-provided so each
/// platform can pass `None` where the toolkit can't deliver the chord.
fn build_sort_submenu<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    accel_name: Option<&str>,
    accel_extension: Option<&str>,
    accel_modified: Option<&str>,
    accel_size: Option<&str>,
) -> tauri::Result<SortSubmenuItems<R>> {
    let sort_by_name = MenuItem::with_id(app, SORT_BY_NAME_ID, "Name", true, accel_name)?;
    let sort_by_ext = MenuItem::with_id(app, SORT_BY_EXTENSION_ID, "Extension", true, accel_extension)?;
    let sort_by_modified = MenuItem::with_id(app, SORT_BY_MODIFIED_ID, "Date modified", true, accel_modified)?;
    let sort_by_size = MenuItem::with_id(app, SORT_BY_SIZE_ID, "Size", true, accel_size)?;
    let sort_by_created = MenuItem::with_id(app, SORT_BY_CREATED_ID, "Date created", true, None::<&str>)?;
    let sort_asc = MenuItem::with_id(app, SORT_ASCENDING_ID, "Ascending", true, None::<&str>)?;
    let sort_desc = MenuItem::with_id(app, SORT_DESCENDING_ID, "Descending", true, None::<&str>)?;

    let submenu = Submenu::with_items(
        app,
        label,
        true,
        &[
            &sort_by_name,
            &sort_by_ext,
            &sort_by_modified,
            &sort_by_size,
            &sort_by_created,
            &PredefinedMenuItem::separator(app)?,
            &sort_asc,
            &sort_desc,
        ],
    )?;

    Ok(SortSubmenuItems {
        submenu,
        by_name: sort_by_name,
        by_extension: sort_by_ext,
        by_modified: sort_by_modified,
        by_size: sort_by_size,
    })
}

/// Builds the View > Zoom submenu (shared between macOS and Linux).
///
/// Each preset item writes `appearance.textSize` directly via the unified
/// command-execute event. Zoom in/out adjust the value by 10 percentage
/// points. `accel_in` / `accel_out` are platform-specific accelerator strings
/// (macOS uses `Cmd+Plus` / `Cmd+Minus`, Linux uses `None` because GTK
/// intercepts these keys at the toolkit level).
fn build_zoom_submenu<R: Runtime>(
    app: &AppHandle<R>,
    accel_100: Option<&str>,
    accel_in: Option<&str>,
    accel_out: Option<&str>,
) -> tauri::Result<Submenu<R>> {
    let zoom_75 = MenuItem::with_id(app, VIEW_ZOOM_75_ID, "75%", true, None::<&str>)?;
    let zoom_100 = MenuItem::with_id(app, VIEW_ZOOM_100_ID, "100%", true, accel_100)?;
    let zoom_125 = MenuItem::with_id(app, VIEW_ZOOM_125_ID, "125%", true, None::<&str>)?;
    let zoom_150 = MenuItem::with_id(app, VIEW_ZOOM_150_ID, "150%", true, None::<&str>)?;
    let zoom_in = MenuItem::with_id(app, VIEW_ZOOM_IN_ID, "Zoom in", true, accel_in)?;
    let zoom_out = MenuItem::with_id(app, VIEW_ZOOM_OUT_ID, "Zoom out", true, accel_out)?;

    Submenu::with_items(
        app,
        "Zoom",
        true,
        &[
            &zoom_75,
            &zoom_100,
            &zoom_125,
            &zoom_150,
            &PredefinedMenuItem::separator(app)?,
            &zoom_in,
            &zoom_out,
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
    /// Whether this path is in iCloud Drive specifically — gates the cloud action menu
    /// items. Eviction / download work via `FileManager` ubiquity APIs, which only
    /// support iCloud (not third-party File Providers). See `cloud_actions.rs` for why.
    pub is_icloud_drive: bool,
    pub open_with: OpenWithChoices,
}

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub struct FileContextInfo;

/// Result of building a file context menu — the menu itself, plus (on macOS) a
/// `bundle_id → app_path` map that the caller stores in `MenuState.context.open_with_apps`
/// so `lib.rs::on_menu_event` can resolve `open-with:<bundle-id>` clicks back to an app URL.
pub struct ContextMenuResult<R: Runtime> {
    pub menu: Menu<R>,
    #[cfg(target_os = "macos")]
    pub open_with_apps: HashMap<String, PathBuf>,
}

/// Max chars in the `Copy "<filename>"` context menu label before middle-ellipsis kicks in.
/// Picked to fit typical filenames while capping pathological 100+ char names that blow the menu width.
const COPY_FILENAME_MAX_CHARS: usize = 50;

/// Truncate a filename for use inside a menu label, preserving the extension.
///
/// If the filename fits within `max_chars` (counted in chars, not bytes), it's returned unchanged.
/// Otherwise produces `<prefix>…<suffix>` where the suffix keeps the file extension plus a few
/// preceding chars, and the prefix takes ~60% of the budget. Operates on chars so multi-byte
/// UTF-8 sequences are never split mid-codepoint.
fn truncate_for_menu_label(filename: &str, max_chars: usize) -> String {
    let total_chars = filename.chars().count();
    if total_chars <= max_chars {
        return filename.to_string();
    }

    // Reserve one char for the ellipsis itself.
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "\u{2026}".to_string();
    }
    let budget = max_chars - 1;
    let prefix_chars = budget * 6 / 10;
    let suffix_chars = budget - prefix_chars;

    // Find the extension (everything after the last '.', but only if there's a non-empty stem).
    // `Path::extension` skips leading-dot files and returns just the ext without the dot, which is
    // what we want here — we treat names like ".gitignore" as extensionless.
    let ext_with_dot = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let ext_chars = ext_with_dot.chars().count();

    // If the extension alone doesn't fit in the suffix budget, fall back to a plain ~60/40
    // middle-ellipsis split (the extension is too long to be useful here anyway).
    let suffix: String = if ext_chars > 0 && ext_chars <= suffix_chars {
        // Keep the full extension plus some chars before it (the part of the stem near the end).
        let pre_ext_chars = suffix_chars - ext_chars;
        let stem_len = total_chars - ext_chars;
        let take_from = stem_len.saturating_sub(pre_ext_chars);
        filename
            .chars()
            .skip(take_from)
            .take(pre_ext_chars + ext_chars)
            .collect()
    } else {
        filename.chars().skip(total_chars - suffix_chars).collect()
    };

    let prefix: String = filename.chars().take(prefix_chars).collect();
    format!("{prefix}\u{2026}{suffix}")
}

/// Builds a context menu for a specific file.
pub fn build_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    is_directory: bool,
    #[cfg_attr(
        not(target_os = "macos"),
        allow(unused_variables, reason = "all reads of `info` sit inside macOS-gated branches")
    )]
    info: &FileContextInfo,
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
            // Open with submenu — Finder convention: shown for files, not directories.
            let (submenu, map) = open_with::build_open_with_submenu(app, &info.open_with.candidates)?;
            menu.append(&submenu)?;
            open_with_apps = map;
        }
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

    // Cloud actions (macOS File Provider) — only show when the file is in a
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

/// Builds a context menu for the breadcrumb path bar.
/// The `accelerator` parameter is the user's configured shortcut for this command
/// (in Tauri accelerator format, e.g. "Ctrl+Shift+C"), or empty if none is set.
pub fn build_breadcrumb_context_menu<R: Runtime>(app: &AppHandle<R>, accelerator: &str) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let accel: Option<&str> = if accelerator.is_empty() {
        None
    } else {
        Some(accelerator)
    };
    let copy_path_item = MenuItem::with_id(app, COPY_CURRENT_DIR_PATH_ID, "Copy path", true, accel)?;
    menu.append(&copy_path_item)?;
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

/// Platform-aware label for the per-pane view-mode CheckMenuItems.
/// Linux uses GTK mnemonics; macOS doesn't.
#[cfg(target_os = "macos")]
pub(crate) fn full_view_label() -> &'static str {
    "Full view"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn full_view_label() -> &'static str {
    "&Full view"
}

#[cfg(target_os = "macos")]
pub(crate) fn brief_view_label() -> &'static str {
    "Brief view"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn brief_view_label() -> &'static str {
    "&Brief view"
}

/// Rebuilds the four per-pane view-mode `CheckMenuItem`s with the current
/// state cached in `MenuState`: active pane, per-pane modes, and full/brief
/// shortcuts.
///
/// The accelerator is attached only to the active pane's pair, so that the
/// shortcut hint visually "follows" focus. Items are removed from the per-pane
/// submenu (Left pane / Right pane) and reinserted at the same position
/// (Full=0, Brief=1), since Tauri has no `set_accelerator()` API. The new
/// `CheckMenuItem` references replace the old ones in `MenuState`.
///
/// Frontend pushes a rebuild on pane focus change and on shortcut customization.
pub fn rebuild_view_mode_items<R: Runtime>(app: &AppHandle<R>, menu_state: &MenuState<R>) -> tauri::Result<()> {
    let left_submenu_guard = menu_state.view_left_pane_submenu.lock_ignore_poison();
    let right_submenu_guard = menu_state.view_right_pane_submenu.lock_ignore_poison();
    let left_submenu = left_submenu_guard
        .as_ref()
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;
    let right_submenu = right_submenu_guard
        .as_ref()
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;

    let active_pane = menu_state.view_mode_active_pane.lock_ignore_poison().clone();
    let left_mode = *menu_state.view_mode_left.lock_ignore_poison();
    let right_mode = *menu_state.view_mode_right.lock_ignore_poison();
    let full_accel = menu_state.view_mode_full_accel.lock_ignore_poison().clone();
    let brief_accel = menu_state.view_mode_brief_accel.lock_ignore_poison().clone();

    let left_active = active_pane == "left";
    let full_label = full_view_label();
    let brief_label = brief_view_label();

    // Helper: replace one CheckMenuItem inside its pane submenu, preserving its position.
    let swap = |slot: &Mutex<Option<CheckMenuItem<R>>>,
                parent: &Submenu<R>,
                position: usize,
                id: &str,
                label: &str,
                checked: bool,
                accel: Option<&str>|
     -> tauri::Result<()> {
        let mut guard = slot.lock_ignore_poison();
        if let Some(old) = guard.as_ref() {
            parent.remove(old)?;
        }
        let new_item = CheckMenuItem::with_id(app, id, label, true, checked, accel)?;
        parent.insert(&new_item, position)?;
        *guard = Some(new_item);
        Ok(())
    };

    swap(
        &menu_state.view_mode_full_left,
        left_submenu,
        0,
        VIEW_MODE_FULL_LEFT_ID,
        full_label,
        left_mode == ViewMode::Full,
        if left_active { full_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_brief_left,
        left_submenu,
        1,
        VIEW_MODE_BRIEF_LEFT_ID,
        brief_label,
        left_mode == ViewMode::Brief,
        if left_active { brief_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_full_right,
        right_submenu,
        0,
        VIEW_MODE_FULL_RIGHT_ID,
        full_label,
        right_mode == ViewMode::Full,
        if !left_active { full_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_brief_right,
        right_submenu,
        1,
        VIEW_MODE_BRIEF_RIGHT_ID,
        brief_label,
        right_mode == ViewMode::Brief,
        if !left_active { brief_accel.as_deref() } else { None },
    )?;

    Ok(())
}

/// Sets only the checked state on the existing per-pane view-mode items,
/// without touching accelerators. Used for in-place updates (a click in
/// the same pane, palette toggle) where active pane and shortcuts are
/// unchanged.
pub fn sync_view_mode_check_states<R: Runtime>(menu_state: &MenuState<R>) -> tauri::Result<()> {
    let left_mode = *menu_state.view_mode_left.lock_ignore_poison();
    let right_mode = *menu_state.view_mode_right.lock_ignore_poison();

    if let Some(item) = menu_state.view_mode_full_left.lock_ignore_poison().as_ref() {
        item.set_checked(left_mode == ViewMode::Full)?;
    }
    if let Some(item) = menu_state.view_mode_brief_left.lock_ignore_poison().as_ref() {
        item.set_checked(left_mode == ViewMode::Brief)?;
    }
    if let Some(item) = menu_state.view_mode_full_right.lock_ignore_poison().as_ref() {
        item.set_checked(right_mode == ViewMode::Full)?;
    }
    if let Some(item) = menu_state.view_mode_brief_right.lock_ignore_poison().as_ref() {
        item.set_checked(right_mode == ViewMode::Brief)?;
    }
    Ok(())
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

/// Builds a context menu for a network host.
/// Always includes "Disconnect". Conditionally adds "Forget server" (manual hosts)
/// and "Forget saved password" (hosts with stored credentials).
pub fn build_network_host_context_menu(
    app: &AppHandle<Wry>,
    is_manual: bool,
    has_credentials: bool,
) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;

    // "Disconnect" is always shown — if nothing is mounted, the backend handles it gracefully
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
        assert_eq!(menu_id_to_command(VIEW_MODE_FULL_LEFT_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_BRIEF_LEFT_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_FULL_RIGHT_ID), None);
        assert_eq!(menu_id_to_command(VIEW_MODE_BRIEF_RIGHT_ID), None);
        assert_eq!(menu_id_to_command(VIEWER_WORD_WRAP_ID), None);
        // Sort order items (ascending/descending) and date-created use the menu-sort
        // event path and are not mapped — only the four shortcut-bound columns are.
        assert_eq!(menu_id_to_command(SORT_ASCENDING_ID), None);
        assert_eq!(menu_id_to_command(SORT_DESCENDING_ID), None);
        assert_eq!(menu_id_to_command(SORT_BY_CREATED_ID), None);
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
            "tab.reopen",
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
            "help.sendErrorReport",
            "app.checkForUpdates",
            "cloud.makeOffline",
            "cloud.removeDownload",
            "sort.byName",
            "sort.byExtension",
            "sort.byModified",
            "sort.bySize",
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
    fn test_truncate_for_menu_label_short_passes_through() {
        assert_eq!(truncate_for_menu_label("hello.txt", 50), "hello.txt");
        assert_eq!(truncate_for_menu_label("", 50), "");
        // Exactly at the limit
        let exactly_50 = "a".repeat(50);
        assert_eq!(truncate_for_menu_label(&exactly_50, 50), exactly_50);
    }

    #[test]
    fn test_truncate_for_menu_label_long_with_extension_keeps_extension() {
        let long = "Obviously Awesome How to Nail Product Positioning so Customers Get It, Buy It, Love It Audiobook - m4b.epub";
        let truncated = truncate_for_menu_label(long, 50);
        assert!(truncated.chars().count() <= 50);
        assert!(
            truncated.ends_with(".epub"),
            "expected extension preserved, got: {truncated}"
        );
        assert!(truncated.contains('\u{2026}'), "expected ellipsis, got: {truncated}");
        assert!(
            truncated.starts_with("Obviously"),
            "expected prefix preserved, got: {truncated}"
        );
    }

    #[test]
    fn test_truncate_for_menu_label_long_without_extension() {
        let long = "a".repeat(100);
        let truncated = truncate_for_menu_label(&long, 50);
        assert!(truncated.chars().count() <= 50);
        assert!(truncated.contains('\u{2026}'));
        // No extension means a ~60/40 split with the ellipsis in the middle.
        let parts: Vec<&str> = truncated.split('\u{2026}').collect();
        assert_eq!(parts.len(), 2);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
    }

    #[test]
    fn test_truncate_for_menu_label_multibyte_utf8() {
        // Each emoji is multi-byte in UTF-8; the helper must count chars and never split mid-byte.
        let name = "🎉".repeat(40) + ".txt";
        let truncated = truncate_for_menu_label(&name, 20);
        assert!(truncated.chars().count() <= 20);
        // Round-trip through str must succeed (already guaranteed by String, but assert it's valid):
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
        assert!(truncated.contains('\u{2026}'));
        assert!(truncated.ends_with(".txt"));

        // Accented chars (single codepoint each) should also work cleanly.
        let accented = "ÁrvíztűrőTükörfúrógép".repeat(5);
        let truncated2 = truncate_for_menu_label(&accented, 15);
        assert!(truncated2.chars().count() <= 15);
        assert!(std::str::from_utf8(truncated2.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_for_menu_label_max_smaller_than_extension() {
        // When the extension is longer than the suffix budget, fall back to plain middle-ellipsis.
        // ".verylongextension" is 18 chars; with max_chars=10, suffix budget is only 4.
        let name = "stem.verylongextension";
        let truncated = truncate_for_menu_label(name, 10);
        assert!(truncated.chars().count() <= 10);
        assert!(truncated.contains('\u{2026}'));
        // Should not panic; should produce valid UTF-8.
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());

        // Edge: max_chars = 1 yields just the ellipsis.
        assert_eq!(truncate_for_menu_label("anything.txt", 1), "\u{2026}");
        // Edge: max_chars = 0 yields empty string.
        assert_eq!(truncate_for_menu_label("anything.txt", 0), "");
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
