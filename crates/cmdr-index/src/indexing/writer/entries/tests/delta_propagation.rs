//! Automatic `dir_stats` delta propagation up the ancestor chain on
//! delete, subtree delete, upsert insert, and upsert update, plus the
//! dir-count bump for a new dir.

use super::*;

#[test]
fn delete_entry_by_id_auto_propagates_delta() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert a parent dir and a file
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "p".into(),
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
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(500),
            physical_size: Some(500),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();

    // Pre-populate dir_stats for the parent
    writer.flush_blocking().unwrap();

    // Manually set dir_stats for parent via direct DB write (using the by-id API)
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: 10,
                recursive_logical_size: 500,
                recursive_physical_size: 500,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();
    }

    // Delete the file: writer should auto-propagate (-500, -1, 0) to parent id=10
    writer.send(WriteMessage::DeleteEntryById(11)).unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(stats.recursive_logical_size, 0, "size should be 0 after file deletion");
    assert_eq!(stats.recursive_file_count, 0, "file count should be 0");

    writer.shutdown();
}

#[test]
fn delete_subtree_by_id_auto_propagates_delta() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Build tree: ROOT(1) -> root_dir(10) -> sub(11) -> file.txt(12, 300 bytes)
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "root".into(),
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
            name: "sub".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 12,
            parent_id: 11,
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(300),
            physical_size: Some(300),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.flush_blocking().unwrap();

    // Pre-populate dir_stats for ancestors
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: ROOT_ID,
                    recursive_logical_size: 300,
                    recursive_physical_size: 300,
                    recursive_file_count: 1,
                    recursive_dir_count: 2,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
                DirStatsById {
                    entry_id: 10,
                    recursive_logical_size: 300,
                    recursive_physical_size: 300,
                    recursive_file_count: 1,
                    recursive_dir_count: 1,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
            ],
        )
        .unwrap();
    }

    // Delete the /root/sub subtree (id=11)
    writer.send(WriteMessage::DeleteSubtreeById(11)).unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // root_dir(10) should have lost: size=300, files=1, dirs=1
    let root_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(root_stats.recursive_logical_size, 0);
    assert_eq!(root_stats.recursive_file_count, 0);
    assert_eq!(root_stats.recursive_dir_count, 0);

    // ROOT(1) should have lost: size=300, files=1, dirs=1
    let vol_stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(vol_stats.recursive_logical_size, 0);
    assert_eq!(vol_stats.recursive_file_count, 0);
    assert_eq!(vol_stats.recursive_dir_count, 1); // root_dir(10) still exists

    writer.shutdown();
}

#[test]
fn upsert_entry_v2_auto_propagates_delta_on_insert() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert a parent directory and pre-populate its dir_stats
    let entries = vec![EntryRow {
        id: 10,
        parent_id: ROOT_ID,
        name: "home".into(),
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
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();
    }

    // Insert a new file via UpsertEntryV2: should auto-propagate to parent
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 10,
            name: "doc.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(500),
            physical_size: Some(500),
            modified_at: Some(1700000000),
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(stats.recursive_logical_size, 500, "parent should have file's size");
    assert_eq!(stats.recursive_file_count, 1, "parent should count the new file");
    assert_eq!(stats.recursive_dir_count, 0);

    writer.shutdown();
}

#[test]
fn upsert_entry_v2_auto_propagates_delta_on_update() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert parent dir with dir_stats
    let entries = vec![EntryRow {
        id: 10,
        parent_id: ROOT_ID,
        name: "home".into(),
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
                recursive_logical_size: 200,
                recursive_physical_size: 200,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();
    }

    // Insert a file via UpsertEntryV2
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 10,
            name: "doc.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(200),
            physical_size: Some(200),
            modified_at: Some(1700000000),
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Update the same file with a larger size: should propagate +100 delta
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 10,
            name: "doc.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(300),
            physical_size: Some(300),
            modified_at: Some(1700000001),
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    // Initial 200 + insert propagated 200 + update propagated +100 = 500
    assert_eq!(
        stats.recursive_logical_size, 500,
        "parent should reflect insert + update deltas"
    );
    assert_eq!(stats.recursive_file_count, 2, "file_count: 1 initial + 1 from insert");

    writer.shutdown();
}

#[test]
fn upsert_entry_v2_auto_propagates_dir_count_on_new_dir() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Pre-populate root dir_stats
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: ROOT_ID,
                recursive_logical_size: 0,
                recursive_physical_size: 0,
                recursive_file_count: 0,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();
    }

    // Insert a new directory via UpsertEntryV2
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "projects".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
    assert_eq!(stats.recursive_dir_count, 1, "root should count the new dir");
    assert_eq!(stats.recursive_file_count, 0);
    assert_eq!(stats.recursive_logical_size, 0);

    writer.shutdown();
}
