//! A child row and the honest number it carries.
//!
//! The vocabulary `list_dir` shapes its rows in, kept apart from the pipeline in
//! `../listing.rs` that fills them. Two ideas live here: what a row IS
//! ([`RowKind`]) and what its size CLAIMS ([`SizeClaim`]), each a type precisely
//! because each replaced a pair of adjacent same-typed booleans that meant
//! opposite things.

use serde::Serialize;

use crate::search::{format_size, format_timestamp};

/// A size string the model can quote verbatim, with any uncertainty INSIDE the
/// string: `≥` when the number can only be higher (a lower bound), `~` when the
/// error runs in both directions.
///
/// The qualifier is part of the string rather than only a neighbouring flag
/// because the agent restates what a tool hands it. Given `"1.8 TB"` plus
/// `sizeIsLowerBound: true`, a model that quotes the number and drops the flag has
/// stated an exact total the index can't back; given `"≥ 1.8 TB"` it physically
/// can't. The flag stays too, for anything that branches on it.
pub(super) fn qualified_size(bytes: u64, qualifier: Option<&str>) -> String {
    match qualifier {
        Some(q) => format!("{q} {}", format_size(bytes)),
        None => format_size(bytes),
    }
}

/// A size for a model to read out: `format_size`, carrying the qualifier its two
/// honesty flags earn. Single-sourced on the `search` table's formatter, so a size
/// reads the same wherever it surfaces (and, like that table, it doesn't consult
/// the user's SI-vs-binary setting — MCP output stays internally consistent).
pub(crate) fn human_size(bytes: u64, is_lower_bound: bool, is_updating: bool) -> String {
    qualified_size(bytes, size_qualifier(is_lower_bound, is_updating))
}

/// Which uncertainty a size wears in front of it, if any.
///
/// **Motion outranks coverage.** `≥` says "the truth is at least this", which is
/// only defensible once the number has stopped: it's derived from unscanned
/// subtrees alone and can't express the opposite error, an index entry for a
/// subtree that's already gone, where the truth is far LOWER. That's exactly what
/// a running walk is busy correcting, so an in-flux total reads `~`: approximate,
/// direction unknown. The flags themselves stay separate on the wire, so an agent
/// that wants to know WHICH uncertainty this is still can.
///
/// The frontend's size column resolves the same collision by dropping its `≥` and
/// leaving the hourglass to speak (`views/full-list-utils.ts`); there a third
/// glyph would compete with the hourglass in a dense column, which is why the two
/// surfaces share the rule but not the symbol.
fn size_qualifier(is_lower_bound: bool, is_updating: bool) -> Option<&'static str> {
    match (is_updating, is_lower_bound) {
        (true, _) => Some("~"),
        (false, true) => Some("≥"),
        (false, false) => None,
    }
}

/// What a listed child is.
///
/// A symlink is a KIND here, not a modifier on a folder, because the distinction
/// is load-bearing: only a real `Folder` gets a recursive total, since following a
/// link could leave the tree or loop. The wire shape still carries two booleans
/// (`isDirectory` / `isSymlink`); this is the vocabulary the code reasons in, so
/// no caller has to pass two same-typed flags in the right order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowKind {
    File,
    Folder,
    SymlinkToFile,
    SymlinkToFolder,
}

impl RowKind {
    /// Read the kind off an index row's two flags. The one place the pair is
    /// interpreted, so a transposition has one place to hide instead of every
    /// call site.
    pub(super) fn of(is_directory: bool, is_symlink: bool) -> Self {
        match (is_directory, is_symlink) {
            (false, false) => Self::File,
            (true, false) => Self::Folder,
            (false, true) => Self::SymlinkToFile,
            (true, true) => Self::SymlinkToFolder,
        }
    }

    fn is_directory(self) -> bool {
        matches!(self, Self::Folder | Self::SymlinkToFolder)
    }

    fn is_symlink(self) -> bool {
        matches!(self, Self::SymlinkToFile | Self::SymlinkToFolder)
    }
}

/// A size and the two caveats that qualify it, as one value.
///
/// This module's contract is that a number never travels without its caveats, and
/// this is that contract as a type: `size_human` is derived from all three at once,
/// so the spoken string can't disagree with the flags, and no caller can hand a row
/// a size while forgetting which kind of uncertainty it carries. The two flags mean
/// opposite things and sat adjacent in an argument list, where swapping them turned
/// `≥` into `~` silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SizeClaim {
    /// `None` when the index has no size at all: never a wrong zero.
    pub bytes: Option<u64>,
    /// Some subtree was never listed, so the truth can only be HIGHER (`≥`).
    pub is_lower_bound: bool,
    /// The indexer can still move this total, in EITHER direction (`~`).
    pub is_updating: bool,
}

impl SizeClaim {
    /// A size the index states outright: exact, and nothing is moving it.
    pub(super) fn exact(bytes: Option<u64>) -> Self {
        Self {
            bytes,
            is_lower_bound: false,
            is_updating: false,
        }
    }

    /// No size to state. `exact(None)` by another name, for the call site that
    /// knows the folder but not its total and must say nothing rather than guess.
    pub(super) fn unknown() -> Self {
        Self::exact(None)
    }

    /// The spoken form, carrying whichever qualifier the two flags earn.
    fn human(self) -> Option<String> {
        self.bytes.map(|b| human_size(b, self.is_lower_bound, self.is_updating))
    }
}

/// One child row, shaped for the model.
///
/// Every raw number has a spoken twin (`size` / `size_human`, `modified` /
/// `modified_human`), because the agent can't run arithmetic: it can't turn
/// 1,975,684,321,280 into "1.8 TB" or an epoch into a date without guessing. Build
/// rows through [`ChildEntry::new`] / [`ChildEntry::set_size`] so the pair can't
/// drift apart.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildEntry {
    pub name: String,
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub is_directory: bool,
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub is_symlink: bool,
    /// How much space this child accounts for: a file's own size, a folder's
    /// RECURSIVE total (from `dir_stats`). One field, because the question a
    /// listing answers is "what's using the space in here", and a folder's own
    /// inode size answers nothing. `None` when the index has no size for it —
    /// never a wrong zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// `size`, spelled out (`"4.2 GB"`, `"≥ 1.8 TB"` for a settled lower bound,
    /// `"~ 1.8 TB"` while the number is still moving). Absent exactly when `size`
    /// is: an unknown size gets no string, never a `"0 B"` that would read as an
    /// empty folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_human: Option<String>,
    /// `true` when a folder's `size` is a lower bound (some subtree was never
    /// fully listed), so the model says "at least" rather than stating a total.
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub size_is_lower_bound: bool,
    /// `true` while the indexer can still move this folder's total: a walk is on
    /// it, above it, or below it (the roll-up repairs ancestors), or its own index
    /// writes are still draining. The number can go DOWN as well as up, which is
    /// why `size_human` reads `~` here rather than claiming a floor.
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub size_is_updating: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    /// `modified` as a date (`"2023-11-14"`). Absent exactly when `modified` is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_human: Option<String>,
}

impl ChildEntry {
    /// Build a row, deriving both spoken forms from the raw values so the two can
    /// never disagree.
    pub(crate) fn new(name: String, kind: RowKind, size: SizeClaim, modified: Option<u64>) -> Self {
        Self {
            name,
            is_directory: kind.is_directory(),
            is_symlink: kind.is_symlink(),
            size: size.bytes,
            size_human: size.human(),
            size_is_lower_bound: size.is_lower_bound,
            size_is_updating: size.is_updating,
            modified,
            modified_human: modified.map(format_timestamp),
        }
    }

    /// Replace the size claim, re-deriving `size_human`. The only way to change a
    /// size after construction — assigning the field alone would leave the string
    /// behind, stating the old number with the old caveat.
    pub(super) fn set_size(&mut self, size: SizeClaim) {
        self.size = size.bytes;
        self.size_human = size.human();
        self.size_is_lower_bound = size.is_lower_bound;
        self.size_is_updating = size.is_updating;
    }

    /// What this row is, read back off the two wire flags.
    pub(super) fn kind(&self) -> RowKind {
        RowKind::of(self.is_directory, self.is_symlink)
    }
}
