//! Drive indexing module.
//!
//! Background-indexes local volumes into a per-volume SQLite database,
//! tracking every file and directory with recursive size aggregates.
//! Design history is in git (former `docs/specs/drive-indexing/`).

pub mod aggregator;
mod enrichment;
mod event_loop;
mod events;
pub mod firmlinks;
mod manager;
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

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::Ordering;
use std::time::Duration;

use enrichment::READ_POOL;
use manager::IndexManager;
use store::{DirStats, IndexStore};
use tauri::AppHandle;
use writer::WriteMessage;

// ── Indexing state machine ────────────────────────────────────────────

/// Lifecycle phases of the indexing system. Single source of truth for
/// whether indexing is active and what capabilities are available.
pub(crate) enum IndexPhase {
    /// Indexing is not active (disabled by user, not yet started, or shut down).
    Disabled,
    /// IndexManager created, `resume_or_scan()` is running. A temporary read
    /// store is available for enrichment and status queries while initialization
    /// completes.
    Initializing { store: IndexStore },
    /// Fully operational: scanning, watching, enrichment, IPC all work.
    Running(Box<IndexManager>),
    /// Shutdown in progress (transitional, cleanup running).
    ShuttingDown,
}

static INDEXING: LazyLock<std::sync::Mutex<IndexPhase>> = LazyLock::new(|| std::sync::Mutex::new(IndexPhase::Disabled));

// ── Initialization ───────────────────────────────────────────────────

/// Force-initialize the INDEXING static. Called during app setup so the
/// LazyLock is ready before any async tasks access it.
pub fn init() {
    drop(INDEXING.lock());
    log::debug!("Indexing state initialized");
}

/// Whether indexing should auto-start on launch.
///
/// - If settings say disabled (`indexing_enabled == Some(false)`): never auto-start.
/// - Otherwise: auto-start by default (both dev and release builds).
pub fn should_auto_start(indexing_enabled: Option<bool>) -> bool {
    // User explicitly disabled indexing in settings
    if indexing_enabled == Some(false) {
        return false;
    }

    // Default true (setting not yet stored means first launch, enabled by default)
    true
}

/// Trigger background verification of a directory against the index DB.
/// Called after enrichment on each navigation. No-op if indexing is not running.
/// Fully fire-and-forget: the INDEXING lock is acquired on a spawned task,
/// so it never blocks the caller (navigation thread).
pub fn trigger_verification(dir_path: &str) {
    let dir_path = dir_path.to_string();
    tauri::async_runtime::spawn(async move {
        let guard = match INDEXING.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if let IndexPhase::Running(ref mgr) = *guard {
            let writer = mgr.writer.clone();
            let app = mgr.app.clone();
            let scanning = mgr.scanning.load(Ordering::Relaxed);
            drop(guard);
            verifier::maybe_verify(dir_path, writer, app, scanning);
        }
    });
}

/// Stop all scans and watcher without deleting the DB.
///
/// Called when the user disables indexing via settings. The index stays on disk
/// but no scanning or watching runs. Directory sizes revert to `<dir>`.
pub fn stop_indexing() -> Result<(), String> {
    verifier::invalidate();

    // Invalidate ReadPool before shutdown so thread-local connections are discarded.
    if let Some(pool) = READ_POOL.lock().unwrap().take() {
        pool.invalidate();
    }

    let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;

    match std::mem::replace(&mut *guard, IndexPhase::ShuttingDown) {
        IndexPhase::Running(mut mgr) => {
            mgr.shutdown();
            *guard = IndexPhase::Disabled;
            log::info!("Indexing stopped (DB preserved on disk)");
        }
        IndexPhase::Initializing { .. } => {
            *guard = IndexPhase::Disabled;
            log::info!("Indexing stopped during initialization");
        }
        other => {
            *guard = other; // put it back, wasn't running
        }
    }

    Ok(())
}

/// Create the IndexManager for the root volume and auto-start indexing
/// (resume from existing index or fresh scan).
///
/// Call after `init()`. On startup this checks for an existing index: if found,
/// it replays the FSEvents journal from the stored `last_event_id`; otherwise
/// it starts a fresh full scan.
pub fn start_indexing(app: &AppHandle) -> Result<(), String> {
    log::info!("start_indexing: begin");
    memory_watchdog::start(app.clone());

    let mut manager = IndexManager::new("root".to_string(), PathBuf::from("/"), app.clone())?;

    // Install ReadPool early so enrichment works during the Initializing phase.
    let pool = Arc::new(
        ReadPool::new(manager.db_path().to_path_buf()).map_err(|e| format!("Failed to create read pool: {e}"))?,
    );
    *READ_POOL.lock().unwrap() = Some(pool);

    // Transition to Initializing: open a temporary store so enrichment
    // and status queries work while resume_or_scan() runs.
    {
        let init_store = IndexStore::open(manager.db_path()).map_err(|e| format!("Failed to open init store: {e}"))?;
        let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;
        *guard = IndexPhase::Initializing { store: init_store };
    }

    let scan_result = manager.resume_or_scan();

    // Clone the writer before moving manager into the state machine, so we
    // can hand it to the vacuum timer if startup succeeds.
    let writer_for_vacuum = manager.writer.clone();

    // Re-lock and check: if someone called stop_indexing() while we were
    // inside resume_or_scan(), the phase is now Disabled. Respect that —
    // shut down the manager instead of overwriting with Running.
    let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;
    match (&*guard, scan_result) {
        (IndexPhase::Initializing { .. }, Ok(())) => {
            *guard = IndexPhase::Running(Box::new(manager));
            log::info!("start_indexing: done — IndexManager is Running");

            // Periodic incremental vacuum: reclaim free pages from deletes/rescans
            // every 30s. Stops automatically when the writer channel closes.
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    if writer_for_vacuum.send(WriteMessage::IncrementalVacuum).is_err() {
                        break;
                    }
                }
            });
        }
        (IndexPhase::Initializing { .. }, Err(e)) => {
            *guard = IndexPhase::Disabled;
            if let Some(pool) = READ_POOL.lock().unwrap().take() {
                pool.invalidate();
            }
            return Err(e);
        }
        (_, Ok(())) => {
            // Phase changed (e.g. stop_indexing set Disabled). Don't override.
            log::info!("start_indexing: phase changed during init, shutting down manager");
            manager.shutdown();
        }
        (_, Err(e)) => {
            log::warn!("start_indexing: resume_or_scan failed and phase changed: {e}");
            manager.shutdown();
        }
    }

    Ok(())
}

/// Stop all scans, shut down the writer, delete the DB file, and reset state.
///
/// Call `start_indexing()` to create a fresh index afterward.
pub fn clear_index() -> Result<(), String> {
    verifier::invalidate();

    // Invalidate ReadPool before deleting DB files so thread-local connections are discarded.
    if let Some(pool) = READ_POOL.lock().unwrap().take() {
        pool.invalidate();
    }

    let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;

    match std::mem::replace(&mut *guard, IndexPhase::ShuttingDown) {
        IndexPhase::Running(mut mgr) => {
            let db_path = mgr.db_path().to_path_buf();
            mgr.shutdown();
            *guard = IndexPhase::Disabled;

            // Delete DB file and WAL/SHM sidecars
            for path in [
                db_path.clone(),
                db_path.with_extension("db-wal"),
                db_path.with_extension("db-shm"),
            ] {
                if path.exists() {
                    std::fs::remove_file(&path).map_err(|e| format!("Failed to delete {}: {e}", path.display()))?;
                }
            }
            log::info!("Drive index cleared (DB deleted)");
        }
        other => {
            *guard = other;
            log::info!("Drive index clear requested but indexing was not active");
        }
    }

    Ok(())
}

// ── Module-level public API (called by IPC commands) ─────────────────

/// Get the current indexing status.
pub fn get_status() -> Result<IndexStatusResponse, String> {
    let guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &*guard {
        IndexPhase::Disabled | IndexPhase::ShuttingDown => Ok(IndexStatusResponse {
            initialized: false,
            scanning: false,
            entries_scanned: 0,
            dirs_found: 0,
            index_status: None,
            db_file_size: None,
        }),
        IndexPhase::Initializing { store, .. } => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            Ok(IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                index_status,
                db_file_size,
            })
        }
        IndexPhase::Running(mgr) => mgr.get_status(),
    }
}

/// Get extended debug status for the debug window.
pub fn get_debug_status() -> Result<IndexDebugStatusResponse, String> {
    let guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &*guard {
        IndexPhase::Disabled | IndexPhase::ShuttingDown => {
            let base = IndexStatusResponse {
                initialized: false,
                scanning: false,
                entries_scanned: 0,
                dirs_found: 0,
                index_status: None,
                db_file_size: None,
            };
            let (activity_phase, phase_started_at, phase_duration_ms, phase_history) =
                IndexManager::read_phase_timeline();
            Ok(IndexDebugStatusResponse {
                base,
                watcher_active: false,
                live_event_count: 0,
                must_scan_count: 0,
                must_scan_rescans_completed: 0,
                live_entry_count: None,
                live_dir_count: None,
                dirs_with_stats: None,
                recent_must_scan_paths: Vec::new(),
                activity_phase,
                phase_started_at,
                phase_duration_ms,
                phase_history,
                verifying: false,
                db_main_size: None,
                db_wal_size: None,
                db_page_count: None,
                db_freelist_count: None,
            })
        }
        IndexPhase::Initializing { store, .. } => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            let base = IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                index_status,
                db_file_size,
            };
            let (activity_phase, phase_started_at, phase_duration_ms, phase_history) =
                IndexManager::read_phase_timeline();
            let db_main_size = store.db_main_size().ok();
            let db_wal_size = store.db_wal_size().ok();
            let conn = store.read_conn();
            let (db_page_count, db_freelist_count) = IndexStore::db_page_stats(conn)
                .map(|(p, f)| (Some(p), Some(f)))
                .unwrap_or((None, None));
            Ok(IndexDebugStatusResponse {
                base,
                watcher_active: DEBUG_STATS.watcher_active.load(Ordering::Relaxed),
                live_event_count: 0,
                must_scan_count: 0,
                must_scan_rescans_completed: 0,
                live_entry_count: None,
                live_dir_count: None,
                dirs_with_stats: None,
                recent_must_scan_paths: Vec::new(),
                activity_phase,
                phase_started_at,
                phase_duration_ms,
                phase_history,
                verifying: DEBUG_STATS.verifying.load(Ordering::Relaxed),
                db_main_size,
                db_wal_size,
                db_page_count,
                db_freelist_count,
            })
        }
        IndexPhase::Running(mgr) => mgr.get_debug_status(),
    }
}

/// Look up recursive stats for a single directory.
pub fn get_dir_stats(path: &str) -> Result<Option<DirStats>, String> {
    let pool = get_read_pool().ok_or_else(|| "Indexing not initialized".to_string())?;
    let normalized = firmlinks::normalize_path(path);

    pool.with_conn(|conn| {
        let entry_id =
            match store::resolve_path(conn, &normalized).map_err(|e| format!("Couldn't resolve path: {e}"))? {
                Some(id) => id,
                None => return Ok(None),
            };

        let stats =
            IndexStore::get_dir_stats_by_id(conn, entry_id).map_err(|e| format!("Couldn't get dir stats: {e}"))?;

        Ok(stats.map(|s| DirStats {
            path: normalized.clone(),
            recursive_size: s.recursive_logical_size,
            recursive_physical_size: s.recursive_physical_size,
            recursive_file_count: s.recursive_file_count,
            recursive_dir_count: s.recursive_dir_count,
        }))
    })?
}

/// Batch lookup of dir_stats for multiple paths.
pub fn get_dir_stats_batch(paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
    let pool = get_read_pool().ok_or_else(|| "Indexing not initialized".to_string())?;

    pool.with_conn(|conn| {
        let mut results = Vec::with_capacity(paths.len());
        let mut id_to_idx: Vec<(i64, usize, String)> = Vec::new();

        for (i, path) in paths.iter().enumerate() {
            let normalized = firmlinks::normalize_path(path);
            match store::resolve_path(conn, &normalized).map_err(|e| format!("Couldn't resolve path: {e}"))? {
                Some(id) => {
                    id_to_idx.push((id, i, normalized));
                    results.push(None);
                }
                None => results.push(None),
            }
        }

        if !id_to_idx.is_empty() {
            let ids: Vec<i64> = id_to_idx.iter().map(|(id, _, _)| *id).collect();
            let stats_batch = IndexStore::get_dir_stats_batch_by_ids(conn, &ids)
                .map_err(|e| format!("Couldn't get dir stats batch: {e}"))?;

            for ((_, idx, normalized), stats_opt) in id_to_idx.into_iter().zip(stats_batch) {
                results[idx] = stats_opt.map(|s| DirStats {
                    path: normalized,
                    recursive_size: s.recursive_logical_size,
                    recursive_physical_size: s.recursive_physical_size,
                    recursive_file_count: s.recursive_file_count,
                    recursive_dir_count: s.recursive_dir_count,
                });
            }
        }

        Ok(results)
    })?
}

/// Force a fresh full scan (for debug/manual trigger).
pub fn force_scan() -> Result<(), String> {
    let mut guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &mut *guard {
        IndexPhase::Running(mgr) => mgr.start_scan("manual start"),
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Stop the active scan without shutting down the manager.
pub fn stop_scan() -> Result<(), String> {
    let mut guard = INDEXING.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match &mut *guard {
        IndexPhase::Running(mgr) => {
            mgr.stop_scan();
            Ok(())
        }
        _ => Err("Indexing not initialized".to_string()),
    }
}

/// Check whether indexing is active (initializing or running).
pub fn is_active() -> bool {
    INDEXING
        .lock()
        .map(|g| matches!(&*g, IndexPhase::Initializing { .. } | IndexPhase::Running(_)))
        .unwrap_or(false)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::FileEntry;
    use enrichment::{READ_POOL_TEST_MUTEX, THREAD_CONN, enrich_via_individual_paths_on, enrich_via_parent_id_on};
    use rusqlite::Connection;
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
        };
        IndexStore::upsert_dir_stats_by_id(&conn, &[updated_user]).expect("update user stats");

        let updated_home = DirStatsById {
            entry_id: 2,
            recursive_logical_size: 1500,
            recursive_physical_size: 1500,
            recursive_file_count: 2,
            recursive_dir_count: 1,
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
        *READ_POOL.lock().unwrap() = Some(Arc::clone(&pool));

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
        *READ_POOL.lock().unwrap() = None;
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

    /// After clearing READ_POOL, `enrich_entries_with_index` returns early
    /// without panic and leaves entries unenriched.
    #[test]
    fn shutdown_enrichment_returns_early() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        // Ensure READ_POOL is empty (simulate post-shutdown state)
        *READ_POOL.lock().unwrap() = None;

        let mut entries = vec![make_file_entry("stuff", "/stuff", true)];
        enrich_entries_with_index(&mut entries);

        assert_eq!(entries[0].recursive_size, None, "unenriched after shutdown");
    }
}
