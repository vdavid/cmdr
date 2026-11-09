//! The Operation log's item tree: which side of an operation its tree shows, what
//! each row is to a rollback, and the per-folder summary that lets a 2M-item
//! operation open by reading a handful of rows (`DETAILS.md` § "Tree summary").
//!
//! - [`TreeAxis`] and [`unit_role`]: the two pure decisions every reader shares (the
//!   writer's summary maintenance, the backfill, the tree view, the selection).
//! - `summary`: the `operation_tree_dirs` rows, their delta maintenance inside the
//!   writer's transactions, and the from-scratch aggregate they must always equal.
//! - `status`: whether an op's summary is built, and the backfill registry the tree
//!   view asks about one that isn't.
//! - `view` (+ `layout`, `read`, `wire`): one window of an op's tree, read off the
//!   summary and the item indexes; `selection`: the rollback selection, normalized
//!   and counted, and THE per-row decision the engine shares.
//!
//! The backfill (building the summary of an op journaled before it existed) is
//! `crate::tree_backfill`, outside this module: it drives the writer, and the writer
//! builds on this module, so keeping it here would weld the two together.

use crate::rollback::action_table::has_inverse;
use crate::store::awaits_reversal;
use crate::types::{EntryType, ItemOutcome, OpKind, RowRole, SkipReason};

pub(crate) mod status;
pub(crate) mod summary;

mod layout;
pub(crate) mod read;
pub(crate) mod rollback_filter;
pub(crate) mod selection;
pub(crate) mod view;
mod wire;

#[cfg(test)]
mod backfill_tests;
#[cfg(test)]
mod maintenance_tests;
#[cfg(test)]
#[allow(
    clippy::print_stderr,
    reason = "the scale tests print what each read took, as a benchmark does"
)]
mod scale_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_test_support;
#[cfg(test)]
mod view_tests;

/// Which side of each row an operation's tree is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TreeAxis {
    /// Where the items came from: a trash, delete, rename, tag write, and every
    /// inverse operation (its rows carry no dest).
    Source,
    /// Where the items landed: a copy, move, create, or archive edit.
    Dest,
}

impl TreeAxis {
    /// The axis for an op of `kind`; `is_inverse` is "this op rolls another back".
    ///
    /// **Why**: the tree shows where the thing the undo acts on lives. On the dest axis
    /// a copy's file rows AND its created-folder rows sit on one side, and a same-FS
    /// move's unit and its `search_only` leaves both carry a landed dest. A trash stays
    /// on the source side, since the Trash's internal names mean nothing to a person.
    /// An inverse records no dest on its rows, so it reads on its source.
    pub(crate) fn of(kind: OpKind, is_inverse: bool) -> Self {
        if is_inverse {
            return TreeAxis::Source;
        }
        match kind {
            OpKind::Copy | OpKind::Move | OpKind::CreateFile | OpKind::CreateFolder | OpKind::ArchiveEdit => {
                TreeAxis::Dest
            }
            OpKind::Trash | OpKind::Delete | OpKind::Rename | OpKind::Tag => TreeAxis::Source,
        }
    }

    /// The `(dir_id, name)` a row sits at on this axis. A row missing its axis side (a
    /// dest-axis leaf recorded source-only, the operation's own skipped row) falls back
    /// to its other side, so it still has a place in the tree, which may then be a
    /// forest. ❌ Never dropped.
    pub(crate) fn side<'a>(self, source: (i64, &'a str), dest: Option<(i64, &'a str)>) -> (i64, &'a str) {
        match (self, dest) {
            (TreeAxis::Dest, Some(dest)) => dest,
            _ => source,
        }
    }
}

/// Whether an op of `kind` writes the folders it creates around its files as rows of
/// their own (created in place): a copy, and a cross-FS move. Those folders are
/// [`UnitRole::Housekeeping`]. A create-folder's folder is the item itself.
pub(crate) fn creates_folders_around_files(kind: OpKind) -> bool {
    matches!(kind, OpKind::Copy | OpKind::Move)
}

/// What a row is to a rollback, as the tree shows it and the selection counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnitRole {
    /// A `rollback_unit` that still awaits reversal: it has a checkbox and counts in
    /// "N of M".
    Selectable,
    /// A folder a copy or move created (written created-in-place). No checkbox of its
    /// own: it goes along only when its whole subtree is selected, since a folder
    /// still holding a file can't be emptied.
    Housekeeping,
    /// A `search_only` row under a unit: it comes back with its folder.
    Contents,
    /// Rolled back, or the operation's own skip or failure: nothing left to do.
    Settled,
    /// A row no rollback has an inverse for (a permanent delete's).
    NotReversible,
}

/// The role of one row, a pure function of its own fields. Op-level rollbackability
/// (a `not_rollbackable` op, one `rolling_back`) is applied by readers at query time,
/// ❌ never here: eligibility is written at finalize, after the rows, so a summary
/// built from it would be wrong for every op that finalizes `not_rollbackable`.
pub(crate) fn unit_role(
    kind: OpKind,
    row_role: RowRole,
    entry_type: EntryType,
    created_in_place: bool,
    outcome: ItemOutcome,
    skip_reason: Option<SkipReason>,
) -> UnitRole {
    if row_role == RowRole::SearchOnly {
        return UnitRole::Contents;
    }
    if !awaits_reversal(outcome, skip_reason) {
        return UnitRole::Settled;
    }
    if !has_inverse(kind, entry_type, created_in_place) {
        return UnitRole::NotReversible;
    }
    if entry_type == EntryType::Dir && created_in_place && creates_folders_around_files(kind) {
        return UnitRole::Housekeeping;
    }
    UnitRole::Selectable
}
