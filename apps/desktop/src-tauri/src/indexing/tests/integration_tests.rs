//! End-to-end integration tests for the indexing subsystem.
//!
//! Exercises the read path (scan -> aggregate -> enrich -> watcher update ->
//! re-enrich, fast path / fallback / root-level enrichment), the `ReadPool`
//! (connection reuse, generation invalidation, cross-thread reads, contention),
//! the `should_auto_start_indexing` FDA gate, and the `IndexPhase` lifecycle
//! transitions reachable without a Tauri runtime. Tests use temp dirs and real
//! SQLite; those touching the per-volume `INDEX_REGISTRY` serialize on a
//! dedicated mutex and clear it before returning.

use crate::file_system::listing::FileEntry;
use crate::indexing::*;
use crate::settings::FullDiskAccessChoice;
use enrichment::{READ_POOL_TEST_MUTEX, THREAD_CONN, enrich_via_individual_paths_on, enrich_via_parent_id_on};
use rusqlite::Connection;
use state::{
    INDEX_REGISTRY, IndexInstance, IndexPhase, IndexVolumeKind, ROOT_VOLUME_ID, is_initializing_phase,
    try_reserve_initializing_phase,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use store::{DirStatsById, EntryRow, IndexStore, ROOT_ID};

/// Helper: open a temp store and write connection for testing.
fn open_temp_store() -> (IndexStore, Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("test-index.db");
    let store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("open write conn");
    (store, conn, dir)
}

/// Helper: create a FileEntry for testing enrichment.
fn make_file_entry(name: &str, path: &str, is_directory: bool) -> FileEntry {
    FileEntry {
        size: if is_directory { None } else { Some(100) },
        permissions: 0o755,
        ..FileEntry::new(name.to_string(), path.to_string(), is_directory, false)
    }
}

/// End-to-end test: insert entries, compute aggregates, enrich FileEntry objects, verify stats.
#[test]
fn enrich_entries_via_parent_id_end_to_end() {
    let (store, conn, _dir) = open_temp_store();

    // Build a tree:
    //   / (ROOT_ID=1)
    //   /projects (dir, id=2)
    //   /projects/alpha (dir, id=3)
    //   /projects/alpha/file1.txt (100 bytes, id=4)
    //   /projects/alpha/file2.txt (200 bytes, id=5)
    //   /projects/beta (dir, id=6)
    //   /projects/beta/file3.txt (300 bytes, id=7)
    //   /projects/readme.txt (file, 50 bytes, id=8)
    let entries = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "projects".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 3,
            parent_id: 2,
            name: "alpha".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 4,
            parent_id: 3,
            name: "file1.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 5,
            parent_id: 3,
            name: "file2.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(200),
            physical_size: Some(200),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 6,
            parent_id: 2,
            name: "beta".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 7,
            parent_id: 6,
            name: "file3.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(300),
            physical_size: Some(300),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 8,
            parent_id: 2,
            name: "readme.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(50),
            physical_size: Some(50),
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert entries");

    // Compute aggregates
    aggregator::compute_all_aggregates(&conn).expect("compute aggregates");

    // Verify aggregates were computed correctly
    let alpha_stats = IndexStore::get_dir_stats_by_id(&conn, 3).expect("get alpha stats");
    assert!(alpha_stats.is_some(), "alpha should have dir_stats");
    let alpha = alpha_stats.unwrap();
    assert_eq!(alpha.recursive_logical_size, 300, "alpha: 100+200=300");
    assert_eq!(alpha.recursive_file_count, 2, "alpha: 2 files");
    assert_eq!(alpha.recursive_dir_count, 0, "alpha: 0 subdirs");

    let beta_stats = IndexStore::get_dir_stats_by_id(&conn, 6).expect("get beta stats");
    assert!(beta_stats.is_some(), "beta should have dir_stats");
    let beta = beta_stats.unwrap();
    assert_eq!(beta.recursive_logical_size, 300, "beta: 300");
    assert_eq!(beta.recursive_file_count, 1, "beta: 1 file");
    assert_eq!(beta.recursive_dir_count, 0, "beta: 0 subdirs");

    let projects_stats = IndexStore::get_dir_stats_by_id(&conn, 2).expect("get projects stats");
    assert!(projects_stats.is_some(), "projects should have dir_stats");
    let proj = projects_stats.unwrap();
    assert_eq!(proj.recursive_logical_size, 650, "projects: 100+200+300+50=650");
    assert_eq!(
        proj.recursive_file_count, 4,
        "projects: 4 files (file1, file2, file3, readme)"
    );
    assert_eq!(proj.recursive_dir_count, 2, "projects: 2 subdirs (alpha, beta)");

    // Now test enrichment: simulate a listing of /projects children
    let mut file_entries = vec![
        make_file_entry("alpha", "/projects/alpha", true),
        make_file_entry("beta", "/projects/beta", true),
        make_file_entry("readme.txt", "/projects/readme.txt", false),
    ];

    // Use the integer-keyed fast path
    let result = enrich_via_parent_id_on(&mut file_entries, store.read_conn(), "/projects", 1);
    assert!(result.is_ok(), "enrich_via_parent_id should succeed: {result:?}");

    // Verify enrichment results
    let alpha_entry = &file_entries[0];
    assert_eq!(alpha_entry.recursive_size, Some(300));
    assert_eq!(alpha_entry.recursive_file_count, Some(2));
    assert_eq!(alpha_entry.recursive_dir_count, Some(0));

    let beta_entry = &file_entries[1];
    assert_eq!(beta_entry.recursive_size, Some(300));
    assert_eq!(beta_entry.recursive_file_count, Some(1));
    assert_eq!(beta_entry.recursive_dir_count, Some(0));

    // Non-directory entries should be unaffected
    let readme_entry = &file_entries[2];
    assert_eq!(readme_entry.recursive_size, None);
}

/// Test enrichment fallback for individual path resolution.
#[test]
fn enrich_entries_fallback_individual_paths() {
    let (store, conn, _dir) = open_temp_store();

    // Simple tree: /docs (dir) with one file
    let entries = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "docs".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 3,
            parent_id: 2,
            name: "guide.md".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(500),
            physical_size: Some(500),
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
    aggregator::compute_all_aggregates(&conn).expect("aggregates");

    let mut file_entries = vec![make_file_entry("docs", "/docs", true)];

    // Use the individual path fallback
    enrich_via_individual_paths_on(ROOT_VOLUME_ID, &mut file_entries, store.read_conn(), 1);

    let docs = &file_entries[0];
    assert_eq!(docs.recursive_size, Some(500));
    assert_eq!(docs.recursive_file_count, Some(1));
    assert_eq!(docs.recursive_dir_count, Some(0));
}

/// Test that enrichment handles empty directory listing.
#[test]
fn enrich_entries_empty_list() {
    let (store, _conn, _dir) = open_temp_store();
    let mut entries: Vec<FileEntry> = Vec::new();
    enrich_via_individual_paths_on(ROOT_VOLUME_ID, &mut entries, store.read_conn(), 1);
}

/// Test that enrichment handles entries with no matching index data.
#[test]
fn enrich_entries_no_matching_index() {
    let (store, _conn, _dir) = open_temp_store();
    let mut entries = vec![make_file_entry("nonexistent", "/nonexistent", true)];
    enrich_via_individual_paths_on(ROOT_VOLUME_ID, &mut entries, store.read_conn(), 1);
    assert_eq!(entries[0].recursive_size, None, "unindexed dir should remain None");
}

/// Test that `list_child_dir_ids_and_names` returns only directories.
#[test]
fn list_child_dir_ids_and_names_filters_files() {
    let (_store, conn, _dir) = open_temp_store();

    let entries = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "dir_a".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 3,
            parent_id: ROOT_ID,
            name: "dir_b".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 4,
            parent_id: ROOT_ID,
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(10),
            physical_size: Some(10),
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");

    let child_dirs = IndexStore::list_child_dir_ids_and_names(&conn, ROOT_ID).expect("list");
    assert_eq!(child_dirs.len(), 2, "should only return directories, not files");

    let names: std::collections::HashSet<&str> = child_dirs.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names.contains("dir_a"));
    assert!(names.contains("dir_b"));
}

/// End-to-end: scan -> aggregate -> enrich -> simulate watcher event -> re-enrich -> verify.
#[test]
fn end_to_end_scan_enrich_watcher_update() {
    let (store, conn, _dir) = open_temp_store();

    // Phase 1: Initial scan
    let entries = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "home".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 3,
            parent_id: 2,
            name: "user".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 4,
            parent_id: 3,
            name: "doc.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1000),
            physical_size: Some(1000),
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
    aggregator::compute_all_aggregates(&conn).expect("aggregates");

    // Verify initial aggregates
    let home_stats = IndexStore::get_dir_stats_by_id(&conn, 2).unwrap().unwrap();
    assert_eq!(home_stats.recursive_logical_size, 1000);
    assert_eq!(home_stats.recursive_file_count, 1);
    assert_eq!(home_stats.recursive_dir_count, 1);

    // Phase 2: Enrich a listing of /home children
    let mut listing = vec![make_file_entry("user", "/home/user", true)];
    let result = enrich_via_parent_id_on(&mut listing, store.read_conn(), "/home", 1);
    assert!(result.is_ok());
    assert_eq!(listing[0].recursive_size, Some(1000));
    assert_eq!(listing[0].recursive_file_count, Some(1));
    assert_eq!(listing[0].recursive_dir_count, Some(0));

    // Phase 3: Simulate a watcher event (new file added via reconciler)
    IndexStore::insert_entry_v2(&conn, 3, "notes.txt", false, false, Some(500), Some(500), None, None)
        .expect("insert new file");

    // Simulate delta propagation (as the writer would do)
    let updated_user = DirStatsById {
        entry_id: 3,
        recursive_logical_size: 1500,
        recursive_physical_size: 1500,
        recursive_file_count: 2,
        recursive_dir_count: 0,
        recursive_has_symlinks: false,
        min_subtree_epoch: 0,
    };
    IndexStore::upsert_dir_stats_by_id(&conn, &[updated_user]).expect("update user stats");

    let updated_home = DirStatsById {
        entry_id: 2,
        recursive_logical_size: 1500,
        recursive_physical_size: 1500,
        recursive_file_count: 2,
        recursive_dir_count: 1,
        recursive_has_symlinks: false,
        min_subtree_epoch: 0,
    };
    IndexStore::upsert_dir_stats_by_id(&conn, &[updated_home]).expect("update home stats");

    // Phase 4: Re-enrich after watcher event
    let mut listing2 = vec![make_file_entry("user", "/home/user", true)];
    let result2 = enrich_via_parent_id_on(&mut listing2, store.read_conn(), "/home", 1);
    assert!(result2.is_ok());
    assert_eq!(listing2[0].recursive_size, Some(1500), "should reflect new file");
    assert_eq!(listing2[0].recursive_file_count, Some(2));

    // Phase 5: Verify integer-keyed lookup works
    let user_id = store::resolve_path(&conn, "/home/user").unwrap().unwrap();
    let user_stats = IndexStore::get_dir_stats_by_id(&conn, user_id).unwrap();
    assert!(user_stats.is_some());
    let user = user_stats.unwrap();
    assert_eq!(user.recursive_logical_size, 1500);
}

/// Test enrichment of entries at the root level (parent = /).
#[test]
fn enrich_entries_at_root_level() {
    let (store, conn, _dir) = open_temp_store();

    let entries = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "Applications".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 3,
            parent_id: 2,
            name: "app.exe".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(5000),
            physical_size: Some(5000),
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 4,
            parent_id: ROOT_ID,
            name: "Users".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 5,
            parent_id: 4,
            name: "someone".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
    aggregator::compute_all_aggregates(&conn).expect("aggregates");

    // Listing at /: children are /Applications and /Users
    let mut listing = vec![
        make_file_entry("Applications", "/Applications", true),
        make_file_entry("Users", "/Users", true),
    ];

    let result = enrich_via_parent_id_on(&mut listing, store.read_conn(), "/", 1);
    assert!(result.is_ok());

    assert_eq!(listing[0].recursive_size, Some(5000));
    assert_eq!(listing[0].recursive_file_count, Some(1));

    assert_eq!(listing[1].recursive_size, Some(0));
    assert_eq!(listing[1].recursive_dir_count, Some(1));
}

// ── ReadPool and contention tests ────────────────────────────────

/// Helper: populate a temp DB with a small tree and aggregates for ReadPool tests.
/// Returns (db_path, TempDir). The TempDir must be kept alive to prevent cleanup.
fn setup_db_for_pool() -> (PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("pool-test.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    let entries = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "projects".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 3,
            parent_id: 2,
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(42),
            physical_size: Some(42),
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &entries).expect("insert");
    aggregator::compute_all_aggregates(&conn).expect("aggregates");
    (db_path, dir)
}

/// Key regression test: enrichment succeeds even while the lifecycle lock is
/// held. Before the ReadPool fix, `enrich_entries_with_index` used `try_lock()`
/// on the lifecycle mutex and silently skipped when the lock was held. With the
/// registry, the lifecycle lock is `INDEX_REGISTRY`; enrichment must still read
/// via the `ReadPool` without contending on it.
#[test]
fn enrichment_under_contention() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    let (db_path, _dir) = setup_db_for_pool();
    let pool = Arc::new(ReadPool::new(db_path).expect("create pool"));

    // Install pool into READ_POOL so `enrich_entries_with_index` can find it.
    // For root, an installed pool IS the skip-vs-route gate signal, so no
    // registry entry is needed here.
    *enrichment::READ_POOL.lock().unwrap() = Some(Arc::clone(&pool));

    // Hold INDEX_REGISTRY.lock() on a background thread for 2 seconds
    let lock_handle = std::thread::spawn(|| {
        let guard = INDEX_REGISTRY.lock().unwrap();
        std::thread::sleep(Duration::from_secs(2));
        drop(guard);
    });

    // Give the locker thread time to acquire
    std::thread::sleep(Duration::from_millis(50));

    // Enrich on this thread; must succeed despite INDEX_REGISTRY being locked
    let mut entries = vec![make_file_entry("projects", "/projects", true)];
    enrich_entries_with_index(&mut entries);

    assert_eq!(
        entries[0].recursive_size,
        Some(42),
        "enrichment should work under contention"
    );
    assert_eq!(entries[0].recursive_file_count, Some(1));

    lock_handle.join().unwrap();

    // Clean up global state
    *enrichment::READ_POOL.lock().unwrap() = None;
}

/// Mid-scan, enrichment reads the partial `dir_stats` rows: a top-level dir's
/// `recursive_size` is non-null and grows across batches, all BEFORE any
/// `ComputeAllAggregates` lands. This pins the read path end of the feature —
/// the writer-side partial pass plus the on-disk rows it commits are visible
/// to `enrich_entries_with_index` while the scan is still in flight.
///
/// Deterministic by construction: each `flush_blocking` is a barrier, so this
/// simulates a mid-scan prefix without racing a live scanner thread.
/// `enrich_entries_with_index` reads the process-global `READ_POOL`, so the
/// test installs a pool and serializes on `READ_POOL_TEST_MUTEX`; without the
/// install, enrichment silently no-ops (proven by the teeth check below).
#[test]
fn partial_aggregation_is_visible_to_enrichment_mid_scan() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();

    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("partial-enrich.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = writer::IndexWriter::spawn(&db_path, None).expect("spawn writer");

    *enrichment::READ_POOL.lock().unwrap() = Some(Arc::new(ReadPool::new(db_path.clone()).expect("create pool")));

    // Top-level dir /big (id=10, depth 1 — within the partial pass's depth cap).
    let big = EntryRow {
        id: 10,
        parent_id: ROOT_ID,
        name: "big".into(),
        is_directory: true,
        is_symlink: false,
        logical_size: None,
        physical_size: None,
        modified_at: None,
        inode: None,
    };

    // Batch 1: the dir plus one 100-byte file directly under it.
    let batch1 = vec![
        big.clone(),
        EntryRow {
            id: 11,
            parent_id: 10,
            name: "f1.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: None,
        },
    ];
    writer.send(writer::WriteMessage::InsertEntriesV2(batch1)).unwrap();
    writer
        .send(writer::WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: writer::AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    // Enrich a /big FileEntry: a partial pass has run, so recursive_size is
    // non-null and reflects batch-1 contents only.
    let mut entries = vec![make_file_entry("big", "/big", true)];
    enrich_entries_with_index(&mut entries);
    assert_eq!(
        entries[0].recursive_size,
        Some(100),
        "after batch 1's partial pass, /big should show a non-null partial size"
    );

    // Batch 2: a second 50-byte file under /big. After its partial pass, the
    // size grows — still before any ComputeAllAggregates.
    let batch2 = vec![EntryRow {
        id: 12,
        parent_id: 10,
        name: "f2.dat".into(),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(50),
        physical_size: Some(50),
        modified_at: None,
        inode: None,
    }];
    writer.send(writer::WriteMessage::InsertEntriesV2(batch2)).unwrap();
    writer
        .send(writer::WriteMessage::ComputePartialAggregates {
            hot_paths: vec![],
            source: writer::AggSource::Maps,
        })
        .unwrap();
    writer.flush_blocking().unwrap();

    let mut entries = vec![make_file_entry("big", "/big", true)];
    enrich_entries_with_index(&mut entries);
    assert_eq!(
        entries[0].recursive_size,
        Some(150),
        "after batch 2's partial pass, /big's partial size should grow to 100 + 50"
    );

    writer.shutdown();
    *enrichment::READ_POOL.lock().unwrap() = None;
}

/// Teeth for `partial_aggregation_is_visible_to_enrichment_mid_scan`: with the
/// partial-agg sends removed, no `dir_stats` rows exist mid-scan, so
/// enrichment leaves `recursive_size` null. This proves the assertions above
/// fail without the feature under test — they aren't vacuously green.
#[test]
fn enrichment_sees_no_partial_size_without_a_partial_pass() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();

    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("partial-enrich-teeth.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = writer::IndexWriter::spawn(&db_path, None).expect("spawn writer");

    *enrichment::READ_POOL.lock().unwrap() = Some(Arc::new(ReadPool::new(db_path.clone()).expect("create pool")));

    let batch = vec![
        EntryRow {
            id: 10,
            parent_id: ROOT_ID,
            name: "big".into(),
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
            name: "f1.dat".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: None,
        },
    ];
    // Insert and flush, but send NO ComputePartialAggregates.
    writer.send(writer::WriteMessage::InsertEntriesV2(batch)).unwrap();
    writer.flush_blocking().unwrap();

    let mut entries = vec![make_file_entry("big", "/big", true)];
    enrich_entries_with_index(&mut entries);
    assert_eq!(
        entries[0].recursive_size, None,
        "without a partial pass, no dir_stats row exists, so enrichment finds no size"
    );

    writer.shutdown();
    *enrichment::READ_POOL.lock().unwrap() = None;
}

/// `get_dir_stats` reflects the in-memory pending-size tracker: a directory
/// with unprocessed writes in flight comes back `recursive_size_pending`,
/// and clears once the tracker is reset (writer drained).
#[test]
fn dir_stats_carry_pending_flag() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    let _pending_guard = pending_sizes::PENDING_SIZES_TEST_MUTEX.lock().unwrap();

    let (db_path, _dir) = setup_db_for_pool();
    let pool = Arc::new(ReadPool::new(db_path).expect("create pool"));
    *enrichment::READ_POOL.lock().unwrap() = Some(pool);
    *pending_sizes::PENDING_SIZES.lock().unwrap() = Some(Arc::new(pending_sizes::PendingSizes::new()));

    // Nothing marked yet: not pending.
    let before = get_dir_stats("/projects").expect("get_dir_stats").expect("dir indexed");
    assert!(!before.recursive_size_pending, "no pending work => flag false");

    // A descendant change marks /projects (and its ancestors) as pending.
    pending_sizes::get_pending_sizes().unwrap().mark("/projects/file.txt");
    let during = get_dir_stats("/projects").expect("get_dir_stats").expect("dir indexed");
    assert!(during.recursive_size_pending, "pending work => flag true");

    // Draining clears the flag.
    pending_sizes::get_pending_sizes().unwrap().clear();
    let after = get_dir_stats("/projects").expect("get_dir_stats").expect("dir indexed");
    assert!(!after.recursive_size_pending, "after drain => flag false");

    *enrichment::READ_POOL.lock().unwrap() = None;
    *pending_sizes::PENDING_SIZES.lock().unwrap() = None;
}

/// Thread-local connection reuse: calling `with_conn` twice from the same
/// thread should reuse the cached connection (same raw pointer).
#[test]
fn read_pool_connection_reuse() {
    let (db_path, _dir) = setup_db_for_pool();
    let pool = ReadPool::new(db_path).expect("create pool");

    let ptr1 = pool
        .with_conn(|conn| conn as *const Connection as usize)
        .expect("first call");
    let ptr2 = pool
        .with_conn(|conn| conn as *const Connection as usize)
        .expect("second call");

    assert_eq!(ptr1, ptr2, "same thread should reuse the cached connection");
}

/// After `invalidate()`, the next `with_conn` opens a fresh connection.
#[test]
fn read_pool_generation_invalidation() {
    let (db_path, _dir) = setup_db_for_pool();
    let pool = ReadPool::new(db_path.clone()).expect("create pool");

    // Warm up the thread-local connection
    pool.with_conn(|_| ()).expect("before invalidation");

    // Verify the cached generation is 0
    let gen_before = THREAD_CONN.with(|cell| cell.borrow().as_ref().map(|(_, g, _)| *g).unwrap());
    assert_eq!(gen_before, 0);

    pool.invalidate();

    // After invalidation, the pool generation is 1 but the cached
    // thread-local still holds generation 0. The next with_conn must
    // detect the mismatch and reopen.
    pool.with_conn(|_| ()).expect("after invalidation");

    let gen_after = THREAD_CONN.with(|cell| cell.borrow().as_ref().map(|(_, g, _)| *g).unwrap());
    assert_eq!(
        gen_after, 1,
        "invalidation should force a new connection with bumped generation"
    );
}

/// Multiple threads can call `with_conn` concurrently without errors.
#[test]
fn read_pool_cross_thread_reads() {
    let (db_path, _dir) = setup_db_for_pool();
    let pool = Arc::new(ReadPool::new(db_path).expect("create pool"));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let p = Arc::clone(&pool);
            std::thread::spawn(move || {
                p.with_conn(|conn| {
                    let stats = IndexStore::get_dir_stats_by_id(conn, 2).expect("query");
                    assert!(stats.is_some(), "each thread should read the data");
                    assert_eq!(stats.unwrap().recursive_logical_size, 42);
                })
                .expect("with_conn should succeed");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread should not panic");
    }
}

// ── should_auto_start_indexing (FDA gate) ────────────────────────

/// First launch with no FDA decision and OS reports no FDA: indexer
/// must NOT auto-start. Otherwise the recursive scan from `/` would
/// trigger native TCC popups behind the in-app FDA modal.
#[test]
fn should_auto_start_indexing_blocked_when_not_asked_and_os_fda_false() {
    assert!(!should_auto_start_indexing(
        None,
        FullDiskAccessChoice::NotAskedYet,
        false
    ));
    assert!(!should_auto_start_indexing(
        Some(true),
        FullDiskAccessChoice::NotAskedYet,
        false
    ));
}

/// `NotAskedYet` but OS already grants FDA (e.g., granted externally
/// before our modal ever ran): safe to auto-start, no popups will fire.
#[test]
fn should_auto_start_indexing_allowed_when_os_fda_true_overrides_not_asked() {
    assert!(should_auto_start_indexing(
        None,
        FullDiskAccessChoice::NotAskedYet,
        true
    ));
    assert!(should_auto_start_indexing(
        Some(true),
        FullDiskAccessChoice::NotAskedYet,
        true
    ));
}

/// User picked Allow: auto-start (after restart the OS probe is true,
/// no popups; if FDA was revoked between sessions the revoked-prompt
/// flow re-asks while the indexer waits for the gate to clear again).
#[test]
fn should_auto_start_indexing_allowed_when_user_choice_is_allow() {
    assert!(should_auto_start_indexing(None, FullDiskAccessChoice::Allow, true));
    // Allow + OS-false: predicate passes the gate. The indexer attempts
    // to scan; per-folder TCC popups fire as it walks protected paths,
    // and the revoked-prompt UI guides the user back into System Settings.
    assert!(should_auto_start_indexing(None, FullDiskAccessChoice::Allow, false));
}

/// User picked Deny: auto-start. Per the onboarding contract, Cmdr
/// proceeds in limited mode and the user gets individual TCC popups for
/// each protected folder the indexer touches; they accept or deny each.
#[test]
fn should_auto_start_indexing_allowed_when_user_choice_is_deny() {
    assert!(should_auto_start_indexing(None, FullDiskAccessChoice::Deny, false));
    assert!(should_auto_start_indexing(
        Some(true),
        FullDiskAccessChoice::Deny,
        false
    ));
}

/// Indexing disabled in settings always wins: never auto-start
/// regardless of FDA state.
#[test]
fn should_auto_start_indexing_blocked_when_indexing_disabled() {
    assert!(!should_auto_start_indexing(
        Some(false),
        FullDiskAccessChoice::Allow,
        true
    ));
    assert!(!should_auto_start_indexing(
        Some(false),
        FullDiskAccessChoice::Deny,
        false
    ));
    assert!(!should_auto_start_indexing(
        Some(false),
        FullDiskAccessChoice::NotAskedYet,
        true
    ));
}

// ── IndexPhase transitions ─────────────────────────────────────────
//
// The `INDEX_REGISTRY` is shared with the running app (and with the
// verifier::trigger_verification path), so these tests serialize via a
// dedicated mutex and always clear the registry before returning. They
// never call `start_indexing` (needs an AppHandle); instead they reserve
// an `Initializing { store }` instance by hand for the `root` volume and
// drive the transitions whose Rust-side state machine is reachable
// without a Tauri runtime: stop_indexing's Initializing -> removed arm,
// and clear_index's no-op arm when not Running.
//
// With the per-volume registry, "Disabled" is the ABSENCE of an instance
// (there is no `IndexPhase::Disabled` variant). So assertions that used to
// read "phase is Disabled" now read "no instance for `root`", testing the
// same invariant: a stopped/never-started volume is fully disabled.

static INDEXING_TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Clear the `root` registry instance and the root read-path globals. Used at
/// the start of each IndexPhase test so transient state from earlier tests (or
/// the running app, if these tests run inside a warmed-up debug build) doesn't
/// bleed in.
fn reset_indexing_for_test() {
    INDEX_REGISTRY.lock().expect("registry poisoned").remove(ROOT_VOLUME_ID);
    // The stop/clear paths invalidate the root READ_POOL/PENDING_SIZES; mirror
    // that so we don't carry stale handles from a prior test.
    *enrichment::READ_POOL.lock().unwrap() = None;
    *pending_sizes::PENDING_SIZES.lock().unwrap() = None;
}

/// Whether the `root` volume has a registered instance (the registry's
/// "indexed / not-disabled" predicate).
fn root_is_registered() -> bool {
    INDEX_REGISTRY.lock().unwrap().contains_key(ROOT_VOLUME_ID)
}

/// Whether the `root` instance is in the `Initializing` phase.
fn root_is_initializing() -> bool {
    INDEX_REGISTRY
        .lock()
        .unwrap()
        .get(ROOT_VOLUME_ID)
        .is_some_and(|i| is_initializing_phase(&i.phase))
}

/// Reserve an `Initializing { store }` instance for a volume id (the harness
/// stand-in for `start_indexing` up to the `IndexManager` build, which needs an
/// `AppHandle`). Returns the temp dir backing the DB.
fn reserve_initializing_for(volume_id: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir for init store");
    let db_path = dir.path().join("init-phase-test.db");
    let store = IndexStore::open(&db_path).expect("open init store");
    let pool = Arc::new(ReadPool::new(db_path.clone()).expect("pool"));
    let pending = Arc::new(pending_sizes::PendingSizes::new());
    try_reserve_initializing_phase(
        volume_id,
        IndexVolumeKind::Local,
        store,
        pool,
        pending,
        Arc::new(std::sync::Mutex::new(None)),
    )
    .unwrap_or_else(|_| panic!("reserve {volume_id} must succeed from absent"));
    dir
}

fn install_initializing_phase() -> tempfile::TempDir {
    reserve_initializing_for(ROOT_VOLUME_ID)
}

#[test]
fn is_initializing_phase_matches_only_initializing_variant() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = IndexStore::open(&dir.path().join("classifier.db")).expect("open store");
    // ShuttingDown classified as not-initializing.
    assert!(!is_initializing_phase(&IndexPhase::ShuttingDown));
    // Initializing classified as initializing.
    assert!(is_initializing_phase(&IndexPhase::Initializing { store }));
}

#[test]
fn try_reserve_initializing_succeeds_only_from_disabled() {
    // The reservation function is the lock-first guard for `start_indexing`,
    // now per volume id. It must atomically transition `(absent) ->
    // Initializing(store)` and reject every other starting phase so a second
    // `start_indexing` call cannot spawn a second `IndexManager` / writer thread
    // on the same DB.
    let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());

    // From absent (Disabled): reservation succeeds.
    reset_indexing_for_test();
    let _tmp = reserve_initializing_for(ROOT_VOLUME_ID);
    assert!(root_is_initializing(), "absent should transition to Initializing");

    // From Initializing: reservation must fail and leave the phase untouched.
    let dir2 = tempfile::tempdir().expect("temp dir");
    let db2 = dir2.path().join("from-init.db");
    let store2 = IndexStore::open(&db2).expect("open store");
    let pool2 = Arc::new(ReadPool::new(db2.clone()).expect("pool"));
    let pending2 = Arc::new(pending_sizes::PendingSizes::new());
    let res = try_reserve_initializing_phase(
        ROOT_VOLUME_ID,
        IndexVolumeKind::Local,
        store2,
        pool2,
        pending2,
        Arc::new(std::sync::Mutex::new(None)),
    );
    assert!(
        res.is_err(),
        "second reservation while already Initializing must fail (would spawn a second writer)"
    );
    assert!(
        root_is_initializing(),
        "failed reservation must leave the phase unchanged"
    );
    reset_indexing_for_test();

    // From ShuttingDown: reservation must fail and leave the instance intact.
    // Pinning ShuttingDown is the analogous case at the other end of the
    // lifecycle (the Running case needs an AppHandle).
    {
        let dir3 = tempfile::tempdir().expect("temp dir");
        let db3 = dir3.path().join("from-shutdown.db");
        let store_sd = IndexStore::open(&db3).expect("open store");
        let pool_sd = Arc::new(ReadPool::new(db3.clone()).expect("pool"));
        let pending_sd = Arc::new(pending_sizes::PendingSizes::new());
        INDEX_REGISTRY.lock().unwrap().insert(
            ROOT_VOLUME_ID.to_string(),
            IndexInstance {
                phase: IndexPhase::ShuttingDown,
                kind: IndexVolumeKind::Local,
                read_pool: pool_sd,
                pending_sizes: pending_sd,
                freshness: Arc::new(std::sync::Mutex::new(None)),
            },
        );
        // store_sd is unused after insert; the ShuttingDown phase carries no store.
        drop(store_sd);
    }
    let dir4 = tempfile::tempdir().expect("temp dir");
    let db4 = dir4.path().join("from-shutdown2.db");
    let store4 = IndexStore::open(&db4).expect("open store");
    let pool4 = Arc::new(ReadPool::new(db4.clone()).expect("pool"));
    let pending4 = Arc::new(pending_sizes::PendingSizes::new());
    let res = try_reserve_initializing_phase(
        ROOT_VOLUME_ID,
        IndexVolumeKind::Local,
        store4,
        pool4,
        pending4,
        Arc::new(std::sync::Mutex::new(None)),
    );
    assert!(res.is_err(), "reservation from ShuttingDown must fail");
    assert!(
        matches!(
            INDEX_REGISTRY.lock().unwrap().get(ROOT_VOLUME_ID).map(|i| &i.phase),
            Some(IndexPhase::ShuttingDown)
        ),
        "failed reservation must leave ShuttingDown intact"
    );
    reset_indexing_for_test();
}

#[test]
fn stop_indexing_during_initialization_transitions_to_disabled() {
    // Pins the Initializing -> disabled (instance removed) race arm in
    // stop_indexing. If `stop_indexing` runs while `start_indexing` is inside
    // `resume_or_scan`, the instance must be removed so the post-scan re-lock
    // observes the change and shuts the half-built manager down.
    let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    reset_indexing_for_test();

    let _tmp = install_initializing_phase();
    stop_indexing(ROOT_VOLUME_ID).expect("stop_indexing must succeed from Initializing");

    assert!(
        !root_is_registered(),
        "stop_indexing must collapse Initializing to disabled (instance removed)"
    );
    reset_indexing_for_test();
}

#[test]
fn stop_indexing_when_disabled_is_a_noop() {
    // Pins the catch-all arm in stop_indexing: if the volume isn't
    // Running or Initializing (here: absent), the call is a no-op and
    // the volume stays disabled (absent).
    let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    reset_indexing_for_test();

    // Already disabled (absent); stop_indexing should stay disabled (no-op).
    stop_indexing(ROOT_VOLUME_ID).expect("stop_indexing from disabled must succeed");
    assert!(
        !root_is_registered(),
        "stop_indexing on an absent volume stays disabled"
    );
}

#[test]
fn clear_index_from_initializing_removes_instance_and_deletes_db() {
    // Forgetting (`clear_index`) an Initializing volume (a re-enabled
    // still-scanning Stale index) must remove the instance — so the badge goes
    // gray, not a dangling Stale — and delete its DB. This mirrors
    // `stop_indexing`'s Initializing arm (already removes the instance), and is
    // race-safe: an in-flight `start_indexing` post-`resume_or_scan` re-lock sees
    // `still_initializing == false` and shuts its half-built manager down.
    // This guards against the Initializing arm early-returning and leaking the
    // instance AND the DB.
    let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    reset_indexing_for_test();

    let tmp = install_initializing_phase();
    let db_path = tmp.path().join("init-phase-test.db");
    assert!(db_path.exists(), "init store DB exists before clear");

    clear_index(ROOT_VOLUME_ID).expect("clear_index from Initializing must succeed");
    assert!(
        !root_is_registered(),
        "clear_index must remove the Initializing instance (gray, not dangling)"
    );
    assert!(!db_path.exists(), "clear_index must delete the DB from disk");
    reset_indexing_for_test();
}

/// Concurrency: the shutdown drain must NOT hold `INDEXING`.
///
/// `stop_indexing`/`clear_index` publish `IndexPhase::ShuttingDown`, release
/// the lock, and only THEN run `mgr.shutdown()` (a blocking up-to-5 s drain of
/// the live-event task). This test models that exact lock discipline against
/// the real `INDEX_REGISTRY` static and the real `get_status()`: it publishes a
/// `ShuttingDown` instance for `root`, drops the guard, spawns a thread that
/// simulates the slow drain (holding NO lock), and asserts a concurrent
/// `get_status()` returns promptly — far under the 5 s drain budget. With the
/// buggy lock-held-across-drain shape, `get_status()` would block for the whole
/// drain; here it returns immediately and reports the `ShuttingDown` phase as
/// not-initialized, the coherent intermediate state concurrent callers see.
///
/// This is a true concurrency assertion on the load-bearing property (the
/// lock is free during the drain), but it drives the lock/phase contract
/// directly rather than `stop_indexing`'s `Running` arm end-to-end: that arm
/// needs a real `IndexManager`, which needs a `tauri::AppHandle` (not
/// constructable in unit tests without the `tauri/test` feature — see this
/// module's IndexPhase test note and `indexing/CLAUDE.md`).
#[test]
fn shutdown_drain_does_not_hold_indexing_lock() {
    let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    reset_indexing_for_test();

    // Publish the intermediate phase and release the lock, mirroring the
    // fixed `stop_indexing`/`clear_index` shape (instance present, ShuttingDown).
    {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = dir.path().join("shutdown-drain.db");
        let store = IndexStore::open(&db).expect("open store");
        drop(store); // ShuttingDown carries no store
        let pool = Arc::new(ReadPool::new(db.clone()).expect("pool"));
        let pending = Arc::new(pending_sizes::PendingSizes::new());
        INDEX_REGISTRY.lock().expect("registry poisoned").insert(
            ROOT_VOLUME_ID.to_string(),
            IndexInstance {
                phase: IndexPhase::ShuttingDown,
                kind: IndexVolumeKind::Local,
                read_pool: pool,
                pending_sizes: pending,
                freshness: Arc::new(std::sync::Mutex::new(None)),
            },
        );
    }

    // Simulate the blocking drain on another thread WITHOUT holding the registry.
    let drain_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let drain_done_bg = Arc::clone(&drain_done);
    let drain = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(800));
        drain_done_bg.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    // Concurrent status poll must return promptly (well under the drain), and
    // must observe the published `ShuttingDown` phase as not-initialized.
    let start = std::time::Instant::now();
    let status = get_status(ROOT_VOLUME_ID).expect("get_status during shutdown drain");
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(300),
        "get_status must not block on the drain; took {elapsed:?}"
    );
    assert!(
        !status.initialized,
        "ShuttingDown phase must report not-initialized to concurrent callers"
    );
    assert!(
        !drain_done.load(std::sync::atomic::Ordering::SeqCst),
        "status returned while the drain was still in flight (proves no lock contention)"
    );

    drain.join().expect("drain thread should not panic");
    reset_indexing_for_test();
}

/// After clearing READ_POOL, `enrich_entries_with_index` returns early
/// without panic and leaves entries unenriched.
#[test]
fn shutdown_enrichment_returns_early() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    // Ensure READ_POOL is empty (simulate post-shutdown state)
    *enrichment::READ_POOL.lock().unwrap() = None;

    let mut entries = vec![make_file_entry("stuff", "/stuff", true)];
    enrich_entries_with_index(&mut entries);

    assert_eq!(entries[0].recursive_size, None, "unenriched after shutdown");
}
