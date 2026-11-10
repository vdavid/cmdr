//! The item tree's wire types: what the dialog asks for and every row it gets back.
//! A leaf of its own, so the walk (`layout.rs`) and the request handler (`view.rs`)
//! both build on it without depending on each other.

use serde::{Deserialize, Serialize};

use super::UnitRole;
use super::selection::RollbackSelection;
use super::status::{BackfillProgress, UnbuiltReason};
use crate::types::{EntryType, ItemOutcome, SkipReason};

/// A row's stable key: the same row answers to it across requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum TreeRowKey {
    /// A folder row; for a compacted chain (`a/b/c`), the deepest folder.
    Folder { dir_id: i64 },
    /// A file row.
    Item { seq: i64 },
}

/// What the dialog asks for.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationTreeRequest {
    /// Folder rows the person expanded.
    pub(crate) expanded: Vec<TreeRowKey>,
    /// The selection, as the last answer normalized it; `null` means everything.
    pub(crate) selection: Option<RollbackSelection>,
    /// The cursor row, so the answer can say where it is now.
    pub(crate) cursor: Option<TreeRowKey>,
    /// The flat row index the window starts at.
    pub(crate) offset: u32,
    /// How many rows, at most 200.
    pub(crate) limit: u32,
}

/// Whether the tree can be read, and if not, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum TreeSummaryView {
    /// The rows are here.
    Built,
    /// The journal is still building its folder indexes (once, after an upgrade).
    IndexesBuilding,
    /// The journal couldn't build its folder indexes this launch; the next launch tries again.
    IndexesUnavailable,
    /// A backfill is building this op's summary.
    Building { progress: BackfillProgress },
    /// Not built and not building, for this reason.
    Unbuilt { reason: UnbuiltReason },
}

/// What a row is to a rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum TreeRowRole {
    /// Awaits reversal and has a checkbox.
    Selectable,
    /// A folder a copy or move made: no checkbox of its own; it goes when everything
    /// under it does.
    Housekeeping,
    /// Recorded inside a folder unit: it comes back with its folder, and shows the
    /// folder's state.
    Contents,
    /// Rolled back, or the operation's own skip or failure.
    Settled,
    /// Nothing can reverse it (a permanent delete).
    NotReversible,
}

impl From<UnitRole> for TreeRowRole {
    fn from(role: UnitRole) -> Self {
        match role {
            UnitRole::Selectable => TreeRowRole::Selectable,
            UnitRole::Housekeeping => TreeRowRole::Housekeeping,
            UnitRole::Contents => TreeRowRole::Contents,
            UnitRole::Settled => TreeRowRole::Settled,
            UnitRole::NotReversible => TreeRowRole::NotReversible,
        }
    }
}

/// A row's checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum TreeCheckbox {
    On,
    Off,
    /// Some of what's under this folder is selected.
    Mixed,
    /// No checkbox: nothing selectable here, or the op is read-only.
    None,
}

/// Why a folder row can't unfold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum UnexpandableReason {
    /// The folder was empty.
    Empty,
    /// Cmdr didn't record what's inside this folder.
    NotRecorded,
    /// Cmdr didn't record what's inside, because the operation held a folder of over
    /// 50,000 files. Op-level: the journal can't say WHICH folder it was.
    Capped,
}

/// Counts for a folder, or for the whole op.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct TreeTally {
    pub files: u64,
    pub bytes: u64,
    /// The "N" of "N of M".
    pub selected: u64,
    /// The "M".
    pub selectable: u64,
    pub rolled_back: u64,
    /// Units a rollback held back (they still await reversal).
    pub held_back: u64,
    /// Units the operation itself never finished.
    pub unfinished: u64,
    /// Folders a copy or move made that still await removal: why "Roll back the
    /// rest" has work at 0 of 0.
    pub housekeeping_waiting: u64,
}

/// One row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct OperationTreeRow {
    /// Its flat row index.
    pub index: u32,
    pub key: TreeRowKey,
    /// The name, or a compacted chain's names joined with `/`, or (a forest's top
    /// row) the folder's full path.
    pub label: String,
    /// 1-based, for `aria-level`.
    pub level: u16,
    pub entry_type: EntryType,
    pub expandable: bool,
    pub expanded: bool,
    /// 1-based, for `aria-posinset`.
    pub pos_in_set: u32,
    /// For `aria-setsize`.
    pub set_size: u32,
    /// `null` for a folder no item row names.
    pub role: Option<TreeRowRole>,
    /// The row's outcome; a `contents` row shows its folder unit's.
    pub outcome: Option<ItemOutcome>,
    pub skip_reason: Option<SkipReason>,
    pub checkbox: TreeCheckbox,
    /// A file's size as recorded.
    pub size: Option<i64>,
    /// A folder's counts.
    pub tally: Option<TreeTally>,
    pub full_path: String,
    /// The row's other end, only when it isn't where the other side's root plus this
    /// row's relative path would put it (a conflict rename). Never for a trash.
    pub other_end: Option<String>,
    pub unexpandable: Option<UnexpandableReason>,
}

/// One window of the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct OperationTreeView {
    /// Anything but `built` comes with no rows.
    pub summary: TreeSummaryView,
    /// A not-rollbackable op, or one rolling back: no checkboxes.
    pub read_only: bool,
    /// The folder every top row sits in; `null` for a forest (rows under several
    /// volume roots), whose top rows carry full paths.
    pub root_path: Option<String>,
    /// Where the other side's rows sit, when one root holds them.
    pub other_root: Option<String>,
    pub total_rows: u32,
    /// Where `rows` starts.
    pub offset: u32,
    pub rows: Vec<OperationTreeRow>,
    /// The open folder rows above the window's first row, outermost first.
    pub ancestors: Vec<OperationTreeRow>,
    /// The request's cursor row's flat index, or its nearest visible ancestor's
    /// when it collapsed away; `null` when it's gone.
    pub cursor_index: Option<u32>,
    pub tally: TreeTally,
    /// The selection after normalizing: send this back with the next toggle.
    pub selection: RollbackSelection,
    /// Normalizing refused toggles past the cap (2,000): pick a folder instead.
    pub toggles_capped: bool,
}

/// A row count or index on the wire, saturating at `u32::MAX`.
pub(super) fn clamp_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
