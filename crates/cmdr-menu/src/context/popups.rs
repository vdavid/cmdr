//! The small context menus: breadcrumb, `..` parent row, function key bar, tab, a servers hub
//! host, the viewer's text, a file list header's date Sort by, and a queue row. (A volume switcher
//! row's, a favorite's, and a servers-hub place's actions are the in-app `Menu`'s, not a popup:
//! `apps/desktop/src/lib/file-explorer/navigation/row-menu.ts`.)

use cmdr_sort::SortColumn;
use serde::{Deserialize, Serialize};

use super::{ContextItem, ContextLabel, ContextRow, Shortcut, command, plain};
use crate::ids::{
    COPY_CURRENT_DIR_PATH_ID, EJECT_ALL_VOLUMES_ID, EJECT_VOLUME_ID, FAVORITES_ADD_CONTEXT_ID,
    FUNCTION_KEY_BAR_HIDE_ID, NETWORK_HOST_DISCONNECT_ID, NETWORK_HOST_EDIT_ID, NETWORK_HOST_FORGET_SECRET_ID,
    NETWORK_HOST_FORGET_SERVER_ID, TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, VIEWER_CONTEXT_COPY_ID,
    VIEWER_CONTEXT_SELECT_ALL_ID,
};
use crate::labels::{LabelText, pin_tab_label};
use crate::spec::Platform;

/// The `..` row's menu: one "Add to favorites" that favorites the parent directory. The full file
/// menu makes no sense on `..`.
pub fn parent_row_menu() -> Vec<ContextRow> {
    vec![plain(
        FAVORITES_ADD_CONTEXT_ID,
        ContextLabel::key("menu.context.addToFavorites"),
        true,
    )]
}

/// The function key bar's menu: one "Hide function key bar".
pub fn function_key_bar_menu() -> Vec<ContextRow> {
    vec![plain(
        FUNCTION_KEY_BAR_HIDE_ID,
        ContextLabel::key("menu.context.hideFunctionKeyBar"),
        true,
    )]
}

/// Whether a row's leave-this-volume item reads `Eject ({name})` or `Disconnect`. A phone gets the
/// second: `adb` has no per-client detach, so nothing is made safe to unplug. The host decides
/// which from the volume id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetachWord {
    Eject,
    Disconnect,
}

/// The breadcrumb's volume, when the breadcrumb stands for one the user can leave.
#[derive(Debug, Clone, Copy)]
pub struct BreadcrumbVolume<'a> {
    pub name: &'a str,
    /// A write op reads from or writes to it, or its eject is still running: the detach items
    /// grey out with a `(busy)` label, as the picker's inline eject button does.
    pub busy: bool,
    pub word: DetachWord,
    /// The row shares its disk with other volumes, which adds "Eject all".
    pub offer_eject_all: bool,
}

/// The detach item's label.
pub fn detach_label(name: &str, busy: bool, word: DetachWord) -> LabelText {
    let named = |key| LabelText {
        key,
        args: vec![("name", name.to_string())],
    };
    match (word, busy) {
        (DetachWord::Eject, false) => named("menu.volume.eject"),
        (DetachWord::Eject, true) => named("menu.volume.ejectBusy"),
        (DetachWord::Disconnect, false) => LabelText {
            key: "menu.network.disconnect",
            args: Vec::new(),
        },
        (DetachWord::Disconnect, true) => LabelText {
            key: "menu.volume.disconnectBusy",
            args: Vec::new(),
        },
    }
}

/// The breadcrumb path bar's menu: Copy path, then the detach items when it stands for a volume.
pub fn breadcrumb_menu(volume: Option<BreadcrumbVolume<'_>>) -> Vec<ContextRow> {
    let mut rows = vec![command(COPY_CURRENT_DIR_PATH_ID, "menu.breadcrumb.copyPath")];
    if let Some(volume) = volume {
        rows.push(plain(
            EJECT_VOLUME_ID,
            ContextLabel::Key(detach_label(volume.name, volume.busy, volume.word)),
            !volume.busy,
        ));
        if volume.offer_eject_all {
            let key = if volume.busy {
                "menu.volume.ejectAllBusy"
            } else {
                "menu.volume.ejectAll"
            };
            rows.push(plain(EJECT_ALL_VOLUMES_ID, ContextLabel::key(key), !volume.busy));
        }
    }
    rows
}

/// A tab's menu.
pub fn tab_menu(is_pinned: bool, can_close: bool, has_other_unpinned_tabs: bool) -> Vec<ContextRow> {
    vec![
        plain(TAB_PIN_ID, ContextLabel::Key(pin_tab_label(is_pinned)), true),
        ContextRow::Separator,
        plain(
            TAB_CLOSE_OTHERS_ID,
            ContextLabel::key("menu.tab.closeOtherTabs"),
            has_other_unpinned_tabs,
        ),
        plain(TAB_CLOSE_ID, ContextLabel::key("menu.tab.closeTab"), can_close),
    ]
}

/// What a servers hub SMB host is, for its menu.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NetworkHostFacts {
    /// Typed in by the user, so "Forget server" can drop it.
    pub is_manual: bool,
    /// Saved, so "Edit" has something to edit.
    pub is_saved: bool,
    /// A password is stored for it.
    pub has_credentials: bool,
    /// A share from it is mounted right now.
    pub has_mounts: bool,
}

/// A host's menu, in one group, in the order the switcher row's menu uses (edit, disconnect,
/// forget the password, forget the server).
///
/// ❗ Only what does something: Disconnect only while a share from this host is mounted (an item
/// that answers "no mounted shares" is an inert affordance), Edit only for a saved host, Forget
/// server only for a typed-in one, Forget saved password only when one is stored.
pub fn network_host_menu(facts: NetworkHostFacts) -> Vec<ContextRow> {
    [
        (facts.is_saved, NETWORK_HOST_EDIT_ID, "menu.network.edit"),
        (facts.has_mounts, NETWORK_HOST_DISCONNECT_ID, "menu.network.disconnect"),
        (
            facts.has_credentials,
            NETWORK_HOST_FORGET_SECRET_ID,
            "menu.network.forgetSavedPassword",
        ),
        (
            facts.is_manual,
            NETWORK_HOST_FORGET_SERVER_ID,
            "menu.network.forgetServer",
        ),
    ]
    .into_iter()
    .filter(|(offered, ..)| *offered)
    .map(|(_, id, key)| plain(id, ContextLabel::key(key), true))
    .collect()
}

/// The viewer's Copy accelerator, shared by its bar's Edit > Copy and its text menu, in Tauri's
/// syntax. ❗ `Cmd` binds to SUPER on GTK (muda maps it to META), so it splits per platform.
pub const fn viewer_copy_accelerator(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "Cmd+C",
        Platform::Linux => "Ctrl+C",
    }
}

/// The viewer's Select all accelerator, as [`viewer_copy_accelerator`].
pub const fn viewer_select_all_accelerator(platform: Platform) -> &'static str {
    match platform {
        Platform::MacOs => "Cmd+A",
        Platform::Linux => "Ctrl+A",
    }
}

/// The viewer's menu over the file text: Copy, enabled only while there's a selection, and Select
/// all. The viewer reads its own selection model at open time and passes `has_selection`.
///
/// Labels and printed chords are the viewer bar's own, so the two menus read alike.
pub fn viewer_text_menu(has_selection: bool, platform: Platform) -> Vec<ContextRow> {
    vec![
        ContextRow::Item(ContextItem {
            id: VIEWER_CONTEXT_COPY_ID.to_string(),
            label: ContextLabel::key("menu.edit.copy"),
            enabled: has_selection,
            shortcut: Shortcut::Fixed(viewer_copy_accelerator(platform)),
        }),
        ContextRow::Item(ContextItem {
            id: VIEWER_CONTEXT_SELECT_ALL_ID.to_string(),
            label: ContextLabel::key("menu.select.all"),
            enabled: true,
            shortcut: Shortcut::Fixed(viewer_select_all_accelerator(platform)),
        }),
    ]
}

/// Which pane a header menu was opened over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PaneSide {
    Left,
    Right,
}

impl PaneSide {
    const ALL: [Self; 2] = [Self::Left, Self::Right];

    const fn word(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

/// The date popup's dates, top to bottom, with their labels: View > Sort by's words, in its order.
const DATES: [(SortColumn, &str); 3] = [
    (SortColumn::Modified, "menu.sort.dateModified"),
    (SortColumn::Created, "menu.sort.dateCreated"),
    (SortColumn::Added, "menu.sort.dateAdded"),
];

/// A date item's id, naming the pane it sorts. ❗ This menu's own, ❌ never the bar's
/// `SORT_BY_*_ID`: one handler serves both, and a bar id would sort the focused pane.
fn date_sort_id(pane: PaneSide, column: SortColumn) -> String {
    let column = match column {
        SortColumn::Created => "created",
        SortColumn::Added => "added",
        _ => "modified",
    };
    format!("date_sort_menu_{}_{column}", pane.word())
}

/// A file list header's Sort by popup over `pane`: the three dates as check items, the date the
/// pane ASKED for checked (none while it's in another column's order, or a search pane's ranked
/// order).
pub fn date_sort_menu(pane: PaneSide, requested: Option<SortColumn>) -> Vec<ContextRow> {
    DATES
        .iter()
        .map(|&(column, key)| ContextRow::Check {
            item: ContextItem {
                id: date_sort_id(pane, column),
                label: ContextLabel::key(key),
                enabled: true,
                shortcut: Shortcut::None,
            },
            checked: requested == Some(column),
        })
        .collect()
}

/// The pane and date a date-sort item id names, or `None` for any other id.
pub fn date_sort_pick(menu_id: &str) -> Option<(PaneSide, SortColumn)> {
    PaneSide::ALL.into_iter().find_map(|pane| {
        DATES
            .iter()
            .find(|&&(column, _)| date_sort_id(pane, column) == menu_id)
            .map(|&(column, _)| (pane, column))
    })
}

/// One thing a queue row's menu can do. Pause and Resume share a key but are separate actions, so
/// the label can say which one this click is, and so a pick that went stale while the menu was up
/// can be told apart. Its menu ids, labels, and commands live right below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum QueueRowMenuAction {
    MoveEarlier,
    MoveLater,
    Select,
    Cancel,
    Dismiss,
    Show,
    Pause,
    Resume,
    Rollback,
}

impl QueueRowMenuAction {
    pub const ALL: [Self; 9] = [
        Self::MoveEarlier,
        Self::MoveLater,
        Self::Select,
        Self::Cancel,
        Self::Dismiss,
        Self::Show,
        Self::Pause,
        Self::Resume,
        Self::Rollback,
    ];

    /// Its menu id. ❗ Its own, ❌ never a bar id: `menu_id_to_command` knows none of these.
    pub const fn menu_id(self) -> &'static str {
        match self {
            Self::MoveEarlier => "queue_row_move_earlier",
            Self::MoveLater => "queue_row_move_later",
            Self::Select => "queue_row_select",
            Self::Cancel => "queue_row_cancel",
            Self::Dismiss => "queue_row_dismiss",
            Self::Show => "queue_row_show",
            Self::Pause => "queue_row_pause",
            Self::Resume => "queue_row_resume",
            Self::Rollback => "queue_row_rollback",
        }
    }

    pub fn from_menu_id(menu_id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.menu_id() == menu_id)
    }

    const fn label_key(self) -> &'static str {
        match self {
            Self::MoveEarlier => "menu.queue.moveUp",
            Self::MoveLater => "menu.queue.moveDown",
            Self::Select => "menu.queue.select",
            Self::Cancel => "menu.queue.cancel",
            Self::Dismiss => "menu.queue.dismiss",
            Self::Show => "menu.queue.show",
            Self::Pause => "menu.queue.pause",
            Self::Resume => "menu.queue.resume",
            Self::Rollback => "menu.queue.rollback",
        }
    }

    /// The queue window's command this item runs, whose binding labels it. These items have no
    /// menu-bar twin to map through, so they bind their own.
    pub const fn command_id(self) -> &'static str {
        match self {
            Self::MoveEarlier => "queue.moveEarlier",
            Self::MoveLater => "queue.moveLater",
            Self::Select => "queue.toggleSelected",
            Self::Cancel => "queue.cancel",
            Self::Dismiss => "queue.cancelOrDismiss",
            Self::Show => "queue.showInMainWindow",
            Self::Pause | Self::Resume => "queue.pauseOrResume",
            Self::Rollback => "queue.rollback",
        }
    }
}

/// One entry of a queue row's menu, as the queue window lists it
/// (`lib/file-operations/queue/queue-row-menu.ts`): every fact behind it lives there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum QueueRowMenuEntry {
    Item {
        action: QueueRowMenuAction,
        enabled: bool,
    },
    /// An item that toggles, ticked while on: Select, ticked on a selected row, the way the
    /// row's checkbox is.
    Check {
        action: QueueRowMenuAction,
        enabled: bool,
        checked: bool,
    },
    Separator,
}

/// A queue row's menu from the window's entries, each labelled and keyed by its action.
pub fn queue_row_menu(entries: &[QueueRowMenuEntry]) -> Vec<ContextRow> {
    let item = |action: QueueRowMenuAction, enabled| ContextItem {
        id: action.menu_id().to_string(),
        label: ContextLabel::key(action.label_key()),
        enabled,
        shortcut: Shortcut::OfCommand(action.command_id()),
    };
    entries
        .iter()
        .map(|entry| match *entry {
            QueueRowMenuEntry::Item { action, enabled } => ContextRow::Item(item(action, enabled)),
            QueueRowMenuEntry::Check {
                action,
                enabled,
                checked,
            } => ContextRow::Check {
                item: item(action, enabled),
                checked,
            },
            QueueRowMenuEntry::Separator => ContextRow::Separator,
        })
        .collect()
}
