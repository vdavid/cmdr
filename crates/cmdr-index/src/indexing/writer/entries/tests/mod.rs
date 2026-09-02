//! Tests for `entries.rs` (`writer::entries::tests`), split into one child per
//! area under test. Shared imports and the `insert_dir_with_stats`, `insert_file`,
//! and `seed_row` fixtures live here; every child starts with `use super::*;`.

use std::path::Path;

use super::*;
use crate::indexing::store::ROOT_ID;
use crate::indexing::writer::tests::{open_read, setup_db};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

mod bulk_reconcile;
mod delete;
mod delta_propagation;
mod hardlink_dedup;
mod id_counter;
mod insert_upsert;
mod move_entry;
mod symlinks;

/// Helper: insert a dir with dir_stats. Returns nothing (the caller knows the id it asked for).
fn insert_dir_with_stats(
    writer: &IndexWriter,
    db_path: &Path,
    id: i64,
    parent_id: i64,
    name: &str,
    stats: DirStatsById,
) {
    writer
        .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        }]))
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(db_path).unwrap();
    IndexStore::upsert_dir_stats_by_id(&conn, &[stats]).unwrap();
}

/// Helper: insert a plain file row.
fn insert_file(writer: &IndexWriter, id: i64, parent_id: i64, name: &str, size: u64) {
    writer
        .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
            id,
            parent_id,
            name: name.into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(size),
            physical_size: Some(size),
            modified_at: None,
            inode: None,
        }]))
        .unwrap();
    writer.flush_blocking().unwrap();
}

/// A plain file row under ROOT, for seeding ids the counter doesn't know about.
fn seed_row(id: i64, name: &str) -> EntryRow {
    EntryRow {
        id,
        parent_id: ROOT_ID,
        name: name.into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(1),
        physical_size: Some(1),
        modified_at: None,
        inode: None,
    }
}
