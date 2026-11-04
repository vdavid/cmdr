//! The bar's labels that change with state, as a catalog key plus the tokens it takes. A host
//! resolves them in its own language: the desktop through `intl::menu_t_with` (literal `{token}`
//! replacement), the browser demo through the frontend's catalog.
//!
//! ❗ Chosen HERE, from a typed payload, ❌ never as words a frontend composes: the desktop's menu
//! speaks the language its native side resolves, which is not necessarily the webview's.

/// A label: a `menu.*` catalog key and its `{token}` values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelText {
    pub key: &'static str,
    pub args: Vec<(&'static str, String)>,
}

impl LabelText {
    const fn plain(key: &'static str) -> Self {
        Self { key, args: Vec::new() }
    }

    fn with(key: &'static str, token: &'static str, value: &str) -> Self {
        Self {
            key,
            args: vec![(token, value.to_string())],
        }
    }
}

/// The Tab menu / tab context-menu label, which flips with the tab's state.
pub const fn pin_tab_label(is_pinned: bool) -> LabelText {
    LabelText::plain(if is_pinned {
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
/// Shared by the bar's item and the right-click Selection submenu, so neither can drift into
/// different words for one command.
pub fn same_kind_label(target: Option<&SameKindTarget>) -> LabelText {
    match target {
        None => LabelText::plain("menu.select.sameKind"),
        Some(SameKindTarget::AllFolders) => LabelText::plain("menu.select.allFolders"),
        Some(SameKindTarget::NoExtension) => LabelText::plain("menu.select.noExtension"),
        Some(SameKindTarget::SameExtension { extension }) => {
            LabelText::with("menu.select.sameExtension", "extension", extension)
        }
    }
}

/// What Edit > Undo move to trash would put back right now: the newest trash this session, as
/// the frontend remembers it (`file-operations/delete/trash-undo.ts`'s `TrashUndoTarget`,
/// wire-identical). `None` is "nothing to undo", which also greys the item out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrashUndoTarget {
    /// One file or folder, named.
    One { name: String },
    /// Two or more, counted. Formatted by the frontend, which owns number formatting.
    Many {
        #[serde(rename = "countText")]
        count_text: String,
    },
}

/// The Edit > Undo move to trash label, which names what it would put back. `menu_t` has no
/// plurals, so the counted form only ever carries 2 or more, and its catalog description tells
/// translators so.
pub fn undo_trash_label(target: Option<&TrashUndoTarget>) -> LabelText {
    match target {
        None => LabelText::plain("menu.edit.undoTrash"),
        Some(TrashUndoTarget::One { name }) => LabelText::with("menu.edit.undoTrashOne", "name", name),
        Some(TrashUndoTarget::Many { count_text }) => {
            LabelText::with("menu.edit.undoTrashMany", "countText", count_text)
        }
    }
}
