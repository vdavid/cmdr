//! Small pieces the menus share: the app menu's title, `cmdr_menu` labels resolved in the native
//! language (pin or unpin a tab, the same-kind and undo-trash labels, eject or disconnect a
//! volume), and which detach word a volume id gets.
//!
//! Visibility stays inside the `menu` module wherever it can.

use crate::intl::{menu_t, menu_t_with};

pub(crate) use cmdr_menu::bar::APP_MENU_TITLE;
pub(crate) use cmdr_menu::context::popups::DetachWord;
use cmdr_menu::labels::{LabelText, same_kind_label, undo_trash_label};
pub use cmdr_menu::labels::{SameKindTarget, TrashUndoTarget};

/// A `cmdr_menu` label in the language the native side speaks.
pub(super) fn resolve(label: &LabelText) -> String {
    let args: Vec<(&str, &str)> = label
        .args
        .iter()
        .map(|(token, value)| (*token, value.as_str()))
        .collect();
    if args.is_empty() {
        menu_t(label.key)
    } else {
        menu_t_with(label.key, &args)
    }
}

/// The Tab menu / tab context-menu label, which flips with the tab's state.
///
/// Shared by the two places that set it (the context menu builds it, the
/// frontend pushes it onto the menu-bar item through `update_pin_tab_label`), so
/// the two can't drift into different words for one command.
pub fn pin_tab_label(is_pinned: bool) -> String {
    resolve(&cmdr_menu::labels::pin_tab_label(is_pinned))
}

/// The "Select all of the same kind" label (`cmdr_menu::labels::same_kind_label`), resolved here.
///
/// ❗ Rendered HERE, from a typed payload, ❌ never as a literal the frontend composes: the menu's
/// words come from `menu_t` in the language the NATIVE side speaks (`menu/CLAUDE.md`).
pub fn same_kind_menu_label(target: Option<&SameKindTarget>) -> String {
    resolve(&same_kind_label(target))
}

/// The Edit > Undo move to trash label (`cmdr_menu::labels::undo_trash_label`), resolved here, for
/// the same reason as [`same_kind_menu_label`].
pub fn undo_trash_menu_label(target: Option<&TrashUndoTarget>) -> String {
    resolve(&undo_trash_label(target))
}

/// Which word a volume row's detach control uses, read off its volume id: the one input both
/// native menus already carry, so neither can drift from the other.
///
/// ❗ A phone says Disconnect, ❌ never Eject: `adb` has no per-client detach, so nothing is made
/// safe to unplug and the device stays on the cable. MTP keeps Eject, which it earns by closing
/// the device session. The menu ITEM is `EJECT_VOLUME_ID` either way (for a phone that routes to
/// `DeviceDisconnect`); only the word differs, which is what keeps the native menus reading the
/// same as the inline control in `VolumeBreadcrumb.svelte`.
pub(crate) fn detach_word_for_volume_id(volume_id: &str) -> DetachWord {
    use cmdr_fs::volume::VolumeScheme;
    match VolumeScheme::of(volume_id) {
        VolumeScheme::Adb => DetachWord::Disconnect,
        // An SMB share, on its OS mount or a direct session: the id says share on every OS, where
        // `category` and `connectionState` don't (a Linux GVFS share carries no state). The
        // frontend twin is `detach-control.ts:detachSubjectOf`.
        VolumeScheme::Smb => DetachWord::Share,
        VolumeScheme::Root
        | VolumeScheme::Local
        | VolumeScheme::Path
        | VolumeScheme::Sftp
        | VolumeScheme::Webdav
        | VolumeScheme::S3
        | VolumeScheme::Nfs
        | VolumeScheme::Mtp
        | VolumeScheme::Cloud
        | VolumeScheme::Favorite
        | VolumeScheme::Unknown => DetachWord::Eject,
    }
}

#[cfg(test)]
#[path = "menu_items_test.rs"]
mod menu_items_test;
