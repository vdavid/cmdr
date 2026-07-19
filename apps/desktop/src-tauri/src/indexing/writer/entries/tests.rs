use std::path::Path;

use super::*;
use crate::indexing::store::ROOT_ID;
use crate::indexing::writer::tests::{open_read, setup_db};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};

// ── Integer-keyed variant tests ──────────────────────────────────

#[test]
fn insert_entries_v2_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    let entries = vec![EntryRow {
        id: 10,
        parent_id: ROOT_ID,
        name: "file.txt".into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(1024),
        physical_size: Some(1024),
        modified_at: Some(1700000000),
        inode: None,
    }];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer.flush_blocking().unwrap();

    let store = open_read(&db_path);
    let children = store.list_children(ROOT_ID).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "file.txt");
    assert_eq!(children[0].logical_size, Some(1024));
    assert_eq!(children[0].id, 10);

    writer.shutdown();
}

// The accumulator must only count rows that actually landed in the DB.
// `insert_entries_v2_batch` uses `INSERT OR IGNORE`, so one duplicate in
// a batch skips just that row and the rest insert. The accumulator maps
// drive `compute_all_aggregates_with_maps`; counting bytes for a row that
// lost an OR-IGNORE produces inflated dir_stats (this was one of the
// mechanisms behind the 1.83 TB ghost size on `..` of a 994 GB volume).
#[test]
fn handle_insert_entries_v2_only_accumulates_rows_that_landed() {
    use std::sync::atomic::AtomicU64;

    let (db_path, _dir) = setup_db();
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Pre-seed: id=100, name="first.txt".
    let entries_first = vec![EntryRow {
        id: 100,
        parent_id: ROOT_ID,
        name: "first.txt".into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(10),
        physical_size: Some(10),
        modified_at: None,
        inode: None,
    }];
    IndexStore::insert_entries_v2_batch(&conn, &entries_first).unwrap();

    // Second batch: row 0 collides on the (parent_id, name_folded) UNIQUE
    // index (same `first.txt` under ROOT_ID). Row 1 is fresh and must land.
    let entries_dup = vec![
        EntryRow {
            id: 200,
            parent_id: ROOT_ID,
            name: "first.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(999_999),
            physical_size: Some(999_999),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 101,
            parent_id: ROOT_ID,
            name: "second.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(20),
            physical_size: Some(20),
            modified_at: None,
            inode: None,
        },
    ];

    let mut accumulator = AccumulatorMaps::new();
    let expected = AtomicU64::new(0);
    let mutation_tracker = MutationTracker::new(true);

    let signal = IndexFailureSignal::new();
    handle_insert_entries_v2(
        &conn,
        entries_dup,
        &mut accumulator,
        &None,
        "root",
        &expected,
        &mutation_tracker,
        &signal,
    );

    // DB has the original first.txt (id=100) and the new second.txt (id=101).
    // id=200 was the OR-IGNORE'd duplicate and must not be in the DB.
    assert_eq!(
        IndexStore::get_entry_by_id(&conn, 100).unwrap().unwrap().name,
        "first.txt"
    );
    assert_eq!(
        IndexStore::get_entry_by_id(&conn, 101).unwrap().unwrap().name,
        "second.txt"
    );
    assert!(IndexStore::get_entry_by_id(&conn, 200).unwrap().is_none());

    // Accumulator must reflect exactly one new entry (the row that landed),
    // never the 999_999-byte phantom. If a regression makes the accumulator
    // count the OR-IGNORE'd row, this assert catches it.
    assert_eq!(
        accumulator.entries_inserted, 1,
        "accumulator must count only rows that landed in the DB"
    );
    let stats = accumulator.direct_stats.get(&ROOT_ID).expect("ROOT_ID stats present");
    assert_eq!(stats.0, 20, "logical bytes must only count the landed row");
    assert_eq!(stats.1, 20, "physical bytes must only count the landed row");
    assert_eq!(stats.2, 1, "file count must only include the landed row");
}

#[test]
fn upsert_entry_v2_insert_and_update() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Insert via UpsertEntryV2 (entry doesn't exist yet)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "new.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(256),
            physical_size: Some(256),
            modified_at: Some(1700000000),
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Update via UpsertEntryV2 (entry now exists)
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "new.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(512),
            physical_size: Some(512),
            modified_at: Some(1700000001),
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let store = open_read(&db_path);
    let children = store.list_children(ROOT_ID).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "new.txt");
    assert_eq!(children[0].logical_size, Some(512), "size should be updated to 512");

    writer.shutdown();
}

#[test]
fn upsert_entry_v2_initializes_dir_stats_for_new_dirs() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Insert a new directory via UpsertEntryV2
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: ROOT_ID,
            name: "newdir".into(),
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

    // The new directory should have a zero-valued dir_stats row
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let dir_id = IndexStore::resolve_component(&conn, ROOT_ID, "newdir")
        .unwrap()
        .expect("newdir should exist");

    let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap();
    assert!(stats.is_some(), "new dir should have dir_stats");
    let stats = stats.unwrap();
    assert_eq!(stats.recursive_logical_size, 0);
    assert_eq!(stats.recursive_file_count, 0);
    assert_eq!(stats.recursive_dir_count, 0);

    writer.shutdown();
}

#[test]
fn delete_entry_by_id_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
fn delete_entry_by_id_auto_propagates_delta() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
fn delete_entry_by_id_for_nonexistent_skips_propagation() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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

#[test]
fn upsert_entry_v2_auto_propagates_delta_on_insert() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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

// ── Hardlink dedup tests ────────────────────────────────────────

#[test]
fn hardlink_dedup_insert_primary_stores_sizes_and_inode() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

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

// ── recursive_has_symlinks tests ─────────────────────────────────

#[test]
fn upsert_symlink_propagates_recursive_has_symlinks_up() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Build a 2-level dir tree first (no symlinks).
    // ROOT -> outer (id=10) -> inner (id=11)
    let entries = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "outer".into(),
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
            name: "inner".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Confirm baseline: no symlinks anywhere
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 11)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
    }

    // Add a symlink under inner via UpsertEntryV2
    writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: 11,
            name: "link".into(),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Flag should propagate up to both inner and outer
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 11)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "inner should flip to true"
        );
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 10)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "outer should propagate from inner"
        );
    }

    writer.shutdown();
}

#[test]
fn delete_last_symlink_clears_recursive_has_symlinks_up() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // ROOT -> outer (id=20) -> link (id=21, symlink)
    let entries = vec![
        EntryRow {
            id: 20,
            parent_id: ROOT_ID,
            name: "outer".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 21,
            parent_id: 20,
            name: "link".into(),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Baseline: outer has the flag set
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 20)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
    }

    // Delete the only symlink
    writer.send(WriteMessage::DeleteEntryById(21)).unwrap();
    writer.flush_blocking().unwrap();

    // Flag should clear up the chain
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 20)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "outer should clear after last symlink removed"
        );
    }

    writer.shutdown();
}

#[test]
fn delete_subtree_with_symlinks_clears_parent_flag() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // ROOT -> top (id=30)
    //   ├── doomed (id=31) -> link (id=32, symlink)
    //   └── safe (id=33)  (no symlinks)
    let entries = vec![
        EntryRow {
            id: 30,
            parent_id: ROOT_ID,
            name: "top".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 31,
            parent_id: 30,
            name: "doomed".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 32,
            parent_id: 31,
            name: "link".into(),
            is_directory: false,
            is_symlink: true,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 33,
            parent_id: 30,
            name: "safe".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(WriteMessage::InsertEntriesV2(entries)).unwrap();
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Baseline: top has the flag
    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            IndexStore::get_dir_stats_by_id(&conn, 30)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks
        );
    }

    // Delete the doomed subtree (which contained the only symlink)
    writer.send(WriteMessage::DeleteSubtreeById(31)).unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        assert!(
            !IndexStore::get_dir_stats_by_id(&conn, 30)
                .unwrap()
                .unwrap()
                .recursive_has_symlinks,
            "top should clear once the subtree containing the symlink is gone"
        );
    }

    writer.shutdown();
}

// ── MoveEntryV2 tests ────────────────────────────────────────────

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

#[test]
fn move_entry_v2_same_parent_preserves_dir_stats() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Parent dir + child dir with non-trivial dir_stats. The whole point
    // of MoveEntryV2 vs. delete+insert is preserving these numbers.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "home",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 5_000,
            recursive_physical_size: 5_000,
            recursive_file_count: 7,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "Foo",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 5_000,
            recursive_physical_size: 5_000,
            recursive_file_count: 7,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    // Same-parent rename: "Foo" → "Bar".
    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 10,
            new_name: "Bar".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(entry.name, "Bar", "name should be updated");
    assert_eq!(entry.parent_id, 10, "parent unchanged");

    let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(
        moved_stats.recursive_logical_size, 5_000,
        "moved dir keeps its own stats"
    );
    assert_eq!(moved_stats.recursive_file_count, 7);

    let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(
        parent_stats.recursive_logical_size, 5_000,
        "parent stats unchanged for same-parent rename"
    );
    assert_eq!(parent_stats.recursive_file_count, 7);
    assert_eq!(parent_stats.recursive_dir_count, 1);

    writer.shutdown();
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

#[test]
fn move_entry_v2_destination_collision_replaces_conflicting_file() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // One dir with two files. Moving "draft.txt" onto "final.txt"'s name
    // (a rename-with-overwrite, or a concurrent upsert racing ahead of the
    // move) used to fail the UNIQUE (parent_id, name_folded) constraint and
    // leave the moved entry stuck at its old name.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "docs",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 150,
            recursive_physical_size: 150,
            recursive_file_count: 2,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_file(&writer, 20, 10, "draft.txt", 100);
    insert_file(&writer, 21, 10, "final.txt", 50);

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 10,
            new_name: "final.txt".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(moved.name, "final.txt", "moved entry owns the destination name");
    assert_eq!(moved.parent_id, 10);
    assert!(
        IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
        "conflicting entry is deleted"
    );

    // The conflicting file's contribution is subtracted from the parent.
    let parent_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(parent_stats.recursive_logical_size, 100);
    assert_eq!(parent_stats.recursive_file_count, 1);

    writer.shutdown();
}

#[test]
fn move_entry_v2_destination_collision_replaces_conflicting_dir_subtree() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // A/proj (id 20, rich dir_stats) moves to B/proj, but B already has a
    // stale dir row "proj" (id 21) with a child file. The stale subtree must
    // go and the moved dir must keep its id and dir_stats.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 1000,
            recursive_physical_size: 1000,
            recursive_file_count: 3,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 500,
            recursive_physical_size: 500,
            recursive_file_count: 1,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "proj",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 1000,
            recursive_physical_size: 1000,
            recursive_file_count: 3,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        21,
        11,
        "proj",
        DirStatsById {
            entry_id: 21,
            recursive_logical_size: 500,
            recursive_physical_size: 500,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_file(&writer, 22, 21, "old.txt", 500);

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 11,
            new_name: "proj".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let moved = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(moved.parent_id, 11, "moved dir landed under B");
    assert_eq!(moved.name, "proj");
    assert!(
        IndexStore::get_entry_by_id(&conn, 21).unwrap().is_none(),
        "conflicting dir is deleted"
    );
    assert!(
        IndexStore::get_entry_by_id(&conn, 22).unwrap().is_none(),
        "conflicting dir's children are deleted"
    );

    let moved_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(
        moved_stats.recursive_logical_size, 1000,
        "moved dir keeps its own stats"
    );
    assert_eq!(moved_stats.recursive_file_count, 3);

    // A lost the moved dir's contribution entirely.
    let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(a_stats.recursive_logical_size, 0);
    assert_eq!(a_stats.recursive_file_count, 0);
    assert_eq!(a_stats.recursive_dir_count, 0);

    // B lost the stale subtree (-500, -1 file, -1 dir) and gained the moved
    // dir (+1000, +3 files, +1 dir).
    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert_eq!(b_stats.recursive_logical_size, 1000);
    assert_eq!(b_stats.recursive_file_count, 3);
    assert_eq!(b_stats.recursive_dir_count, 1);

    writer.shutdown();
}

#[test]
fn move_entry_v2_cross_parent_propagates_deltas() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Two sibling dirs A and B, each with their own pre-populated stats.
    // Then a child dir D under A with non-trivial stats.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 1024,
            recursive_physical_size: 2048,
            recursive_file_count: 5,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "D",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 1024,
            recursive_physical_size: 2048,
            recursive_file_count: 5,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 11,
            new_name: "D".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // D itself: same dir_stats, new parent.
    let d_entry = IndexStore::get_entry_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(d_entry.parent_id, 11);
    let d_stats = IndexStore::get_dir_stats_by_id(&conn, 20).unwrap().unwrap();
    assert_eq!(d_stats.recursive_logical_size, 1024);
    assert_eq!(d_stats.recursive_file_count, 5);

    // A: lost D's contribution (size 1024, 5 files, 1 dir for D itself).
    let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(a_stats.recursive_logical_size, 0);
    assert_eq!(a_stats.recursive_physical_size, 0);
    assert_eq!(a_stats.recursive_file_count, 0);
    assert_eq!(a_stats.recursive_dir_count, 0);

    // B: gained D's contribution.
    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert_eq!(b_stats.recursive_logical_size, 1024);
    assert_eq!(b_stats.recursive_physical_size, 2048);
    assert_eq!(b_stats.recursive_file_count, 5);
    assert_eq!(b_stats.recursive_dir_count, 1);

    writer.shutdown();
}

#[test]
fn move_entry_v2_file_cross_parent_propagates_deltas() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Two parent dirs, both starting with empty stats.
    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 700,
            recursive_physical_size: 700,
            recursive_file_count: 1,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    // Insert a file under A (size 700, contributes 1 file).
    writer
        .send(WriteMessage::InsertEntriesV2(vec![EntryRow {
            id: 30,
            parent_id: 10,
            name: "f.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(700),
            physical_size: Some(700),
            modified_at: Some(1700000000),
            inode: Some(99),
        }]))
        .unwrap();
    writer.flush_blocking().unwrap();

    // Move file to B.
    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 30,
            new_parent_id: 11,
            new_name: "f.txt".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let a_stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(a_stats.recursive_logical_size, 0, "A loses the file's size");
    assert_eq!(a_stats.recursive_file_count, 0);

    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert_eq!(b_stats.recursive_logical_size, 700);
    assert_eq!(b_stats.recursive_file_count, 1);
    assert_eq!(b_stats.recursive_dir_count, 0, "files don't contribute to dir count");

    writer.shutdown();
}

#[test]
fn move_entry_v2_no_op_when_target_matches_current() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "home",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 1024,
            recursive_physical_size: 1024,
            recursive_file_count: 3,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    // Capture the per-writer mutation count before the no-op. Reading the
    // global `WRITER_GENERATION` here would flake under concurrent tests,
    // since `cargo test` runs tests as threads in one process and any other
    // writer that mutates between `before` and `after` would bump it.
    let gen_before = writer.mutation_count();

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 10,
            new_parent_id: ROOT_ID,
            new_name: "home".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let stats = IndexStore::get_dir_stats_by_id(&conn, 10).unwrap().unwrap();
    assert_eq!(stats.recursive_logical_size, 1024, "no-op preserves stats");
    assert_eq!(stats.recursive_file_count, 3);

    // The per-writer counter should not have moved (the no-op short-circuits
    // before `bump_generation`).
    let gen_after = writer.mutation_count();
    assert_eq!(
        gen_before, gen_after,
        "no-op should not bump the writer's mutation counter"
    );

    writer.shutdown();
}

#[test]
fn move_entry_v2_cross_parent_propagates_recursive_has_symlinks() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "A",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 1,
            recursive_has_symlinks: true,
            min_subtree_epoch: 0,
        },
    );
    insert_dir_with_stats(
        &writer,
        &db_path,
        11,
        ROOT_ID,
        "B",
        DirStatsById {
            entry_id: 11,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );
    // The dir being moved carries the symlink flag in its own subtree.
    insert_dir_with_stats(
        &writer,
        &db_path,
        20,
        10,
        "D",
        DirStatsById {
            entry_id: 20,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: true,
            min_subtree_epoch: 0,
        },
    );

    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 20,
            new_parent_id: 11,
            new_name: "D".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    let b_stats = IndexStore::get_dir_stats_by_id(&conn, 11).unwrap().unwrap();
    assert!(
        b_stats.recursive_has_symlinks,
        "new parent should pick up the symlink-bearing subtree"
    );

    writer.shutdown();
}

#[test]
fn move_entry_v2_bumps_writer_generation() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    insert_dir_with_stats(
        &writer,
        &db_path,
        10,
        ROOT_ID,
        "Foo",
        DirStatsById {
            entry_id: 10,
            recursive_logical_size: 0,
            recursive_physical_size: 0,
            recursive_file_count: 0,
            recursive_dir_count: 0,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        },
    );

    let before = writer.mutation_count();
    writer
        .send(WriteMessage::MoveEntryV2 {
            entry_id: 10,
            new_parent_id: ROOT_ID,
            new_name: "Bar".into(),
        })
        .unwrap();
    writer.flush_blocking().unwrap();
    let after = writer.mutation_count();
    assert!(
        after > before,
        "writer's mutation counter should bump after a real move"
    );

    writer.shutdown();
}

// ── Bulk-reconcile delta-propagation suppression ─────────────────────

/// Large-delta regression guard (the test the original wedge needed):
///
/// The FULL reconcile sends thousands of `UpsertEntryV2` per pass. The bug was
/// that EACH one walked the ancestor `dir_stats` chain (O(entries × depth)),
/// wedging the writer for hours on a 270k→6M delta. `SetDeltaPropagation(false)`
/// suppresses that per-entry walk; the reconcile's single `ComputeAllAggregates`
/// recomputes every dir from the entries table instead.
///
/// This drives the writer with the SAME message stream a reconcile emits — bulk
/// mode ON, then thousands of `UpsertEntryV2`, then one `ComputeAllAggregates` —
/// and asserts BOTH halves of the contract on its OWN db (so it's immune to
/// other concurrent test writers, unlike a global counter would be):
///
/// 1. MID-WALK (after every upsert is flushed, BEFORE the aggregate) every dir's
///    `dir_stats` is still its zero-valued init row: the per-entry propagation did
///    NOT run. With propagation left ON this is where it FAILS — each dir would
///    already read its `FILES_PER_DIR` files (RED).
/// 2. POST-AGGREGATE the recomputed `dir_stats` are exactly correct, proving the
///    suppression is invisible to the final result (and that skipping the
///    aggregate would leave them wrong — the other RED).
#[test]
fn bulk_reconcile_suppresses_per_entry_propagation_until_final_aggregate() {
    const DIR_COUNT: i64 = 30;
    const FILES_PER_DIR: i64 = 100;
    const FILE_SIZE: u64 = 100;

    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, None).unwrap();

    // Enter bulk-reconcile mode: per-entry ancestor propagation is now OFF.
    writer.send(WriteMessage::SetDeltaPropagation(false)).unwrap();

    // Wave 1: create DIR_COUNT directories directly under ROOT. Their ids aren't
    // known yet (UpsertEntryV2 lets the writer allocate), so flush + resolve.
    for i in 0..DIR_COUNT {
        writer
            .send(WriteMessage::UpsertEntryV2 {
                parent_id: ROOT_ID,
                name: format!("dir{i}"),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
                nlink: None,
            })
            .unwrap();
    }
    writer.flush_blocking().unwrap();

    let dir_ids: Vec<i64> = {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        (0..DIR_COUNT)
            .map(|i| {
                IndexStore::resolve_component(&conn, ROOT_ID, &format!("dir{i}"))
                    .unwrap()
                    .expect("dir resolved")
            })
            .collect()
    };

    // Wave 2: FILES_PER_DIR files in each directory — the bulk of the delta.
    for &dir_id in &dir_ids {
        for f in 0..FILES_PER_DIR {
            writer
                .send(WriteMessage::UpsertEntryV2 {
                    parent_id: dir_id,
                    name: format!("f{f}.dat"),
                    is_directory: false,
                    is_symlink: false,
                    logical_size: Some(FILE_SIZE),
                    physical_size: Some(FILE_SIZE),
                    modified_at: None,
                    inode: None,
                    nlink: None,
                })
                .unwrap();
        }
    }
    writer.flush_blocking().unwrap();

    // 1. MID-WALK: propagation suppressed, so every dir still shows its
    //    zero-valued init row despite holding FILES_PER_DIR files. (RED here if
    //    propagation is left on: each dir would read FILES_PER_DIR.)
    {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        for &dir_id in &dir_ids {
            let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap().unwrap();
            assert_eq!(
                stats.recursive_file_count, 0,
                "bulk mode must NOT propagate the files into dir {dir_id}'s dir_stats"
            );
            assert_eq!(
                stats.recursive_logical_size, 0,
                "bulk mode must NOT propagate file sizes into dir {dir_id}'s dir_stats"
            );
        }
        // ROOT was never touched by propagation either.
        let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap();
        assert!(
            root.map(|s| s.recursive_file_count).unwrap_or(0) == 0,
            "bulk mode must NOT propagate anything into ROOT's dir_stats"
        );
    }

    // 2. The single final aggregate recomputes everything correctly.
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();
    writer.send(WriteMessage::SetDeltaPropagation(true)).unwrap();
    writer.flush_blocking().unwrap();

    {
        let conn = IndexStore::open_read_connection(&db_path).unwrap();
        for &dir_id in &dir_ids {
            let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap().unwrap();
            assert_eq!(
                stats.recursive_file_count, FILES_PER_DIR as u64,
                "aggregate must fill dir {dir_id}'s file count"
            );
            assert_eq!(
                stats.recursive_logical_size,
                FILE_SIZE * FILES_PER_DIR as u64,
                "aggregate must fill dir {dir_id}'s recursive size"
            );
        }
        let root = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID).unwrap().unwrap();
        assert_eq!(
            root.recursive_file_count,
            (DIR_COUNT * FILES_PER_DIR) as u64,
            "ROOT must total every file across every dir"
        );
        assert_eq!(
            root.recursive_dir_count, DIR_COUNT as u64,
            "ROOT must count every directory"
        );
        assert_eq!(
            root.recursive_logical_size,
            FILE_SIZE * (DIR_COUNT * FILES_PER_DIR) as u64,
            "ROOT must total every file's size"
        );
    }

    writer.shutdown();
}

// ── Entry-ID counter self-healing ────────────────────────────────

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

/// The shared `next_id` counter can fall behind the table's real `MAX(id)`
/// (a second process writing the same DB, a restart racing an in-flight batch).
/// The allocated id then hits `SQLITE_CONSTRAINT_PRIMARYKEY` and the live upsert
/// used to drop the file from the index silently, for every following insert too
/// (one incident: ~9,600 warnings in seconds). The handler must resync from the
/// DB and retry, so the entry lands.
#[test]
fn upsert_heals_an_id_counter_that_drifted_behind_the_table() {
    let (db_path, _dir) = setup_db();
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    IndexStore::insert_entries_v2_batch(&conn, &[seed_row(40, "a.txt"), seed_row(41, "b.txt")]).unwrap();

    // The counter still points at 40: ids 40 and 41 are already taken.
    let next_id = AtomicI64::new(40);
    let mutation_tracker = MutationTracker::new(true);
    let signal = IndexFailureSignal::new();

    handle_upsert_entry_v2(
        &conn,
        ROOT_ID,
        "fresh.txt".into(),
        false,
        false,
        Some(7),
        Some(7),
        None,
        None,
        None,
        &next_id,
        &mutation_tracker,
        true,
        &DeferredRepairs::new(),
        &signal,
    );

    let landed = IndexStore::resolve_component(&conn, ROOT_ID, "fresh.txt")
        .unwrap()
        .expect("a new file must land in the index even when the ID counter drifted behind the table");
    assert!(
        landed > 41,
        "the retry must use a fresh id past the table's MAX, got {landed}"
    );
    assert!(
        next_id.load(Ordering::Relaxed) > landed,
        "the counter must end up past the id it just used, so the next insert doesn't collide again"
    );
    assert_eq!(
        IndexStore::get_entry_by_id(&conn, landed).unwrap().unwrap().name,
        "fresh.txt"
    );
}

/// A `(parent_id, name_folded)` UNIQUE conflict (2067) is a genuinely different
/// situation from an id collision (1555): the name is already in the table.
/// Retrying it with a fresh id would insert a duplicate row, so it must NOT heal.
/// Reachable when another writer inserts the name between `resolve_component`
/// and the insert, which is why this drives `upsert_insert_new` directly.
#[test]
fn upsert_does_not_reassign_an_id_on_a_name_conflict() {
    let (db_path, _dir) = setup_db();
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    IndexStore::insert_entries_v2_batch(&conn, &[seed_row(5, "dup.txt")]).unwrap();

    let next_id = AtomicI64::new(6);
    let signal = IndexFailureSignal::new();

    upsert_insert_new(
        &conn,
        ROOT_ID,
        "dup.txt",
        false,
        false,
        Some(9),
        Some(9),
        None,
        None,
        false,
        &next_id,
        true,
        &DeferredRepairs::new(),
        &signal,
    );

    let children = IndexStore::list_children_on(ROOT_ID, &conn).unwrap();
    assert_eq!(
        children.len(),
        1,
        "a name conflict must not produce a second row under a fresh id"
    );
    assert_eq!(children[0].id, 5, "the original row must be the one that stays");
}
