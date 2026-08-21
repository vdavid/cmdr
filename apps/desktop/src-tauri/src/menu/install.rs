//! Building the menu bar at app startup and stashing what mutates it later.
//!
//! One call from `lib.rs`'s `setup`. The order inside is load-bearing: the UI
//! language has to be pinned before `build_menu` reads the catalog through
//! `menu_t`, the macOS AppKit passes only work on an already-installed bar, and
//! the `MenuState` refs have to be stored before any frontend push can arrive.
//!
//! Later rebuilds (a language change, a licence change) go through
//! `rebuild::rebuild_menu_bar`, not here.

use tauri::Manager;

use crate::file_system;
use crate::intl;
use crate::licensing;
use crate::settings::loader::Settings;

use super::{MenuState, ViewMode};
use crate::ignore_poison::IgnorePoison;

/// Build the menu bar in the user's language, run the macOS post-construction
/// passes, and place the `MenuState` the frontend and IPC layer mutate.
pub fn at_startup(app: &tauri::App, settings: &Settings) -> tauri::Result<()> {
    // Check if there's an existing license (for menu text)
    let has_existing_license = licensing::get_license_info(app.handle()).is_some();

    // The menu bar is built next, and it reads the catalog through
    // `menu_t`, so the language the user pinned (or `'system'`, meaning
    // the OS's answer) has to be known first. `settings.json` is already
    // on hand; the frontend re-pushes the same value when it loads, and
    // `set_ui_language` rebuilds the bar if the two ever disagree.
    // The `bool` means "rebuild the menu bar", and there is no menu bar yet: it's built a few
    // statements down, already in the right language.
    intl::set_language_preference(settings.appearance_language.clone());

    // Build and set the application menu with persisted showHiddenFiles
    // Note: view mode is per-pane and managed by frontend, so we default to Brief here
    let menu_items = super::build_menu(
        app.handle(),
        settings.show_hidden_files,
        ViewMode::Brief,
        has_existing_license,
    )?;

    let menu_state = MenuState::default();
    // Cached so a language-change rebuild puts the same licence wording
    // back without repeating the lookup.
    menu_state
        .has_existing_license
        .store(has_existing_license, std::sync::atomic::Ordering::Relaxed);
    let main_menu = menu_state.store_item_refs(menu_items);

    // On macOS, keep a clone of the main menu so `activate_window_menu` can swap the
    // app-level menu bar back to it on main / Settings / Debug focus-gain. The clone shares
    // the same underlying items (Tauri's `Menu` is reference-counted), so the item refs
    // stored above keep mutating the live menu. macOS has a single app-level menu bar
    // (tauri-apps/tauri#5768), so there's no per-window menu to set here.
    #[cfg(target_os = "macos")]
    let main_menu_clone = main_menu.clone();
    app.set_menu(main_menu)?;

    // Remove macOS system-injected Edit menu items and register Help menu for search
    #[cfg(target_os = "macos")]
    super::cleanup_macos_menus(app.handle());

    // Set SF Symbol icons on menu items (macOS only)
    #[cfg(target_os = "macos")]
    super::set_macos_menu_icons(app.handle());

    // Subscribe to NSWorkspace launch/terminate notifications so the "Open with"
    // candidate cache invalidates when the user installs or removes apps.
    #[cfg(target_os = "macos")]
    file_system::open_with::start_invalidation_observer();

    // On macOS, build the shared viewer menu once and store it (plus the main-menu clone and
    // the viewer word-wrap ref). `activate_window_menu` swaps the app-level menu bar between
    // these on window focus-gain; `viewer_set_word_wrap` flips the stored CheckMenuItem.
    #[cfg(target_os = "macos")]
    {
        *menu_state.main_menu.lock_ignore_poison() = Some(main_menu_clone);
        let viewer_menu_items = super::build_viewer_menu(app.handle())?;
        *menu_state.viewer_word_wrap.lock_ignore_poison() = Some(viewer_menu_items.word_wrap);
        *menu_state.viewer_menu.lock_ignore_poison() = Some(viewer_menu_items.menu);
    }

    app.manage(menu_state);

    Ok(())
}
