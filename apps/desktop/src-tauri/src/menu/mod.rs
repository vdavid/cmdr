//! Application menu configuration.
//!
//! ## File layout
//!
//! - `mod.rs` (this file): shared types (`MenuState`, `MenuItems`, `MenuItemEntry`, `MenuContext`,
//!   `NetworkHostMenuContext`, `CommandScope`, `ViewMode`), all menu item ID constants, and the ID
//!   ↔ command-registry mapping (`menu_id_to_command` and `command_id_to_menu_id`).
//! - `menu_items.rs`: menu item builder helpers and submenu factories (sort, zoom),
//!   accelerator/label platform-aware helpers, `register_item`, and `truncate_for_menu_label`.
//! - `menu_structure.rs`: hierarchical assembly: `build_menu` dispatcher, context menus (file,
//!   breadcrumb, tab, network host), viewer menu, plus `FileContextInfo` / `ContextMenuResult`.
//! - `menu_handlers.rs`: event handlers and live-update helpers: `rebuild_view_mode_items`,
//!   `sync_view_mode_check_states`, `update_menu_item_accelerator`,
//!   `frontend_shortcut_to_accelerator`, and the macOS post-construction helpers
//!   (`cleanup_macos_menus`, `set_macos_menu_icons`).
//! - `macos.rs` / `linux.rs`: platform-specific menu bar shape.
//! - `open_with.rs` (macOS): "Open with" submenu builder.

#[cfg(not(target_os = "macos"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod menu_handlers;
mod menu_items;
mod menu_structure;
#[cfg(target_os = "macos")]
pub mod open_with;

use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{
    Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
};

// Re-export the public API consumed from outside the menu module.
#[cfg(target_os = "macos")]
pub use menu_handlers::{cleanup_macos_menus, set_macos_menu_icons};
pub use menu_handlers::{
    frontend_shortcut_to_accelerator, rebuild_view_mode_items, sync_view_mode_check_states,
    update_menu_item_accelerator,
};
pub use menu_structure::{
    FileContextInfo, build_breadcrumb_context_menu, build_context_menu, build_menu, build_network_host_context_menu,
    build_tab_context_menu, build_viewer_menu,
};

/// Menu item IDs for file actions.
pub const SHOW_HIDDEN_FILES_ID: &str = "show_hidden_files";
/// View mode CheckMenuItems, one pair per pane, nested under per-pane submenus
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
pub const SELECT_FILES_ID: &str = "select_files";
pub const DESELECT_FILES_ID: &str = "deselect_files";
pub const TOGGLE_SELECTION_ID: &str = "toggle_selection";

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
/// "Go to path…" (⌘G): opens the Go-to-path dialog (dialog-open is idempotency-guarded).
pub const GO_TO_PATH_ID: &str = "go_to_path";
/// "Go to latest download" (⌘J): jumps the focused pane to the most recent download.
pub const GO_LATEST_DOWNLOAD_ID: &str = "go_latest_download";

/// Menu item IDs for sorting.
pub const SORT_BY_NAME_ID: &str = "sort_by_name";
pub const SORT_BY_EXTENSION_ID: &str = "sort_by_extension";
pub const SORT_BY_SIZE_ID: &str = "sort_by_size";
pub const SORT_BY_MODIFIED_ID: &str = "sort_by_modified";
pub const SORT_BY_CREATED_ID: &str = "sort_by_created";
pub const SORT_ASCENDING_ID: &str = "sort_ascending";
pub const SORT_DESCENDING_ID: &str = "sort_descending";

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

/// Menu item ID for "Eject (name)" in the breadcrumb / volume context menus.
pub const EJECT_VOLUME_ID: &str = "eject_volume";

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

/// Menu item ID for "Onboarding…" (under the Cmdr app menu, macOS only).
/// Linux re-entry to the onboarding wizard is palette-only by design (see
/// `apps/desktop/src/lib/onboarding/CLAUDE.md` § "Re-entry points").
pub const OPEN_ONBOARDING_ID: &str = "open_onboarding";

/// Whether a command requires the main window to be focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandScope {
    /// Always emit regardless of window focus (Settings, About, Command palette, etc.)
    App,
    /// Only emit when the main window is focused (file operations, navigation, etc.)
    FileScoped,
}

/// View mode type that matches the frontend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    Full,
    #[default]
    Brief,
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
        OPEN_ONBOARDING_ID => Some(("cmdr.openOnboarding", CommandScope::App)),

        // Pane commands (file-scoped)
        SWITCH_PANE_ID => Some(("pane.switch", CommandScope::FileScoped)),
        SWAP_PANES_ID => Some(("pane.swap", CommandScope::FileScoped)),

        // Navigation commands (file-scoped)
        GO_BACK_ID => Some(("nav.back", CommandScope::FileScoped)),
        GO_FORWARD_ID => Some(("nav.forward", CommandScope::FileScoped)),
        GO_PARENT_ID => Some(("nav.parent", CommandScope::FileScoped)),
        GO_TO_PATH_ID => Some(("nav.goToPath", CommandScope::FileScoped)),
        GO_LATEST_DOWNLOAD_ID => Some(("downloads.goToLatest", CommandScope::FileScoped)),

        // Tab commands (file-scoped)
        NEW_TAB_ID => Some(("tab.new", CommandScope::FileScoped)),
        CLOSE_TAB_ID => Some(("tab.close", CommandScope::FileScoped)),
        REOPEN_CLOSED_TAB_ID => Some(("tab.reopen", CommandScope::FileScoped)),
        NEXT_TAB_ID => Some(("tab.next", CommandScope::FileScoped)),
        PREV_TAB_ID => Some(("tab.prev", CommandScope::FileScoped)),
        PIN_TAB_MENU_ID => Some(("tab.togglePin", CommandScope::FileScoped)),
        CLOSE_OTHER_TABS_ID => Some(("tab.closeOthers", CommandScope::FileScoped)),

        // Clipboard operations: cut/copy/paste are handled specially in on_menu_event
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
        SELECT_FILES_ID => Some(("selection.selectFiles", CommandScope::FileScoped)),
        DESELECT_FILES_ID => Some(("selection.deselectFiles", CommandScope::FileScoped)),
        TOGGLE_SELECTION_ID => Some(("selection.toggle", CommandScope::FileScoped)),

        // Cloud actions (macOS File Provider)
        CLOUD_MAKE_OFFLINE_ID => Some(("cloud.makeOffline", CommandScope::FileScoped)),
        CLOUD_REMOVE_DOWNLOAD_ID => Some(("cloud.removeDownload", CommandScope::FileScoped)),

        // Zoom (text size): App scope so ⌘0/⌘+/⌘- work in any focused window.
        VIEW_ZOOM_75_ID => Some(("view.zoom.set75", CommandScope::App)),
        VIEW_ZOOM_100_ID => Some(("view.zoom.set100", CommandScope::App)),
        VIEW_ZOOM_125_ID => Some(("view.zoom.set125", CommandScope::App)),
        VIEW_ZOOM_150_ID => Some(("view.zoom.set150", CommandScope::App)),
        VIEW_ZOOM_IN_ID => Some(("view.zoom.in", CommandScope::App)),
        VIEW_ZOOM_OUT_ID => Some(("view.zoom.out", CommandScope::App)),

        // Sort items: mapped so user-customized accelerators can flow into the menu via the
        // generic update path. At runtime, `on_menu_event` intercepts these IDs *before* this
        // lookup and emits `menu-sort` instead of `execute-command`, so this mapping never
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
        "cmdr.openOnboarding" => Some(OPEN_ONBOARDING_ID),
        "pane.switch" => Some(SWITCH_PANE_ID),
        "pane.swap" => Some(SWAP_PANES_ID),
        "nav.back" => Some(GO_BACK_ID),
        "nav.forward" => Some(GO_FORWARD_ID),
        "nav.parent" => Some(GO_PARENT_ID),
        "nav.goToPath" => Some(GO_TO_PATH_ID),
        "downloads.goToLatest" => Some(GO_LATEST_DOWNLOAD_ID),
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
        "selection.selectFiles" => Some(SELECT_FILES_ID),
        "selection.deselectFiles" => Some(DESELECT_FILES_ID),
        "selection.toggle" => Some(TOGGLE_SELECTION_ID),
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

/// Context for the volume-eject menu item (stored so on_menu_event can emit it).
/// Populated by `show_breadcrumb_context_menu` when an ejectable volume is in scope.
#[derive(Clone, Default)]
pub struct VolumeEjectMenuContext {
    pub volume_id: String,
    pub volume_name: String,
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
    /// Context for the most recent breadcrumb / volume context menu's eject item.
    /// Cleared (volume_id empty) when the menu was built without an ejectable target.
    pub volume_eject_context: Mutex<VolumeEjectMenuContext>,
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
            volume_eject_context: Mutex::new(VolumeEjectMenuContext::default()),
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
    /// Per-pane submenus (Full at position 0, Brief at position 1), used by
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // Command palette is FileScoped: disabled when Settings/viewer has focus
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
            menu_id_to_command(GO_TO_PATH_ID),
            Some(("nav.goToPath", CommandScope::FileScoped))
        );
        assert_eq!(
            menu_id_to_command(GO_LATEST_DOWNLOAD_ID),
            Some(("downloads.goToLatest", CommandScope::FileScoped))
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
        // event path and are not mapped. Only the four shortcut-bound columns are.
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
            "nav.goToPath",
            "downloads.goToLatest",
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
            "selection.selectFiles",
            "selection.deselectFiles",
            "help.sendErrorReport",
            "app.checkForUpdates",
            "cmdr.openOnboarding",
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
    fn test_command_id_to_menu_id_unmapped() {
        // Commands without menu items return None
        assert_eq!(command_id_to_menu_id("view.fullMode"), None);
        assert_eq!(command_id_to_menu_id("view.briefMode"), None);
        assert_eq!(command_id_to_menu_id("view.showHidden"), None);
        assert_eq!(command_id_to_menu_id("unknown"), None);
    }
}
