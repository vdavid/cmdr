//! The smaller context menus (breadcrumb, parent row, function key bar, tab,
//! network host, the viewer's right-click menu) and the viewer-window menu,
//! plus the `ContextMenuShortcuts` / `context_item` vocabulary every popup here
//! shares. The file context menu is `file_context_menu.rs`; the main menu bar is
//! `menu_bar.rs`.

use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::intl::menu_t;

#[cfg(target_os = "macos")]
use super::HELP_MENU_ID;
#[cfg(target_os = "macos")]
use super::menu_items::APP_MENU_TITLE;
use super::menu_items::{DetachWord, detach_label, pin_tab_label};
use super::{
    COPY_CURRENT_DIR_PATH_ID, EDIT_MENU_ID, EJECT_VOLUME_ID, FAVORITES_ADD_CONTEXT_ID, FUNCTION_KEY_BAR_HIDE_ID,
    NETWORK_HOST_DISCONNECT_ID, NETWORK_HOST_EDIT_ID, NETWORK_HOST_FORGET_SECRET_ID, NETWORK_HOST_FORGET_SERVER_ID,
    TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, VIEWER_CONTEXT_COPY_ID, VIEWER_CONTEXT_SELECT_ALL_ID,
    VIEWER_EDIT_COPY_ID, VIEWER_SELECT_ALL_ID, VIEWER_WORD_WRAP_ID, ViewerMenuItems,
};
#[cfg(target_os = "macos")]
use super::{VIEWER_EDIT_CUT_ID, VIEWER_EDIT_PASTE_ID};
use super::{frontend_shortcut_to_menu_text, menu_id_to_command};

/// The user's keyboard shortcuts as they stand right now: command-registry id → the
/// combo in the frontend's canonical spelling (`⌘⇧C`, `F5`, `Space`). The frontend
/// pushes it with every popup, straight out of the shortcut registry, so a command
/// nobody has bound is simply absent.
///
/// This is how a popup menu's accelerator labels stay true after a rebind. They were
/// literals once, and a literal starts lying the moment the user changes a key in
/// Settings > Shortcuts.
///
/// ❗ These accelerators never FIRE. A popup's key equivalents are never registered
/// with the app (only the menu bar's are), so every one of them is display text. That
/// is why the lookup goes through `frontend_shortcut_to_menu_text` and ❌ never
/// `frontend_shortcut_to_accelerator`: the menu bar's modifier floor would drop a bare
/// `Space` or `F5`, which is right there and wrong here.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(transparent)]
pub struct ContextMenuShortcuts(HashMap<String, String>);

impl ContextMenuShortcuts {
    /// The accelerator to show beside the item with this MENU id, in Tauri's format.
    ///
    /// Menu id in, not command id: `menu_id_to_command` is already the one table
    /// binding the two vocabularies, and taking the same id the item is built with
    /// means the label can't end up describing a different command.
    ///
    /// `None` when nothing is bound — and also for a menu id that maps to no command
    /// at all, which is a caller bug rather than a user state, and shows up as a
    /// missing label rather than a wrong one.
    pub fn for_menu_item(&self, menu_id: &str) -> Option<String> {
        let (command_id, _scope) = menu_id_to_command(menu_id)?;
        frontend_shortcut_to_menu_text(self.0.get(command_id)?)
    }
}

/// One context-menu item, labelled with whatever its command is bound to right now.
///
/// The menu id is used twice on purpose: as the item's own id, and as the key the
/// accelerator is looked up by. One argument, so the label can't drift from the item.
pub(super) fn context_item<R: Runtime>(
    app: &AppHandle<R>,
    shortcuts: &ContextMenuShortcuts,
    menu_id: &str,
    label: String,
    enabled: bool,
) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(
        app,
        menu_id,
        label,
        enabled,
        shortcuts.for_menu_item(menu_id).as_deref(),
    )
}

/// Builds the minimal context menu for the `..` parent row: a single "Add to favorites" item that
/// favorites the parent directory. The full file context menu (Copy / Move / Delete, etc.) makes no
/// sense on `..`, so this is its own one-item menu. The caller stashes the parent dir in
/// `MenuState.context.path`; `on_menu_event` reads it back for the `FAVORITES_ADD_CONTEXT_ID` click.
pub fn build_parent_row_context_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let add_favorite_item = MenuItem::with_id(
        app,
        FAVORITES_ADD_CONTEXT_ID,
        menu_t("menu.context.addToFavorites"),
        true,
        None::<&str>,
    )?;
    menu.append(&add_favorite_item)?;
    Ok(menu)
}

/// Builds the minimal context menu for the function key bar: a single "Hide function key bar"
/// item. Unlike the parent-row favorite, the action needs no right-clicked context to stash, so
/// `on_menu_event` routes the click straight to the frontend via the `FunctionKeyBarHideRequested`
/// event rather than intercepting it with stashed state.
pub fn build_function_key_bar_context_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let hide_item = MenuItem::with_id(
        app,
        FUNCTION_KEY_BAR_HIDE_ID,
        menu_t("menu.context.hideFunctionKeyBar"),
        true,
        None::<&str>,
    )?;
    menu.append(&hide_item)?;
    Ok(menu)
}

/// Builds a context menu for the breadcrumb path bar.
///
/// Its one command's accelerator comes from `shortcuts` like the file menu's do; see
/// [`ContextMenuShortcuts`]. `eject_volume_name`, when present, appends the detach item that lets the user
/// leave the volume the breadcrumb represents: `Eject ({name})` for a disk,
/// `Disconnect` for a phone (`detach_word`). The caller is responsible for
/// stashing the matching `volume_id` in `MenuState.volume_eject_context` so
/// `on_menu_event` can dispatch the click.
///
/// When `eject_busy` is true, the item is rendered disabled with a ` (busy)`
/// suffix, so a volume with a write op reading from / writing to it can't be
/// ejected mid-transfer (mirrors the disabled eject button in the picker).
pub fn build_breadcrumb_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    shortcuts: &ContextMenuShortcuts,
    eject_volume_name: Option<&str>,
    eject_busy: bool,
    detach_word: DetachWord,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let copy_path_item = context_item(
        app,
        shortcuts,
        COPY_CURRENT_DIR_PATH_ID,
        menu_t("menu.breadcrumb.copyPath"),
        true,
    )?;
    menu.append(&copy_path_item)?;
    if let Some(name) = eject_volume_name {
        let eject_item = MenuItem::with_id(
            app,
            EJECT_VOLUME_ID,
            detach_label(name, eject_busy, detach_word),
            !eject_busy,
            None::<&str>,
        )?;
        menu.append(&eject_item)?;
    }
    Ok(menu)
}

/// The viewer Edit menu's two custom accelerators, in Tauri's accelerator syntax.
///
/// They replace what the Predefined items used to carry, so the printed chords don't move: ⌘C /
/// ⌘A on macOS, Ctrl+C / Ctrl+A on Linux. ❗ `Cmd` binds to SUPER on GTK (muda maps it to META),
/// which is why this splits per platform rather than sharing one string; `menu_bar.rs`'s header
/// has the full story.
#[cfg(target_os = "macos")]
const VIEWER_COPY_ACCELERATOR: &str = "Cmd+C";
#[cfg(not(target_os = "macos"))]
const VIEWER_COPY_ACCELERATOR: &str = "Ctrl+C";
#[cfg(target_os = "macos")]
const VIEWER_SELECT_ALL_ACCELERATOR: &str = "Cmd+A";
#[cfg(not(target_os = "macos"))]
const VIEWER_SELECT_ALL_ACCELERATOR: &str = "Ctrl+A";
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
                Some(VIEWER_COPY_ACCELERATOR),
            )?,
            &edit_paste,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                VIEWER_SELECT_ALL_ID,
                menu_t("menu.select.all"),
                true,
                Some(VIEWER_SELECT_ALL_ACCELERATOR),
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
        #[cfg(target_os = "macos")]
        edit_cut,
        #[cfg(target_os = "macos")]
        edit_paste,
    })
}

/// Builds the viewer's right-click menu over the file text: Copy, enabled only while there's a
/// selection, and Select all. The frontend reads its own selection model at open time and passes
/// `has_selection`, so the native menu never needs to see it.
///
/// Labels and printed chords are the viewer bar's own, so the two menus read alike. The chords
/// are display text here: a popup's key equivalents are never registered.
pub fn build_viewer_context_menu<R: Runtime>(app: &AppHandle<R>, has_selection: bool) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let copy_item = MenuItem::with_id(
        app,
        VIEWER_CONTEXT_COPY_ID,
        menu_t("menu.edit.copy"),
        has_selection,
        Some(VIEWER_COPY_ACCELERATOR),
    )?;
    let select_all_item = MenuItem::with_id(
        app,
        VIEWER_CONTEXT_SELECT_ALL_ID,
        menu_t("menu.select.all"),
        true,
        Some(VIEWER_SELECT_ALL_ACCELERATOR),
    )?;
    menu.append(&copy_item)?;
    menu.append(&select_all_item)?;
    Ok(menu)
}

/// Builds a context menu for a tab.
pub fn build_tab_context_menu(
    app: &AppHandle<Wry>,
    is_pinned: bool,
    can_close: bool,
    has_other_unpinned_tabs: bool,
) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;

    let pin_item = MenuItem::with_id(app, TAB_PIN_ID, pin_tab_label(is_pinned), true, None::<&str>)?;
    let close_others_item = MenuItem::with_id(
        app,
        TAB_CLOSE_OTHERS_ID,
        menu_t("menu.tab.closeOtherTabs"),
        has_other_unpinned_tabs,
        None::<&str>,
    )?;
    let close_item = MenuItem::with_id(app, TAB_CLOSE_ID, menu_t("menu.tab.closeTab"), can_close, None::<&str>)?;

    menu.append(&pin_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&close_others_item)?;
    menu.append(&close_item)?;

    Ok(menu)
}

/// Which items an SMB host's context menu offers, in order, as menu ids.
///
/// ❗ Only what does something: Disconnect only while a share from this host is
/// mounted (an item that answers "no mounted shares" is an inert affordance), Edit
/// only for a saved host, Forget server only for a typed-in one, Forget saved
/// password only when one is stored. One group, in the order the switcher row's
/// menu uses (edit, disconnect, forget the password, forget the server): three
/// items used to sit in three separator groups.
pub fn network_host_menu_items(
    is_manual: bool,
    is_saved: bool,
    has_credentials: bool,
    has_mounts: bool,
) -> Vec<&'static str> {
    [
        (is_saved, NETWORK_HOST_EDIT_ID),
        (has_mounts, NETWORK_HOST_DISCONNECT_ID),
        (has_credentials, NETWORK_HOST_FORGET_SECRET_ID),
        (is_manual, NETWORK_HOST_FORGET_SERVER_ID),
    ]
    .into_iter()
    .filter_map(|(offered, id)| offered.then_some(id))
    .collect()
}

/// Builds a context menu for a network host from [`network_host_menu_items`].
pub fn build_network_host_context_menu(
    app: &AppHandle<Wry>,
    is_manual: bool,
    is_saved: bool,
    has_credentials: bool,
    has_mounts: bool,
) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for id in network_host_menu_items(is_manual, is_saved, has_credentials, has_mounts) {
        let label = match id {
            NETWORK_HOST_EDIT_ID => "menu.network.edit",
            NETWORK_HOST_DISCONNECT_ID => "menu.network.disconnect",
            NETWORK_HOST_FORGET_SECRET_ID => "menu.network.forgetSavedPassword",
            _ => "menu.network.forgetServer",
        };
        menu.append(&MenuItem::with_id(app, id, menu_t(label), true, None::<&str>)?)?;
    }
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ❗ An SMB host's menu offers only what does something, in one group: no
    /// Disconnect with nothing mounted (QA round 2).
    #[test]
    fn a_host_menu_offers_only_what_does_something() {
        assert_eq!(
            network_host_menu_items(true, true, false, false),
            [NETWORK_HOST_EDIT_ID, NETWORK_HOST_FORGET_SERVER_ID]
        );
        assert_eq!(
            network_host_menu_items(true, true, true, true),
            [
                NETWORK_HOST_EDIT_ID,
                NETWORK_HOST_DISCONNECT_ID,
                NETWORK_HOST_FORGET_SECRET_ID,
                NETWORK_HOST_FORGET_SERVER_ID
            ]
        );
        assert!(network_host_menu_items(false, false, false, false).is_empty());
    }
    use crate::menu::{FAVORITES_ADD_CONTEXT_ID, FILE_COPY_ID, FILE_VIEW_ID, TOGGLE_SELECTION_ID};

    fn shortcuts(pairs: &[(&str, &str)]) -> ContextMenuShortcuts {
        ContextMenuShortcuts(
            pairs
                .iter()
                .map(|(id, combo)| ((*id).to_string(), (*combo).to_string()))
                .collect(),
        )
    }

    /// The default binding reaches the item through the registry, not through a
    /// literal in the builder.
    #[test]
    fn a_bound_command_labels_its_menu_item() {
        let shortcuts = shortcuts(&[("file.copy", "F5")]);
        assert_eq!(shortcuts.for_menu_item(FILE_COPY_ID), Some("F5".to_string()));
    }

    /// The whole point of the milestone: rebind Copy in Settings and the right-click
    /// menu says the new key.
    #[test]
    fn a_rebound_command_shows_the_new_key() {
        let shortcuts = shortcuts(&[("file.copy", "⌘⇧K")]);
        assert_eq!(shortcuts.for_menu_item(FILE_COPY_ID), Some("Cmd+Shift+K".to_string()));
    }

    /// A bare key stays a real label here, where the menu bar would refuse one: a
    /// popup accelerator is never registered, so it can't swallow the key app-wide.
    /// Space is the only place the toggle-selection shortcut is discoverable.
    #[test]
    fn a_bare_key_still_labels_a_popup_item() {
        let shortcuts = shortcuts(&[("selection.toggle", "Space")]);
        assert_eq!(shortcuts.for_menu_item(TOGGLE_SELECTION_ID), Some("Space".to_string()));
    }

    /// A command the user has unbound shows no label, rather than the one it shipped
    /// with.
    #[test]
    fn an_unbound_command_shows_nothing() {
        let shortcuts = shortcuts(&[("file.copy", "F5")]);
        assert_eq!(shortcuts.for_menu_item(FILE_VIEW_ID), None);
    }

    /// An item with no command behind it (this one favorites the right-clicked path
    /// in `on_menu_event`) can't resolve a shortcut, and says nothing.
    #[test]
    fn an_item_outside_the_command_map_shows_nothing() {
        let shortcuts = shortcuts(&[("favorites.add", "⌘D")]);
        assert_eq!(shortcuts.for_menu_item(FAVORITES_ADD_CONTEXT_ID), None);
    }
}
