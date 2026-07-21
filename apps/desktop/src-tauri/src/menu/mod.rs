//! Application menu configuration.
//!
//! ## File layout
//!
//! - `mod.rs` (this file): shared types (`MenuState`, `MenuItems`, `MenuItemEntry`, `MenuContext`,
//!   `NetworkHostMenuContext`, `CommandScope`, `ViewMode`).
//! - `command_map.rs`: all menu item ID constants plus the ID ↔ command-registry mapping
//!   (`menu_id_to_command` and `command_id_to_menu_id`), glob-re-exported from `mod.rs`.
//! - `menu_items.rs`: menu item builder helpers and submenu factories (sort, zoom),
//!   accelerator/label platform-aware helpers, `register_item`, and `truncate_for_menu_label`.
//! - `menu_structure.rs`: hierarchical assembly: `build_menu` dispatcher, context menus (file,
//!   breadcrumb, tab, network host), viewer menu, plus `FileContextInfo` / `ContextMenuResult`.
//! - `menu_handlers.rs`: event handlers and live-update helpers: `handle_menu_event` (the
//!   `.on_menu_event` dispatcher wired into the Tauri builder), `rebuild_view_mode_items`,
//!   `sync_view_mode_check_states`, `update_menu_item_accelerator`,
//!   `frontend_shortcut_to_accelerator`, and the macOS post-construction helpers
//!   (`cleanup_macos_menus`, `set_macos_menu_icons`).
//! - `macos.rs` / `linux.rs`: platform-specific menu bar shape.
//! - `open_with.rs` (macOS): "Open with" submenu builder.

mod command_map;
#[cfg(not(target_os = "macos"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod media_index_items;
mod menu_handlers;
mod menu_items;
mod menu_structure;
#[cfg(target_os = "macos")]
pub mod open_with;
#[cfg(target_os = "macos")]
mod tag_icons;

use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{
    Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
};

// Re-export the public API consumed from outside the menu module.
// All menu item ID constants and the ID ↔ command-registry mapping functions live in
// `command_map`; the glob keeps every existing `crate::menu::…` / `super::…` import path valid.
pub use command_map::*;
pub use media_index_items::{ImageIndexMenuState, image_index_menu_items};
#[cfg(target_os = "macos")]
pub use menu_handlers::{cleanup_macos_menus, cleanup_macos_menus_from_command, set_macos_menu_icons};
pub use menu_handlers::{
    frontend_shortcut_to_accelerator, handle_menu_event, rebuild_view_mode_items, sync_view_mode_check_states,
    update_menu_item_accelerator,
};
pub use menu_structure::{
    FileContextInfo, build_breadcrumb_context_menu, build_context_menu, build_menu, build_network_host_context_menu,
    build_parent_row_context_menu, build_tab_context_menu, build_viewer_menu, build_volume_row_context_menu,
};

/// `settings-changed`: a CheckMenuItem toggle (currently only "Show hidden
/// files") flipped a setting from the native menu. The menu click is the
/// authoritative state change (see `menu/CLAUDE.md`), so the FE applies the new
/// value rather than re-toggling. Also emitted from `commands/ui.rs` when the
/// `toggle_hidden_files` IPC flips the same setting.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct SettingsChanged {
    pub show_hidden_files: bool,
}

/// `view-mode-changed`: a per-pane view-mode CheckMenuItem (Full / Brief)
/// flipped from the native menu. Carries the target pane so the FE updates that
/// pane's mode without changing focus. `mode` is `"full"` / `"brief"`, `pane`
/// is `"left"` / `"right"`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct ViewModeChanged {
    pub mode: String,
    pub pane: String,
}

/// `menu-sort`: a Sort-by menu item (column or order) clicked. `action` is
/// `"sortBy"` (then `value` is a column name) or `"sortOrder"` (then `value` is
/// `"asc"` / `"desc"`). The FE has a dedicated listener that maps this onto a
/// focused-pane `sort.*` command.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MenuSort {
    pub action: String,
    pub value: String,
}

/// `media-index-folder-exclusion`: a folder's "Don't index images in this folder" /
/// "Index images here again" context-menu item was clicked. Carries the right-clicked
/// folder's absolute path and the target state. The FE listens and drives its persist +
/// live-apply path (`mediaIndex.excludedFolders` + `media_index_set_excluded_folder`),
/// so the setting survives a restart (the native menu can't write the FE settings
/// store). Emitted `emit_to("main")`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MediaIndexFolderExclusion {
    pub folder: String,
    pub excluded: bool,
}

/// `media-index-folder-choice`: a folder's "Add to indexed folders" / "Remove from
/// indexed folders" context-menu item was clicked. Carries the right-clicked folder's
/// absolute path and the target membership. The FE listens and drives its persist +
/// live-apply path (`mediaIndex.alwaysIndexFolders` + `media_index_set_always_index_folder`,
/// which kicks a pass on an add), so the choice survives a restart (the native menu can't
/// write the FE settings store). Emitted `emit_to("main")`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct MediaIndexFolderChoice {
    pub folder: String,
    pub chosen: bool,
}

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

/// Which app-level menu is currently installed on macOS.
///
/// macOS has a single app-level menu bar (no per-window menus, see tauri-apps/tauri#5768), so we
/// swap the whole bar via `app.set_menu()` when windows gain focus. This tracker lets
/// `activate_window_menu` skip redundant swaps (main→main, viewer→viewer). Settings / Debug reuse
/// the main menu (with items disabled), so they map to `Main` too.
#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveMenuKind {
    #[default]
    Main,
    Viewer,
}

/// The viewer menu plus the `Word wrap` CheckMenuItem ref captured at build time.
///
/// On macOS the viewer menu is built once at startup and shared across all viewer windows. Holding
/// the `word_wrap` ref lets `viewer_set_word_wrap` update the checkbox in O(1) instead of walking
/// the menu tree.
pub struct ViewerMenuItems<R: Runtime> {
    pub menu: Menu<R>,
    /// Only read on macOS: the viewer app-menu swap is macOS-only, and `lib.rs` captures this ref
    /// under `#[cfg(target_os = "macos")]` for `viewer_set_word_wrap`. On Linux the field is built
    /// but never read, so allow dead_code off macOS to keep the `deny(dead_code)` Linux build green.
    #[cfg_attr(
        not(target_os = "macos"),
        allow(
            dead_code,
            reason = "viewer word-wrap menu swap is macOS-only; the field is built but never read on Linux"
        )
    )]
    pub word_wrap: CheckMenuItem<R>,
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
    /// The focused pane's `listing_id` at the time the menu was shown, so a
    /// `tag-color:N` click can refresh that listing's cache after writing tags.
    /// Empty when the caller has no listing to refresh (the tag still writes to disk).
    pub tags_listing_id: String,
}

/// Context for the network host context menu (stored so on_menu_event can emit it).
#[derive(Clone, Default)]
pub struct NetworkHostMenuContext {
    pub host_id: String,
    pub host_name: String,
}

/// Context for a volume / favorite row context menu (stored so on_menu_event can emit it).
/// Carries the target's id + name for whichever action the user picks (eject, or favorite
/// rename / remove). Populated by `show_breadcrumb_context_menu` and `show_volume_row_context_menu`.
#[derive(Clone, Default)]
pub struct VolumeRowMenuContext {
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
    /// Context for the most recent breadcrumb / volume / favorite row context menu.
    /// Holds the target id + name for the picked action (eject, favorite rename / remove).
    /// Cleared (volume_id empty) when a breadcrumb menu was built without an ejectable target.
    pub volume_row_context: Mutex<VolumeRowMenuContext>,
    /// The main app menu, cloned at startup before `app.set_menu()`. `app.set_menu()` swaps the
    /// app-level menu bar back to this when the main / Settings / Debug window gains focus. The
    /// clone shares the same underlying items (Tauri's `Menu` is a reference-counted handle), so
    /// the stored item refs in the fields above keep mutating the live menu.
    #[cfg(target_os = "macos")]
    pub main_menu: Mutex<Option<Menu<R>>>,
    /// The shared viewer menu, built once at startup. Installed via `app.set_menu()` on viewer
    /// focus-gain.
    #[cfg(target_os = "macos")]
    pub viewer_menu: Mutex<Option<Menu<R>>>,
    /// Which app-level menu is installed right now. Lets `activate_window_menu` skip redundant
    /// swaps.
    #[cfg(target_os = "macos")]
    pub active_menu_kind: Mutex<ActiveMenuKind>,
    /// The viewer menu's `Word wrap` CheckMenuItem, captured at build time so `viewer_set_word_wrap`
    /// updates it in O(1) without a tree walk.
    #[cfg(target_os = "macos")]
    pub viewer_word_wrap: Mutex<Option<CheckMenuItem<R>>>,
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
            volume_row_context: Mutex::new(VolumeRowMenuContext::default()),
            #[cfg(target_os = "macos")]
            main_menu: Mutex::new(None),
            #[cfg(target_os = "macos")]
            viewer_menu: Mutex::new(None),
            #[cfg(target_os = "macos")]
            active_menu_kind: Mutex::new(ActiveMenuKind::default()),
            #[cfg(target_os = "macos")]
            viewer_word_wrap: Mutex::new(None),
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
