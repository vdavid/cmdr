//! The file context menu: the one a right-click on a row opens, built from the crate's rows
//! (`cmdr_menu::context::file`) with each macOS [`Slot`] filled from the slow per-file facts
//! (`FileContextInfo`, `context_menu_facts.rs`'s), plus the result around it
//! (`ContextMenuResult`).
//!
//! What the menu holds, in what order, for which row and pane, is the crate's; this file owns
//! only what needs the OS: Open with, the tag colors, Share, the cloud groups, the provider's
//! actions, and Services, each built from what answered in time or as a placeholder the live menu
//! fills in.

// Both the map and its values are the macOS-only "Open with" list, so the imports
// carry the same gate the fields do; ungated, Linux fails `-D unused-imports`.
#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

use cmdr_menu::context::file::{FileMenuFacts, file_menu};
use cmdr_menu::context::{ContextMenuShortcuts, Slot};
use cmdr_menu::spec::Platform;
#[cfg(target_os = "macos")]
use tauri::menu::{MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Runtime, menu::Menu};

#[cfg(target_os = "macos")]
use crate::file_system::sync_status::SyncStatus;
#[cfg(target_os = "macos")]
use crate::intl::menu_t;
#[cfg(target_os = "macos")]
use cmdr_menu::context::file::{DRIVE_ITEMS, EVICTION_ITEMS};

#[cfg(target_os = "macos")]
use super::context_menu_facts::{FileContextInfo, Slow};
#[cfg(target_os = "macos")]
use super::context_menu_live::LateTargets;
#[cfg(target_os = "macos")]
use super::context_menu_slots::SlotGroup;
use super::context_rows::{MenuParent, append_rows};
#[cfg(target_os = "macos")]
use super::file_provider_items::{append_file_provider_group, append_pending_file_provider_group};
#[cfg(target_os = "macos")]
use super::open_with::{build_open_with_submenu, build_pending_open_with_submenu};
#[cfg(target_os = "macos")]
use super::share_submenu::{build_pending_share_submenu, build_share_submenu};

/// Per-file information needed to build a fully-populated context menu: empty off macOS. The
/// macOS one, all slow facts, is `context_menu_facts::FileContextInfo`.
#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub struct FileContextInfo;

/// Result of building a file context menu: the menu itself, plus (on macOS) a
/// `bundle_id → app_path` map that the caller stores in `MenuState.context.open_with_apps`
/// so `on_menu_event` can resolve `open-with:<bundle-id>` clicks back to an app URL.
pub struct ContextMenuResult<R: Runtime> {
    pub menu: Menu<R>,
    #[cfg(target_os = "macos")]
    pub open_with_apps: HashMap<String, PathBuf>,
    /// The parts built for facts still pending, which the live menu fills in
    /// (`context_menu_live.rs`). Empty when every fact answered in time.
    #[cfg(target_os = "macos")]
    pub late: LateTargets<R>,
}

/// Builds the file context menu for one right-click. `shortcuts` is the live shortcut registry
/// every accelerator label reads.
pub fn build_context_menu<R: Runtime>(
    app: &AppHandle<R>,
    facts: &FileMenuFacts<'_>,
    #[cfg_attr(
        not(target_os = "macos"),
        allow(unused_variables, reason = "every slot, the only reader of `info`, is macOS-only")
    )]
    info: &FileContextInfo,
    shortcuts: &ContextMenuShortcuts,
) -> tauri::Result<ContextMenuResult<R>> {
    let menu = Menu::new(app)?;
    let rows = file_menu(facts, Platform::current());

    #[cfg(target_os = "macos")]
    let mut late = LateTargets::new(app);
    #[cfg(target_os = "macos")]
    let mut open_with_apps: HashMap<String, PathBuf> = HashMap::new();
    {
        // Every slot sits at the top level, so the filler appends to `menu` itself.
        #[cfg(target_os = "macos")]
        let mut fill =
            |slot: Slot, _: &MenuParent<'_, R>| fill_slot(app, &menu, slot, info, &mut late, &mut open_with_apps);
        // `file_menu` lays out no slot off macOS.
        #[cfg(not(target_os = "macos"))]
        let mut fill = |slot: Slot, _: &MenuParent<'_, R>| {
            log::warn!(target: "menu", "a {slot:?} slot off macOS; left out");
            Ok(())
        };
        append_rows(app, &MenuParent::Menu(&menu), &rows, shortcuts, &mut fill)?;
    }

    Ok(ContextMenuResult {
        menu,
        #[cfg(target_os = "macos")]
        open_with_apps,
        #[cfg(target_os = "macos")]
        late,
    })
}

/// Builds one OS-filled group at its position: from what answered in time, or as a placeholder
/// the live menu fills in (`context_menu_live.rs`) for a fact still out.
#[cfg(target_os = "macos")]
fn fill_slot<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    slot: Slot,
    info: &FileContextInfo,
    late: &mut LateTargets<R>,
    open_with_apps: &mut HashMap<String, PathBuf>,
) -> tauri::Result<()> {
    match slot {
        Slot::OpenWith => match &info.open_with {
            Slow::Ready(choices) => {
                let (submenu, map) = build_open_with_submenu(app, &choices.candidates)?;
                menu.append(&submenu)?;
                *open_with_apps = map;
            }
            Slow::Pending => {
                let pending = build_pending_open_with_submenu(app)?;
                menu.append(&pending.submenu)?;
                late.open_with = Some(pending);
            }
        },
        Slot::TagColors => {
            let tag_items = append_tag_color_group(app, menu)?;
            if info.applied_tag_colors.is_pending() {
                late.tag_items = tag_items;
            }
        }
        // ABSENT where macOS offers no service for this selection (a path that vanished, a broken
        // symlink), which only an enumeration can answer: the system popover would come up empty
        // but for `Edit Extensions…`, which is the bug that put the list in a submenu. Still
        // enumerating, it's there with a placeholder, and an empty answer says so inside it
        // (`share_submenu::fill_share_submenu`).
        Slot::Share => match &info.share_services {
            Slow::Ready(services) if !services.is_empty() => menu.append(&build_share_submenu(app, services)?)?,
            Slow::Ready(_) => {}
            Slow::Pending => {
                let pending = build_pending_share_submenu(app)?;
                menu.append(&pending.submenu)?;
                late.share = Some(pending);
            }
        },
        // A group whose fact is still out is built whole and hidden as the menu starts tracking,
        // then revealed when the fact lands: these sit near the bottom, and revealing rows moves
        // only what's below them. `file_system/google_drive/` has the Drive story.
        Slot::GoogleDrive => match &info.google_drive_links {
            Slow::Ready(Some(links)) => {
                let [open_item, copy_link_item, ask_gemini_item] = DRIVE_ITEMS;
                menu.append(&PredefinedMenuItem::separator(app)?)?;
                menu.append(&item(app, open_item)?)?;
                menu.append(&item(app, copy_link_item)?)?;
                // Files only: Gemini's `?di=` names a document, and a folder resolves no Gemini URL.
                if links.gemini_url.is_some() {
                    menu.append(&item(app, ask_gemini_item)?)?;
                }
            }
            Slow::Ready(None) => {}
            Slow::Pending => late.drive = Some(append_slots(app, menu, &DRIVE_ITEMS)?),
        },
        // iCloud Drive ONLY, and gated by sync status. The `FileManager` ubiquity APIs behind
        // these accept iCloud URLs and nothing else; a third-party provider's pin/unpin is a File
        // Provider custom action reserved for the app that bundles the extension. ❌ Don't widen
        // this to other providers: `file_system/cloud_actions.rs`.
        Slot::ICloudEviction if info.is_icloud_drive => {
            let [make_offline, remove_download] = EVICTION_ITEMS;
            match info.sync_status {
                Slow::Ready(SyncStatus::OnlineOnly) => {
                    menu.append(&PredefinedMenuItem::separator(app)?)?;
                    menu.append(&item(app, make_offline)?)?;
                }
                Slow::Ready(SyncStatus::Synced) => {
                    menu.append(&PredefinedMenuItem::separator(app)?)?;
                    menu.append(&item(app, remove_download)?)?;
                }
                // Uploading/Downloading: already in flight, offer neither. Unknown: the status
                // query failed, hide to avoid confusion.
                Slow::Ready(_) => {}
                Slow::Pending => late.icloud = Some(append_slots(app, menu, &EVICTION_ITEMS)?),
            }
        }
        Slot::ICloudEviction => {}
        // The provider's own actions (Dropbox, Google Drive, MacDroid, …), evaluated the way
        // Finder does and in the provider's order and words, below Cmdr's own cloud items.
        Slot::FileProvider => match &info.file_provider_offer {
            Slow::Ready(Some(offer)) => append_file_provider_group(app, menu, offer)?,
            Slow::Ready(None) => {}
            Slow::Pending => late.provider = Some(append_pending_file_provider_group(app, menu)?),
        },
        // Last, where Finder puts it. Only the item is built here: AppKit's own menu is borrowed
        // while the menu is up (`services_context.rs`).
        Slot::Services => super::services_context::append_services_submenu(app, menu)?,
    }
    Ok(())
}

/// A plain item from a crate table row, `(id, catalog key)`.
#[cfg(target_os = "macos")]
fn item<R: Runtime>(app: &AppHandle<R>, (id, key): (&str, &str)) -> tauri::Result<MenuItem<R>> {
    MenuItem::with_id(app, id, menu_t(key), true, None::<&str>)
}

/// Appends a separator and every item of a group whose fact is still out, for the live menu
/// to hide and reveal.
#[cfg(target_os = "macos")]
fn append_slots<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>, table: &[(&str, &str)]) -> tauri::Result<SlotGroup<R>> {
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let mut items = Vec::with_capacity(table.len());
    for row in table {
        let built = item(app, *row)?;
        menu.append(&built)?;
        items.push(built);
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
