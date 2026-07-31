//! `dir_stats` round-trips and the honest-sizes epoch model built on them:
//! `listed_epoch` stamps, the current epoch, the ledger-heal marker, and the
//! 0-absorbing `min_subtree_epoch` recomputation.

use super::*;

/// `min_subtree_epoch` survives a `dir_stats` write + read round-trip
/// (single and batch paths), and defaults to 0 for an un-set row.
#[test]
fn dir_stats_min_subtree_epoch_round_trips() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let a = insert_entry(&conn, ROOT_ID, "a", true, None);
    let b = insert_entry(&conn, ROOT_ID, "b", true, None);

    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[
            DirStatsById {
                entry_id: a,
                recursive_logical_size: 100,
                min_subtree_epoch: 7,
                ..Default::default()
            },
            DirStatsById {
                entry_id: b,
                recursive_logical_size: 0,
                min_subtree_epoch: 0,
                ..Default::default()
            },
        ],
    )
    .unwrap();

    let single = IndexStore::get_dir_stats_by_id(&conn, a).unwrap().unwrap();
    assert_eq!(single.min_subtree_epoch, 7);

    let batch = IndexStore::get_dir_stats_batch_by_ids(&conn, &[a, b]).unwrap();
    assert_eq!(batch[0].as_ref().unwrap().min_subtree_epoch, 7);
    assert_eq!(batch[1].as_ref().unwrap().min_subtree_epoch, 0);
}

/// A fresh entry defaults to `listed_epoch = 0`; `mark_dirs_listed` stamps the
/// given ids and leaves unlisted ones at 0.
#[test]
fn mark_dirs_listed_stamps_only_given_ids() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let a = insert_entry(&conn, ROOT_ID, "a", true, None);
    let b = insert_entry(&conn, ROOT_ID, "b", true, None);

    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(),
        Some(0),
        "default is 0"
    );

    IndexStore::mark_dirs_listed(&conn, &[a], 3).unwrap();
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(),
        Some(3),
        "a stamped"
    );
    assert_eq!(
        IndexStore::get_listed_epoch_by_id(&conn, b).unwrap(),
        Some(0),
        "b untouched"
    );

    // Empty id list is a no-op.
    IndexStore::mark_dirs_listed(&conn, &[], 9).unwrap();
    assert_eq!(IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(), Some(3));
}

/// `current_epoch` helpers: absent reads as 1, seed makes it 1, bump increments.
#[test]
fn current_epoch_helpers() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // Absent ⇒ treated as 1 (all current, not all stale).
    assert_eq!(IndexStore::get_meta(&conn, CURRENT_EPOCH_KEY).unwrap(), None);
    assert_eq!(IndexStore::read_current_epoch(&conn).unwrap(), 1);

    // Seeding writes "1" and is idempotent.
    assert_eq!(IndexStore::seed_current_epoch(&conn).unwrap(), 1);
    assert_eq!(
        IndexStore::get_meta(&conn, CURRENT_EPOCH_KEY).unwrap().as_deref(),
        Some("1")
    );
    assert_eq!(
        IndexStore::seed_current_epoch(&conn).unwrap(),
        1,
        "seed leaves existing value"
    );

    // Bump increments and persists.
    assert_eq!(IndexStore::bump_current_epoch(&conn).unwrap(), 2);
    assert_eq!(IndexStore::read_current_epoch(&conn).unwrap(), 2);
}

/// The ledger-heal marker: absent on a fresh DB, present after `mark`.
#[test]
fn ledger_heal_marker_round_trip() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // A fresh DB has never healed.
    assert!(!IndexStore::ledger_heal_done(&conn).unwrap());

    // Marking it done makes the check report present, and it's idempotent.
    IndexStore::mark_ledger_heal_done(&conn).unwrap();
    assert!(IndexStore::ledger_heal_done(&conn).unwrap());
    IndexStore::mark_ledger_heal_done(&conn).unwrap();
    assert!(IndexStore::ledger_heal_done(&conn).unwrap());
}

/// `recompute_min_subtree_epoch`: the 0-absorbing min over the dir's own
/// `listed_epoch` and every child dir's stored `min_subtree_epoch`.
#[test]
fn recompute_min_subtree_epoch_cases() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // An unlisted dir (listed_epoch = 0) is always 0, regardless of children.
    let unlisted = insert_entry(&conn, ROOT_ID, "unlisted", true, None);
    assert_eq!(IndexStore::recompute_min_subtree_epoch(&conn, unlisted).unwrap(), 0);

    // A listed dir with NO child dirs is covered at its own epoch.
    let leaf = insert_entry(&conn, ROOT_ID, "leaf", true, None);
    IndexStore::mark_dirs_listed(&conn, &[leaf], 5).unwrap();
    assert_eq!(
        IndexStore::recompute_min_subtree_epoch(&conn, leaf).unwrap(),
        5,
        "listed-childless ⇒ own epoch"
    );

    // A listed parent with one complete child (epoch 4) and one incomplete
    // child (epoch 0) ⇒ 0 (the 0 absorbs).
    let parent = insert_entry(&conn, ROOT_ID, "parent", true, None);
    IndexStore::mark_dirs_listed(&conn, &[parent], 9).unwrap();
    let complete = insert_entry(&conn, parent, "complete", true, None);
    let incomplete = insert_entry(&conn, parent, "incomplete", true, None);
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[
            DirStatsById {
                entry_id: complete,
                min_subtree_epoch: 4,
                ..Default::default()
            },
            DirStatsById {
                entry_id: incomplete,
                min_subtree_epoch: 0,
                ..Default::default()
            },
        ],
    )
    .unwrap();
    assert_eq!(
        IndexStore::recompute_min_subtree_epoch(&conn, parent).unwrap(),
        0,
        "an incomplete child absorbs to 0"
    );

    // With both children complete (4 and 6), the parent is the weakest link
    // across self (9) and children ⇒ 4.
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: incomplete,
            min_subtree_epoch: 6,
            ..Default::default()
        }],
    )
    .unwrap();
    assert_eq!(
        IndexStore::recompute_min_subtree_epoch(&conn, parent).unwrap(),
        4,
        "weakest link = min(own=9, 4, 6) = 4"
    );
}

#[test]
fn dir_stats_roundtrip() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users_id = insert_entry(&conn, ROOT_ID, "Users", true, None);
    let test_id = insert_entry(&conn, users_id, "test", true, None);

    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: test_id,
            recursive_logical_size: 50_000,
            recursive_physical_size: 50_000,
            recursive_file_count: 42,
            recursive_dir_count: 5,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        }],
    )
    .unwrap();

    let result = IndexStore::get_dir_stats_by_id(&conn, test_id).unwrap().unwrap();
    assert_eq!(result.recursive_logical_size, 50_000);
    assert_eq!(result.recursive_file_count, 42);
    assert_eq!(result.recursive_dir_count, 5);
}

#[test]
fn dir_stats_batch_lookup() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let a_id = insert_entry(&conn, ROOT_ID, "a", true, None);
    let b_id = insert_entry(&conn, ROOT_ID, "b", true, None);

    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[
            DirStatsById {
                entry_id: a_id,
                recursive_logical_size: 100,
                recursive_physical_size: 100,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
            DirStatsById {
                entry_id: b_id,
                recursive_logical_size: 200,
                recursive_physical_size: 200,
                recursive_file_count: 2,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
        ],
    )
    .unwrap();

    let result = IndexStore::get_dir_stats_batch_by_ids(&conn, &[a_id, 99999, b_id]).unwrap();
    assert_eq!(result.len(), 3);
    assert!(result[0].is_some());
    assert!(result[1].is_none());
    assert!(result[2].is_some());
    assert_eq!(result[0].as_ref().unwrap().recursive_logical_size, 100);
    assert_eq!(result[2].as_ref().unwrap().recursive_logical_size, 200);
}

#[test]
fn dir_stats_by_id_roundtrip() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let dir_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "mydir", true, false, None, None, None, None).unwrap();
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: dir_id,
            recursive_logical_size: 12345,
            recursive_physical_size: 12345,
            recursive_file_count: 10,
            recursive_dir_count: 3,
            recursive_has_symlinks: false,
            min_subtree_epoch: 0,
        }],
    )
    .unwrap();

    let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap().unwrap();
    assert_eq!(stats.recursive_logical_size, 12345);
    assert_eq!(stats.recursive_file_count, 10);
    assert_eq!(stats.recursive_dir_count, 3);
}

#[test]
fn dir_stats_batch_by_ids() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let d1 = IndexStore::insert_entry_v2(&conn, ROOT_ID, "d1", true, false, None, None, None, None).unwrap();
    let d2 = IndexStore::insert_entry_v2(&conn, ROOT_ID, "d2", true, false, None, None, None, None).unwrap();

    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[
            DirStatsById {
                entry_id: d1,
                recursive_logical_size: 100,
                recursive_physical_size: 100,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
            DirStatsById {
                entry_id: d2,
                recursive_logical_size: 200,
                recursive_physical_size: 200,
                recursive_file_count: 2,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
        ],
    )
    .unwrap();

    let result = IndexStore::get_dir_stats_batch_by_ids(&conn, &[d1, 99999, d2]).unwrap();
    assert_eq!(result.len(), 3);
    assert!(result[0].is_some());
    assert!(result[1].is_none());
    assert!(result[2].is_some());
    assert_eq!(result[0].as_ref().unwrap().recursive_logical_size, 100);
    assert_eq!(result[2].as_ref().unwrap().recursive_logical_size, 200);
}
