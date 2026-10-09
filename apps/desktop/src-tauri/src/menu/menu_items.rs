//! Small pieces the menus share: the app menu's title, the labels that flip with state (pin or
//! unpin a tab, eject or disconnect a volume), and filename truncation for a menu label.
//!
//! Visibility stays inside the `menu` module wherever it can.

use crate::intl::{menu_t, menu_t_with};

/// Max chars in the `Copy "<filename>"` context menu label before middle-ellipsis kicks in.
/// Picked to fit typical filenames while capping pathological 100+ char names that blow the menu
/// width.
pub(super) const COPY_FILENAME_MAX_CHARS: usize = 50;

/// The macOS app menu's title, and the viewer bar's.
///
/// Deliberately NOT a catalog key: macOS names the app menu after the
/// application, and the application is called `Cmdr` (the `productName` in
/// `tauri.conf.json`, and the spelling the brand uses everywhere). Translating
/// it would make the one item every macOS user navigates by unrecognizable, and
/// it would earn a `sameAsSourceJustification` in all nine locales for nothing.
///
/// Only macOS has an app menu: Linux puts About under Help and Settings under Edit.
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

/// What "Select all of the same kind" would select right now, as the focused pane's cursor row
/// decides it (`pane/select-same-kind.ts`'s `SameKindTarget`, wire-identical).
///
/// `None` rather than a fourth variant covers the row that implies no kind at all: the `..` row,
/// an empty listing, a cursor entry still resolving. The item then falls back to its neutral
/// label, which is also what the bar is built with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SameKindTarget {
    /// The cursor is on a folder: every folder, whatever its name.
    AllFolders,
    /// The cursor is on a file with an extension: every file carrying it. Lowercased, no dot.
    SameExtension { extension: String },
    /// The cursor is on an extension-less file: every other one.
    NoExtension,
}

/// The "Select all of the same kind" label, which says what the command would do right now.
///
/// Shared by the two surfaces that draw it (the menu bar's item, rewritten on cursor moves by
/// `update_select_same_kind_menu`, and the right-click Selection submenu, which composes it fresh
/// at popup time), so neither can drift into different words for one command.
///
/// ❗ Rendered HERE, from a typed payload, ❌ never as a literal the frontend composes: the menu's
/// words come from `menu_t` in the language the NATIVE side speaks (`menu/CLAUDE.md`).
pub fn same_kind_menu_label(target: Option<&SameKindTarget>) -> String {
    match target {
        None => menu_t("menu.select.sameKind"),
        Some(SameKindTarget::AllFolders) => menu_t("menu.select.allFolders"),
        Some(SameKindTarget::NoExtension) => menu_t("menu.select.noExtension"),
        Some(SameKindTarget::SameExtension { extension }) => {
            menu_t_with("menu.select.sameExtension", &[("extension", extension)])
        }
    }
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
        use cmdr_fs::volume::VolumeScheme;
        match VolumeScheme::of(volume_id) {
            VolumeScheme::Adb => Self::Disconnect,
            VolumeScheme::Root
            | VolumeScheme::Local
            | VolumeScheme::Path
            | VolumeScheme::Smb
            | VolumeScheme::Sftp
            | VolumeScheme::Webdav
            | VolumeScheme::S3
            | VolumeScheme::Mtp
            | VolumeScheme::Cloud
            | VolumeScheme::Favorite
            | VolumeScheme::Unknown => Self::Eject,
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
