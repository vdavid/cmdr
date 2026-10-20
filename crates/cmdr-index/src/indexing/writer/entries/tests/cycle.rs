//! A `parent_id` cycle that is already in the DB, whatever put it there.
//!
//! `MoveEntryV2` refuses to close one (`move_entry.rs`), but the writer can't
//! prove that nothing else ever did, and a single unbounded walk around a cycle
//! wedges the whole volume for the session: the walk spins inside the open
//! batch, no flush is answered again, and the uncommitted transaction holds the
//! write lock (GitHub issue #564). So every walk the writer takes over the tree
//! ends, and a cycle it runs into marks the index for a rebuild.

use super::*;

/// `left` (10) and `sub-dir` (20) pointing at each other, cut off from the root.
/// Closed behind the writer's back, the way an older build's DB could hold it.
fn close_a_cycle(writer: &IndexWriter, db_path: &Path) {
    insert_parent_and_child(writer, db_path);
    let conn = IndexStore::open_write_connection(db_path).unwrap();
    IndexStore::move_entry(&conn, 10, 20).unwrap();
}

fn marked_for_rebuild(db_path: &Path) -> bool {
    let conn = IndexStore::open_write_connection(db_path).unwrap();
    IndexStore::index_needs_rebuild(&conn).unwrap()
}

/// A size delta walks up from the new file's parent and has no short-circuit, so
/// on a cycle it never reached the top of the chain.
#[test]
fn a_size_delta_into_a_cycle_ends_and_marks_the_index_for_a_rebuild() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();
    close_a_cycle(&writer, &db_path);

    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 20,
            name: "new.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(5),
            physical_size: Some(5),
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    flush_within(&writer, std::time::Duration::from_secs(5));

    assert!(marked_for_rebuild(&db_path), "a cycle means the index can't be trusted");
    writer.shutdown();
}

/// A subtree delete reads the subtree's totals with a recursive query first,
/// which never ran dry on a cycle.
#[test]
fn a_subtree_delete_inside_a_cycle_ends() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();
    close_a_cycle(&writer, &db_path);

    writer.send(WriteMessage::DeleteSubtreeById(10)).unwrap();
    flush_within(&writer, std::time::Duration::from_secs(5));

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    assert!(
        IndexStore::get_entry_by_id(&conn, 10).unwrap().is_none()
            && IndexStore::get_entry_by_id(&conn, 20).unwrap().is_none(),
        "the delete takes the whole cycle with it"
    );
    writer.shutdown();
}
