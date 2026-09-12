//! Menu item builder helpers and shared submenu factories.
//!
//! These helpers are reused by `menu_structure` (top-level menu bar assembly
//! for macOS / Linux) and by the platform `macos.rs` / `linux.rs` modules.
//! Visibility is `pub(super)` so the items stay scoped to the `menu` module.

use std::collections::HashMap;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, IsMenuItem, MenuItem, PredefinedMenuItem, Submenu},
};

use crate::intl::{menu_t, menu_t_with};

pub(crate) use super::mnemonics::Mnemonics;

use super::{
    MenuItemEntry, SORT_ASCENDING_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MENU_ID, SORT_BY_MODIFIED_ID,
    SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID,
    VIEW_MODE_FULL_LEFT_ID, VIEW_MODE_FULL_RIGHT_ID, VIEW_ZOOM_75_ID, VIEW_ZOOM_100_ID, VIEW_ZOOM_125_ID,
    VIEW_ZOOM_150_ID, VIEW_ZOOM_IN_ID, VIEW_ZOOM_OUT_ID, ViewMode,
};

/// Max chars in the `Copy "<filename>"` context menu label before middle-ellipsis kicks in.
/// Picked to fit typical filenames while capping pathological 100+ char names that blow the menu
/// width.
pub(super) const COPY_FILENAME_MAX_CHARS: usize = 50;

/// Platform-aware accelerator for "Copy path to clipboard", matching Finder's "Copy as Pathname".
/// Must stay in sync with the `file.copyPath` default in `src/lib/commands/sources/file-list.ts`
/// (`⌘⌥C`), which is what the frontend dispatcher listens for.
#[cfg(target_os = "macos")]
pub(crate) fn copy_path_accelerator() -> &'static str {
    "Cmd+Opt+C"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn copy_path_accelerator() -> &'static str {
    "Ctrl+Alt+C"
}

/// Platform-aware accelerator for "Show in Finder / file manager".
#[cfg(target_os = "macos")]
pub(crate) fn show_in_file_manager_accelerator() -> &'static str {
    "Opt+Cmd+O"
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn show_in_file_manager_accelerator() -> &'static str {
    "Alt+Ctrl+O"
}

/// The macOS app menu's title, and the viewer bar's.
///
/// Deliberately NOT a catalog key: macOS names the app menu after the
/// application, and the application is called `Cmdr` (the `productName` in
/// `tauri.conf.json`, and the spelling the brand uses everywhere). Translating
/// it would make the one item every macOS user navigates by unrecognizable, and
/// it would earn a `sameAsSourceJustification` in all nine locales for nothing.
///
/// macOS-only, because only macOS has an app menu: Linux puts About under Help
/// and Settings under Edit.
#[cfg(target_os = "macos")]
pub(crate) const APP_MENU_TITLE: &str = "Cmdr";

/// The Tab menu / tab context-menu label, which flips with the tab's state.
///
/// Shared by the two places that set it (the context menu builds it, the
/// frontend pushes it onto the menu-bar item through `update_pin_tab_label`), so
/// the two can't drift into different words for one command.
pub fn pin_tab_label(is_pinned: bool) -> String {
    menu_t(if is_pinned {
        "menu.tab.unpinTab"
    } else {
        "menu.tab.pinTab"
    })
}

/// The label for the "Show in Finder" / "Show in file manager" action.
///
/// Two catalog keys rather than one with a platform token: "Finder" is Apple's
/// app name and stays English everywhere, while the Linux wording names a
/// generic kind of program, so the two don't have the same shape in every
/// language.
pub(crate) fn show_in_file_manager_label() -> String {
    #[cfg(target_os = "macos")]
    {
        menu_t("menu.file.showInFinder")
    }

    #[cfg(not(target_os = "macos"))]
    {
        menu_t("menu.file.showInFileManager")
    }
}

/// The View menu's per-pane view-mode block: four `CheckMenuItem`s and the two
/// submenus holding them.
pub(crate) struct ViewModeItems<R: Runtime> {
    pub full_left: CheckMenuItem<R>,
    pub brief_left: CheckMenuItem<R>,
    pub full_right: CheckMenuItem<R>,
    pub brief_right: CheckMenuItem<R>,
    pub left_submenu: Submenu<R>,
    pub right_submenu: Submenu<R>,
}

/// Builds `View > Left pane > {Full, Brief}` and the same for the right pane
/// (shared between macOS and Linux).
///
/// Both pairs always exist; only the ACTIVE pane's pair carries the accelerator,
/// so the shortcut hint visually follows focus as the user tabs between panes.
/// This is the initial build, where left is the active pane and the right pane
/// defaults to Brief; `menu_handlers::rebuild_view_mode_items` takes over from
/// there, and it depends on Full sitting at position 0 and Brief at 1 in each
/// submenu — it removes and reinserts by index, because Tauri has no
/// `set_accelerator()`.
pub(crate) fn build_view_mode_items<R: Runtime>(
    app: &AppHandle<R>,
    view_mode: ViewMode,
    left_pane_label: &str,
    right_pane_label: &str,
) -> tauri::Result<ViewModeItems<R>> {
    // Each pane submenu holds the same two items, so each gets its own
    // mnemonic allocator: the letters only have to be unique within one submenu.
    let mut left_mnemonics = Mnemonics::new();
    let mut right_mnemonics = Mnemonics::new();

    let full_left = CheckMenuItem::with_id(
        app,
        VIEW_MODE_FULL_LEFT_ID,
        left_mnemonics.assign(&menu_t("menu.view.fullView")),
        true,
        view_mode == ViewMode::Full,
        Some("Cmd+1"),
    )?;
    let brief_left = CheckMenuItem::with_id(
        app,
        VIEW_MODE_BRIEF_LEFT_ID,
        left_mnemonics.assign(&menu_t("menu.view.briefView")),
        true,
        view_mode == ViewMode::Brief,
        Some("Cmd+2"),
    )?;
    let full_right = CheckMenuItem::with_id(
        app,
        VIEW_MODE_FULL_RIGHT_ID,
        right_mnemonics.assign(&menu_t("menu.view.fullView")),
        true,
        false,
        None::<&str>,
    )?;
    let brief_right = CheckMenuItem::with_id(
        app,
        VIEW_MODE_BRIEF_RIGHT_ID,
        right_mnemonics.assign(&menu_t("menu.view.briefView")),
        true,
        true,
        None::<&str>,
    )?;

    let left_submenu = Submenu::with_items(app, left_pane_label, true, &[&full_left, &brief_left])?;
    let right_submenu = Submenu::with_items(app, right_pane_label, true, &[&full_right, &brief_right])?;

    Ok(ViewModeItems {
        full_left,
        brief_left,
        full_right,
        brief_right,
        left_submenu,
        right_submenu,
    })
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
pub(crate) fn build_sort_submenu<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    accel_name: Option<&str>,
    accel_extension: Option<&str>,
    accel_modified: Option<&str>,
    accel_size: Option<&str>,
) -> tauri::Result<SortSubmenuItems<R>> {
    let mut mnemonics = Mnemonics::new();
    let sort_by_name = MenuItem::with_id(
        app,
        SORT_BY_NAME_ID,
        mnemonics.assign(&menu_t("menu.sort.name")),
        true,
        accel_name,
    )?;
    let sort_by_ext = MenuItem::with_id(
        app,
        SORT_BY_EXTENSION_ID,
        mnemonics.assign(&menu_t("menu.sort.extension")),
        true,
        accel_extension,
    )?;
    let sort_by_modified = MenuItem::with_id(
        app,
        SORT_BY_MODIFIED_ID,
        mnemonics.assign(&menu_t("menu.sort.dateModified")),
        true,
        accel_modified,
    )?;
    let sort_by_size = MenuItem::with_id(
        app,
        SORT_BY_SIZE_ID,
        mnemonics.assign(&menu_t("menu.sort.size")),
        true,
        accel_size,
    )?;
    let sort_by_created = MenuItem::with_id(
        app,
        SORT_BY_CREATED_ID,
        mnemonics.assign(&menu_t("menu.sort.dateCreated")),
        true,
        None::<&str>,
    )?;
    let sort_asc = MenuItem::with_id(
        app,
        SORT_ASCENDING_ID,
        mnemonics.assign(&menu_t("menu.sort.ascending")),
        true,
        None::<&str>,
    )?;
    let sort_desc = MenuItem::with_id(
        app,
        SORT_DESCENDING_ID,
        mnemonics.assign(&menu_t("menu.sort.descending")),
        true,
        None::<&str>,
    )?;

    let submenu = Submenu::with_id_and_items(
        app,
        SORT_BY_MENU_ID,
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

/// Registers the four shortcut-bound Sort by items for accelerator updates.
///
/// The positions live here, beside the `Submenu::with_items` call that sets them.
/// `register_item_positions_match_submenu_order` can only cross-check a submenu whose
/// item array a platform file spells out itself, so indices hardcoded over there against
/// this layout would go stale unnoticed the moment the order changes.
///
/// Date created and the ascending / descending items carry no accelerator and no
/// user-customizable shortcut, so nothing needs to reinsert them.
pub(crate) fn register_sort_items<R: Runtime>(
    items: &mut HashMap<String, MenuItemEntry<R>>,
    sort_items: &SortSubmenuItems<R>,
) {
    let submenu = &sort_items.submenu;
    register_item(items, SORT_BY_NAME_ID, &sort_items.by_name, submenu, 0);
    register_item(items, SORT_BY_EXTENSION_ID, &sort_items.by_extension, submenu, 1);
    register_item(items, SORT_BY_MODIFIED_ID, &sort_items.by_modified, submenu, 2);
    register_item(items, SORT_BY_SIZE_ID, &sort_items.by_size, submenu, 3);
}

/// Builds the View > Zoom submenu (shared between macOS and Linux).
///
/// Each preset item writes `appearance.textSize` directly via the unified
/// command-execute event. Zoom in/out adjust the value by 10 percentage
/// points. `accel_in` / `accel_out` are platform-specific accelerator strings
/// (macOS uses `Cmd+Plus` / `Cmd+Minus`, Linux uses `None` because GTK
/// intercepts these keys at the toolkit level).
pub(crate) fn build_zoom_submenu<R: Runtime>(
    app: &AppHandle<R>,
    zoom_label: &str,
    accel_100: Option<&str>,
    accel_in: Option<&str>,
    accel_out: Option<&str>,
) -> tauri::Result<Submenu<R>> {
    let mut mnemonics = Mnemonics::new();
    let zoom_75 = MenuItem::with_id(
        app,
        VIEW_ZOOM_75_ID,
        mnemonics.assign(&menu_t("menu.zoom.percent75")),
        true,
        None::<&str>,
    )?;
    let zoom_100 = MenuItem::with_id(
        app,
        VIEW_ZOOM_100_ID,
        mnemonics.assign(&menu_t("menu.zoom.percent100")),
        true,
        accel_100,
    )?;
    let zoom_125 = MenuItem::with_id(
        app,
        VIEW_ZOOM_125_ID,
        mnemonics.assign(&menu_t("menu.zoom.percent125")),
        true,
        None::<&str>,
    )?;
    let zoom_150 = MenuItem::with_id(
        app,
        VIEW_ZOOM_150_ID,
        mnemonics.assign(&menu_t("menu.zoom.percent150")),
        true,
        None::<&str>,
    )?;
    let zoom_in = MenuItem::with_id(
        app,
        VIEW_ZOOM_IN_ID,
        mnemonics.assign(&menu_t("menu.zoom.in")),
        true,
        accel_in,
    )?;
    let zoom_out = MenuItem::with_id(
        app,
        VIEW_ZOOM_OUT_ID,
        mnemonics.assign(&menu_t("menu.zoom.out")),
        true,
        accel_out,
    )?;

    Submenu::with_items(
        app,
        zoom_label,
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

/// One entry in a top-level submenu's build order, in display order.
///
/// A submenu built from a `&[MenuSlot]` and registered with [`build_registered_submenu`] can't
/// suffer the bug `register_item_positions_match_submenu_order` used to guard against: there's no
/// hand-typed index anywhere to drift from the array beside it, because the position IS that
/// item's index in the very array the submenu is built from.
pub(crate) enum MenuSlot<'a, R: Runtime> {
    /// An item tracked for accelerator updates (`MenuState.items`), keyed by its menu ID.
    Reg(&'static str, &'a MenuItem<R>),
    /// Anything else: a separator, a submenu, a `PredefinedMenuItem`, a `CheckMenuItem` not synced
    /// through the generic accelerator-update path, or a `MenuItem` with nothing to register (a
    /// dialog opener with no accelerator, kept in the menu but never rebound).
    Plain(&'a dyn IsMenuItem<R>),
}

impl<'a, R: Runtime> MenuSlot<'a, R> {
    fn as_dyn(&self) -> &'a dyn IsMenuItem<R> {
        match self {
            MenuSlot::Reg(_, item) => *item,
            MenuSlot::Plain(item) => *item,
        }
    }
}

/// Builds a top-level submenu from `slots` and registers every [`MenuSlot::Reg`] entry in `items`
/// at its real index in `slots`, so a position can never go stale: see [`MenuSlot`].
///
/// `id` is `Some` on macOS, where the post-construction AppKit passes resolve a submenu by ID
/// (`menu/DETAILS.md` § "Finding a menu from AppKit"); `None` on Linux, which does none of that and
/// builds every top-level submenu with a plain `Submenu::with_items`.
pub(crate) fn build_registered_submenu<R: Runtime>(
    app: &AppHandle<R>,
    id: Option<&str>,
    label: &str,
    slots: &[MenuSlot<R>],
    items: &mut HashMap<String, MenuItemEntry<R>>,
) -> tauri::Result<Submenu<R>> {
    let refs: Vec<&dyn IsMenuItem<R>> = slots.iter().map(MenuSlot::as_dyn).collect();
    let submenu = match id {
        Some(id) => Submenu::with_id_and_items(app, id, label, true, &refs)?,
        None => Submenu::with_items(app, label, true, &refs)?,
    };
    for (position, slot) in slots.iter().enumerate() {
        if let MenuSlot::Reg(item_id, item) = slot {
            register_item(items, item_id, item, &submenu, position);
        }
    }
    Ok(submenu)
}

/// Registers a regular MenuItem in the items HashMap for accelerator updates.
pub(crate) fn register_item<R: Runtime>(
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

/// Which word a volume row's detach control uses.
///
/// ❗ A phone says Disconnect, ❌ never Eject: `adb` has no per-client detach, so
/// nothing is made safe to unplug and the device stays on the cable. MTP keeps
/// Eject, which it earns by closing the device session. The menu ITEM is
/// `EJECT_VOLUME_ID` either way (for a phone that routes to `DeviceDisconnect`);
/// only the word differs, which is what keeps the native menus reading the same
/// as the inline control in `VolumeBreadcrumb.svelte`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetachWord {
    Eject,
    Disconnect,
}

impl DetachWord {
    /// The word for a row, read off its volume id: the one input both native
    /// menus already carry, so neither can drift from the other.
    pub(crate) fn for_volume_id(volume_id: &str) -> Self {
        if cmdr_fs::volume::is_adb_volume_id(volume_id) {
            Self::Disconnect
        } else {
            Self::Eject
        }
    }
}

/// The "Eject (Backup)" / "Disconnect" label, in its busy variant while a write
/// op still touches the volume. Shared by the breadcrumb and volume-row menus so
/// the two can't drift; the volume name is uncontrolled, so it rides in as a
/// literal token.
///
/// The Disconnect pair carries no name token: it reuses the two keys a server row
/// already spells, rather than paying eleven catalogs for a second wording of one
/// word, and the row it sits on is the one the user right-clicked.
pub(crate) fn detach_label(name: &str, busy: bool, word: DetachWord) -> String {
    match (word, busy) {
        (DetachWord::Eject, false) => menu_t_with("menu.volume.eject", &[("name", name)]),
        (DetachWord::Eject, true) => menu_t_with("menu.volume.ejectBusy", &[("name", name)]),
        (DetachWord::Disconnect, false) => menu_t("menu.network.disconnect"),
        (DetachWord::Disconnect, true) => menu_t("menu.volume.disconnectBusy"),
    }
}

/// Truncate a filename for use inside a menu label, preserving the extension.
///
/// If the filename fits within `max_chars` (counted in chars, not bytes), it's returned unchanged.
/// Otherwise produces `<prefix>…<suffix>` where the suffix keeps the file extension plus a few
/// preceding chars, and the prefix takes ~60% of the budget. Operates on chars so multi-byte
/// UTF-8 sequences are never split mid-codepoint.
pub(super) fn truncate_for_menu_label(filename: &str, max_chars: usize) -> String {
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
    // what we want here; we treat names like ".gitignore" as extensionless.
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

#[cfg(test)]
#[path = "menu_items_test.rs"]
mod menu_items_test;
