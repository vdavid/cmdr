//! The per-kind inverse table: which act undoes each recorded row. Pure, and a leaf
//! of its own so the item tree (`crate::tree`, for its row roles) reads it without
//! depending on the planner, which depends on the tree.

use crate::types::{EntryType, OpKind};

/// The shape of the inverse for one item, derived purely from the op kind and the
/// item's entry type (`DETAILS.md` § "Per-kind inverse table").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InverseAction {
    /// Remove the created file at its dest, only if it still matches the snapshot
    /// (create_file, compress archive).
    RemoveFileIfUnchanged,
    /// Remove a copied file at its dest, only if it still matches the snapshot AND
    /// its original is still where it was copied from, unchanged in size: removing
    /// a copy is permanent, so it must never take the only copy left.
    RemoveCopyIfOriginalStays,
    /// Remove the created/copied directory at its dest, only if still empty (copy's
    /// created dirs, create_folder).
    RemoveDirIfEmpty,
    /// Move the item back from where it ended up (dest) to its original (source):
    /// move, trash restore, rename-back.
    RestoreMove,
    /// Write the file's prior Finder-tags value back, only while it still carries
    /// the value the write left (a tag write).
    RestoreTags,
}

/// The inverse for one item, the ONE decision both the reversal and the dispatch
/// summary read. A directory the operation created in place is taken away again
/// whatever the op kind; everything else follows the kind's table.
pub(super) fn item_action(kind: OpKind, entry_type: EntryType, created_in_place: bool) -> Option<InverseAction> {
    if entry_type == EntryType::Dir && created_in_place {
        return Some(InverseAction::RemoveDirIfEmpty);
    }
    inverse_action(kind, entry_type)
}

/// Whether a rollback has any inverse for this row at all: [`item_action`]'s answer,
/// for readers that only need yes or no (the tree's row roles, `crate::tree`).
pub(crate) fn has_inverse(kind: OpKind, entry_type: EntryType, created_in_place: bool) -> bool {
    item_action(kind, entry_type, created_in_place).is_some()
}

pub(super) fn inverse_action(kind: OpKind, entry_type: EntryType) -> Option<InverseAction> {
    match kind {
        OpKind::Copy => Some(match entry_type {
            EntryType::File => InverseAction::RemoveCopyIfOriginalStays,
            EntryType::Dir => InverseAction::RemoveDirIfEmpty,
        }),
        OpKind::CreateFile | OpKind::ArchiveEdit => Some(InverseAction::RemoveFileIfUnchanged),
        OpKind::CreateFolder => Some(InverseAction::RemoveDirIfEmpty),
        OpKind::Move | OpKind::Trash | OpKind::Rename => Some(InverseAction::RestoreMove),
        OpKind::Tag => Some(InverseAction::RestoreTags),
        // Delete is never rollbackable (gated before we reach items).
        OpKind::Delete => None,
    }
}
