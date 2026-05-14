//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! Design history is in git (former `docs/specs/drive-indexing/`).
//!
//! `mod.rs` is a thin public-API facade. The state machine (the global
//! `INDEXING` mutex, `IndexPhase` enum, phase transitions, and the
//! `IndexManager` + `ReadPool` bootstrap) lives in [`state`].

pub mod aggregator;
mod enrichment;
mod event_loop;
mod events;
pub mod expected_totals;
pub mod firmlinks;
mod manager;
mod state;
pub mod store;
pub mod writer;

mod memory_watchdog;
mod metadata;
mod reconciler;
pub(crate) mod scanner;
mod verifier;
pub(crate) mod watcher;

#[cfg(test)]
mod stress_test_helpers;
#[cfg(test)]
mod stress_tests_concurrency;
#[cfg(test)]
mod stress_tests_lifecycle;

pub use enrichment::enrich_entries_with_index;
pub(crate) use enrichment::{ReadPool, get_read_pool};
pub(crate) use events::DEBUG_STATS;
pub use events::*;

pub use state::{
    clear_index, force_scan, get_debug_status, get_dir_stats, get_dir_stats_batch, get_status, init, is_active,
    should_auto_start, should_auto_start_indexing, start_indexing, stop_indexing, stop_scan, trigger_verification,
};

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::FileEntry;
    use crate::settings::FullDiskAccessChoice;
    use enrichment::{READ_POOL_TEST_MUTEX, THREAD_CONN, enrich_via_individual_paths_on, enrich_via_parent_id_on};
    use rusqlite::Connection;
    use state::{INDEXING, IndexPhase, is_initializing_phase};
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
        let result = enrich_via_parent_id_on(&mut file_entries, store.read_conn(), "/projects");
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
        enrich_via_individual_paths_on(&mut file_entries, store.read_conn());

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
        enrich_via_individual_paths_on(&mut entries, store.read_conn());
    }

    /// Test that enrichment handles entries with no matching index data.
    #[test]
    fn enrich_entries_no_matching_index() {
        let (store, _conn, _dir) = open_temp_store();
        let mut entries = vec![make_file_entry("nonexistent", "/nonexistent", true)];
        enrich_via_individual_paths_on(&mut entries, store.read_conn());
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
        let result = enrich_via_parent_id_on(&mut listing, store.read_conn(), "/home");
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
        };
        IndexStore::upsert_dir_stats_by_id(&conn, &[updated_user]).expect("update user stats");

        let updated_home = DirStatsById {
            entry_id: 2,
            recursive_logical_size: 1500,
            recursive_physical_size: 1500,
            recursive_file_count: 2,
            recursive_dir_count: 1,
            recursive_has_symlinks: false,
        };
        IndexStore::upsert_dir_stats_by_id(&conn, &[updated_home]).expect("update home stats");

        // Phase 4: Re-enrich after watcher event
        let mut listing2 = vec![make_file_entry("user", "/home/user", true)];
        let result2 = enrich_via_parent_id_on(&mut listing2, store.read_conn(), "/home");
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

        let result = enrich_via_parent_id_on(&mut listing, store.read_conn(), "/");
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

    /// Key regression test: enrichment succeeds even while INDEXING is locked.
    /// Before the ReadPool fix, `enrich_entries_with_index` used `try_lock()` on
    /// INDEXING and silently skipped when the lock was held.
    #[test]
    fn enrichment_under_contention() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let (db_path, _dir) = setup_db_for_pool();
        let pool = Arc::new(ReadPool::new(db_path).expect("create pool"));

        // Install pool into READ_POOL so `enrich_entries_with_index` can find it
        *enrichment::READ_POOL.lock().unwrap() = Some(Arc::clone(&pool));

        // Hold INDEXING.lock() on a background thread for 2 seconds
        let lock_handle = std::thread::spawn(|| {
            let guard = INDEXING.lock().unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(guard);
        });

        // Give the locker thread time to acquire
        std::thread::sleep(Duration::from_millis(50));

        // Enrich on this thread — must succeed despite INDEXING being locked
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
    /// each protected folder the indexer touches — they accept or deny each.
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
    // The global INDEXING cell is shared with the running app (and with the
    // verifier::trigger_verification path), so these tests serialize via a
    // dedicated mutex and always restore the cell to Disabled before
    // returning. They never call `start_indexing` (needs an AppHandle) —
    // instead they install an `Initializing { store }` phase by hand and
    // drive the transitions whose Rust-side state machine is reachable
    // without a Tauri runtime: stop_indexing's Initializing -> Disabled
    // arm, and clear_index's no-op arm when not Running.

    static INDEXING_TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Replace INDEXING with `Disabled` and clear READ_POOL. Used at the
    /// start of each IndexPhase test so transient state from earlier tests
    /// (or the running app, if these tests are run inside a debug build
    /// with the app warmed up) doesn't bleed in.
    fn reset_indexing_for_test() {
        let mut guard = INDEXING.lock().expect("INDEXING lock poisoned");
        *guard = IndexPhase::Disabled;
        drop(guard);
        // The stop/clear paths invalidate READ_POOL; mirror that so we
        // don't carry a stale pool from a prior test.
        *enrichment::READ_POOL.lock().unwrap() = None;
    }

    fn install_initializing_phase() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir for init store");
        let db_path = dir.path().join("init-phase-test.db");
        let store = IndexStore::open(&db_path).expect("open init store");
        let mut guard = INDEXING.lock().expect("INDEXING lock poisoned");
        *guard = IndexPhase::Initializing { store };
        dir
    }

    #[test]
    fn is_initializing_phase_matches_only_initializing_variant() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = IndexStore::open(&dir.path().join("classifier.db")).expect("open store");
        // Disabled / ShuttingDown / Running classified as not-initializing.
        assert!(!is_initializing_phase(&IndexPhase::Disabled));
        assert!(!is_initializing_phase(&IndexPhase::ShuttingDown));
        // Initializing classified as initializing.
        assert!(is_initializing_phase(&IndexPhase::Initializing { store }));
    }

    #[test]
    fn stop_indexing_during_initialization_transitions_to_disabled() {
        // Pins the Initializing -> Disabled race arm in stop_indexing.
        // If `stop_indexing` runs while `start_indexing` is inside
        // `resume_or_scan`, the phase must be cleared to Disabled so the
        // post-scan re-lock observes the change and shuts the half-built
        // manager down.
        let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        reset_indexing_for_test();

        let _tmp = install_initializing_phase();
        stop_indexing().expect("stop_indexing must succeed from Initializing");

        let phase_guard = INDEXING.lock().expect("INDEXING lock poisoned");
        assert!(
            matches!(*phase_guard, IndexPhase::Disabled),
            "stop_indexing must collapse Initializing to Disabled"
        );
        drop(phase_guard);
        reset_indexing_for_test();
    }

    #[test]
    fn stop_indexing_when_disabled_is_a_noop() {
        // Pins the catch-all arm in stop_indexing: if the phase isn't
        // Running or Initializing, the original phase must be restored
        // (not silently replaced with Disabled).
        let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        reset_indexing_for_test();

        // Already Disabled; stop_indexing should remain Disabled (no-op).
        stop_indexing().expect("stop_indexing from Disabled must succeed");
        let phase_guard = INDEXING.lock().expect("INDEXING lock poisoned");
        assert!(matches!(*phase_guard, IndexPhase::Disabled));
        drop(phase_guard);
    }

    #[test]
    fn clear_index_when_not_running_is_a_noop() {
        // Pins the catch-all arm in clear_index: clear must not touch the
        // DB or change phase unless Running. Initializing is preserved so
        // an in-flight start_indexing can keep walking.
        let _guard = INDEXING_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        reset_indexing_for_test();

        let _tmp = install_initializing_phase();
        clear_index().expect("clear_index from Initializing must succeed");
        let phase_guard = INDEXING.lock().expect("INDEXING lock poisoned");
        assert!(
            matches!(*phase_guard, IndexPhase::Initializing { .. }),
            "clear_index must preserve a non-Running phase"
        );
        drop(phase_guard);
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
}
