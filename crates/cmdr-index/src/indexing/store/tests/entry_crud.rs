//! Entry-tree CRUD: inserting (single and batch), listing, updating, renaming,
//! moving, the `(parent_id, name_folded)` uniqueness contract, and the
//! inode lookup that backs hardlink dedup.

use super::*;
use rusqlite::params;

#[test]
fn root_sentinel_exists() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let root = IndexStore::get_entry_by_id(&write_conn, ROOT_ID).unwrap();
    assert!(root.is_some());
    let root = root.unwrap();
    assert_eq!(root.id, ROOT_ID);
    assert_eq!(root.parent_id, ROOT_PARENT_ID);
    assert_eq!(root.name, "");
    assert!(root.is_directory);
}

#[test]
fn insert_and_list_entries() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    let users_id = insert_entry(&write_conn, ROOT_ID, "Users", true, None);
    let test_id = insert_entry(&write_conn, users_id, "test", true, None);
    insert_entry(&write_conn, test_id, "a.txt", false, Some(1024));
    insert_entry(&write_conn, test_id, "docs", true, None);

    let result = store.list_children(test_id).unwrap();
    assert_eq!(result.len(), 2);

    let file = result.iter().find(|e| e.name == "a.txt").unwrap();
    assert!(!file.is_directory);
    assert_eq!(file.logical_size, Some(1024));

    let dir = result.iter().find(|e| e.name == "docs").unwrap();
    assert!(dir.is_directory);
}

#[test]
fn children_stats() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let p_id = insert_entry(&conn, ROOT_ID, "p", true, None);
    insert_entry(&conn, p_id, "f1.txt", false, Some(100));
    insert_entry(&conn, p_id, "f2.txt", false, Some(200));
    insert_entry(&conn, p_id, "sub", true, None);

    let (logical_size, physical_size, file_count, dir_count) =
        IndexStore::get_children_stats_by_id(&conn, p_id).unwrap();
    assert_eq!(logical_size, 300);
    assert_eq!(physical_size, 300);
    assert_eq!(file_count, 2);
    assert_eq!(dir_count, 1);
}

#[test]
fn get_all_directory_paths() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let a_id = insert_entry(&conn, ROOT_ID, "a", true, None);
    insert_entry(&conn, ROOT_ID, "b", true, None);
    insert_entry(&conn, a_id, "file.txt", false, Some(100));

    let dirs = IndexStore::get_all_directory_paths(&conn).unwrap();
    assert_eq!(dirs.len(), 2);
    assert!(dirs.contains(&"/a".to_string()));
    assert!(dirs.contains(&"/b".to_string()));
}

#[test]
fn empty_batch_operations_are_noops() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    IndexStore::insert_entries_v2_batch(&conn, &[]).unwrap();
    IndexStore::upsert_dir_stats_by_id(&conn, &[]).unwrap();
}

#[test]
fn get_entry_by_id_found() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let test_id = insert_entry(&conn, ROOT_ID, "test", true, None);
    let file_id = IndexStore::insert_entry_v2(
        &conn,
        test_id,
        "file.txt",
        false,
        false,
        Some(512),
        Some(512),
        Some(1700000000),
        None,
    )
    .unwrap();

    let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap();
    assert!(result.is_some());
    let found = result.unwrap();
    assert_eq!(found.name, "file.txt");
    assert_eq!(found.logical_size, Some(512));
    assert!(!found.is_directory);
}

#[test]
fn get_entry_by_id_not_found() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let result = IndexStore::get_entry_by_id(&conn, 99999).unwrap();
    assert!(result.is_none());
}

#[test]
fn update_entry_modifies_in_place() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let test_id = insert_entry(&conn, ROOT_ID, "test", true, None);
    let file_id = IndexStore::insert_entry_v2(
        &conn,
        test_id,
        "file.txt",
        false,
        false,
        Some(100),
        Some(100),
        Some(1000),
        None,
    )
    .unwrap();

    let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
    assert_eq!(result.logical_size, Some(100));

    // Update with new size
    IndexStore::update_entry(&conn, file_id, false, false, Some(200), Some(200), Some(2000), None).unwrap();

    let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
    assert_eq!(result.logical_size, Some(200));
    assert_eq!(result.modified_at, Some(2000));
}

#[test]
fn insert_entry_v2_and_get_by_id() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let id = IndexStore::insert_entry_v2(
        &conn,
        ROOT_ID,
        "myfile.txt",
        false,
        false,
        Some(4096),
        Some(4096),
        Some(999),
        None,
    )
    .unwrap();
    assert!(id > ROOT_ID);

    let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
    assert_eq!(entry.name, "myfile.txt");
    assert_eq!(entry.parent_id, ROOT_ID);
    assert!(!entry.is_directory);
    assert_eq!(entry.logical_size, Some(4096));
    assert_eq!(entry.modified_at, Some(999));
}

#[test]
fn list_children_v2() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    let dir_id =
        IndexStore::insert_entry_v2(&write_conn, ROOT_ID, "mydir", true, false, None, None, None, None).unwrap();
    IndexStore::insert_entry_v2(
        &write_conn,
        dir_id,
        "a.txt",
        false,
        false,
        Some(100),
        Some(100),
        None,
        None,
    )
    .unwrap();
    IndexStore::insert_entry_v2(
        &write_conn,
        dir_id,
        "b.txt",
        false,
        false,
        Some(200),
        Some(200),
        None,
        None,
    )
    .unwrap();

    let children = store.list_children(dir_id).unwrap();
    assert_eq!(children.len(), 2);
}

#[test]
fn update_entry_v2() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let id = IndexStore::insert_entry_v2(
        &conn,
        ROOT_ID,
        "file.txt",
        false,
        false,
        Some(100),
        Some(100),
        Some(1000),
        None,
    )
    .unwrap();

    IndexStore::update_entry(&conn, id, false, false, Some(999), Some(999), Some(2000), None).unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
    assert_eq!(entry.logical_size, Some(999));
    assert_eq!(entry.modified_at, Some(2000));
}

#[test]
fn rename_and_move_entry() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let dir_a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "dir_a", true, false, None, None, None, None).unwrap();
    let dir_b = IndexStore::insert_entry_v2(&conn, ROOT_ID, "dir_b", true, false, None, None, None, None).unwrap();
    let file_id =
        IndexStore::insert_entry_v2(&conn, dir_a, "old.txt", false, false, Some(50), Some(50), None, None).unwrap();

    // Rename
    IndexStore::rename_entry(&conn, file_id, "new.txt").unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
    assert_eq!(entry.name, "new.txt");

    // Move to dir_b
    IndexStore::move_entry(&conn, file_id, dir_b).unwrap();
    let entry = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
    assert_eq!(entry.parent_id, dir_b);
}

#[test]
fn delete_entry_by_id_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let id = IndexStore::insert_entry_v2(
        &conn,
        ROOT_ID,
        "file.txt",
        false,
        false,
        Some(100),
        Some(100),
        None,
        None,
    )
    .unwrap();
    assert!(IndexStore::get_entry_by_id(&conn, id).unwrap().is_some());

    IndexStore::delete_entry_by_id(&conn, id).unwrap();
    assert!(IndexStore::get_entry_by_id(&conn, id).unwrap().is_none());
}

#[test]
fn subtree_totals_by_id() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None, None, None).unwrap();
    IndexStore::insert_entry_v2(&conn, a, "f1.txt", false, false, Some(100), Some(100), None, None).unwrap();
    IndexStore::insert_entry_v2(&conn, a, "f2.txt", false, false, Some(200), Some(200), None, None).unwrap();
    let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None, None, None).unwrap();
    IndexStore::insert_entry_v2(&conn, b, "f3.txt", false, false, Some(300), Some(300), None, None).unwrap();

    let (logical_size, physical_size, file_count, dir_count) = IndexStore::get_subtree_totals_by_id(&conn, a).unwrap();
    assert_eq!(logical_size, 600);
    assert_eq!(physical_size, 600);
    assert_eq!(file_count, 3);
    assert_eq!(dir_count, 2); // a + b
}

#[test]
fn get_next_id() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Root sentinel is id=1, so next should be 2
    let next = IndexStore::get_next_id(&conn).unwrap();
    assert_eq!(next, 2);

    IndexStore::insert_entry_v2(&conn, ROOT_ID, "file.txt", false, false, None, None, None, None).unwrap();
    let next = IndexStore::get_next_id(&conn).unwrap();
    assert!(next >= 3);
}

#[test]
fn insert_entries_v2_batch_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let entries = vec![
        EntryRow {
            id: 100,
            parent_id: ROOT_ID,
            name: "dir1".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 101,
            parent_id: 100,
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(42),
            physical_size: Some(42),
            modified_at: Some(1234),
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).unwrap();

    let entry = IndexStore::get_entry_by_id(&conn, 100).unwrap().unwrap();
    assert_eq!(entry.name, "dir1");
    assert!(entry.is_directory);

    let entry = IndexStore::get_entry_by_id(&conn, 101).unwrap().unwrap();
    assert_eq!(entry.name, "file.txt");
    assert_eq!(entry.logical_size, Some(42));
}

/// An SMB file id routinely has its high bit set, and the whole batch used to
/// die on it.
///
/// `ATTR_CMN_FILEID` on a mounted smbfs share is the server's 64-bit file id (or
/// a path hash when the server has no usable one), so values above `i64::MAX`
/// are ordinary: 43% of the files in one directory on a QNAP measured that way.
/// SQLite's `INTEGER` holds every bit of one, but binding a bare `u64` asks
/// rusqlite to prove it's positive, and the `TryFromIntError` it raises aborts
/// the savepoint — losing all ~2000 rows of the batch, including the ones whose
/// inodes were fine. A user's 609-entry directory indexed as empty that way
/// (`ERR-AYVM4`, 2026-08-27) and stayed empty, because the scan marks the
/// directory listed regardless.
///
/// The inode is an identity, so it round-trips as a bit-cast. ❌ Not a
/// saturating clamp: that collapses every high-bit inode onto one value, and
/// `find_entry_by_inode` would then match unrelated files into each other
/// during rename detection.
#[test]
fn a_high_bit_inode_round_trips_and_keeps_the_rest_of_the_batch() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // A real value sampled from a mounted QNAP share.
    let smb_inode: u64 = 16_927_209_734_986_940_580;
    assert!(i64::try_from(smb_inode).is_err(), "the fixture has to exercise the case");

    let entries = vec![
        EntryRow {
            id: 200,
            parent_id: ROOT_ID,
            name: "from-the-nas.jpg".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1024),
            physical_size: Some(4096),
            modified_at: Some(1234),
            inode: Some(smb_inode),
        },
        EntryRow {
            id: 201,
            parent_id: ROOT_ID,
            name: "ordinary.jpg".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(2048),
            physical_size: Some(4096),
            modified_at: Some(1234),
            inode: Some(42),
        },
    ];
    let landed = IndexStore::insert_entries_v2_batch(&conn, &entries).expect("the batch lands");
    assert_eq!(landed, vec![true, true], "both rows land, high-bit inode and all");

    let entry = IndexStore::get_entry_by_id(&conn, 200).unwrap().unwrap();
    assert_eq!(
        entry.inode,
        Some(smb_inode),
        "the inode reads back bit for bit, or rename detection matches the wrong file"
    );

    // And the index seek still finds it, which is the only reason to store it.
    assert_eq!(
        IndexStore::find_entry_by_inode(&conn, smb_inode).unwrap(),
        Some(200),
        "the `idx_inode` lookup has to agree with what the insert wrote"
    );
    assert_eq!(IndexStore::find_entry_by_inode(&conn, 42).unwrap(), Some(201));
}

/// A size is a magnitude, not an identity, so an absurd one saturates instead of
/// taking the batch down with it. `physical_size` is the reachable case: it
/// comes from `st_blocks * 512`, which wraps in release on a bogus block count.
#[test]
fn an_absurd_size_saturates_rather_than_losing_the_batch() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let entries = vec![EntryRow {
        id: 300,
        parent_id: ROOT_ID,
        name: "impossible.bin".into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(u64::MAX),
        physical_size: Some(u64::MAX),
        modified_at: Some(u64::MAX),
        inode: None,
    }];
    let landed = IndexStore::insert_entries_v2_batch(&conn, &entries).expect("the batch lands");
    assert_eq!(landed, vec![true]);

    let entry = IndexStore::get_entry_by_id(&conn, 300).unwrap().unwrap();
    assert_eq!(entry.logical_size, Some(i64::MAX as u64), "clamped, not lost");
    assert_eq!(entry.physical_size, Some(i64::MAX as u64));
    assert_eq!(entry.modified_at, Some(i64::MAX as u64));
}

// Duplicate (parent_id, name_folded) must be rejected by the schema.
// The aggregator walks parent_id chains and sums every row; a duplicate would
// double-count its size into ancestor dir_stats. Schema v12 reinstated the
// UNIQUE constraint that v5 dropped for collation-cost reasons (since v6,
// `name_folded` carries pre-folded bytes, so binary collation is fine).
#[test]
fn duplicate_parent_name_folded_rejected_individual_insert() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    IndexStore::insert_entry_v2(&conn, ROOT_ID, "dup.txt", false, false, Some(10), Some(10), None, None).unwrap();
    let second = IndexStore::insert_entry_v2(&conn, ROOT_ID, "dup.txt", false, false, Some(10), Some(10), None, None);
    assert!(
        second.is_err(),
        "second insert with same (parent_id, name_folded) must fail; got {second:?}"
    );
}

/// Batch insert uses `INSERT OR IGNORE`: a duplicate `(parent_id, name_folded)`
/// in the batch (or against an existing row) skips just that row, keeping
/// every other entry in the batch. The returned `Vec<bool>` flags which
/// rows actually landed. This replaces the previous roll-back-the-whole-batch
/// behavior, which silently dropped ~2000 unrelated entries every time a
/// scan encountered two siblings with colliding `name_folded` (case-sensitive
/// volumes, NFC/NFD duplicates, etc.).
#[test]
fn duplicate_parent_name_folded_skipped_in_batch_insert() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let entries = vec![
        EntryRow {
            id: 100,
            parent_id: ROOT_ID,
            name: "dup.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(10),
            physical_size: Some(10),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 101,
            parent_id: ROOT_ID,
            name: "dup.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(20),
            physical_size: Some(20),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 102,
            parent_id: ROOT_ID,
            name: "unrelated.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(30),
            physical_size: Some(30),
            modified_at: None,
            inode: None,
        },
    ];
    let inserted = IndexStore::insert_entries_v2_batch(&conn, &entries).unwrap();
    assert_eq!(inserted, vec![true, false, true]);

    // First duplicate wins; the second is dropped; the unrelated entry survives.
    // Without the per-row skip, the savepoint used to roll back ALL THREE.
    assert!(IndexStore::get_entry_by_id(&conn, 100).unwrap().is_some());
    assert!(IndexStore::get_entry_by_id(&conn, 101).unwrap().is_none());
    assert!(IndexStore::get_entry_by_id(&conn, 102).unwrap().is_some());
}

#[test]
fn name_folded_populated_on_single_insert() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let name = "MyFolder";
    let id = IndexStore::insert_entry_v2(&conn, ROOT_ID, name, true, false, None, None, None, None).unwrap();

    let folded: String = conn
        .query_row("SELECT name_folded FROM entries WHERE id = ?1", params![id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(folded, normalize_for_comparison(name));
}

#[test]
fn name_folded_populated_on_batch_insert() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let entries = vec![
        EntryRow {
            id: 200,
            parent_id: ROOT_ID,
            name: "Documents".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 201,
            parent_id: 200,
            name: "Café.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(10),
            physical_size: Some(10),
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).unwrap();

    for e in &entries {
        let folded: String = conn
            .query_row("SELECT name_folded FROM entries WHERE id = ?1", params![e.id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(folded, normalize_for_comparison(&e.name));
    }
}

#[test]
fn get_children_stats_by_id_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let dir_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "mydir", true, false, None, None, None, None).unwrap();
    IndexStore::insert_entry_v2(&conn, dir_id, "f1.txt", false, false, Some(100), Some(100), None, None).unwrap();
    IndexStore::insert_entry_v2(&conn, dir_id, "f2.txt", false, false, Some(200), Some(200), None, None).unwrap();
    IndexStore::insert_entry_v2(&conn, dir_id, "subdir", true, false, None, None, None, None).unwrap();

    let (logical_size, physical_size, files, dirs) = IndexStore::get_children_stats_by_id(&conn, dir_id).unwrap();
    assert_eq!(logical_size, 300);
    assert_eq!(physical_size, 300);
    assert_eq!(files, 2);
    assert_eq!(dirs, 1);
}

// ── has_sized_entry_for_inode tests ──────────────────────────────

/// Helper: insert an entry with explicit inode and size. Returns the new ID.
fn insert_entry_with_inode(
    conn: &Connection,
    parent_id: i64,
    name: &str,
    size: Option<u64>,
    inode: Option<u64>,
) -> i64 {
    IndexStore::insert_entry_v2(conn, parent_id, name, false, false, size, size, None, inode).unwrap()
}

#[test]
fn has_sized_entry_for_inode_returns_false_when_no_entry() {
    let (_store, dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

    let result = IndexStore::has_sized_entry_for_inode(&conn, 12345, None).unwrap();
    assert!(!result);
}

#[test]
fn has_sized_entry_for_inode_returns_true_when_sized_entry_exists() {
    let (_store, dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

    insert_entry_with_inode(&conn, ROOT_ID, "primary.txt", Some(1000), Some(100));

    assert!(IndexStore::has_sized_entry_for_inode(&conn, 100, None).unwrap());
}

#[test]
fn has_sized_entry_for_inode_returns_false_when_sizes_are_null() {
    let (_store, dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

    // Secondary link: same inode but NULL sizes (deduped)
    insert_entry_with_inode(&conn, ROOT_ID, "secondary.txt", None, Some(100));

    assert!(!IndexStore::has_sized_entry_for_inode(&conn, 100, None).unwrap());
}

#[test]
fn has_sized_entry_for_inode_exclude_id_skips_self() {
    let (_store, dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

    let id = insert_entry_with_inode(&conn, ROOT_ID, "only.txt", Some(1000), Some(100));

    // Excluding the only sized entry should return false
    assert!(!IndexStore::has_sized_entry_for_inode(&conn, 100, Some(id)).unwrap());
    // Without excluding, it should return true
    assert!(IndexStore::has_sized_entry_for_inode(&conn, 100, None).unwrap());
}

#[test]
fn has_sized_entry_for_inode_multiple_entries_one_has_sizes() {
    let (_store, dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

    let primary_id = insert_entry_with_inode(&conn, ROOT_ID, "primary.txt", Some(1000), Some(100));
    let secondary_id = insert_entry_with_inode(&conn, ROOT_ID, "secondary.txt", None, Some(100));

    // From secondary's perspective (exclude self): primary has sizes
    assert!(IndexStore::has_sized_entry_for_inode(&conn, 100, Some(secondary_id)).unwrap());
    // From primary's perspective (exclude self): secondary has no sizes
    assert!(!IndexStore::has_sized_entry_for_inode(&conn, 100, Some(primary_id)).unwrap());
}
