//! The file context menu: the one a right-click on a row opens, and the facts it
//! is built from (`FileContextInfo`, `ContextMenuPaneFacts`, `ContextMenuResult`).
//!
//! Its own file because it is by far the biggest menu here, and the only one whose
//! shape depends on the row, the pane, the cloud provider, and the OS all at once.
//! The smaller context menus (breadcrumb, tab, server row, volume row, favorite,
//! parent row, function key bar) and the viewer menu stay in `menu_structure.rs`,
//! which also owns the `ContextMenuShortcuts` / `context_item` vocabulary all of
//! them share.

// Both the map and its values are the macOS-only "Open with" list, so the imports
// carry the same gate the fields do; ungated, Linux fails `-D unused-imports`.
#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

use tauri::{
    AppHandle, Runtime,
    menu::{Menu, MenuItem, PredefinedMenuItem},
};

#[cfg(target_os = "macos")]
use crate::file_system::file_provider_actions::ProviderOffer;
#[cfg(target_os = "macos")]
use crate::file_system::google_drive::DriveItemLinks;
#[cfg(target_os = "macos")]
use crate::file_system::open_with::OpenWithChoices;
#[cfg(target_os = "macos")]
use crate::file_system::share::ShareService;
#[cfg(target_os = "macos")]
use crate::file_system::sync_status::SyncStatus;

use crate::intl::{menu_t, menu_t_with};

#[cfg(target_os = "macos")]
use super::OPEN_TERMINAL_HERE_ID;
use super::context_menu_header::{ContextMenuTargetFacts, append_context_menu_header};
#[cfg(target_os = "macos")]
use super::context_menu_live::{LateTargets, SlotGroup};
use super::menu_bar::SHOW_IN_FILE_MANAGER_KEY;
use super::menu_items::{COPY_FILENAME_MAX_CHARS, SameKindTarget, truncate_for_menu_label};
use super::menu_structure::{ContextMenuShortcuts, context_item};
use super::selection_submenu::build_selection_submenu;
#[cfg(target_os = "macos")]
use super::{
    CLOUD_MAKE_OFFLINE_ID, CLOUD_REMOVE_DOWNLOAD_ID, DRIVE_ASK_GEMINI_ID, DRIVE_COPY_LINK_ID, DRIVE_OPEN_ID,
    GET_INFO_ID, QUICK_LOOK_ID,
};
use super::{
    COPY_FILENAME_ID, COPY_PATH_ID, EDIT_ID, FAVORITES_ADD_CONTEXT_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DUPLICATE_ID,
    FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, FILE_VIEW_ID, ImageIndexMenuState, OPEN_ID, RENAME_ID,
    SHOW_IN_FINDER_ID, image_index_menu_items,
};

/// A fact the menu asked off the main thread (`context_menu_facts.rs`): answered in time,
/// or still out when the menu went up, in which case its part of the menu is built to be
/// filled in while open (`context_menu_live.rs`).
#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
pub enum Slow<T> {
    Ready(T),
    Pending,
}

#[cfg(target_os = "macos")]
impl<T> Slow<T> {
    /// The answer, when there is one yet.
    pub fn ready(&self) -> Option<&T> {
        match self {
            Slow::Ready(value) => Some(value),
            Slow::Pending => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Slow::Pending)
    }
}

/// Answered, with the empty answer, which is what a fact the menu never asked about is.
#[cfg(target_os = "macos")]
impl<T: Default> Default for Slow<T> {
    fn default() -> Self {
        Slow::Ready(T::default())
    }
}

/// Per-file information needed to build a fully-populated context menu.
///
/// On non-macOS this is empty. On macOS every field but `is_icloud_drive` is a slow fact
/// ([`Slow`]), asked off the main thread and possibly still out when the menu goes up.
#[cfg(target_os = "macos")]
#[derive(Default)]
pub struct FileContextInfo {
    /// The primary row's iCloud sync status, which picks between "Make available offline"
    /// and "Remove download". Asked only in iCloud Drive.
    pub sync_status: Slow<SyncStatus>,
    /// Whether this path is in iCloud Drive specifically. Gates the cloud action menu
    /// items. Eviction / download work via `FileManager` ubiquity APIs, which only
    /// support iCloud (not third-party File Providers). See `cloud_actions.rs` for why.
    /// A path check, so never pending.
    pub is_icloud_drive: bool,
    /// The Google Drive web URLs for this item, when it resolves to one. `Some`
    /// gates the Drive menu group, which is self-validating: no ID, no item; its
    /// `gemini_url` gates `Ask Gemini` alone, since folders have none. See
    /// `file_system/google_drive/` for why this isn't a path-prefix check (Drive's
    /// mirror mode puts real files outside `~/Library/CloudStorage`).
    pub google_drive_links: Slow<Option<DriveItemLinks>>,
    /// The right-clicked rows' File Provider actions, when their provider offers any. Drawn
    /// as one flat group below the cloud items, and kept in `MenuContext` so a click can run
    /// one. `file_system/file_provider_actions/`.
    pub file_provider_offer: Slow<Option<ProviderOffer>>,
    pub open_with: Slow<OpenWithChoices>,
    /// The services macOS offers for this selection, in its own order, one `Share`
    /// submenu item each. EMPTY means macOS offers none and the whole item is left
    /// out: an empty share sheet holding only `Edit Extensions…` is the symptom the
    /// submenu replaced. From `file_system::share::enumerate_offer`.
    pub share_services: Slow<Vec<ShareService>>,
    /// Which of the seven Finder color tags (index 1..=7) the selection already carries.
    /// "Applied" = EVERY selected path has a tag of that color, so the menu shows a
    /// checked (checkmark-composited) circle and the click toggles it off. Index 0 is
    /// unused (colorless). Pending shows no checks until the reads answer.
    pub applied_tag_colors: Slow<[bool; 8]>,
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
    /// The parts built for facts still pending, which the live menu fills in
    /// (`context_menu_live.rs`). Empty when every fact answered in time.
    #[cfg(target_os = "macos")]
    pub late: LateTargets<R>,
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
    /// Whether "Add to favorites" may appear on a folder row at all: whether that row is a
    /// place a favorite could point back to next launch. The ROW's answer, like `can_share`,
    /// and the affordance half of Rust's own `add_favorite` gate — which refuses an archive's
    /// insides, a `.git`-portal folder, a phone, and a protocol-only server. Without it the
    /// item is offered where the add would be refused, and the user gets silence.
    pub can_favorite: bool,
}
/// Builds a context menu for a specific file.
///
/// `pane` is what the surface the click landed in contributes; see
/// [`ContextMenuPaneFacts`] for each answer and who gives it. `shortcuts` is the live
/// shortcut registry every accelerator label in here reads; see
/// [`ContextMenuShortcuts`].
#[allow(
    clippy::too_many_arguments,
    reason = "each one is a separate question about the right-click; folding the four that describe the ROW (`filename`, `is_directory`, and the two the caller's `path` / `paths` become) into `ContextMenuTargetFacts` is the consolidation this wants, and it reaches across the IPC boundary"
)]
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
    // Every accelerator label in this menu, as the registry has them right now.
    shortcuts: &ContextMenuShortcuts,
    // What "Select all of the same kind" would select from the right-clicked row, computed by
    // the frontend as this menu opens. ❗ Live, ❌ never the menu bar's debounced value.
    same_kind: Option<&SameKindTarget>,
) -> tauri::Result<ContextMenuResult<R>> {
    let ContextMenuPaneFacts {
        restrict_destination_actions,
        can_open_terminal_here,
        can_share,
        can_favorite,
    } = pane;
    // Both gate macOS-only items, so on Linux they're read nowhere.
    #[cfg(not(target_os = "macos"))]
    let _ = (can_open_terminal_here, can_share);
    let menu = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    let mut late = LateTargets::new(app);

    // What this menu will act on, first line, above everything. Cmdr acts on the whole
    // selection or on the one right-clicked row depending on whether the click landed
    // inside the selection, and this is the only place that says which.
    append_context_menu_header(app, &menu, filename, target)?;

    // Open / View / Edit group (files only)
    #[cfg(target_os = "macos")]
    let mut open_with_apps: HashMap<String, PathBuf> = HashMap::new();
    if !is_directory {
        let open_item = MenuItem::with_id(app, OPEN_ID, menu_t("menu.file.open"), true, None::<&str>)?;
        let view_item = context_item(app, shortcuts, FILE_VIEW_ID, menu_t("menu.file.view"), true)?;
        let edit_item = context_item(app, shortcuts, EDIT_ID, menu_t("menu.context.edit"), true)?;
        menu.append(&open_item)?;
        #[cfg(target_os = "macos")]
        {
            // Open with submenu: Finder convention, shown for files, not directories.
            match &info.open_with {
                Slow::Ready(choices) => {
                    let (submenu, map) = super::open_with::build_open_with_submenu(app, &choices.candidates)?;
                    menu.append(&submenu)?;
                    open_with_apps = map;
                }
                Slow::Pending => {
                    let pending = super::open_with::build_pending_open_with_submenu(app)?;
                    menu.append(&pending.submenu)?;
                    late.open_with = Some(pending);
                }
            }
        }
        menu.append(&view_item)?;
        menu.append(&edit_item)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    // Everything that changes WHAT is selected, in its own group between activation
    // (Open / View / Edit) and the operations (Copy / Move / Rename) that act on it.
    // Toggle selection leads it, which is the only place Space is discoverable.
    // `super::selection_submenu` owns the rows.
    menu.append(&build_selection_submenu(app, shortcuts, same_kind)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Finder tag colors (macOS): seven circles that toggle the system color tags on the
    // selection. Shown for files and folders (Finder tags both).
    #[cfg(target_os = "macos")]
    {
        let tag_items = append_tag_color_group(app, &menu)?;
        if info.applied_tag_colors.is_pending() {
            late.tag_items = tag_items;
        }
    }

    // Copy / Move / Duplicate / Rename group. Rename and Duplicate are omitted on the
    // search-results virtual pane: the underlying file CAN be renamed, but doing it from
    // the snapshot view splits the file (snapshot keeps the old name, disk has the new)
    // which is confusing, and a duplicate would have to land in each item's own real
    // folder, which one transfer can't express. The user can navigate to the real folder
    // and do either there.
    let copy_item = context_item(app, shortcuts, FILE_COPY_ID, menu_t("menu.file.copy"), true)?;
    let move_item = context_item(app, shortcuts, FILE_MOVE_ID, menu_t("menu.file.move"), true)?;
    menu.append(&copy_item)?;
    menu.append(&move_item)?;
    if !restrict_destination_actions {
        let duplicate_item = context_item(app, shortcuts, FILE_DUPLICATE_ID, menu_t("menu.file.duplicate"), true)?;
        menu.append(&duplicate_item)?;
        let rename_item = context_item(app, shortcuts, RENAME_ID, menu_t("menu.file.rename"), true)?;
        menu.append(&rename_item)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // New folder / New file — also omitted on search-results panes (no destination
    // folder to create into; the pane IS the snapshot, not a directory).
    if !restrict_destination_actions {
        let new_folder_item = context_item(app, shortcuts, FILE_NEW_FOLDER_ID, menu_t("menu.file.newFolder"), true)?;
        let new_file_item = context_item(app, shortcuts, FILE_NEW_FILE_ID, menu_t("menu.file.newFile"), true)?;
        menu.append(&new_folder_item)?;
        menu.append(&new_file_item)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }

    // Delete
    let delete_item = context_item(app, shortcuts, FILE_DELETE_ID, menu_t("menu.file.delete"), true)?;
    menu.append(&delete_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Utility group: Show in Finder, Copy filename, Copy path
    let show_in_finder_item = context_item(
        app,
        shortcuts,
        SHOW_IN_FINDER_ID,
        menu_t(SHOW_IN_FILE_MANAGER_KEY.current()),
        true,
    )?;
    let copy_filename_item = context_item(
        app,
        shortcuts,
        COPY_FILENAME_ID,
        menu_t_with(
            "menu.context.copyNamed",
            &[("name", &truncate_for_menu_label(filename, COPY_FILENAME_MAX_CHARS))],
        ),
        true,
    )?;
    let copy_path_item = context_item(app, shortcuts, COPY_PATH_ID, menu_t("menu.edit.copyPath"), true)?;
    menu.append(&show_in_finder_item)?;
    // "Open terminal here" rides beside Show in Finder, same gesture aimed at a
    // different app. macOS only, like the launch module behind it.
    #[cfg(target_os = "macos")]
    {
        let open_terminal_here_item = context_item(
            app,
            shortcuts,
            OPEN_TERMINAL_HERE_ID,
            menu_t("menu.file.openTerminalHere"),
            can_open_terminal_here,
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
        // Still enumerating when the menu went up, it's there with a placeholder, and an
        // empty answer says so inside it (`share_submenu::fill_share_submenu`).
        if can_share {
            match &info.share_services {
                Slow::Ready(services) if !services.is_empty() => {
                    menu.append(&super::share_submenu::build_share_submenu(app, services)?)?;
                }
                Slow::Ready(_) => {}
                Slow::Pending => {
                    let pending = super::share_submenu::build_pending_share_submenu(app)?;
                    menu.append(&pending.submenu)?;
                    late.share = Some(pending);
                }
            }
        }
    }
    menu.append(&copy_filename_item)?;
    menu.append(&copy_path_item)?;

    // Add to favorites — directories only (favorites are folders), and only where a favorite
    // could point back: `can_favorite` is the caller's reading of the row, matching the gate
    // `add_favorite` enforces. Favorites the right-clicked folder's path, which
    // `on_menu_event` reads from `MenuState.context.path`.
    if is_directory && can_favorite {
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
    // Google Drive: open on the web / copy the link / ask Gemini about it. These build
    // web URLs, so they work in mirror mode too; Drive's own File Provider actions (Share
    // among them) come in the provider group below, minus these three.
    // `file_system/google_drive/` has the full story.
    //
    // A group whose fact is still out is built whole and hidden as the menu starts
    // tracking, then revealed when the fact lands (`context_menu_live.rs`): these sit near
    // the bottom, and revealing rows moves only what's below them.
    #[cfg(target_os = "macos")]
    match &info.google_drive_links {
        Slow::Ready(Some(links)) => {
            let [open_item, copy_link_item, ask_gemini_item] = drive_items(app)?;
            menu.append(&PredefinedMenuItem::separator(app)?)?;
            menu.append(&open_item)?;
            menu.append(&copy_link_item)?;
            // Files only: Gemini's `?di=` names a document, and a folder resolves no
            // Gemini URL at all.
            if links.gemini_url.is_some() {
                menu.append(&ask_gemini_item)?;
            }
        }
        Slow::Ready(None) => {}
        Slow::Pending => late.drive = Some(append_slots(app, &menu, drive_items(app)?.into())?),
    }

    // Eviction pair: iCloud Drive ONLY, and gated by sync status. The
    // `FileManager` ubiquity APIs behind these accept iCloud URLs and nothing
    // else; a third-party provider's pin/unpin is a File Provider custom action
    // reserved for the app that bundles the extension. ❌ Don't widen this to
    // other providers — see `file_system/cloud_actions.rs`.
    #[cfg(target_os = "macos")]
    if info.is_icloud_drive {
        let [make_offline_item, remove_download_item] = eviction_items(app)?;
        match info.sync_status {
            Slow::Ready(SyncStatus::OnlineOnly) => {
                menu.append(&PredefinedMenuItem::separator(app)?)?;
                menu.append(&make_offline_item)?;
            }
            Slow::Ready(SyncStatus::Synced) => {
                menu.append(&PredefinedMenuItem::separator(app)?)?;
                menu.append(&remove_download_item)?;
            }
            // Uploading/Downloading: action already in flight, don't offer either.
            // Unknown: status query failed, hide to avoid confusion.
            Slow::Ready(_) => {}
            Slow::Pending => {
                late.icloud = Some(append_slots(app, &menu, vec![make_offline_item, remove_download_item])?);
            }
        }
    }

    // The provider's own actions (Dropbox, Google Drive, MacDroid, …), evaluated the way
    // Finder does and in the provider's order and words, below Cmdr's own cloud items.
    #[cfg(target_os = "macos")]
    match &info.file_provider_offer {
        Slow::Ready(Some(offer)) => super::file_provider_items::append_file_provider_group(app, &menu, offer)?,
        Slow::Ready(None) => {}
        Slow::Pending => {
            late.provider = Some(super::file_provider_items::append_pending_file_provider_group(
                app, &menu,
            )?);
        }
    }

    // Quick Look and Get Info are macOS-only
    #[cfg(target_os = "macos")]
    {
        let get_info_item = context_item(app, shortcuts, GET_INFO_ID, menu_t("menu.file.getInfo"), true)?;
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
        #[cfg(target_os = "macos")]
        late,
    })
}

/// Open in Google Drive, Copy Google Drive link, and Ask Gemini, in menu order.
#[cfg(target_os = "macos")]
fn drive_items<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<[MenuItem<R>; 3]> {
    Ok([
        MenuItem::with_id(
            app,
            DRIVE_OPEN_ID,
            menu_t("menu.context.openInGoogleDrive"),
            true,
            None::<&str>,
        )?,
        MenuItem::with_id(
            app,
            DRIVE_COPY_LINK_ID,
            menu_t("menu.context.copyGoogleDriveLink"),
            true,
            None::<&str>,
        )?,
        MenuItem::with_id(
            app,
            DRIVE_ASK_GEMINI_ID,
            menu_t("menu.context.askGemini"),
            true,
            None::<&str>,
        )?,
    ])
}

/// Make available offline, then Remove download.
#[cfg(target_os = "macos")]
fn eviction_items<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<[MenuItem<R>; 2]> {
    Ok([
        MenuItem::with_id(
            app,
            CLOUD_MAKE_OFFLINE_ID,
            menu_t("menu.context.makeAvailableOffline"),
            true,
            None::<&str>,
        )?,
        MenuItem::with_id(
            app,
            CLOUD_REMOVE_DOWNLOAD_ID,
            menu_t("menu.context.removeDownload"),
            true,
            None::<&str>,
        )?,
    ])
}

/// Appends a separator and every item of a group whose fact is still out, for the live menu
/// to hide and reveal.
#[cfg(target_os = "macos")]
fn append_slots<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    items: Vec<MenuItem<R>>,
) -> tauri::Result<SlotGroup<R>> {
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    for item in &items {
        menu.append(item)?;
    }
    Ok(SlotGroup::new(items))
}

/// Appends the seven Finder-tag color items (macOS) plus a trailing separator.
///
/// Each item is a plain item whose color circle `context_menu_icons.rs` puts on it once the
/// menu tracks; the "applied" colors (every selected file already carries them) get the
/// checkmark-composited variant. IDs are `tag-color:<index>`, prefix-routed in
/// `handle_menu_event`. Colors run in Finder's order (Red … Gray). These are the
/// FALLBACK look: once the menu tracks, `tag_row` folds them into Finder's single row of
/// circles and fires these same items on a click. The label carries the color's NAME,
/// which the row matches on and VoiceOver reads, which is why the names are translated
/// alongside everything else. macOS-only — Linux menus carry no icons. Answers the seven
/// items, in row order, for the live menu to check once the tag reads answer.
#[cfg(target_os = "macos")]
fn append_tag_color_group<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<Vec<MenuItem<R>>> {
    let mut items = Vec::with_capacity(super::tag_row::SWATCHES.len());
    for swatch in &super::tag_row::SWATCHES {
        let id = format!("{}{}", super::TAG_COLOR_ID_PREFIX, swatch.color);
        let item = MenuItem::with_id(app, &id, menu_t(swatch.name_key), true, None::<&str>)?;
        menu.append(&item)?;
        items.push(item);
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    Ok(items)
}
