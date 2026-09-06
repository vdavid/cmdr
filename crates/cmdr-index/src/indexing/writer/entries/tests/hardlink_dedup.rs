//! Hardlink dedup: the primary row carries the sizes and inode, secondaries
//! get NULL sizes, the dedup self-heals after the primary is deleted, the
//! skip cases (`nlink == 1`, no inode), and what `dir_stats` counts.

use super::*;

#[test]
fn hardlink_dedup_insert_primary_stores_sizes_and_inode() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "primary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000000),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let id = IndexStore::resolve_component(&conn, ROOT_ID, "primary.txt")
        .unwrap()
        .unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
    assert_eq!(entry.logical_size, Some(1000), "primary should keep its sizes");
    assert_eq!(entry.inode, Some(100), "inode should be stored");

    writer.shutdown();
}

#[test]
fn hardlink_dedup_insert_secondary_gets_null_sizes() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert primary link
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "primary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000000),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Insert secondary link (same inode, different name)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "secondary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000000),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
        .unwrap()
        .unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
    assert_eq!(entry.logical_size, None, "secondary should have NULL sizes");
    assert_eq!(entry.physical_size, None);
    assert_eq!(entry.inode, Some(100), "inode should still be stored");

    writer.shutdown();
}

#[test]
fn hardlink_dedup_update_secondary_keeps_null_sizes() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert primary
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "primary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000000),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Insert secondary (gets NULL sizes via dedup)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "secondary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000000),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Reconciler sends update for secondary with full sizes: dedup should fire again
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "secondary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000001),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
        .unwrap()
        .unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
    assert_eq!(
        entry.logical_size, None,
        "secondary sizes should stay NULL after update"
    );

    writer.shutdown();
}

#[test]
fn hardlink_dedup_self_healing_after_primary_deleted() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Pre-populate root dir_stats so delta propagation works
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

    // Insert primary
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "primary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000000),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Insert secondary (gets NULL sizes)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "secondary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000000),
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Delete primary
    let primary_id = {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::resolve_component(&conn, ROOT_ID, "primary.txt")
            .unwrap()
            .unwrap()
    };
    writer.send(WriteMessage::DeleteEntryById(primary_id)).unwrap();
    writer.flush_blocking().unwrap();

    // Reconciler sends update for secondary: nlink=1 since it's the only link now
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "secondary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: Some(1700000001),
            inode: Some(100),
            nlink: Some(1),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let sec_id = IndexStore::resolve_component(&conn, ROOT_ID, "secondary.txt")
        .unwrap()
        .unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, sec_id).unwrap().unwrap();
    assert_eq!(
        entry.logical_size,
        Some(1000),
        "secondary should recover sizes after primary deleted"
    );
    assert_eq!(entry.physical_size, Some(1000));

    writer.shutdown();
}

#[test]
fn hardlink_dedup_nlink_1_skips_dedup() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert two files with the same inode but nlink=1 (not actually hardlinked)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "file_a.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(500),
            physical_size: Some(500),
            modified_at: None,
            inode: Some(200),
            nlink: Some(1),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "file_b.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(500),
            physical_size: Some(500),
            modified_at: None,
            inode: Some(200),
            nlink: Some(1),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let b_id = IndexStore::resolve_component(&conn, ROOT_ID, "file_b.txt")
        .unwrap()
        .unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, b_id).unwrap().unwrap();
    assert_eq!(entry.logical_size, Some(500), "nlink=1 should never trigger dedup");

    writer.shutdown();
}

#[test]
fn hardlink_dedup_no_inode_skips_dedup() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert first file with inode
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "file_a.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(500),
            physical_size: Some(500),
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Insert second file with no inode (non-Unix)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "file_b.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(500),
            physical_size: Some(500),
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let b_id = IndexStore::resolve_component(&conn, ROOT_ID, "file_b.txt")
        .unwrap()
        .unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, b_id).unwrap().unwrap();
    assert_eq!(entry.logical_size, Some(500), "no inode should never trigger dedup");

    writer.shutdown();
}

#[test]
fn hardlink_dedup_dir_stats_only_counts_primary_size() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

    // Insert a parent directory and pre-populate its dir_stats
    let entries = vec![EntryRow {
        id: 10,
        parent_id: ROOT_ID,
        name: "mydir".into(),
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

    // Insert primary hardlink into dir
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 10,
            name: "primary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: None,
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Insert secondary hardlink into dir (same inode)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 10,
            name: "secondary.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: None,
            inode: Some(100),
            nlink: Some(2),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        stats.recursive_logical_size, 1000,
        "dir should only count the primary's size"
    );
    assert_eq!(stats.recursive_file_count, 2, "both links count as files");

    writer.shutdown();
}
