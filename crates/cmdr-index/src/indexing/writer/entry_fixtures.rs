//! `EntryRow` fixtures for the writer tests: the three row shapes a test tree is
//! built from, so each `tests` module spells a tree as `dir_entry(10, ROOT_ID, "A")`
//! rather than a ten-line struct literal per node. Shared by `delta.rs`,
//! `repair.rs`, and `deferred_repair.rs`.
use crate::indexing::store::EntryRow;

/// A directory row with no sizes, no mtime, and no inode.
pub(super) fn dir_entry(id: i64, parent_id: i64, name: &str) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: true,
        is_symlink: false,
        logical_size: None,
        physical_size: None,
        modified_at: None,
        inode: None,
    }
}

/// A plain file row whose logical and physical size are both `size`.
pub(super) fn file_entry(id: i64, parent_id: i64, name: &str, size: u64) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(size),
        physical_size: Some(size),
        modified_at: None,
        inode: None,
    }
}

/// A zero-byte symlink row.
pub(super) fn symlink_entry(id: i64, parent_id: i64, name: &str) -> EntryRow {
    EntryRow {
        id,
        parent_id,
        name: name.into(),
        is_directory: false,
        is_symlink: true,
        logical_size: Some(0),
        physical_size: Some(0),
        modified_at: None,
        inode: None,
    }
}
