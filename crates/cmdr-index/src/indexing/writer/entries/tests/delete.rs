//! Deleting a single entry and a whole subtree by id, and the no-op path
//! when the id isn't in the table.

use super::*;

#[test]
fn delete_entry_by_id_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert an entry
    let entries = vec![EntryRow {
        id: 20,
        parent_id: ROOT_ID,
        name: "doomed.txt".into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(100),
        physical_size: Some(100),
        modified_at: None,
        inode: None,
    }];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.flush_blocking().unwrap();

    // Delete by ID
    writer.send(WriteMessage::DeleteEntryById(20)).unwrap();
    writer.flush_blocking().unwrap();

    let store = open_read(&db_path);
    let children = store.list_children(ROOT_ID).unwrap();
    assert!(children.is_empty(), "entry should be deleted");

    writer.shutdown();
}

#[test]
fn delete_subtree_by_id_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Build a tree: ROOT -> dir(10) -> file(11) + subdir(12)
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "a".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "b.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(50),
            physical_size: Some(50),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 12,
            parent_id: 10,
            name: "c".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.flush_blocking().unwrap();

    // Delete the subtree rooted at id=10
    writer.send(WriteMessage::DeleteSubtreeById(10)).unwrap();
    writer.flush_blocking().unwrap();

    let store = open_read(&db_path);
    let root_children = store.list_children(ROOT_ID).unwrap();
    assert!(root_children.is_empty(), "dir /a should be deleted");
    let a_children = store.list_children(10).unwrap();
    assert!(a_children.is_empty(), "children of /a should be deleted");

    writer.shutdown();
}

#[test]
fn delete_entry_by_id_for_nonexistent_skips_propagation() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert a directory and pre-populate its dir_stats
    let entries = vec![EntryRow {
        id: 10,
        parent_id: ROOT_ID,
        name: "p".into(),
        is_directory: true,
        is_symlink: false,
        logical_size: None,
        physical_size: None,
        modified_at: None,
        inode: None,
    }];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: 10,
                recursive_logical_size: 100,
                recursive_physical_size: 100,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();
    }

    // Delete a non-existent entry: should not propagate any delta
    writer.send(WriteMessage::DeleteEntryById(999)).unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(stats.recursive_logical_size, 100, "stats should be unchanged");
    assert_eq!(stats.recursive_file_count, 1);

    writer.shutdown();
}
