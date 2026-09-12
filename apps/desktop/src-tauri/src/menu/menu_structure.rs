//! Context menus (file, breadcrumb, tab, network host, function key bar) and the
//! viewer-window menu. The main menu bar is `menu_bar.rs`.

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

use tauri::{
    AppHandle, Runtime, Wry,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

#[cfg(target_os = "macos")]
use crate::file_system::google_drive::DriveItemLinks;
#[cfg(target_os = "macos")]
use crate::file_system::open_with::OpenWithChoices;
#[cfg(target_os = "macos")]
use crate::file_system::share::ShareService;
#[cfg(target_os = "macos")]
use crate::file_system::sync_status::SyncStatus;

use crate::intl::{menu_t, menu_t_with};

use super::context_menu_header::{ContextMenuTargetFacts, append_context_menu_header};

#[cfg(target_os = "macos")]
use super::OPEN_TERMINAL_HERE_ID;
use super::menu_bar::{COPY_PATH_ACCELERATOR, SHOW_IN_FILE_MANAGER_ACCELERATOR, SHOW_IN_FILE_MANAGER_KEY};
#[cfg(target_os = "macos")]
use super::menu_items::APP_MENU_TITLE;
use super::menu_items::{COPY_FILENAME_MAX_CHARS, DetachWord, detach_label, pin_tab_label, truncate_for_menu_label};
#[cfg(target_os = "macos")]
use super::{
    CLOUD_MAKE_OFFLINE_ID, CLOUD_REMOVE_DOWNLOAD_ID, DRIVE_ASK_GEMINI_ID, DRIVE_COPY_LINK_ID, DRIVE_OPEN_ID,
    GET_INFO_ID, HELP_MENU_ID, QUICK_LOOK_ID,
};
use super::{
    COPY_CURRENT_DIR_PATH_ID, COPY_FILENAME_ID, COPY_PATH_ID, EDIT_ID, EDIT_MENU_ID, EJECT_VOLUME_ID,
    FAVORITE_REMOVE_ID, FAVORITE_RENAME_ID, FAVORITES_ADD_CONTEXT_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DUPLICATE_ID,
    FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, FUNCTION_KEY_BAR_HIDE_ID, ImageIndexMenuState,
    NETWORK_HOST_DISCONNECT_ID, NETWORK_HOST_FORGET_SECRET_ID, NETWORK_HOST_FORGET_SERVER_ID, OPEN_ID, RENAME_ID,
    SERVER_DISCONNECT_ID, SERVER_EDIT_ID, SERVER_FORGET_ID, SERVER_FORGET_SECRET_ID, SERVER_OPEN_ID, SERVER_PIN_ID,
    SERVER_UNPIN_ID, SHOW_IN_FINDER_ID, TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, TOGGLE_SELECTION_ID,
    VIEWER_WORD_WRAP_ID, ViewerMenuItems, image_index_menu_items,
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
    /// The Google Drive web URLs for this item, when it resolves to one. `Some`
    /// gates the Drive menu group, which is self-validating: no ID, no item; its
    /// `gemini_url` gates `Ask Gemini` alone, since folders have none. See
    /// `file_system/google_drive/` for why this isn't a path-prefix check (Drive's
    /// mirror mode puts real files outside `~/Library/CloudStorage`).
    pub google_drive_links: Option<DriveItemLinks>,
    pub open_with: OpenWithChoices,
    /// The services macOS offers for this selection, in its own order, one `Share`
    /// submenu item each. EMPTY means macOS offers none and the whole item is left
    /// out: an empty share sheet holding only `Edit Extensions…` is the symptom the
    /// submenu replaced. Filled by `file_system::share::services_for`.
    pub share_services: Vec<ShareService>,
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

/// What the PANE the right-click landed in contributes, as opposed to the file
/// under the cursor.
///
/// One struct rather than three trailing `bool`s, for the reason the frontend's
/// `PaneContextMenuFacts` gives: same-typed positional flags are exactly what binds
/// to the wrong slot when one is inserted. Every field's most restrictive answer is
/// its `Default`, so a surface that can't answer says nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct ContextMenuPaneFacts {
    /// Suppresses Rename, Duplicate, and the two create items, which only make
    /// sense on a real directory. `true` from the search-results virtual pane
    /// (`volumeId == "search-results"`, see `apps/desktop/src/lib/search/capabilities.ts`).
    /// Source-side actions (Open, Copy, Move, Delete, Show in Finder, Copy filename,
    /// Copy path) stay, because the underlying paths are real.
    pub restrict_destination_actions: bool,
    /// Whether "Open terminal here" is clickable. It acts on the pane's FOLDER, not
    /// this file, so a pane on MTP or ADB shows it greyed out; the snapshot pane and
    /// the Search dialog pass `false` too, having no folder of their own to open.
    pub can_open_terminal_here: bool,
    /// Whether `Share` and `Services` may appear at all. The pane's answer too, but to
    /// a different question: whether its ROWS are real OS paths, which is what both a
    /// share service and a macOS service need (each takes file URLs). The search-results
    /// snapshot says yes (its rows are real files) where `can_open_terminal_here` says
    /// no, so the two can't be folded into one flag. `Share` needs one more yes on top:
    /// macOS has to actually offer a service (`FileContextInfo::share_services`).
    pub can_share: bool,
}

/// Builds a context menu for a specific file.
///
/// `pane` is what the surface the click landed in contributes; see
/// [`ContextMenuPaneFacts`] for each answer and who gives it.
pub fn build_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    filename: &str,
    is_directory: bool,
    #[cfg_attr(
        not(target_os = "macos"),
        allow(unused_variables, reason = "all reads of `info` sit inside macOS-gated branches")
    )]
    info: &FileContextInfo,
    pane: ContextMenuPaneFacts,
    // Media-index image-search facts about the right-clicked folder; `image_index_menu_items`
    // turns them into the folder-only chosen/exclusion items (empty when the master toggle
    // is off).
    image_index: ImageIndexMenuState,
    // What the right-clicked ROW(S) are, for the header line at the very top; see
    // `ContextMenuTargetFacts`.
    target: ContextMenuTargetFacts<'_>,
) -> tauri::Result<ContextMenuResult<R>> {
    let ContextMenuPaneFacts {
        restrict_destination_actions,
        can_open_terminal_here,
        can_share,
    } = pane;
    // Both gate macOS-only items, so on Linux they're read nowhere.
    #[cfg(not(target_os = "macos"))]
    let _ = (can_open_terminal_here, can_share);
    let menu = Menu::new(app)?;

    // What this menu will act on, first line, above everything. Cmdr acts on the whole
    // selection or on the one right-clicked row depending on whether the click landed
    // inside the selection, and this is the only place that says which.
    append_context_menu_header(app, &menu, filename, target)?;

    // Open / View / Edit group (files only)
    #[cfg(target_os = "macos")]
    let mut open_with_apps: HashMap<String, PathBuf> = HashMap::new();
    if !is_directory {
        let open_item = MenuItem::with_id(app, OPEN_ID, menu_t("menu.file.open"), true, None::<&str>)?;
        let view_item = MenuItem::with_id(app, FILE_VIEW_ID, menu_t("menu.file.view"), true, Some("F3"))?;
        let edit_item = MenuItem::with_id(app, EDIT_ID, menu_t("menu.context.edit"), true, Some("F4"))?;
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
    let toggle_selection_item = MenuItem::with_id(
        app,
        TOGGLE_SELECTION_ID,
        menu_t("menu.context.toggleSelection"),
        true,
        Some("Space"),
    )?;
    menu.append(&toggle_selection_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Finder tag colors (macOS): seven circles that toggle the system color tags on the
    // selection. Shown for files and folders (Finder tags both).
    #[cfg(target_os = "macos")]
    append_tag_color_group(app, &menu, info)?;

    // Copy / Move / Duplicate / Rename group. Rename and Duplicate are omitted on the
    // search-results virtual pane: the underlying file CAN be renamed, but doing it from
    // the snapshot view splits the file (snapshot keeps the old name, disk has the new)
    // which is confusing, and a duplicate would have to land in each item's own real
    // folder, which one transfer can't express. The user can navigate to the real folder
    // and do either there.
    let copy_item = MenuItem::with_id(app, FILE_COPY_ID, menu_t("menu.file.copy"), true, Some("F5"))?;
    let move_item = MenuItem::with_id(app, FILE_MOVE_ID, menu_t("menu.file.move"), true, Some("F6"))?;
    menu.append(&copy_item)?;
    menu.append(&move_item)?;
    if !restrict_destination_actions {
        let duplicate_item = MenuItem::with_id(
            app,
            FILE_DUPLICATE_ID,
            menu_t("menu.file.duplicate"),
            true,
            Some("Cmd+D"),
        )?;
        menu.append(&duplicate_item)?;
        let rename_item = MenuItem::with_id(app, RENAME_ID, menu_t("menu.file.rename"), true, Some("F2"))?;
        menu.append(&rename_item)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // New folder / New file — also omitted on search-results panes (no destination
    // folder to create into; the pane IS the snapshot, not a directory).
    if !restrict_destination_actions {
        let new_folder_item =
            MenuItem::with_id(app, FILE_NEW_FOLDER_ID, menu_t("menu.file.newFolder"), true, Some("F7"))?;
        let new_file_item = MenuItem::with_id(
            app,
            FILE_NEW_FILE_ID,
            menu_t("menu.file.newFile"),
            true,
            Some("Shift+F4"),
        )?;
        menu.append(&new_folder_item)?;
        menu.append(&new_file_item)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    // Delete
    let delete_item = MenuItem::with_id(app, FILE_DELETE_ID, menu_t("menu.file.delete"), true, Some("F8"))?;
    menu.append(&delete_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Utility group: Show in Finder, Copy filename, Copy path
    let show_in_finder_item = MenuItem::with_id(
        app,
        SHOW_IN_FINDER_ID,
        menu_t(SHOW_IN_FILE_MANAGER_KEY.current()),
        true,
        SHOW_IN_FILE_MANAGER_ACCELERATOR.current(),
    )?;
    let copy_filename_item = MenuItem::with_id(
        app,
        COPY_FILENAME_ID,
        menu_t_with(
            "menu.context.copyNamed",
            &[("name", &truncate_for_menu_label(filename, COPY_FILENAME_MAX_CHARS))],
        ),
        true,
        Some("Cmd+C"),
    )?;
    let copy_path_item = MenuItem::with_id(
        app,
        COPY_PATH_ID,
        menu_t("menu.edit.copyPath"),
        true,
        COPY_PATH_ACCELERATOR.current(),
    )?;
    menu.append(&show_in_finder_item)?;
    // "Open terminal here" rides beside Show in Finder, same gesture aimed at a
    // different app. macOS only, like the launch module behind it.
    #[cfg(target_os = "macos")]
    {
        let open_terminal_here_item = MenuItem::with_id(
            app,
            OPEN_TERMINAL_HERE_ID,
            menu_t("menu.file.openTerminalHere"),
            can_open_terminal_here,
            Some("Alt+Cmd+T"),
        )?;
        menu.append(&open_terminal_here_item)?;
        // `Share` rides with them for the same reason: all three hand the selection to
        // something outside Cmdr. It's ABSENT, never greyed, on two counts, and the
        // second is why it's a submenu at all:
        // - the pane's rows don't live on the OS filesystem (a phone, an archive's
        //   insides), so there are no file URLs to hand over, and no wording of a greyed
        //   item explains "this row isn't a file yet" better than its absence does;
        // - macOS offers no service for this selection (a path that vanished, a broken
        //   symlink), which only an enumeration can answer. The system popover can't:
        //   it comes up empty but for `Edit Extensions…`, which is the bug that put the
        //   list in a submenu.
        if can_share && !info.share_services.is_empty() {
            menu.append(&super::share_submenu::build_share_submenu(app, &info.share_services)?)?;
        }
    }
    menu.append(&copy_filename_item)?;
    menu.append(&copy_path_item)?;

    // Add to favorites — directories only (favorites are folders), and not on the search-results
    // snapshot pane (its rows aren't a stable folder to favorite). Favorites the right-clicked
    // folder's path, which `on_menu_event` reads from `MenuState.context.path`.
    if is_directory && !restrict_destination_actions {
        let add_favorite_item = MenuItem::with_id(
            app,
            FAVORITES_ADD_CONTEXT_ID,
            menu_t("menu.context.addToFavorites"),
            true,
            None::<&str>,
        )?;
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
                let menu_item = MenuItem::with_id(app, item.id, menu_t(item.label_key), item.enabled, None::<&str>)?;
                menu.append(&menu_item)?;
            }
        }
    }

    // Cloud group (macOS). Provider-aware: each provider contributes only the
    // actions it can actually carry out, so the group is a concatenation rather
    // than one iCloud-shaped block.
    //
    // Google Drive: open on the web / copy the link / ask Gemini about it. Drive's
    // own Share sheet and its pin-offline toggle are File Provider custom actions
    // only Finder can invoke, so the web page (where Share is one click away) is
    // the honest equivalent. `file_system/google_drive/` has the full story.
    #[cfg(target_os = "macos")]
    if let Some(links) = &info.google_drive_links {
        let open_item = MenuItem::with_id(
            app,
            DRIVE_OPEN_ID,
            menu_t("menu.context.openInGoogleDrive"),
            true,
            None::<&str>,
        )?;
        let copy_link_item = MenuItem::with_id(
            app,
            DRIVE_COPY_LINK_ID,
            menu_t("menu.context.copyGoogleDriveLink"),
            true,
            None::<&str>,
        )?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&open_item)?;
        menu.append(&copy_link_item)?;
        // Files only: Gemini's `?di=` names a document, and a folder resolves no
        // Gemini URL at all.
        if links.gemini_url.is_some() {
            let ask_gemini_item = MenuItem::with_id(
                app,
                DRIVE_ASK_GEMINI_ID,
                menu_t("menu.context.askGemini"),
                true,
                None::<&str>,
            )?;
            menu.append(&ask_gemini_item)?;
        }
    }

    // Eviction pair: iCloud Drive ONLY, and gated by sync status. The
    // `FileManager` ubiquity APIs behind these accept iCloud URLs and nothing
    // else; a third-party provider's pin/unpin is a File Provider custom action
    // reserved for the app that bundles the extension. ❌ Don't widen this to
    // other providers — see `file_system/cloud_actions.rs`.
    #[cfg(target_os = "macos")]
    if info.is_icloud_drive {
        let cloud_item = match info.sync_status {
            SyncStatus::OnlineOnly => Some(MenuItem::with_id(
                app,
                CLOUD_MAKE_OFFLINE_ID,
                menu_t("menu.context.makeAvailableOffline"),
                true,
                None::<&str>,
            )?),
            SyncStatus::Synced => Some(MenuItem::with_id(
                app,
                CLOUD_REMOVE_DOWNLOAD_ID,
                menu_t("menu.context.removeDownload"),
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
        let get_info_item = MenuItem::with_id(app, GET_INFO_ID, menu_t("menu.file.getInfo"), true, Some("Cmd+I"))?;
        let quick_look_item = MenuItem::with_id(app, QUICK_LOOK_ID, menu_t("menu.file.quickLook"), true, None::<&str>)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&get_info_item)?;
        menu.append(&quick_look_item)?;
        // `Services` goes last, where Finder puts it, and rides on the same fact as
        // "Share…": AppKit hands a service file URLs, so a pane whose rows aren't OS
        // paths has nothing to offer. Only the item is built here — AppKit's own menu
        // is borrowed while the menu is up (`services_context.rs`).
        if can_share {
            super::services_context::append_services_submenu(app, &menu)?;
        }
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
/// `handle_menu_event`. Colors run in Finder's order (Red … Gray). The label carries
/// the color's NAME so the items stay accessible (screen readers read the text; the
/// circle is the icon), which is why the names are translated alongside everything
/// else. macOS-only — Linux menus carry no icons.
#[cfg(target_os = "macos")]
fn append_tag_color_group<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>, info: &FileContextInfo) -> tauri::Result<()> {
    use tauri::menu::IconMenuItem;

    // (color index, catalog key), in Finder's color-row order.
    const COLORS: [(u8, &str); 7] = [
        (6, "menu.tag.red"),
        (7, "menu.tag.orange"),
        (5, "menu.tag.yellow"),
        (2, "menu.tag.green"),
        (4, "menu.tag.blue"),
        (3, "menu.tag.purple"),
        (1, "menu.tag.gray"),
    ];

    for (color, name_key) in COLORS {
        let id = format!("{}{}", super::TAG_COLOR_ID_PREFIX, color);
        let checked = info.applied_tag_colors[color as usize];
        // `IconMenuItem` with `Some(image)` falls back to a text-only item if the image
        // build fails, so the menu still works without the circle.
        let icon = super::tag_icons::tag_circle_image(color, checked);
        let item = IconMenuItem::with_id(app, &id, menu_t(name_key), true, icon, None::<&str>)?;
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
/// `accelerator` is the user's configured shortcut for the "Copy path" command (in
/// Tauri accelerator format, e.g. "Cmd+Opt+C"), or empty if none is set.
/// `eject_volume_name`, when present, appends the detach item that lets the user
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
    accelerator: &str,
    eject_volume_name: Option<&str>,
    eject_busy: bool,
    detach_word: DetachWord,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    let accel: Option<&str> = if accelerator.is_empty() {
        None
    } else {
        Some(accelerator)
    };
    let copy_path_item = MenuItem::with_id(
        app,
        COPY_CURRENT_DIR_PATH_ID,
        menu_t("menu.breadcrumb.copyPath"),
        true,
        accel,
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

/// What a SERVER row's context menu offers, as the caller sees the row.
///
/// ❗ The caller decides which items apply, ❌ never this builder:
/// `show_volume_row_context_menu` is a synchronous command. ❗ And "is a secret
/// stored for this?" is asked by NOBODY on this path: it costs a Keychain read,
/// every read of one can raise a system prompt, and a right-click is not a moment
/// to spend one, so "Forget saved password" is offered unconditionally and the
/// command it runs reports whether there was one.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerRowMenu {
    /// Whether there is a session to drop (`showsDisconnect` in
    /// `navigation/connection-state.ts`: a `direct` or `disconnected` place).
    pub shows_disconnect: bool,
    /// Whether a saved entry exists, so "Forget server" has something to forget.
    pub is_saved: bool,
    /// Whether the place is in the volume switcher right now, which decides
    /// whether the row offers "Pin to switcher" or "Unpin".
    pub pinned: bool,
    /// Whether a write operation is touching the volume right now. ❗ Disables
    /// every destructive item exactly like the eject item, because dropping the
    /// session or the credential under a running copy breaks it.
    pub busy: bool,
}

/// Appends a server row's items, in the order
/// `apps/desktop/src/lib/file-explorer/navigation/DETAILS.md` § "Eject button +
/// row context menu" records: Open, Edit…, Disconnect (when live), Pin to
/// switcher / Unpin, Forget saved password, Forget server (when it is saved).
///
/// ❗ A server row shows Disconnect, ❌ never Eject: "Eject" promises
/// safe-to-unplug, and a server has nothing to unplug.
fn append_server_row_items<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    server: &ServerRowMenu,
) -> tauri::Result<()> {
    // Open and Edit… lead, the way the row's own two purposes rank: going there,
    // and changing what "there" means. ❗ Neither is gated by `busy`, unlike the
    // three destructive items below: navigating into a server a copy is reading
    // from is fine, and editing its settings touches no session.
    let open = MenuItem::with_id(app, SERVER_OPEN_ID, menu_t("menu.network.open"), true, None::<&str>)?;
    menu.append(&open)?;
    if server.is_saved {
        // ❌ Only for a SAVED server: the sheet edits a store entry, and a live
        // volume nothing saved has none to open.
        let edit = MenuItem::with_id(app, SERVER_EDIT_ID, menu_t("menu.network.edit"), true, None::<&str>)?;
        menu.append(&edit)?;
    }
    if server.shows_disconnect {
        let key = if server.busy {
            "menu.volume.disconnectBusy"
        } else {
            "menu.network.disconnect"
        };
        let item = MenuItem::with_id(app, SERVER_DISCONNECT_ID, menu_t(key), !server.busy, None::<&str>)?;
        menu.append(&item)?;
    }
    // ❗ Never disabled by `busy`, unlike the three below it: a pin is a view
    // preference the switcher reads, so moving it while a copy runs breaks
    // nothing.
    let (pin_id, pin_key) = if server.pinned {
        (SERVER_UNPIN_ID, "menu.network.unpin")
    } else {
        (SERVER_PIN_ID, "menu.network.pinToSwitcher")
    };
    let pin_item = MenuItem::with_id(app, pin_id, menu_t(pin_key), true, None::<&str>)?;
    menu.append(&pin_item)?;
    // ❗ Offered on every server row, ❌ never gated on "is a secret stored?":
    // answering that costs a Keychain read, and every read of one can raise a
    // system prompt. A right-click is not a moment to spend one, which is the
    // rule SMB's host menu already follows. The COMMAND answers instead:
    // `forget_server_secret` reports whether an entry was there, and the caller
    // words a `false` (`navigation/server-row-actions.ts::forgetSavedSecret`).
    let key = if server.busy {
        "menu.volume.forgetSavedPasswordBusy"
    } else {
        "menu.network.forgetSavedPassword"
    };
    let item = MenuItem::with_id(app, SERVER_FORGET_SECRET_ID, menu_t(key), !server.busy, None::<&str>)?;
    menu.append(&item)?;
    if server.is_saved {
        let key = if server.busy {
            "menu.volume.forgetServerBusy"
        } else {
            "menu.network.forgetServer"
        };
        let item = MenuItem::with_id(app, SERVER_FORGET_ID, menu_t(key), !server.busy, None::<&str>)?;
        menu.append(&item)?;
    }
    Ok(())
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
    // Predefined items carry the native cut:/copy:/paste:/selectAll: selectors, which
    // macOS routes to the focused text field (the search box) through the responder
    // chain. All four are needed: without Cut/Paste, ⌘X/⌘V are dead in the viewer's
    // search input (the viewer menu is the active app menu while a viewer is focused).
    // Carries the same ID as the main bar's Edit menu: `cleanup_macos_menus` runs against whichever
    // bar is installed, and AppKit injects its Writing Tools / AutoFill / Dictation items into this
    // one too. Only one of the two bars is ever installed at a time, so the shared ID never collides.
    let edit_menu = Submenu::with_id_and_items(
        app,
        EDIT_MENU_ID,
        menu_t("menu.bar.edit"),
        true,
        &[
            &PredefinedMenuItem::cut(app, Some(&menu_t("menu.edit.cut")))?,
            &PredefinedMenuItem::copy(app, Some(&menu_t("menu.edit.copy")))?,
            &PredefinedMenuItem::paste(app, Some(&menu_t("menu.edit.paste")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::select_all(app, Some(&menu_t("menu.select.all")))?,
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
    let disconnect = MenuItem::with_id(
        app,
        NETWORK_HOST_DISCONNECT_ID,
        menu_t("menu.network.disconnect"),
        true,
        None::<&str>,
    )?;
    menu.append(&disconnect)?;

    if is_manual {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        let forget_server = MenuItem::with_id(
            app,
            NETWORK_HOST_FORGET_SERVER_ID,
            menu_t("menu.network.forgetServer"),
            true,
            None::<&str>,
        )?;
        menu.append(&forget_server)?;
    }

    if has_credentials {
        if !is_manual {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        let forget_secret = MenuItem::with_id(
            app,
            NETWORK_HOST_FORGET_SECRET_ID,
            menu_t("menu.network.forgetSavedPassword"),
            true,
            None::<&str>,
        )?;
        menu.append(&forget_secret)?;
    }

    Ok(menu)
}

/// Builds the context menu for a row in the volume-selector dropdown.
///
/// Favorites get `Rename` + `Remove`; an ejectable volume gets its detach item,
/// `Eject ({name})` for a disk and `Disconnect` for a phone (`detach_word`),
/// disabled with a ` (busy)` suffix while a write op touches it, mirroring the
/// breadcrumb menu and the inline control. The caller stashes the target id +
/// name in `MenuState.volume_row_context` so `on_menu_event` can dispatch the click.
pub fn build_volume_row_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    is_favorite: bool,
    eject_volume_name: Option<&str>,
    eject_busy: bool,
    detach_word: DetachWord,
    server: Option<&ServerRowMenu>,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;

    if let Some(server) = server {
        append_server_row_items(app, &menu, server)?;
        return Ok(menu);
    }

    if is_favorite {
        let rename_item = MenuItem::with_id(
            app,
            FAVORITE_RENAME_ID,
            menu_t("menu.volume.renameFavorite"),
            true,
            None::<&str>,
        )?;
        menu.append(&rename_item)?;
        let remove_item = MenuItem::with_id(
            app,
            FAVORITE_REMOVE_ID,
            menu_t("menu.volume.removeFavorite"),
            true,
            None::<&str>,
        )?;
        menu.append(&remove_item)?;
    } else if let Some(name) = eject_volume_name {
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
