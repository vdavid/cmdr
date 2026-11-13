//! The file context menu: the one a right-click on a row opens. By far the biggest menu, and the
//! only one whose shape depends on the row, the pane, the cloud provider, and the OS at once.
//!
//! [`file_menu`] lays it out for a set of facts. The OS-supplied groups are [`Slot`]s at their
//! positions, which the desktop fills (`apps/desktop/src-tauri/src/menu/file_context_menu.rs`).

use super::image_index::{ImageIndexMenuState, image_index_menu_items};
use super::{ContextItem, ContextLabel, ContextRow, Shortcut, Slot, command, plain, truncate_for_menu_label};
use crate::bar::SHOW_IN_FILE_MANAGER_KEY;
use crate::ids::{
    CLOUD_MAKE_OFFLINE_ID, CLOUD_REMOVE_DOWNLOAD_ID, DRIVE_ASK_GEMINI_ID, DRIVE_COPY_LINK_ID, DRIVE_OPEN_ID,
};
use crate::ids::{
    COPY_FILENAME_ID, COPY_PATH_ID, DESELECT_ALL_ID, DESELECT_FILES_ID, EDIT_ID, FAVORITES_ADD_CONTEXT_ID,
    FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID, FILE_DUPLICATE_ID, FILE_MOVE_ID, FILE_NEW_FILE_ID,
    FILE_NEW_FOLDER_ID, FILE_VIEW_ID, GET_INFO_ID, GO_PARENT_ID, INVERT_SELECTION_ID, OPEN_ID, OPEN_TERMINAL_HERE_ID,
    QUICK_LOOK_ID, RENAME_ID, SELECT_ALL_ID, SELECT_FILES_ID, SELECT_SAME_KIND_ID, SHARE_LINK_ONE_DAY_ID,
    SHARE_LINK_ONE_HOUR_ID, SHARE_LINK_SEVEN_DAYS_ID, SHARE_LINK_SUBMENU_ID, SHOW_IN_FINDER_ID,
    SHOW_SEARCH_RESULT_IN_FOLDER_ID, TELL_ME_ABOUT_ID, TOGGLE_SELECTION_ID,
};
use crate::labels::{LabelText, SameKindTarget, same_kind_label};
use crate::spec::Platform;

/// Max chars of filename in `Copy "<name>"` and in the header before the middle ellipsis kicks
/// in, so the two longest lines of the menu agree on width.
pub const FILENAME_MAX_CHARS: usize = 50;

/// What the PANE the right-click landed in contributes, as opposed to the file under the cursor.
///
/// One struct rather than trailing `bool`s, for the reason the frontend's `PaneContextMenuFacts`
/// gives: same-typed positional flags are exactly what binds to the wrong slot when one is
/// inserted. Every field's most restrictive answer is its `Default`, so a surface that can't
/// answer says nothing. The serde shape is the frontend's `pane` payload (camelCase, extra fields
/// ignored), so the demo reads it straight off the IPC args.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PaneFacts {
    /// Suppresses Rename, Duplicate, and the two create items, which only make sense on a real
    /// directory. `true` from the search-results virtual pane
    /// (`apps/desktop/src/lib/search/capabilities.ts`). Source-side actions stay, because the
    /// underlying paths are real.
    pub restrict_destination_actions: bool,
    /// Whether to offer the snapshot-only "Show in folder" action. Its handler emits the primary
    /// right-clicked path, not the pane cursor or selected paths.
    pub can_show_in_folder: bool,
    /// Whether "Open terminal here" is clickable. It acts on the pane's FOLDER, not this file, so a
    /// pane on MTP or ADB shows it greyed out; the snapshot pane and the Search dialog pass `false`
    /// too, having no folder of their own to open.
    pub can_open_terminal_here: bool,
    /// Whether `Share` and `Services` may appear at all: whether the pane's ROWS are real OS paths,
    /// which is what both a share service and a macOS service need. The search-results snapshot
    /// says yes (its rows are real files) where `can_open_terminal_here` says no, so the two can't
    /// be folded into one flag.
    pub can_share: bool,
    /// Whether the seven Finder tag colors may appear (macOS): a tag is an xattr written through
    /// the row's path. ❗ Off on a phone, an SFTP / WebDAV server, an archive's insides, or a
    /// `.git`-portal row: the click would write nothing and say nothing.
    pub can_tag: bool,
    /// Whether "Add to favorites" may appear on a folder row: the affordance half of the desktop's
    /// `add_favorite` gate, which refuses an archive's insides, a `.git`-portal folder, a phone,
    /// and a protocol-only server. Without it the item is offered where the add would be refused.
    pub can_favorite: bool,
    /// Whether "Copy share link" may appear: the row is a file on a volume that can mint a link.
    /// A file-only item, so a folder row never shows it whatever this says.
    pub can_share_link: bool,
}

/// What the right-clicked ROW(S) are, for the header line: how many, and the frontend's own words
/// for the count and the total size (it owns number formatting).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TargetFacts<'a> {
    pub count: usize,
    pub count_text: Option<&'a str>,
    pub size_text: Option<&'a str>,
}

/// Everything one right-click on a row decides the menu from.
#[derive(Debug, Clone, Copy)]
pub struct FileMenuFacts<'a> {
    /// The primary right-clicked row's name.
    pub filename: &'a str,
    pub is_directory: bool,
    pub pane: PaneFacts,
    /// The image-search group's facts about the folder; all `false` hides the group.
    pub image_index: ImageIndexMenuState,
    pub target: TargetFacts<'a>,
    /// What "Select all of the same kind" would select from the right-clicked row, computed by
    /// the frontend as this menu opens. ❗ Live, ❌ never the menu bar's debounced value.
    pub same_kind: Option<&'a SameKindTarget>,
}

/// One row of the `Selection >` submenu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionRow {
    /// A command item: its menu id and the catalog key its label reads.
    Item(&'static str, &'static str),
    /// "Select all of the same kind", whose label says what THIS right-click would select.
    SameKind,
    Separator,
}

/// The `Selection >` submenu, in display order: everything the Select menu holds, plus Toggle
/// selection, one right-click away from the row it acts on.
///
/// Toggle selection leads because Space is discoverable nowhere else, and the two dialog items sit
/// below a separator because they're the only ones that ask a question before acting.
pub const SELECTION_ROWS: &[SelectionRow] = &[
    SelectionRow::Item(TOGGLE_SELECTION_ID, "menu.context.toggleSelection"),
    SelectionRow::Item(SELECT_ALL_ID, "menu.select.all"),
    SelectionRow::Item(DESELECT_ALL_ID, "menu.select.deselectAll"),
    SelectionRow::SameKind,
    SelectionRow::Item(INVERT_SELECTION_ID, "menu.select.invert"),
    SelectionRow::Separator,
    SelectionRow::Item(SELECT_FILES_ID, "menu.select.files"),
    SelectionRow::Item(DESELECT_FILES_ID, "menu.select.deselectFiles"),
];

/// Google Drive's three web items, in menu order: open on the web, copy the link, ask Gemini.
/// These build web URLs, so they work in mirror mode too. [`Slot::GoogleDrive`]'s rows; the host
/// decides which show (Gemini is files only).
pub const DRIVE_ITEMS: [(&str, &str); 3] = [
    (DRIVE_OPEN_ID, "menu.context.openInGoogleDrive"),
    (DRIVE_COPY_LINK_ID, "menu.context.copyGoogleDriveLink"),
    (DRIVE_ASK_GEMINI_ID, "menu.context.askGemini"),
];

/// iCloud Drive's eviction pair: make available offline, then remove download.
/// [`Slot::ICloudEviction`]'s rows; the sync status decides which one shows.
pub const EVICTION_ITEMS: [(&str, &str); 2] = [
    (CLOUD_MAKE_OFFLINE_ID, "menu.context.makeAvailableOffline"),
    (CLOUD_REMOVE_DOWNLOAD_ID, "menu.context.removeDownload"),
];

/// The file context menu for one right-click, on `platform`.
pub fn file_menu(facts: &FileMenuFacts<'_>, platform: Platform) -> Vec<ContextRow> {
    let FileMenuFacts {
        filename,
        is_directory,
        pane,
        image_index,
        target,
        same_kind,
    } = *facts;
    let mac = platform == Platform::MacOs;
    let mut rows = Vec::new();

    // What this menu will act on, first line, above everything. Cmdr acts on the whole selection
    // or on the one right-clicked row depending on whether the click landed inside the selection,
    // and this is the only place that says which.
    rows.push(ContextRow::Header(header_label(filename, target)));
    rows.push(ContextRow::Separator);

    // Open / View / Edit (files only), with Open with after Open, the Finder convention.
    if !is_directory {
        rows.push(plain(OPEN_ID, ContextLabel::key("menu.file.open"), true));
        if mac {
            rows.push(ContextRow::Slot(Slot::OpenWith));
        }
        rows.push(command(FILE_VIEW_ID, "menu.file.view"));
        rows.push(command(EDIT_ID, "menu.context.edit"));
        rows.push(ContextRow::Separator);
    }

    // Everything that changes WHAT is selected, between activation and the operations.
    rows.push(ContextRow::Submenu {
        id: None,
        label: ContextLabel::key("menu.context.selection"),
        rows: selection_rows(same_kind),
    });
    rows.push(ContextRow::Separator);

    // Finder tag colors, for files and folders (Finder tags both), on rows a tag can be written to.
    if mac && pane.can_tag {
        rows.push(ContextRow::Slot(Slot::TagColors));
    }

    // Copy / Move / Duplicate / Compress / Rename, in the File menu's order. Rename and Duplicate
    // are left out on the search-results pane: renaming from the snapshot splits the file (the
    // snapshot keeps the old name), and a duplicate would have to land in each item's own folder,
    // which one transfer can't express. Compress stays, as Copy and Move do: it writes into the
    // other pane.
    let restrict = pane.restrict_destination_actions;
    rows.push(command(FILE_COPY_ID, "menu.file.copy"));
    rows.push(command(FILE_MOVE_ID, "menu.file.move"));
    if !restrict {
        rows.push(command(FILE_DUPLICATE_ID, "menu.file.duplicate"));
    }
    rows.push(command(FILE_COMPRESS_ID, "menu.file.compress"));
    if !restrict {
        rows.push(command(RENAME_ID, "menu.file.rename"));
    }
    rows.push(ContextRow::Separator);

    // New folder / New file: the search-results pane has no folder to create into.
    if !restrict {
        rows.push(command(FILE_NEW_FOLDER_ID, "menu.file.newFolder"));
        rows.push(command(FILE_NEW_FILE_ID, "menu.file.newFile"));
        rows.push(ContextRow::Separator);
    }

    rows.push(command(FILE_DELETE_ID, "menu.file.delete"));
    rows.push(ContextRow::Separator);

    // The utility group: everything that hands the row to something outside Cmdr, or copies
    // something about it.
    if pane.can_show_in_folder {
        rows.push(ContextRow::Item(ContextItem {
            id: SHOW_SEARCH_RESULT_IN_FOLDER_ID.to_string(),
            label: ContextLabel::key("menu.context.showInFolder"),
            enabled: true,
            shortcut: Shortcut::OfMenuId(GO_PARENT_ID),
        }));
    }
    rows.push(command(SHOW_IN_FINDER_ID, SHOW_IN_FILE_MANAGER_KEY));
    if mac {
        // Beside Show in Finder: the same gesture aimed at another app, greyed where the pane's
        // folder isn't one a shell can `cd` into.
        rows.push(ContextRow::Item(ContextItem {
            id: OPEN_TERMINAL_HERE_ID.to_string(),
            label: ContextLabel::key("menu.file.openTerminalHere"),
            enabled: pane.can_open_terminal_here,
            shortcut: Shortcut::OfMenuId(OPEN_TERMINAL_HERE_ID),
        }));
        // ABSENT, never greyed, where the rows aren't OS paths: no wording of a greyed item
        // explains "this row isn't a file yet" better than its absence does.
        if pane.can_share {
            rows.push(ContextRow::Slot(Slot::Share));
        }
    }
    rows.push(ContextRow::Item(ContextItem {
        id: COPY_FILENAME_ID.to_string(),
        label: ContextLabel::Key(LabelText {
            key: "menu.context.copyNamed",
            args: vec![("name", truncate_for_menu_label(filename, FILENAME_MAX_CHARS))],
        }),
        enabled: true,
        shortcut: Shortcut::OfMenuId(COPY_FILENAME_ID),
    }));
    rows.push(command(COPY_PATH_ID, "menu.edit.copyPath"));
    // Absent, never greyed, where the volume can't mint one: nothing a user could do enables it.
    if !is_directory && pane.can_share_link {
        rows.push(ContextRow::Submenu {
            id: Some(SHARE_LINK_SUBMENU_ID),
            label: ContextLabel::key("menu.context.copyShareLink"),
            // The seven-day default first.
            rows: vec![
                command(SHARE_LINK_SEVEN_DAYS_ID, "menu.context.shareLinkSevenDays"),
                command(SHARE_LINK_ONE_DAY_ID, "menu.context.shareLinkOneDay"),
                command(SHARE_LINK_ONE_HOUR_ID, "menu.context.shareLinkOneHour"),
            ],
        });
    }

    // Add to favorites: folders only (favorites are folders), and only where a favorite could
    // point back.
    if is_directory && pane.can_favorite {
        rows.push(ContextRow::Separator);
        rows.push(plain(
            FAVORITES_ADD_CONTEXT_ID,
            ContextLabel::key("menu.context.addToFavorites"),
            true,
        ));
    }

    // The image-search group: folders only, and only while image indexing is on.
    if is_directory {
        let items = image_index_menu_items(image_index);
        if !items.is_empty() {
            rows.push(ContextRow::Separator);
            rows.extend(
                items
                    .into_iter()
                    .map(|item| plain(item.id, ContextLabel::key(item.label_key), item.enabled)),
            );
        }
    }

    if mac {
        // The cloud groups, provider-aware: each contributes only the actions it can carry out,
        // so the group is a concatenation rather than one iCloud-shaped block. Each slot brings
        // its own leading separator, since only the host knows whether it shows.
        rows.push(ContextRow::Slot(Slot::GoogleDrive));
        rows.push(ContextRow::Slot(Slot::ICloudEviction));
        rows.push(ContextRow::Slot(Slot::FileProvider));

        rows.push(ContextRow::Separator);
        rows.push(command(GET_INFO_ID, "menu.file.getInfo"));
    } else {
        rows.push(ContextRow::Separator);
    }
    // Right below Get info, and on every OS: the card is Cmdr's own.
    rows.push(command(TELL_ME_ABOUT_ID, "menu.file.tellMeAbout"));
    if mac {
        rows.push(plain(QUICK_LOOK_ID, ContextLabel::key("menu.file.quickLook"), true));
        // Last, where Finder puts it, on the same fact as Share: a service takes file URLs.
        if pane.can_share {
            rows.push(ContextRow::Slot(Slot::Services));
        }
    }

    rows
}

/// The `Selection >` submenu's rows for one right-click.
fn selection_rows(same_kind: Option<&SameKindTarget>) -> Vec<ContextRow> {
    SELECTION_ROWS
        .iter()
        .map(|row| match *row {
            SelectionRow::Item(id, key) => command(id, key),
            SelectionRow::SameKind => ContextRow::Item(ContextItem {
                id: SELECT_SAME_KIND_ID.to_string(),
                label: ContextLabel::Key(same_kind_label(same_kind)),
                enabled: true,
                shortcut: Shortcut::OfMenuId(SELECT_SAME_KIND_ID),
            }),
            SelectionRow::Separator => ContextRow::Separator,
        })
        .collect()
}

/// Between the subject and the size in the header: a middle dot, which reads as "and" in every
/// script the app ships, with no word to translate.
const PART_SEPARATOR: &str = " \u{00B7} ";

/// The header line: the row's name (or the frontend's count wording, for several), then the
/// total size when the frontend has one.
///
/// ❗ It composes no WORDS and formats no numbers: it decides the label's shape from the one fact
/// it owns (how many rows the menu acts on) and fills each slot with text the frontend rendered.
pub fn header_label(filename: &str, target: TargetFacts<'_>) -> String {
    let subject = if target.count > 1 {
        non_empty(target.count_text).map_or_else(|| target.count.to_string(), str::to_string)
    } else {
        truncate_for_menu_label(filename, FILENAME_MAX_CHARS)
    };
    match non_empty(target.size_text) {
        Some(size) => format!("{subject}{PART_SEPARATOR}{size}"),
        None => subject,
    }
}

fn non_empty(text: Option<&str>) -> Option<&str> {
    text.filter(|value| !value.is_empty())
}
