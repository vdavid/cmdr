//! The two item indexes the item tree's folder listings read: "this folder's files of
//! this op, in name order, from position k", on the source side and the dest side.
//!
//! **Files only** (`WHERE entry_type = 'file'`): a folder's DIR rows list as its child
//! folders, off the summary. Without the condition in the index, the listing's
//! `entry_type` filter reads the table for every row an `OFFSET` skips: the middle of
//! a 1M-file folder took 800 ms; index-only it takes 10 ms (release, Apple Silicon,
//! `src/tree/scale_tests.rs`, 2026-11-07).
//!
//! **Why outside the migration ladder**: the ladder runs inside `OperationLog::open`,
//! which the app calls from `setup()` on its main thread, before the event loop runs
//! and the window can show. These two indexes are the only part of the tree's schema
//! that scales with the journal (5.35 s on a 4M-row one, `DETAILS.md` § "Tree
//! summary"), so the writer builds them as its first message after open instead.
//! Operations journal behind it (the channel's backpressure), and until both exist the
//! tree answers `IndexesBuilding` (`IndexesUnavailable` once a build this launch couldn't
//! finish; every open queues it again). `IF NOT EXISTS` makes it a schema lookup on every
//! later launch, and it's the same call on a fresh file, where it's instant.

use rusqlite::Connection;

use super::OperationLogStoreError;

/// The index names, which [`tree_item_indexes_exist`] looks up.
const NAMES: [&str; 2] = ["operation_items_source_dir", "operation_items_dest_dir"];

/// Build both indexes if either is missing, in one transaction, and log what it cost.
/// Runs on the writer's connection.
pub(crate) fn build_tree_item_indexes(conn: &Connection) -> Result<(), OperationLogStoreError> {
    if tree_item_indexes_exist(conn)? {
        return Ok(());
    }
    let started = cmdr_fs::rt::now();
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS operation_items_source_dir
             ON operation_items (op_id, source_dir_id, source_name_folded)
             WHERE entry_type = 'file';
         CREATE INDEX IF NOT EXISTS operation_items_dest_dir
             ON operation_items (op_id, dest_dir_id, dest_name_folded)
             WHERE dest_dir_id IS NOT NULL AND entry_type = 'file';",
    )?;
    tx.commit()?;
    log::info!(
        target: "operation_log",
        "built the item tree's folder indexes in {:.1?}",
        cmdr_fs::rt::since(started)
    );
    Ok(())
}

/// Whether both indexes exist: what the tree checks before reading a folder.
pub(crate) fn tree_item_indexes_exist(conn: &Connection) -> Result<bool, OperationLogStoreError> {
    let found: i64 = conn
        .prepare_cached("SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name IN (?1, ?2)")?
        .query_row(rusqlite::params![NAMES[0], NAMES[1]], |row| row.get(0))?;
    Ok(found == NAMES.len() as i64)
}
