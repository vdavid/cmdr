//! Tests for `IndexStore` (schema, collation, entry/dir-stats/meta CRUD, path
//! resolution, and `platform_case` property tests). Extracted verbatim from the
//! former `store.rs` `mod tests`; pure code movement.

use super::*;

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

#[test]
fn schema_creation_and_version() {
    let (store, _dir) = open_temp_store();
    let status = store.get_index_status().unwrap();
    assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
}

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

/// A schema-version mismatch recreates the DB file; the rebuilt DB still has the
/// new v13 columns (a write/read round-trip through them succeeds).
#[test]
fn schema_bump_rebuild_has_new_columns() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bump.db");

    // Open, then stamp a stale version to force a drop+rebuild on reopen.
    {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        IndexStore::update_meta(&conn, "schema_version", "1").unwrap();
    }

    let store = IndexStore::open(&db_path).unwrap();
    assert_eq!(
        store.get_index_status().unwrap().schema_version.as_deref(),
        Some(SCHEMA_VERSION)
    );

    // The new columns exist and round-trip on the rebuilt schema.
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let a = insert_entry(&conn, ROOT_ID, "a", true, None);
    IndexStore::mark_dirs_listed(&conn, &[a], 5).unwrap();
    assert_eq!(IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(), Some(5));
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[DirStatsById {
            entry_id: a,
            min_subtree_epoch: 5,
            ..Default::default()
        }],
    )
    .unwrap();
    assert_eq!(
        IndexStore::get_dir_stats_by_id(&conn, a)
            .unwrap()
            .unwrap()
            .min_subtree_epoch,
        5
    );
}

/// `apply_pragmas` must set a non-zero `busy_timeout` on both read and
/// write connections. Without it, concurrent connections fail with
/// `SQLITE_BUSY` on the first lock contention instead of waiting.
#[test]
fn apply_pragmas_sets_busy_timeout_on_both_modes() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
    let write_timeout: i64 = write_conn
        .pragma_query_value(None, "busy_timeout", |r| r.get(0))
        .unwrap();
    assert!(
        write_timeout > 0,
        "write connection should have busy_timeout set, got {write_timeout}"
    );

    let read_conn = IndexStore::open_read_connection(store.db_path()).unwrap();
    let read_timeout: i64 = read_conn
        .pragma_query_value(None, "busy_timeout", |r| r.get(0))
        .unwrap();
    assert!(
        read_timeout > 0,
        "read connection should have busy_timeout set, got {read_timeout}"
    );
}

/// `open_read_connection` must succeed while another connection holds a
/// write transaction. The live and replay event loops rely on this to
/// open their path-resolution connection without racing the writer
/// thread. Regression: switching this call site to `open_write_connection`
/// (or removing the `busy_timeout` pragma) makes the open fail on every
/// concurrent commit, which silently kills the FSEvents receiver and
/// stops live index updates for the rest of the session.
#[test]
fn open_read_connection_succeeds_under_write_lock() {
    let (store, _dir) = open_temp_store();
    let db_path = store.db_path().to_path_buf();
    let writer = IndexStore::open_write_connection(&db_path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();

    // The read connection should open and be usable while the writer's
    // transaction is still in flight.
    let read = IndexStore::open_read_connection(&db_path).expect("read connection should open under write lock");
    let root = IndexStore::get_entry_by_id(&read, ROOT_ID).unwrap();
    assert!(root.is_some(), "read connection should see committed root sentinel");

    // Release the writer's lock so the tempdir can clean up cleanly.
    writer.execute_batch("ROLLBACK").unwrap();
}

/// `persisted_scan_completed` is the on-connect auto-resume gate: it reports
/// `true` ONLY for a DB that recorded a completed scan (the "the user enabled
/// indexing for this volume and it finished at least once" signal). A missing
/// file, a fresh DB with no completed scan, and an unreadable path all read
/// `false`, so a never-enabled SMB share is never auto-indexed on connect.
#[test]
fn persisted_scan_completed_reflects_the_marker() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-test.db");

    // No file yet ⇒ never enabled.
    assert!(
        !IndexStore::persisted_scan_completed(&db_path),
        "a missing DB must read as not-yet-enabled"
    );

    // A fresh DB with no completed scan ⇒ still not the resume signal (the user
    // may have started an enable that never finished; don't auto-resume it).
    let store = IndexStore::open(&db_path).expect("open store");
    drop(store);
    assert!(
        !IndexStore::persisted_scan_completed(&db_path),
        "a DB with no scan_completed_at must read as not-enabled"
    );

    // Stamp a completed scan ⇒ the resume signal.
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan_completed_at");
    drop(conn);
    assert!(
        IndexStore::persisted_scan_completed(&db_path),
        "a completed scan must read as enabled (auto-resume on connect)"
    );
}

/// The sticky `user_disabled` marker round-trips and, combined with a completed
/// scan, gates auto-resume: set ⇒ suppress resume even with a completed scan;
/// cleared ⇒ resume again. This is what makes "turn off indexing for this drive"
/// survive a reconnect (the DB stays on disk for a fast re-enable, but the marker
/// records intent).
#[test]
fn user_disabled_marker_gates_auto_resume() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index-smb-test.db");

    // Absent DB / marker ⇒ not disabled.
    assert!(!IndexStore::user_disabled(&db_path), "no DB ⇒ not disabled");

    // A completed scan with no marker is eligible to auto-resume.
    let store = IndexStore::open(&db_path).expect("open store");
    drop(store);
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, "scan_completed_at", "1700000000").expect("stamp scan");
    drop(conn);
    assert!(IndexStore::persisted_scan_completed(&db_path));
    assert!(!IndexStore::user_disabled(&db_path), "fresh index isn't user-disabled");

    // Turn indexing off ⇒ marker set ⇒ no auto-resume (even though a scan completed).
    IndexStore::set_user_disabled(&db_path, true).expect("set marker");
    assert!(IndexStore::user_disabled(&db_path), "marker must persist");
    assert!(
        IndexStore::persisted_scan_completed(&db_path),
        "the completed-scan fact is untouched by the disable marker (DB preserved for fast resume)"
    );

    // Re-enable ⇒ marker cleared ⇒ eligible again.
    IndexStore::set_user_disabled(&db_path, false).expect("clear marker");
    assert!(!IndexStore::user_disabled(&db_path), "re-enable clears the marker");
}

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
fn meta_roundtrip() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    IndexStore::update_meta(&write_conn, "volume_path", "/").unwrap();
    IndexStore::update_meta(&write_conn, "scan_duration_ms", "1234").unwrap();

    let val = IndexStore::get_meta(&write_conn, "volume_path").unwrap();
    assert_eq!(val.as_deref(), Some("/"));

    let status = store.get_index_status().unwrap();
    assert_eq!(status.volume_path.as_deref(), Some("/"));
    assert_eq!(status.scan_duration_ms.as_deref(), Some("1234"));
}

/// `set_volume_path` heals an index DB that has no `volume_path` meta (the shape a
/// real SMB index has — only the local scan-completion path ever wrote it), so
/// search can strip the mount root off scope paths without a rescan.
#[test]
fn set_volume_path_heals_a_db_missing_it() {
    let (store, _dir) = open_temp_store();
    let db_path = store.db_path().to_path_buf();

    // A fresh DB has no volume_path meta.
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    assert_eq!(IndexStore::get_meta(&conn, "volume_path").unwrap(), None);
    drop(conn);

    IndexStore::set_volume_path(&db_path, "/Volumes/naspi").unwrap();

    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    assert_eq!(
        IndexStore::get_meta(&conn, "volume_path").unwrap().as_deref(),
        Some("/Volumes/naspi")
    );
}

#[test]
fn read_scan_calibration_reads_seeded_keys() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    IndexStore::update_meta(&write_conn, "total_entries", "5000000").unwrap();
    IndexStore::update_meta(&write_conn, "total_physical_bytes", "905000000000").unwrap();
    IndexStore::update_meta(&write_conn, "scan_duration_ms", "149000").unwrap();

    let calibration = IndexStore::read_scan_calibration(&write_conn).unwrap();
    assert_eq!(calibration.total_entries, Some(5_000_000));
    assert_eq!(calibration.total_physical_bytes, Some(905_000_000_000));
    assert_eq!(calibration.scan_duration_ms, Some(149_000));
}

#[test]
fn read_scan_calibration_missing_keys_are_none() {
    let (store, _dir) = open_temp_store();
    let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // Fresh DB: none of the calibration keys exist yet.
    let calibration = IndexStore::read_scan_calibration(&conn).unwrap();
    assert_eq!(calibration, ScanCalibration::default());
    assert_eq!(calibration.total_entries, None);
    assert_eq!(calibration.total_physical_bytes, None);
    assert_eq!(calibration.scan_duration_ms, None);

    // A non-numeric value also maps to None (parse failure), not an error.
    IndexStore::update_meta(&conn, "total_entries", "not-a-number").unwrap();
    let calibration = IndexStore::read_scan_calibration(&conn).unwrap();
    assert_eq!(calibration.total_entries, None);
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
fn delete_entry_and_subtree() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    // Build tree: /a, /a/b.txt, /a/c, /a/c/d.txt
    let a_id = insert_entry(&write_conn, ROOT_ID, "a", true, None);
    let b_id = insert_entry(&write_conn, a_id, "b.txt", false, Some(10));
    let c_id = insert_entry(&write_conn, a_id, "c", true, None);
    insert_entry(&write_conn, c_id, "d.txt", false, Some(20));

    // Delete single entry
    IndexStore::delete_entry_by_id(&write_conn, b_id).unwrap();
    let children = store.list_children(a_id).unwrap();
    assert_eq!(children.len(), 1); // only c remains

    // Delete subtree
    IndexStore::delete_subtree_by_id(&write_conn, a_id).unwrap();
    let children = store.list_children(a_id).unwrap();
    assert!(children.is_empty());
    let root_children = store.list_children(ROOT_ID).unwrap();
    assert!(root_children.is_empty()); // /a itself is also gone
}

#[test]
fn clear_all_resets_schema() {
    let (store, _dir) = open_temp_store();
    let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

    insert_entry(&write_conn, ROOT_ID, "x", false, Some(1));

    IndexStore::clear_all(&write_conn).unwrap();

    // Schema version should be re-stamped
    let version = IndexStore::get_meta(&write_conn, "schema_version").unwrap();
    assert_eq!(version.as_deref(), Some(SCHEMA_VERSION));

    // Entries should be gone (except root sentinel)
    let children = store.list_children(ROOT_ID).unwrap();
    assert!(children.is_empty());
}

#[test]
fn schema_mismatch_recreates_to_current_version() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("mismatch.db");

    // Create a store and tamper with the version
    {
        let store = IndexStore::open(&db_path).unwrap();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        IndexStore::update_meta(&write_conn, "schema_version", "0").unwrap();
    }

    // Re-open: should detect the mismatch and recreate the file at the current version
    let store = IndexStore::open(&db_path).unwrap();
    let status = store.get_index_status().unwrap();
    assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
}

/// A schema-version mismatch recreates the DB as a fresh, zero-freelist FILE
/// (delete + recreate), rather than DROP-ing tables on the live file (which
/// leaves the freed pages stranded on the freelist). The reclaim is the whole
/// point, so this asserts `freelist_count == 0` after reopen.
#[test]
fn schema_mismatch_recreates_file_reclaiming_freelist() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("bloat.db");

    {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        // Bloat the file so DROP-ing the tables strands many pages on the
        // freelist (auto_vacuum = INCREMENTAL never returns them on its own).
        conn.execute_batch("BEGIN").unwrap();
        for i in 0..5000 {
            insert_entry(&conn, ROOT_ID, &format!("entry-{i}"), false, Some(i));
        }
        conn.execute_batch("COMMIT").unwrap();

        // Stamp an OLD schema version, then DROP entries + dir_stats but KEEP
        // `meta` intact. If we dropped `meta`, the reopen would read version
        // `None`, treat the DB as fresh, and never recreate -> false pass.
        IndexStore::update_meta(&conn, "schema_version", "1").unwrap();
        conn.execute_batch("DROP TABLE entries; DROP TABLE dir_stats;").unwrap();

        let (_pages, freelist) = IndexStore::db_page_stats(&conn).unwrap();
        assert!(freelist > 0, "expected a non-zero freelist after DROP, got {freelist}");
    }

    // Reopen: the schema mismatch must recreate the file fresh, not DROP on it.
    let store = IndexStore::open(&db_path).unwrap();

    assert_eq!(
        store.get_index_status().unwrap().schema_version.as_deref(),
        Some(SCHEMA_VERSION),
        "schema version should be re-stamped to current"
    );
    assert!(
        store.list_children(ROOT_ID).unwrap().is_empty(),
        "recreated DB should hold only the ROOT sentinel"
    );
    let (_pages, freelist) = IndexStore::db_page_stats(store.read_conn()).unwrap();
    assert_eq!(freelist, 0, "recreated file must have zero freelist (disk reclaimed)");
}

#[test]
fn corruption_recovery_deletes_and_recreates() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("corrupt.db");

    // Write garbage to simulate corruption
    std::fs::write(&db_path, b"this is not a sqlite database").unwrap();

    // open() should recover by deleting and recreating
    let store = IndexStore::open(&db_path).unwrap();
    let status = store.get_index_status().unwrap();
    assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
}

#[test]
fn db_file_size_returns_nonzero() {
    let (store, _dir) = open_temp_store();
    let size = store.db_file_size().unwrap();
    assert!(size > 0, "DB file should have nonzero size after creation");
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
fn resolve_path_basic() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Root resolves to ROOT_ID
    assert_eq!(resolve_path(&conn, "/").unwrap(), Some(ROOT_ID));

    // Insert /Users/test
    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    let test_id = IndexStore::insert_entry_v2(&conn, users_id, "test", true, false, None, None, None, None).unwrap();

    assert_eq!(resolve_path(&conn, "/Users").unwrap(), Some(users_id));
    assert_eq!(resolve_path(&conn, "/Users/test").unwrap(), Some(test_id));
    assert_eq!(resolve_path(&conn, "/nonexistent").unwrap(), None);
    assert_eq!(resolve_path(&conn, "/Users/nonexistent").unwrap(), None);
}

/// `resolve_path_under` walks from an ARBITRARY root id, not just `ROOT_ID`.
///
/// This is the network/MTP case: the index is rooted at the volume root, so a
/// deep dir must resolve relative to a non-`/` root. The tree here mimics a share
/// whose mount root is `share` (id `share_id`); `sub/deep` resolves under it, a
/// leading-slash variant resolves identically, an empty path resolves to the root
/// itself, and a missing component returns `None`.
#[test]
fn resolve_path_under_walks_from_a_non_root_id() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // /share/sub/deep, plus a sibling /share/other to prove we don't wander.
    let share_id = insert_entry(&conn, ROOT_ID, "share", true, None);
    let sub_id = insert_entry(&conn, share_id, "sub", true, None);
    let deep_id = insert_entry(&conn, sub_id, "deep", true, None);
    insert_entry(&conn, share_id, "other", true, None);

    // Resolve a deep dir RELATIVE to `share_id` (the index's volume root would be
    // `share_id` for a non-`/`-rooted index).
    assert_eq!(resolve_path_under(&conn, share_id, "sub/deep").unwrap(), Some(deep_id));
    // A leading slash is relative to the given root, not the index root.
    assert_eq!(resolve_path_under(&conn, share_id, "/sub/deep").unwrap(), Some(deep_id));
    // The empty path and "/" resolve to the root id itself.
    assert_eq!(resolve_path_under(&conn, share_id, "").unwrap(), Some(share_id));
    assert_eq!(resolve_path_under(&conn, share_id, "/").unwrap(), Some(share_id));
    // One level under the root resolves.
    assert_eq!(resolve_path_under(&conn, share_id, "sub").unwrap(), Some(sub_id));
    // A missing component returns None.
    assert_eq!(resolve_path_under(&conn, share_id, "sub/missing").unwrap(), None);
    // The absolute path that `resolve_path` would use FAILS at the first
    // component (the volume root isn't `/`), which is exactly the gap
    // `resolve_path_under` closes.
    assert_eq!(resolve_path(&conn, "/sub/deep").unwrap(), None);
}

#[test]
fn resolve_path_trailing_slash() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    assert_eq!(resolve_path(&conn, "/Users/").unwrap(), Some(users_id));
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
fn delete_subtree_by_id_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Build tree: /a/b/c.txt
    let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None, None, None).unwrap();
    let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None, None, None).unwrap();
    let c = IndexStore::insert_entry_v2(&conn, b, "c.txt", false, false, Some(42), Some(42), None, None).unwrap();

    // Add dir_stats for a and b
    IndexStore::upsert_dir_stats_by_id(
        &conn,
        &[
            DirStatsById {
                entry_id: a,
                recursive_logical_size: 42,
                recursive_physical_size: 42,
                recursive_file_count: 1,
                recursive_dir_count: 1,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
            DirStatsById {
                entry_id: b,
                recursive_logical_size: 42,
                recursive_physical_size: 42,
                recursive_file_count: 1,
                recursive_dir_count: 0,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            },
        ],
    )
    .unwrap();

    // Delete subtree rooted at /a
    IndexStore::delete_subtree_by_id(&conn, a).unwrap();

    assert!(IndexStore::get_entry_by_id(&conn, a).unwrap().is_none());
    assert!(IndexStore::get_entry_by_id(&conn, b).unwrap().is_none());
    assert!(IndexStore::get_entry_by_id(&conn, c).unwrap().is_none());
    assert!(IndexStore::get_dir_stats_by_id(&conn, a).unwrap().is_none());
    assert!(IndexStore::get_dir_stats_by_id(&conn, b).unwrap().is_none());
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
fn reconstruct_path_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    assert_eq!(IndexStore::reconstruct_path(&conn, ROOT_ID).unwrap(), "/");

    let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    let foo = IndexStore::insert_entry_v2(&conn, users, "foo", true, false, None, None, None, None).unwrap();
    let file =
        IndexStore::insert_entry_v2(&conn, foo, "bar.txt", false, false, Some(10), Some(10), None, None).unwrap();

    assert_eq!(IndexStore::reconstruct_path(&conn, users).unwrap(), "/Users");
    assert_eq!(IndexStore::reconstruct_path(&conn, foo).unwrap(), "/Users/foo");
    assert_eq!(IndexStore::reconstruct_path(&conn, file).unwrap(), "/Users/foo/bar.txt");
}

#[test]
fn resolve_component_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "Users").unwrap(),
        Some(users)
    );
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "nonexistent").unwrap(),
        None
    );
}

#[test]
fn get_parent_id_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    assert_eq!(IndexStore::get_parent_id(&conn, users).unwrap(), Some(ROOT_ID));
    assert_eq!(IndexStore::get_parent_id(&conn, ROOT_ID).unwrap(), Some(ROOT_PARENT_ID));
    assert_eq!(IndexStore::get_parent_id(&conn, 999999).unwrap(), None);
}

#[cfg(target_os = "macos")]
#[test]
fn platform_case_collation_macos() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Insert "Users" dir
    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();

    // Resolve with different case should work on macOS
    assert_eq!(resolve_path(&conn, "/users").unwrap(), Some(users_id));
    assert_eq!(resolve_path(&conn, "/USERS").unwrap(), Some(users_id));
    assert_eq!(resolve_path(&conn, "/Users").unwrap(), Some(users_id));

    // Schema v12 reinstated UNIQUE on (parent_id, name_folded). On macOS
    // `normalize_for_comparison("Users") == normalize_for_comparison("users")`
    // (NFD + case fold), so this insert must collide.
    let result = IndexStore::insert_entry_v2(&conn, ROOT_ID, "users", true, false, None, None, None, None);
    assert!(
        result.is_err(),
        "case-variant insert must collide on the UNIQUE (parent_id, name_folded) index; got {result:?}"
    );
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

#[cfg(target_os = "macos")]
#[test]
fn resolve_component_case_insensitive() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();

    // Different casings should all resolve to the same ID
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "users").unwrap(),
        Some(users_id)
    );
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "USERS").unwrap(),
        Some(users_id)
    );
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "uSeRs").unwrap(),
        Some(users_id)
    );

    // Nonexistent name returns None
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "nonexistent").unwrap(),
        None
    );
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

#[test]
fn deeply_nested_path_resolution() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Create /a/b/c/d/e/f/g/h/i/j (10 levels deep)
    let mut parent_id = ROOT_ID;
    let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
    let mut ids = Vec::new();
    for name in &names {
        let id = IndexStore::insert_entry_v2(&conn, parent_id, name, true, false, None, None, None, None).unwrap();
        ids.push(id);
        parent_id = id;
    }

    // Resolve full path
    let path = "/a/b/c/d/e/f/g/h/i/j";
    assert_eq!(resolve_path(&conn, path).unwrap(), Some(*ids.last().unwrap()));

    // Reconstruct from deepest
    let reconstructed = IndexStore::reconstruct_path(&conn, *ids.last().unwrap()).unwrap();
    assert_eq!(reconstructed, path);

    // Partial path
    assert_eq!(resolve_path(&conn, "/a/b/c").unwrap(), Some(ids[2]));
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

// ====================================================================
// platform_case_compare / normalize_for_comparison
//
// The collation function backs SQLite's `platform_case` collation, which
// every path-resolution query relies on. cargo-mutants showed the
// structural mutants `platform_case_compare -> Default::default()` and
// `normalize_for_comparison -> String::new() / "xyzzy".into()` survive
// when the only test exercises one direction of equality.
// ====================================================================

#[cfg(target_os = "macos")]
#[test]
fn platform_case_compare_distinguishes_distinct_names() {
    // Kills: replace platform_case_compare -> Default::default() (which is
    // Ordering::Equal, so every comparison would say "equal"; sort order
    // and SQLite's collation-driven uniqueness would collapse).
    assert_eq!(platform_case_compare("a", "a"), std::cmp::Ordering::Equal);
    assert_eq!(platform_case_compare("a", "b"), std::cmp::Ordering::Less);
    assert_eq!(platform_case_compare("b", "a"), std::cmp::Ordering::Greater);
}

#[cfg(target_os = "macos")]
#[test]
fn platform_case_compare_case_insensitive_on_macos() {
    // APFS is case-preserving but case-insensitive by default. The
    // collation must report equality across case variants for path
    // resolution to work.
    assert_eq!(platform_case_compare("Users", "users"), std::cmp::Ordering::Equal);
    assert_eq!(
        platform_case_compare("README.MD", "readme.md"),
        std::cmp::Ordering::Equal
    );
}

#[cfg(target_os = "macos")]
#[test]
fn platform_case_compare_normalizes_unicode_nfc_to_nfd() {
    // "é" can be one codepoint (NFC, U+00E9) or two (NFD, U+0065 U+0301).
    // APFS stores NFD; the collation must treat the two representations
    // as equal so a user typing NFC resolves NFD-stored entries.
    let nfc = "café"; // typically NFC in Rust source
    let nfd = "cafe\u{0301}"; // 'e' + combining acute
    // Make sure they're actually different byte sequences (sanity check).
    assert_ne!(nfc.as_bytes(), nfd.as_bytes());
    assert_eq!(
        platform_case_compare(nfc, nfd),
        std::cmp::Ordering::Equal,
        "NFC and NFD forms of 'café' must compare equal on APFS"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn platform_case_compare_is_binary_off_macos() {
    // Linux ext4/btrfs: exact byte comparison, NOT case-folded.
    assert_eq!(platform_case_compare("a", "a"), std::cmp::Ordering::Equal);
    assert_eq!(platform_case_compare("Users", "users"), std::cmp::Ordering::Less);
    // ('U' = 0x55, 'u' = 0x75 → 'U' < 'u' in ASCII, so "Users" < "users".)
}

#[cfg(target_os = "macos")]
#[test]
fn normalize_for_comparison_lowercases_and_nfd_normalizes() {
    // Kills: replace normalize_for_comparison -> String::new() / "xyzzy".
    assert_eq!(normalize_for_comparison("Users"), "users");
    let nfc = "café";
    let nfd = "cafe\u{0301}";
    // After normalization, both should be NFD-lowercased and equal.
    assert_eq!(normalize_for_comparison(nfc), normalize_for_comparison(nfd));
    assert!(
        !normalize_for_comparison("hello").is_empty(),
        "normalize_for_comparison must not return an empty string for non-empty input"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn normalize_for_comparison_is_identity_off_macos() {
    assert_eq!(normalize_for_comparison("Users"), "Users");
    assert_eq!(normalize_for_comparison("hello"), "hello");
}

// ── platform_case_compare (property-based) ───────────────────────
//
// The collation is used on every `entries.name` comparison in the
// SQLite index. A bug in the comparator would corrupt the index's
// sort order and, worse, cause `resolve_path` to fail to find
// entries the user typed in a different case or Unicode form.
// These properties pin the comparator algebra (reflexivity,
// antisymmetry, transitivity) plus the platform-specific normalization
// semantics (NFC≡NFD on macOS, byte-equal off macOS).

mod platform_case_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Reflexivity: `cmp(a, a) == Equal` for any string.
        #[test]
        fn reflexivity(s in ".*") {
            prop_assert_eq!(platform_case_compare(&s, &s), std::cmp::Ordering::Equal);
        }

        /// Antisymmetry: `cmp(a, b)` and `cmp(b, a)` must be reverses
        /// of each other.
        #[test]
        fn antisymmetry(a in ".*", b in ".*") {
            let ab = platform_case_compare(&a, &b);
            let ba = platform_case_compare(&b, &a);
            prop_assert_eq!(
                ab,
                ba.reverse(),
                "cmp({:?}, {:?}) = {:?} but cmp({:?}, {:?}) = {:?} should be its reverse",
                a, b, ab, b, a, ba
            );
        }

        /// Transitivity: if `cmp(a, b) <= 0` and `cmp(b, c) <= 0`,
        /// then `cmp(a, c) <= 0`. We also check the strict-less and
        /// equal flavors.
        #[test]
        fn transitivity(a in ".*", b in ".*", c in ".*") {
            use std::cmp::Ordering::*;
            let ab = platform_case_compare(&a, &b);
            let bc = platform_case_compare(&b, &c);
            let ac = platform_case_compare(&a, &c);
            if ab != Greater && bc != Greater {
                prop_assert!(
                    ac != Greater,
                    "transitivity violated: cmp(a,b)={:?} cmp(b,c)={:?} cmp(a,c)={:?} for a={:?} b={:?} c={:?}",
                    ab, bc, ac, a, b, c
                );
            }
            if ab != Less && bc != Less {
                prop_assert!(
                    ac != Less,
                    "transitivity violated (>=): cmp(a,b)={:?} cmp(b,c)={:?} cmp(a,c)={:?}",
                    ab, bc, ac
                );
            }
        }
    }

    // On macOS, NFC and NFD forms of the same logical string must
    // compare equal: APFS stores NFD, but users may type NFC, and
    // `resolve_path` must find the stored entry either way.
    #[cfg(target_os = "macos")]
    proptest! {
        #[test]
        fn nfc_equals_nfd_on_macos(s in ".*") {
            use unicode_normalization::UnicodeNormalization;
            let nfc: String = s.nfc().collect();
            let nfd: String = s.nfd().collect();
            prop_assert_eq!(
                platform_case_compare(&nfc, &nfd),
                std::cmp::Ordering::Equal,
                "NFC {:?} and NFD {:?} of {:?} must compare equal on APFS",
                nfc, nfd, s
            );
        }
    }

    // Off macOS, the comparator is exact byte comparison. We pin
    // this by checking that the result matches `str::cmp`.
    #[cfg(not(target_os = "macos"))]
    proptest! {
        #[test]
        fn matches_byte_cmp_off_macos(a in ".*", b in ".*") {
            prop_assert_eq!(platform_case_compare(&a, &b), a.cmp(&b));
        }
    }
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
