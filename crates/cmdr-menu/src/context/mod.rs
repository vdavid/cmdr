//! Cmdr's right-click menus as data: every popup's rows, ids, labels, enabled state, and
//! accelerator source, for a set of facts, plus what a click on one of their rows asks for.
//!
//! Two hosts, like the bar: the desktop builds a native popup from the rows
//! (`apps/desktop/src-tauri/src/menu/context_rows.rs`), and the browser demo draws them as an HTML
//! menu (`crates/cmdr-listing-wasm/src/context_menu.rs`). A [`Slot`] is the one thing neither the
//! rows nor the demo know: a group only the OS can fill (Open with, Share, the provider's actions,
//! the tag colors, Services), which the desktop builds in place and the demo leaves out.
//!
//! - `file.rs`: the file context menu, the big one.
//! - `popups.rs`: the small ones (breadcrumb, parent row, function key bar, tab, network host,
//!   the viewer's text, a header's date Sort by, a queue row).
//! - `click.rs`: what a click on any of their rows asks for.

pub mod click;
pub mod file;
pub mod image_index;
pub mod popups;

#[cfg(test)]
#[path = "context_test.rs"]
mod context_test;

use std::collections::HashMap;

use crate::accelerators::to_menu_text;
use crate::ids::menu_id_to_command;
use crate::labels::LabelText;
use crate::spec::Platform;

/// One row of a context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextRow {
    /// A command item.
    Item(ContextItem),
    /// A check item.
    Check {
        item: ContextItem,
        checked: bool,
    },
    /// A nested submenu.
    Submenu {
        /// The submenu's own id, where a host needs one (`share_link_submenu`).
        id: Option<&'static str>,
        label: ContextLabel,
        rows: Vec<ContextRow>,
    },
    /// The file menu's first line, naming what it acts on: a disabled item with
    /// [`crate::ids::CONTEXT_MENU_TARGET_ID`], words the frontend rendered.
    Header(String),
    Separator,
    /// A group the host fills from the OS, at this position.
    Slot(Slot),
}

/// A command or check item's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItem {
    /// The menu id it's built with, which [`click::context_click`] reads back.
    pub id: String,
    pub label: ContextLabel,
    pub enabled: bool,
    pub shortcut: Shortcut,
}

/// What a row says: a catalog key with its tokens, or words that are already final (the header,
/// which the frontend worded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextLabel {
    Key(LabelText),
    Text(String),
}

impl ContextLabel {
    pub const fn key(key: &'static str) -> Self {
        Self::Key(LabelText { key, args: Vec::new() })
    }
}

/// Where a row's accelerator label comes from.
///
/// ❗ A popup's key equivalents are never registered with the app, so every one of them is display
/// text. The live ones come from the user's shortcut map ([`ContextMenuShortcuts`]), ❌ never a
/// literal: a literal starts lying the moment the user rebinds a key in Settings > Shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    None,
    /// Whatever the command behind this MENU id is bound to (`menu_id_to_command`). Usually the
    /// row's own id; Show in folder borrows Go to parent's, the key it acts like.
    OfMenuId(&'static str),
    /// Whatever this COMMAND id is bound to, for rows with no menu-bar twin (a queue row's).
    OfCommand(&'static str),
    /// A fixed Tauri accelerator the viewer's text menu shares with the viewer bar, which the
    /// viewer's own keydown can't rebind.
    Fixed(&'static str),
}

/// A group of rows only the OS can supply. The desktop fills each one (macOS only, every one of
/// them); the demo has no OS to ask and draws none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// "Open with", LaunchServices' apps for the file.
    OpenWith,
    /// The seven Finder tag colors and their trailing separator.
    TagColors,
    /// "Share", the system's share services for the rows.
    Share,
    /// Google Drive's three web items, with their leading separator, when the row is in Drive.
    GoogleDrive,
    /// iCloud Drive's make-offline / remove-download pair, with its leading separator.
    ICloudEviction,
    /// The file's File Provider's own actions.
    FileProvider,
    /// AppKit's Services submenu.
    Services,
}

/// The user's keyboard shortcuts as they stand right now: command-registry id → the combo in the
/// frontend's canonical spelling (`⌘⇧C`, `F5`, `Space`). The frontend pushes it with every popup,
/// straight out of the shortcut registry, so a command nobody has bound is simply absent.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(transparent)]
pub struct ContextMenuShortcuts(HashMap<String, String>);

impl ContextMenuShortcuts {
    /// A map from `(command id, combo)` pairs, for tests and hosts that build one by hand.
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self(
            pairs
                .into_iter()
                .map(|(id, combo)| (id.to_string(), combo.to_string()))
                .collect(),
        )
    }

    /// The accelerator to show beside the item with this MENU id, in Tauri's format.
    ///
    /// Menu id in, not command id: `menu_id_to_command` is already the one table binding the two
    /// vocabularies. `None` when nothing is bound, and for a menu id that maps to no command at
    /// all, which is a caller bug and shows up as a missing label rather than a wrong one.
    pub fn for_menu_item(&self, menu_id: &str, platform: Platform) -> Option<String> {
        let (command_id, _scope) = menu_id_to_command(menu_id)?;
        self.for_command(command_id, platform)
    }

    /// The accelerator to show for this COMMAND id, in Tauri's format. Through [`to_menu_text`],
    /// ❌ never the bar's floored `to_accelerator`: the floor would drop a bare `Space` or `F5`,
    /// which is right there and wrong here.
    pub fn for_command(&self, command_id: &str, platform: Platform) -> Option<String> {
        to_menu_text(self.0.get(command_id)?, platform)
    }
}

impl Shortcut {
    /// This row's accelerator in Tauri's format, read from `shortcuts` for `platform`.
    pub fn resolve(self, shortcuts: &ContextMenuShortcuts, platform: Platform) -> Option<String> {
        match self {
            Self::None => None,
            Self::OfMenuId(menu_id) => shortcuts.for_menu_item(menu_id, platform),
            Self::OfCommand(command_id) => shortcuts.for_command(command_id, platform),
            Self::Fixed(accelerator) => Some(accelerator.to_string()),
        }
    }
}

/// A command item whose accelerator is its own command's.
fn command(id: &'static str, key: &'static str) -> ContextRow {
    ContextRow::Item(ContextItem {
        id: id.to_string(),
        label: ContextLabel::key(key),
        enabled: true,
        shortcut: Shortcut::OfMenuId(id),
    })
}

/// An item with no accelerator.
fn plain(id: &'static str, label: ContextLabel, enabled: bool) -> ContextRow {
    ContextRow::Item(ContextItem {
        id: id.to_string(),
        label,
        enabled,
        shortcut: Shortcut::None,
    })
}

/// `filename` cut to `max_chars` characters with a middle ellipsis, keeping the extension when it
/// fits, so `Copy "<name>"` and the header line stay readable on a long name.
pub fn truncate_for_menu_label(filename: &str, max_chars: usize) -> String {
    let total_chars = filename.chars().count();
    if total_chars <= max_chars {
        return filename.to_string();
    }

    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "\u{2026}".to_string();
    }
    // One char for the ellipsis; the rest split 60/40 between the head and the tail, the tail
    // keeping the extension whole when it fits.
    let budget = max_chars - 1;
    let prefix_chars = budget * 6 / 10;
    let suffix_chars = budget - prefix_chars;

    let ext_with_dot = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let ext_chars = ext_with_dot.chars().count();

    let suffix: String = if ext_chars > 0 && ext_chars <= suffix_chars {
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
