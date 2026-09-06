//! Insert and upsert basics: `insert_entries_v2` through the writer, the
//! accumulator that only counts rows that landed, and `upsert_entry_v2`
//! insert/update plus its `dir_stats` init for new dirs.

use super::*;

#[test]
fn insert_entries_v2_via_writer() {
    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

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

    let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());
    handle_insert_entries_v2(
        &conn,
        entries_dup,
        &mut accumulator,
        &crate::NoopEventSink,
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
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

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
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).unwrap();

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
