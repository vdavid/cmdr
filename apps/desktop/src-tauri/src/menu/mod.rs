//! Application menu configuration.
//!
//! ## File layout
//!
//! - `mod.rs` (this file): shared types (`MenuState`, `MenuItems`, `MenuItemEntry`, `MenuContext`,
//!   `NetworkHostMenuContext`, `CommandScope`, `ViewMode`).
//! - `command_map.rs`: all menu item ID constants plus the ID ↔ command-registry mapping
//!   (`menu_id_to_command` and `command_id_to_menu_id`), glob-re-exported from `mod.rs`.
//! - `menu_bar.rs`: the menu bar for both platforms, one row per item (`MENU_BAR`), written in the
//!   words `menu_spec.rs` defines. `menu_bar_builder.rs` builds it for the running platform
//!   (`build_menu`), and `mnemonics.rs` allocates the Linux underline letters.
//! - `menu_items.rs`: small shared pieces: `APP_MENU_TITLE`, `pin_tab_label`, `detach_label`, and
//!   `truncate_for_menu_label`.
//! - `file_context_menu.rs`: the file context menu (the one a right-click on a row opens), plus
//!   `FileContextInfo` / `ContextMenuPaneFacts` / `ContextMenuResult`.
//! - `menu_structure.rs`: the smaller context menus (breadcrumb, parent row, function key bar, tab,
//!   network host, server row, volume row, favorite), the viewer menu, and the
//!   `ContextMenuShortcuts` / `context_item` vocabulary they all share with the file one.
//! - `item_states.rs`: `apply_menu_item_states` (the single writer of every main-menu item's
//!   enabled state), `set_menu_context`, and the macOS app-menu-bar swap between main and viewer.
//! - `menu_handlers.rs`: `handle_menu_event`, the `.on_menu_event` dispatcher wired into the Tauri
//!   builder, plus the macOS post-construction helpers it shares a platform with
//!   (`cleanup_macos_menus`, `set_macos_menu_icons`, and the responder-chain edit actions).
//! - `accelerators.rs`: `frontend_shortcut_to_menu_text` (frontend glyphs → Tauri accelerator
//!   strings), `frontend_shortcut_to_accelerator` (the same, floored to combos the menu bar may
//!   register), and `update_menu_item_accelerator` (swapping one on a live item).
//! - `view_mode_items.rs`: keeping the per-pane view-mode items in step, via a full
//!   `rebuild_view_mode_items` or a cheap `sync_view_mode_check_states`.
//! - `macos_appkit.rs`: the objc2 passes that fix the built menu bar up (`cleanup_macos_menus`,
//!   `set_macos_menu_icons`), plus the `MENU_BAR_ICONS` table.
//! - `display_accelerators.rs` (macOS): the third such pass, drawing the shortcuts a menu item can
//!   only SHOW as a right-aligned, dimmed run on its attributed title.
//! - `context_menu_facts.rs` (macOS): the file context menu's slow facts (tags, Drive, File
//!   Provider, "Open with", Share, iCloud status), gathered off the main thread under a grace
//!   period. `context_menu_live.rs` fills in the ones that answered after the menu went up.
//! - `open_with.rs` (macOS): "Open with" submenu builder.
//! - `context_menu_icons.rs` (macOS): every image on right-click items (SF Symbols, provider
//!   logos, app icons, share icons, tag circles), which needs the tracking notification because
//!   Tauri exposes no `NSMenu` for a context menu.
//! - `provider_logos.rs` (macOS): which File Provider's logo is which, by app bundle ID.
//! - `context_menu_header.rs`: the right-click menu's first line, naming what it will act on, plus
//!   the macOS pass that makes it read as a header rather than a greyed-out command.
//! - `tag_row/`: the right-click menu's Finder tag colors as one row of circles (macOS), installed
//!   over the seven plain tag items when the menu starts tracking. `tag_icons.rs` draws those
//!   items' fallback bitmaps.

mod accelerators;
mod command_map;
#[cfg(target_os = "macos")]
pub mod context_menu_facts;
mod context_menu_header;
#[cfg(target_os = "macos")]
mod context_menu_icons;
#[cfg(target_os = "macos")]
mod context_menu_live;
#[cfg(target_os = "macos")]
mod display_accelerators;
mod file_context_menu;
#[cfg(target_os = "macos")]
mod file_provider_items;
pub mod install;
mod item_states;
// `pub(crate)` for one helper: `dock::menu` builds its own `NSMenu` by hand (Tauri
// exposes none) and puts SF Symbols on it with `set_sf_symbol`, so the same glyph
// rules cover the menu bar and the Dock tile menu. Everything else here stays internal.
#[cfg(target_os = "macos")]
pub(crate) mod macos_appkit;
mod media_index_items;
mod menu_bar;
mod menu_bar_builder;
mod menu_handlers;
mod menu_items;
mod menu_spec;
mod menu_structure;
mod mnemonics;
#[cfg(target_os = "macos")]
pub mod open_with;
#[cfg(target_os = "macos")]
mod provider_logos;
mod rebuild;
mod selection_submenu;
#[cfg(target_os = "macos")]
mod services_context;
#[cfg(target_os = "macos")]
pub mod share_submenu;
#[cfg(target_os = "macos")]
mod tag_icons;
mod tag_row;
mod view_mode_items;

use std::collections::{HashMap, HashSet};

use self::menu_spec::{Platform, display_accelerator_label};
use crate::ignore_poison::IgnorePoison as _;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use tauri::{
    Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, Submenu},
};

// Re-export the public API consumed from outside the menu module.
// All menu item ID constants and the ID ↔ command-registry mapping functions live in
// `command_map`; the glob keeps every existing `crate::menu::…` / `super::…` import path valid.
pub use accelerators::{
    frontend_shortcut_to_accelerator, frontend_shortcut_to_menu_text, update_menu_item_accelerator,
};
pub use command_map::*;
#[cfg(target_os = "macos")]
pub use context_menu_header::lend_context_menu_header;
pub use context_menu_header::{ContextMenuTarget, ContextMenuTargetFacts};
#[cfg(target_os = "macos")]
pub use context_menu_icons::lend_context_menu_icons;
#[cfg(target_os = "macos")]
pub use context_menu_live::{late_sink, lend_live_menu, next_generation};
// On macOS the command builds its `FileContextInfo` through `context_menu_facts`, never by name.
#[cfg(not(target_os = "macos"))]
pub use file_context_menu::FileContextInfo;
pub use file_context_menu::{ContextMenuPaneFacts, build_context_menu};
pub(crate) use item_states::{apply_menu_item_states, set_menu_context};
#[cfg(target_os = "macos")]
pub(crate) use item_states::{note_viewer_search_focus, swap_to_main_menu, swap_to_viewer_menu};
pub use media_index_items::{ImageIndexMenuState, image_index_menu_items};
pub use menu_bar_builder::build_menu;
pub use menu_handlers::handle_menu_event;
#[cfg(target_os = "macos")]
pub use menu_handlers::{
    cleanup_macos_menus, cleanup_macos_menus_from_command, set_display_accelerators,
    set_display_accelerators_from_command, set_macos_menu_icons, set_macos_menu_icons_from_command,
};
pub(crate) use menu_items::DetachWord;
pub use menu_items::{SameKindTarget, pin_tab_label, same_kind_menu_label};
pub use menu_structure::{
    ContextMenuShortcuts, build_breadcrumb_context_menu, build_function_key_bar_context_menu,
    build_network_host_context_menu, build_parent_row_context_menu, build_tab_context_menu, build_viewer_menu,
};
pub use rebuild::rebuild_menu_bar;
#[cfg(target_os = "macos")]
pub use services_context::lend_services_menu;
#[cfg(target_os = "macos")]
pub use tag_row::lend_tag_row;
pub use view_mode_items::{rebuild_view_mode_items, sync_view_mode_check_states};

/// `settings-changed`: a CheckMenuItem toggle (currently only "Show hidden
/// files") flipped a setting from the native menu. The menu click is the
/// authoritative state change (see `menu/CLAUDE.md`), so the FE writes the new
/// value into `listing.showHiddenFiles` rather than re-toggling. Also emitted
/// from `commands/menu.rs` when the `toggle_hidden_files` IPC (the MCP
/// `toggle_hidden` tool) flips the same item.
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

/// The viewer menu plus the item refs captured at build time.
///
/// On macOS the viewer menu is built once at startup and shared across all viewer windows. Holding
/// the refs lets `viewer_set_word_wrap` update the checkbox, and `apply_menu_item_states` grey the
/// two Edit items, in O(1) instead of walking the menu tree.
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
    /// The Edit submenu's Cut and Paste, which are Custom items on macOS alone (elsewhere they
    /// stay Predefined, because forwarding the native selector needs the responder chain).
    #[cfg(target_os = "macos")]
    pub edit_cut: MenuItem<R>,
    #[cfg(target_os = "macos")]
    pub edit_paste: MenuItem<R>,
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
    /// The File Provider actions the most recent file context menu offered, which an
    /// `fp-action:<index>` click indexes into. Set when the menu is built.
    #[cfg(target_os = "macos")]
    pub file_provider_offer: Option<crate::file_system::file_provider_actions::ProviderOffer>,
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

/// The breadcrumb menu's Eject target (stored so on_menu_event can emit it): the volume's id +
/// name. Populated by `show_breadcrumb_context_menu`.
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
    /// The item's label WITHOUT any display-only accelerator spelled into it, mnemonic and
    /// all. ❗ Kept separately because `item.text()` is the built label, which on Linux ends
    /// in ` (⇧8)`: recreating the item from that would stack a second shortcut on the end
    /// every time the user rebinds. `update_menu_item_accelerator` composes from here.
    pub label: String,
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
    /// Whether the user already had a licence key when the bar was last built,
    /// which decides between the "See license details" and "Enter license key…"
    /// wording. Cached so `rebuild_menu_bar` can put the same label back without
    /// a second licence lookup (which isn't generic over the runtime).
    pub has_existing_license: AtomicBool,
    /// Whether the main file explorer owns the menu right now (`activate_window_menu`).
    /// Stored so the operation-item state can be recomputed from BOTH inputs; without
    /// it, a focus round-trip through Settings would re-enable Copy while a dialog is
    /// still up, since `set_menu_context` enables every explorer item.
    pub explorer_menu_active: AtomicBool,
    /// Whether the main window currently refuses to START a file operation: a dialog
    /// is up, or the Ask Cmdr composer has focus. Pushed by the frontend
    /// (`routes/(main)/menu-operation-gate.svelte.ts`), which is the only writer.
    ///
    /// ⚠️ Greying items is CHROME. A disabled item's accelerator still fires, so this
    /// is never the guard: the refusals in `mcp/executor`, `pane/operation-start-gate.ts`,
    /// and `pane/dialog-state.svelte.ts` are.
    pub file_operations_blocked: AtomicBool,
    /// Whether the FOCUSED PANE sits somewhere a shell can `cd` into, deciding
    /// "Open terminal here". Pushed by the frontend
    /// (`lib/open-terminal/menu-gate.svelte.ts`), its only writer. Stored for the
    /// same reason as `file_operations_blocked`: `set_menu_context` enables every
    /// explorer item, so the verdict has to be re-appliable after a focus round-trip
    /// or a menu-bar rebuild. Starts `true`, matching the item as it's built.
    pub open_terminal_here_enabled: AtomicBool,
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
    /// Defaults match the left pane's accelerators in `menu_bar.rs` (Cmd+1 / Cmd+2).
    pub view_mode_full_accel: Mutex<Option<String>>,
    pub view_mode_brief_accel: Mutex<Option<String>>,
    /// Pin/unpin tab menu item (label toggles based on active tab state)
    pub pin_tab: Mutex<Option<MenuItem<R>>>,
    /// Whether the focused pane's closed-tab stack has entries, deciding "Reopen closed tab". Pushed by
    /// `set_reopen_closed_tab_enabled`, and stored for the same reason as `open_terminal_here_enabled`: every
    /// item's enabled state is recomputed from all its inputs at once. Starts `false`, matching the item as
    /// it's built.
    pub reopen_closed_tab_enabled: AtomicBool,
    /// The frontend commands the main window's dialog gate refuses right now: every `BLOCKED_BY_DIALOGS`
    /// command while a dialog, an explorer overlay, or the command palette is up, and nothing once it's gone.
    /// Pushed by `set_commands_refused_over_dialog`, whose only writer is
    /// `routes/(main)/menu-dialog-gate.svelte.ts`. Greys those items out, and reverts a refused click on
    /// the two check items (`refuses_over_dialog`).
    pub commands_refused_over_dialog: Mutex<HashSet<String>>,
    /// Generic menu items keyed by menu item ID, for accelerator and enable/disable updates.
    pub items: Mutex<HashMap<String, MenuItemEntry<R>>>,
    /// What each rebound item's DISPLAY-only accelerator should say now, overriding the one
    /// `MENU_BAR` was built with. `Some` draws that combo, `None` draws nothing at all.
    ///
    /// Written by `update_menu_accelerator` alone, which knows which way a rebind went:
    /// a combo carrying ⌘ / ⌃ / ⌥ becomes a real accelerator and this says `None` so the two
    /// can't both show; anything else can't be registered (`accelerators.rs`'s modifier floor)
    /// and lands here instead. Empty until the first rebind, which is why a fresh bar reads
    /// straight off the spec. A language rebuild throws the map away with the bar it described
    /// and the frontend re-pushes, exactly as it does for real accelerators.
    pub display_accelerators: Mutex<HashMap<String, Option<String>>>,
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
    /// The viewer menu's Edit > Cut and Edit > Paste, captured at build time for the same reason.
    /// Their enabled state follows `viewer_search_focus`, through `apply_menu_item_states`.
    #[cfg(target_os = "macos")]
    pub viewer_edit_cut: Mutex<Option<MenuItem<R>>>,
    #[cfg(target_os = "macos")]
    pub viewer_edit_paste: Mutex<Option<MenuItem<R>>>,
    /// The label of the viewer whose SEARCH BOX holds keyboard focus, or `None` when none does.
    /// It's the only editable field in a viewer window, so it's what decides whether the two items
    /// above are live. Pushed by each viewer (`viewer_set_search_input_focused`) on the input's
    /// focus and blur, on the search bar closing, and on the window's focus-gain, which is what
    /// makes a window switch converge (`note_viewer_search_focus`).
    ///
    /// ⚠️ Greying is CHROME here as everywhere else, and ⌘X / ⌘V reach the search box whatever
    /// this says: see `viewer_text_edit_enabled`.
    #[cfg(target_os = "macos")]
    pub viewer_search_focus: Mutex<Option<String>>,
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
            has_existing_license: AtomicBool::new(false),
            explorer_menu_active: AtomicBool::new(true),
            file_operations_blocked: AtomicBool::new(false),
            open_terminal_here_enabled: AtomicBool::new(true),
            view_left_pane_submenu: Mutex::new(None),
            view_right_pane_submenu: Mutex::new(None),
            view_mode_active_pane: Mutex::new("left".to_string()),
            view_mode_left: Mutex::new(ViewMode::Brief),
            view_mode_right: Mutex::new(ViewMode::Brief),
            view_mode_full_accel: Mutex::new(Some("Cmd+1".to_string())),
            view_mode_brief_accel: Mutex::new(Some("Cmd+2".to_string())),
            pin_tab: Mutex::new(None),
            reopen_closed_tab_enabled: AtomicBool::new(false),
            commands_refused_over_dialog: Mutex::new(HashSet::new()),
            items: Mutex::new(HashMap::new()),
            display_accelerators: Mutex::new(HashMap::new()),
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
            #[cfg(target_os = "macos")]
            viewer_edit_cut: Mutex::new(None),
            #[cfg(target_os = "macos")]
            viewer_edit_paste: Mutex::new(None),
            #[cfg(target_os = "macos")]
            viewer_search_focus: Mutex::new(None),
        }
    }
}

impl<R: Runtime> MenuState<R> {
    /// Whether the main window's dialog gate refuses `command_id` right now (`commands_refused_over_dialog`).
    pub fn refuses_over_dialog(&self, command_id: &str) -> bool {
        self.commands_refused_over_dialog
            .lock_ignore_poison()
            .contains(command_id)
    }

    /// Give a tracked menu-bar item a new label, keeping whatever display-only shortcut it draws.
    ///
    /// For an item whose words depend on live state (today only "Select all of the same kind",
    /// which names what the cursor row would select). Two things have to move together, which is
    /// why this isn't a bare `set_text`:
    ///
    /// - on Linux the display-only shortcut lives INSIDE the label, so it's recomposed here;
    /// - [`MenuItemEntry::label`] is what `update_menu_item_accelerator` rebuilds the item from, so
    ///   leaving it stale would revert the label the next time the user rebinds the command.
    ///
    /// ❗ On macOS the caller must follow with `set_display_accelerators`: `set_text` replaces the
    /// `NSMenuItem`'s attributed title with a plain one, and the dimmed glyph goes with it.
    pub fn set_item_label(&self, menu_id: &str, label: String) -> Result<(), String> {
        // Read before taking `items`: `set_text` below blocks on the main thread, which may be
        // inside `set_display_accelerators` holding this very lock.
        let shortcut = match self.display_accelerators.lock_ignore_poison().get(menu_id) {
            Some(rebound) => rebound.clone(),
            None => menu_bar::spec_display_accelerator(menu_id).map(str::to_string),
        };
        let mut items = self.items.lock_ignore_poison();
        let entry = items
            .get_mut(menu_id)
            .ok_or_else(|| format!("The menu bar holds no `{menu_id}` item"))?;
        let built = match shortcut {
            Some(shortcut) => display_accelerator_label(&label, &shortcut, Platform::current()),
            None => label.clone(),
        };
        entry.item.set_text(built).map_err(|e| e.to_string())?;
        entry.label = label;
        Ok(())
    }

    /// Store every item reference a freshly built bar hands back, and return the bar itself.
    ///
    /// Both paths that build a bar end in the same assignments: the startup build
    /// (`install::at_startup`) and a language rebuild (`rebuild::rebuild_menu_bar`).
    /// Handing the `Menu` back only from here means a new [`MenuItems`] field can't be
    /// stored in one path and quietly forgotten in the other.
    pub fn store_item_refs(&self, items: MenuItems<R>) -> Menu<R> {
        *self.show_hidden_files.lock_ignore_poison() = Some(items.show_hidden_files);
        *self.view_mode_full_left.lock_ignore_poison() = Some(items.view_mode_full_left);
        *self.view_mode_brief_left.lock_ignore_poison() = Some(items.view_mode_brief_left);
        *self.view_mode_full_right.lock_ignore_poison() = Some(items.view_mode_full_right);
        *self.view_mode_brief_right.lock_ignore_poison() = Some(items.view_mode_brief_right);
        *self.view_left_pane_submenu.lock_ignore_poison() = Some(items.view_left_pane_submenu);
        *self.view_right_pane_submenu.lock_ignore_poison() = Some(items.view_right_pane_submenu);
        *self.pin_tab.lock_ignore_poison() = Some(items.pin_tab);
        *self.items.lock_ignore_poison() = items.items;
        // The overrides described the bar being replaced, so they go with it. The frontend
        // re-pushes every custom shortcut after a rebuild (`MenuBarRebuilt`), which is what
        // puts back the real accelerators too.
        self.display_accelerators.lock_ignore_poison().clear();
        *self.sort_submenu.lock_ignore_poison() = Some(items.sort_submenu);
        items.menu
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
    /// Generic menu items for accelerator updates, keyed by menu item ID.
    pub items: HashMap<String, MenuItemEntry<R>>,
    /// Sort by submenu (disabled when not in explorer context)
    pub sort_submenu: Submenu<R>,
}
