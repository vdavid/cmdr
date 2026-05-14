//! Indexing state machine and lifecycle.
//!
//! Holds the global `INDEXING` mutex and the `IndexPhase` enum that gates
//! every public operation. Also owns the bootstrap logic that spins up the
//! `IndexManager`, the `ReadPool`, and the incremental-vacuum timer.
//!
//! `mod.rs` is a thin facade that re-exports the public functions defined
//! here; module-internal callers (e.g. `manager.rs`) can use the items
//! directly via `super::state`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::AppHandle;

use super::enrichment::{READ_POOL, ReadPool, get_read_pool};
use super::events::{DEBUG_STATS, IndexDebugStatusResponse, IndexStatusResponse};
use super::firmlinks;
use super::manager::IndexManager;
use super::store::{self, DirStats, IndexStore};
use super::verifier;
use super::writer::WriteMessage;

use crate::settings::FullDiskAccessChoice;

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

pub(crate) static INDEXING: LazyLock<std::sync::Mutex<IndexPhase>> =
    LazyLock::new(|| std::sync::Mutex::new(IndexPhase::Disabled));

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

/// Pure decision: should the indexer auto-start at app launch?
///
/// Combines the user's indexing-enabled setting with the FDA gate. The FDA gate
/// blocks the indexer from scanning `/` before the user has decided about Full
/// Disk Access — otherwise macOS native permission popups (iCloud, Photos, etc.)
/// stack on top of the in-app FDA modal at first launch.
///
/// Auto-start when ALL of the following hold:
/// - The user has not disabled indexing (`indexing_enabled != Some(false)`).
/// - The FDA gate isn't pending (see `crate::fda_gate::is_fda_pending`). The
///   gate is pending only when `fda_choice == NotAskedYet` AND the OS reports
///   FDA isn't granted — i.e., we're still showing the in-app onboarding
///   modal. Once the user picks Deny (same session via
///   `start_indexing_after_fda_decision`) or Allow (which restarts the app),
///   the indexer auto-starts. After Deny, the scan triggers per-folder TCC
///   prompts as it walks protected paths — that's the "individual Allow/Deny
///   prompts" contract the user opted into by denying FDA.
pub fn should_auto_start_indexing(
    indexing_enabled: Option<bool>,
    fda_choice: FullDiskAccessChoice,
    os_fda_granted: bool,
) -> bool {
    should_auto_start(indexing_enabled) && !crate::fda_gate::is_fda_pending(fda_choice, os_fda_granted)
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

/// Phase classifier used by `start_indexing`'s post-`resume_or_scan` branch.
/// Returns true only while the phase carries the temporary init store. If
/// `stop_indexing` swapped the state out from under us during `resume_or_scan`,
/// the phase is `Disabled` (or briefly `ShuttingDown`) and this returns false
/// — the caller treats that as "phase changed, shut the manager down".
///
/// Extracted as a pure helper so the state-machine race fragment is testable
/// without standing up an `AppHandle` / `IndexManager`.
pub(crate) fn is_initializing_phase(phase: &IndexPhase) -> bool {
    matches!(phase, IndexPhase::Initializing { .. })
}

/// Create the IndexManager for the root volume and auto-start indexing
/// (resume from existing index or fresh scan).
///
/// Call after `init()`. On startup this checks for an existing index: if found,
/// it replays the FSEvents journal from the stored `last_event_id`; otherwise
/// it starts a fresh full scan.
pub fn start_indexing(app: &AppHandle) -> Result<(), String> {
    log::info!("start_indexing: begin");
    super::memory_watchdog::start(app.clone());

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
    // inside resume_or_scan(), the phase is no longer Initializing.
    // Respect that — shut down the manager instead of overwriting.
    let mut guard = INDEXING.lock().map_err(|e| format!("Failed to lock state: {e}"))?;
    match (is_initializing_phase(&guard), scan_result) {
        (true, Ok(())) => {
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
        (true, Err(e)) => {
            *guard = IndexPhase::Disabled;
            if let Some(pool) = READ_POOL.lock().unwrap().take() {
                pool.invalidate();
            }
            return Err(e);
        }
        (false, Ok(())) => {
            // Phase changed (e.g. stop_indexing set Disabled). Don't override.
            log::info!("start_indexing: phase changed during init, shutting down manager");
            manager.shutdown();
        }
        (false, Err(e)) => {
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
            recursive_has_symlinks: s.recursive_has_symlinks,
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
                    recursive_has_symlinks: s.recursive_has_symlinks,
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
