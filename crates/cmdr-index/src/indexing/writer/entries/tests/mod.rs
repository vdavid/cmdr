//! Tests for `entries.rs` (`writer::entries::tests`), split into one child per
//! area under test. Shared imports and the `insert_dir_with_stats`, `insert_file`,
//! and `seed_row` fixtures live here; every child starts with `use super::*;`.

use std::path::Path;

use super::*;
use crate::indexing::store::ROOT_ID;
use crate::indexing::writer::tests::{open_read, setup_db};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

mod bulk_reconcile;
mod cycle;
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

/// Flush with a deadline, so a writer that never answers fails the test rather
/// than hanging it.
fn flush_within(writer: &IndexWriter, deadline: std::time::Duration) {
    let (tx, rx) = std::sync::mpsc::channel();
    let flusher = writer.clone();
    std::thread::spawn(move || {
        let _ = tx.send(flusher.flush_blocking());
    });
    // Under nextest's 8 s slow-test timeout, so a wedge reports as this, not a kill.
    rx.recv_timeout(deadline)
        .expect("the writer never answered the flush: it's wedged")
        .unwrap();
}

/// A directory with one child directory, both with stats, for the cycle tests:
/// `left` (id 10) under the root, and `sub-dir` (id 20) under it.
fn insert_parent_and_child(writer: &IndexWriter, db_path: &Path) {
    let stats = |entry_id, dirs| DirStatsById {
        entry_id,
        recursive_logical_size: 100,
        recursive_physical_size: 100,
        recursive_file_count: 1,
        recursive_dir_count: dirs,
        recursive_has_symlinks: false,
        min_subtree_epoch: 0,
    };
    insert_dir_with_stats(writer, db_path, 10, ROOT_ID, "left", stats(10, 1));
    insert_dir_with_stats(writer, db_path, 20, 10, "sub-dir", stats(20, 0));
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
