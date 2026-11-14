//! What a click on a context-menu row asks for, read off its id.
//!
//! The desktop's `handle_menu_event` and the browser demo's context menu both route a pick through
//! [`context_click`], so the two can't disagree about which event a row sends. A [`super::Slot`]'s
//! rows are the host's own, and so are their clicks: they never reach here.

use cmdr_sort::SortColumn;
use serde::{Deserialize, Serialize};

use super::popups::{PaneSide, QueueRowMenuAction, date_sort_pick};
use crate::ids::{
    CommandScope, EJECT_ALL_VOLUMES_ID, EJECT_VOLUME_ID, FAVORITES_ADD_CONTEXT_ID, FUNCTION_KEY_BAR_HIDE_ID,
    MEDIA_INDEX_ADD_FOLDER_ID, MEDIA_INDEX_EXCLUDE_FOLDER_ID, MEDIA_INDEX_INCLUDE_FOLDER_ID,
    MEDIA_INDEX_REMOVE_FOLDER_ID, NETWORK_HOST_DISCONNECT_ID, NETWORK_HOST_EDIT_ID, NETWORK_HOST_FORGET_SECRET_ID,
    NETWORK_HOST_FORGET_SERVER_ID, SHOW_SEARCH_RESULT_IN_FOLDER_ID, TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID,
    TELL_ME_ABOUT_CONTEXT_ID, VIEWER_CONTEXT_COPY_ID, VIEWER_CONTEXT_SELECT_ALL_ID, menu_id_to_command,
};

/// Which item of the viewer menu bar's Edit submenu (or the viewer's text menu) was picked. A
/// typed variant rather than the menu id as a string: the viewer frontend dispatches on it, and an
/// id is a backend detail nothing across IPC should be matching on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ViewerEditActionKind {
    /// Edit > Copy (⌘C).
    Copy,
    /// Edit > Select all (⌘A).
    SelectAll,
}

/// A breadcrumb detach item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeAction {
    Eject,
    EjectAll,
}

impl VolumeAction {
    /// The frontend's word for it (`VolumeContextActionKind`).
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Eject => "eject",
            Self::EjectAll => "eject-all",
        }
    }
}

/// A servers hub host item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkHostAction {
    ForgetServer,
    ForgetSecret,
    Disconnect,
    Edit,
}

impl NetworkHostAction {
    /// The frontend's word for it (`NetworkHostContextActionKind`).
    pub const fn wire(self) -> &'static str {
        match self {
            Self::ForgetServer => "forget-server",
            Self::ForgetSecret => "forget-secret",
            Self::Disconnect => "disconnect",
            Self::Edit => "edit",
        }
    }
}

/// What a pick asks for. Every lane but [`ContextClick::Command`] acts on what the popup was
/// opened over (a path, a volume, a host, a tab, a pane), which the host stashed at popup time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextClick {
    /// `execute-command` with `command_id`, to the main window. A `FileScoped` one runs only while
    /// it has focus, which every popup over it gives it first.
    Command {
        command_id: &'static str,
        scope: CommandScope,
    },
    /// The same as [`ContextClick::Command`], from a row only a context menu carries
    /// (`TELL_ME_ABOUT_CONTEXT_ID`), so the host can tell the main window the command came
    /// from a right-click. A row sharing the bar's id can't say that.
    RowCommand {
        command_id: &'static str,
        scope: CommandScope,
    },
    /// `show-search-result-in-folder` with the primary right-clicked path.
    ShowSearchResultInFolder,
    /// The `add_favorite` command on the right-clicked folder, so this surface meets the same add
    /// gate as the palette; ❌ never `favorites.add`, which favorites the focused pane's folder.
    AddFavorite,
    /// `media-index-folder-exclusion` on the right-clicked folder.
    MediaIndexExclusion { excluded: bool },
    /// `media-index-folder-choice` on the right-clicked folder.
    MediaIndexChoice { chosen: bool },
    /// `tab-context-action`, whose `action` is the item's own id.
    Tab(&'static str),
    /// `function-key-bar-hide-requested`.
    HideFunctionKeyBar,
    /// `volume-context-action` on the stashed volume.
    Volume(VolumeAction),
    /// `network-host-context-action` on the stashed host row.
    NetworkHost(NetworkHostAction),
    /// `viewer-context-menu-action`, to the viewer the menu was popped over.
    ViewerText(ViewerEditActionKind),
    /// `queue-row-context-action`, to the `queue` window, which knows the row.
    QueueRow(QueueRowMenuAction),
    /// `date-sort-menu-pick`, with the pane the popup was opened over.
    DateSort { pane: PaneSide, column: SortColumn },
}

/// What a pick of `menu_id` from a context menu asks for. `None` for an id no crate-built context
/// row carries (a slot's, or an unknown one).
pub fn context_click(menu_id: &str) -> Option<ContextClick> {
    Some(match menu_id {
        SHOW_SEARCH_RESULT_IN_FOLDER_ID => ContextClick::ShowSearchResultInFolder,
        FAVORITES_ADD_CONTEXT_ID => ContextClick::AddFavorite,
        MEDIA_INDEX_EXCLUDE_FOLDER_ID => ContextClick::MediaIndexExclusion { excluded: true },
        MEDIA_INDEX_INCLUDE_FOLDER_ID => ContextClick::MediaIndexExclusion { excluded: false },
        MEDIA_INDEX_ADD_FOLDER_ID => ContextClick::MediaIndexChoice { chosen: true },
        MEDIA_INDEX_REMOVE_FOLDER_ID => ContextClick::MediaIndexChoice { chosen: false },
        TAB_PIN_ID => ContextClick::Tab(TAB_PIN_ID),
        TAB_CLOSE_OTHERS_ID => ContextClick::Tab(TAB_CLOSE_OTHERS_ID),
        TAB_CLOSE_ID => ContextClick::Tab(TAB_CLOSE_ID),
        FUNCTION_KEY_BAR_HIDE_ID => ContextClick::HideFunctionKeyBar,
        EJECT_VOLUME_ID => ContextClick::Volume(VolumeAction::Eject),
        EJECT_ALL_VOLUMES_ID => ContextClick::Volume(VolumeAction::EjectAll),
        NETWORK_HOST_FORGET_SERVER_ID => ContextClick::NetworkHost(NetworkHostAction::ForgetServer),
        NETWORK_HOST_FORGET_SECRET_ID => ContextClick::NetworkHost(NetworkHostAction::ForgetSecret),
        NETWORK_HOST_DISCONNECT_ID => ContextClick::NetworkHost(NetworkHostAction::Disconnect),
        NETWORK_HOST_EDIT_ID => ContextClick::NetworkHost(NetworkHostAction::Edit),
        VIEWER_CONTEXT_COPY_ID => ContextClick::ViewerText(ViewerEditActionKind::Copy),
        VIEWER_CONTEXT_SELECT_ALL_ID => ContextClick::ViewerText(ViewerEditActionKind::SelectAll),
        TELL_ME_ABOUT_CONTEXT_ID => ContextClick::RowCommand {
            command_id: "file.tellMeAbout",
            scope: CommandScope::FileScoped,
        },
        _ => {
            if let Some(action) = QueueRowMenuAction::from_menu_id(menu_id) {
                ContextClick::QueueRow(action)
            } else if let Some((pane, column)) = date_sort_pick(menu_id) {
                ContextClick::DateSort { pane, column }
            } else {
                let (command_id, scope) = menu_id_to_command(menu_id)?;
                ContextClick::Command { command_id, scope }
            }
        }
    })
}
