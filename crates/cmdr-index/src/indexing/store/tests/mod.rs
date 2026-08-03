//! `IndexStore` tests, split by theme, each named after the store concern it
//! covers. The temp-DB fixture and the insert helper every theme uses live
//! here; the themes are the sibling modules below.

use super::*;

mod dir_stats_and_epochs;
mod entry_crud;
mod error_classification;
mod insert_throughput_probe;
mod meta_and_calibration;
mod open_and_recover;
mod path_resolution;
mod subtree_deletes;

/// Create an IndexStore backed by a temporary file.
fn open_temp_store() -> (IndexStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let store = IndexStore::open(&db_path).expect("failed to open store");
    (store, dir)
}

/// Helper: insert an entry using integer-keyed API. Returns the new ID.
fn insert_entry(conn: &Connection, parent_id: i64, name: &str, is_dir: bool, size: Option<u64>) -> i64 {
    IndexStore::insert_entry_v2(conn, parent_id, name, is_dir, false, size, size, None, None).unwrap()
}
