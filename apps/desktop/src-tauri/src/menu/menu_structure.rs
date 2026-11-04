//! The viewer-window menu bar (`build_viewer_menu`). Every context menu is data in
//! `cmdr_menu::context`, built by `context_rows.rs`; the main menu bar is `cmdr_menu::bar`.

use cmdr_menu::context::popups::{viewer_copy_accelerator, viewer_select_all_accelerator};
use cmdr_menu::spec::Platform;
use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::intl::menu_t;

#[cfg(target_os = "macos")]
use super::HELP_MENU_ID;
#[cfg(target_os = "macos")]
use super::menu_items::APP_MENU_TITLE;
use super::{
    EDIT_MENU_ID, VIEWER_EDIT_COPY_ID, VIEWER_GO_BACK_ID, VIEWER_GO_FORWARD_ID, VIEWER_SELECT_ALL_ID,
    VIEWER_WORD_WRAP_ID, ViewerMenuItems,
};
#[cfg(target_os = "macos")]
use super::{VIEWER_EDIT_CUT_ID, VIEWER_EDIT_PASTE_ID};

/// The viewer Edit menu's Cut and Paste accelerators (macOS, where they're Custom items), in
/// Tauri's syntax. Copy and Select all share theirs with the viewer's text menu
/// (`cmdr_menu::context::popups`).
#[cfg(target_os = "macos")]
const VIEWER_CUT_ACCELERATOR: &str = "Cmd+X";
#[cfg(target_os = "macos")]
const VIEWER_PASTE_ACCELERATOR: &str = "Cmd+V";

/// Builds a menu for viewer windows (built from scratch on all platforms).
///
/// Returns the menu plus the item refs the caller stores in `MenuState`: the `Word wrap`
/// CheckMenuItem, whose checked state then flips in O(1), and on macOS the Edit > Cut / Paste
/// items, which `apply_menu_item_states` greys with the viewer's search box (see
/// `ViewerMenuItems`). On macOS the menu is installed app-level via `app.set_menu()`; on Linux it's
/// a per-window menu (`window.set_menu()`).
pub fn build_viewer_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<ViewerMenuItems<R>> {
    let menu = Menu::new(app)?;

    #[cfg(target_os = "macos")]
    {
        // --- cmdr app menu (minimal for viewer) ---
        let viewer_app_menu = Submenu::with_items(
            app,
            APP_MENU_TITLE,
            true,
            &[
                &PredefinedMenuItem::hide(app, Some(&menu_t("menu.app.hide")))?,
                &PredefinedMenuItem::hide_others(app, Some(&menu_t("menu.app.hideOthers")))?,
                &PredefinedMenuItem::show_all(app, Some(&menu_t("menu.app.showAll")))?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, Some(&menu_t("menu.app.quit")))?,
            ],
        )?;
        menu.append(&viewer_app_menu)?;
    }

    // --- File menu ---
    let file_menu = Submenu::with_items(
        app,
        menu_t("menu.bar.file"),
        true,
        &[&PredefinedMenuItem::close_window(
            app,
            Some(&menu_t("menu.viewer.close")),
        )?],
    )?;
    menu.append(&file_menu)?;

    // --- Edit menu ---
    // A deliberate mix, because the two halves act on different things:
    //
    // - Cut and Paste belong to the search box, the only editable field in the window. On macOS
    //   they're Custom items that forward the native `cut:` / `paste:` selectors down the
    //   responder chain themselves (`send_native_edit_action`), which is what lets
    //   `apply_menu_item_states` grey them out while that box doesn't have focus. ❗ Everywhere
    //   else they stay Predefined: the responder chain is macOS-only, so a Custom item would
    //   leave ⌘X / ⌘V dead in the viewer's search field, which is exactly what trimming them did
    //   once.
    // - Copy and Select all are Custom items routed to the focused viewer's FRONTEND
    //   (`ViewerEditAction`). Their native selectors reach the DOM, and the viewed file isn't
    //   there: `.file-content` is `user-select: none` because the viewer owns an offset-based
    //   selection model, so `selectAll:` would highlight the status bar and `copy:` would copy
    //   it. The frontend runs the same two functions ⌘A / ⌘C already run.
    //
    // ❗ Their ids are VIEWER-specific, not the main bar's `EDIT_COPY_ID` / `SELECT_ALL_ID`: the
    // two bars share `EDIT_MENU_ID` and `handle_menu_event` tells the lanes apart by item id.
    //
    // The submenu carries the same ID as the main bar's Edit menu: `cleanup_macos_menus` runs
    // against whichever bar is installed, and AppKit injects its Writing Tools / AutoFill /
    // Dictation items into this one too. Only one of the two bars is ever installed at a time, so
    // the shared ID never collides.
    //
    // Cut and Paste come up disabled, matching a fresh `MenuState`: no search box has focus yet.
    #[cfg(target_os = "macos")]
    let edit_cut = MenuItem::with_id(
        app,
        VIEWER_EDIT_CUT_ID,
        menu_t("menu.edit.cut"),
        false,
        Some(VIEWER_CUT_ACCELERATOR),
    )?;
    #[cfg(not(target_os = "macos"))]
    let edit_cut = PredefinedMenuItem::cut(app, Some(&menu_t("menu.edit.cut")))?;
    #[cfg(target_os = "macos")]
    let edit_paste = MenuItem::with_id(
        app,
        VIEWER_EDIT_PASTE_ID,
        menu_t("menu.edit.paste"),
        false,
        Some(VIEWER_PASTE_ACCELERATOR),
    )?;
    #[cfg(not(target_os = "macos"))]
    let edit_paste = PredefinedMenuItem::paste(app, Some(&menu_t("menu.edit.paste")))?;

    let edit_menu = Submenu::with_id_and_items(
        app,
        EDIT_MENU_ID,
        menu_t("menu.bar.edit"),
        true,
        &[
            &edit_cut,
            &MenuItem::with_id(
                app,
                VIEWER_EDIT_COPY_ID,
                menu_t("menu.edit.copy"),
                true,
                Some(viewer_copy_accelerator(Platform::current())),
            )?,
            &edit_paste,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                VIEWER_SELECT_ALL_ID,
                menu_t("menu.select.all"),
                true,
                Some(viewer_select_all_accelerator(Platform::current())),
            )?,
        ],
    )?;
    menu.append(&edit_menu)?;

    // --- View menu ---
    // `word_wrap` is returned so the caller (and `viewer_set_word_wrap`) can flip its checked state
    // directly without a tree walk.
    let word_wrap = CheckMenuItem::with_id(
        app,
        VIEWER_WORD_WRAP_ID,
        menu_t("menu.viewer.wordWrap"),
        true,
        false,
        None::<&str>,
    )?;
    let view_submenu = Submenu::with_items(app, menu_t("menu.bar.view"), true, &[&word_wrap])?;
    menu.append(&view_submenu)?;

    // --- Go menu ---
    // Back / Forward through the documents Markdown links led to, in the viewer in front. They
    // come up disabled (a fresh viewer has no history) and `apply_menu_item_states` follows the
    // viewer's pushes (`viewer_set_history_state`). No accelerator, like Word wrap: the viewer's
    // keydown owns ⌘[ / ⌘] through the command registry, so a rebind stays honest and one press
    // can't step twice (a registered key equivalent can reach the webview as well).
    let go_back = MenuItem::with_id(app, VIEWER_GO_BACK_ID, menu_t("menu.go.back"), false, None::<&str>)?;
    let go_forward = MenuItem::with_id(
        app,
        VIEWER_GO_FORWARD_ID,
        menu_t("menu.go.forward"),
        false,
        None::<&str>,
    )?;
    let go_submenu = Submenu::with_items(app, menu_t("menu.bar.go"), true, &[&go_back, &go_forward])?;
    menu.append(&go_submenu)?;

    #[cfg(target_os = "macos")]
    {
        // --- Window menu ---
        let window_menu = Submenu::with_items(
            app,
            menu_t("menu.bar.window"),
            true,
            &[
                &PredefinedMenuItem::minimize(app, Some(&menu_t("menu.window.minimize")))?,
                &PredefinedMenuItem::maximize(app, Some(&menu_t("menu.window.zoom")))?,
            ],
        )?;
        menu.append(&window_menu)?;

        // --- Help menu ---
        // Empty, but it still needs the ID: `cleanup_macos_menus` hands it to
        // `NSApplication.setHelpMenu:` so the viewer bar gets the search field too.
        let help_menu = Submenu::with_id_and_items(app, HELP_MENU_ID, menu_t("menu.bar.help"), true, &[])?;
        menu.append(&help_menu)?;
    }

    Ok(ViewerMenuItems {
        menu,
        word_wrap,
        go_back,
        go_forward,
        #[cfg(target_os = "macos")]
        edit_cut,
        #[cfg(target_os = "macos")]
        edit_paste,
    })
}
