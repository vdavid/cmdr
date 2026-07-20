//! The read-only index query surface the IPC commands call (status + dir-stats),
//! distinct from the lifecycle / registry core in `state.rs`.
//!
//! These functions never mutate registry state: they read a volume's freshness +
//! phase (`get_status` / `get_debug_status` / `get_volume_index_status`) or look
//! up directory aggregates from the volume's `ReadPool` (`get_dir_stats*`). The
//! path-based forms resolve the owning volume via `routing::volume_id_for_local_path`
//! and map the read path into the volume's index space via `routing::index_read_path`.

use std::sync::atomic::Ordering;

use super::enrichment::get_read_pool_for;
use super::events::{DEBUG_STATS, IndexDebugStatusResponse, IndexStatusResponse, VolumeIndexStatus};
use super::firmlinks;
use super::manager::IndexManager;
use super::pending_sizes::get_pending_sizes_for;
use super::routing::{index_read_path, volume_id_for_local_path};
use super::state::{INDEX_REGISTRY, IndexPhase, ROOT_VOLUME_ID, get_freshness, index_failure, is_active};
use super::store::{self, DirStats, IndexStore};

/// Per-volume index status for the per-drive freshness badge.
///
/// Carries the volume's freshness color plus the last completed scan's facts
/// (`scan_completed_at`, `scan_duration_ms`) for the tooltip/menu footer. This
/// is the shape the badge consumes for EVERY drive (local included). A volume with no
/// registered instance is the gray / not-indexed state (`enabled: false`,
/// `freshness: None`); a registered one always carries a `freshness`.
///
/// Freshness is read from the registry; the scan facts come from the persisted
/// `meta` surfaced by `get_status`. The two can briefly disagree during a
/// transition, which is fine for a status badge.
pub fn get_volume_index_status(volume_id: &str) -> VolumeIndexStatus {
    let freshness = get_freshness(volume_id);
    let enabled = is_active(volume_id);

    // Pull the persisted last-scan facts from the status response (best-effort;
    // a not-indexed volume yields `None`s).
    let (scan_completed_at, scan_duration_ms) = get_status(volume_id)
        .ok()
        .and_then(|s| s.index_status)
        .map(|st| {
            (
                st.scan_completed_at.and_then(|v| v.parse::<u64>().ok()),
                st.scan_duration_ms.and_then(|v| v.parse::<u64>().ok()),
            )
        })
        .unwrap_or((None, None));

    VolumeIndexStatus {
        volume_id: volume_id.to_string(),
        enabled,
        freshness,
        failure: index_failure(volume_id),
        scan_completed_at,
        scan_duration_ms,
    }
}

/// Per-volume index status, resolving the owning volume from a path (the IPC
/// stays path-based, like `get_dir_stats`). Routes via
/// [`volume_id_for_local_path`]: SMB / MTP / a registered local external mount map
/// to their own index, so an external drive reports ITS status (`off` when
/// unindexed), not `root`'s. The boot disk and cloud-drive folders map to `root`.
pub fn get_volume_index_status_for_path(path: &str) -> VolumeIndexStatus {
    get_volume_index_status(&volume_id_for_local_path(path))
}

/// The empty/disabled status response (a volume with no running index).
fn disabled_status_response() -> IndexStatusResponse {
    IndexStatusResponse {
        initialized: false,
        scanning: false,
        entries_scanned: 0,
        dirs_found: 0,
        bytes_scanned: 0,
        index_status: None,
        db_file_size: None,
        volume_used_bytes: None,
    }
}

/// Get the current indexing status for a volume.
pub fn get_status(volume_id: &str) -> Result<IndexStatusResponse, String> {
    let reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get(volume_id).map(|i| &i.phase) {
        // A `Failed` volume reports the same not-scanning shape as disabled — its
        // distinct state rides `freshness: Failed` + `failure` on
        // `VolumeIndexStatus`, not this scan-progress response.
        None | Some(IndexPhase::ShuttingDown | IndexPhase::Failed { .. }) => Ok(disabled_status_response()),
        Some(IndexPhase::Initializing { store, .. }) => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            Ok(IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                bytes_scanned: 0,
                index_status,
                db_file_size,
                volume_used_bytes: None,
            })
        }
        Some(IndexPhase::Running(mgr)) => mgr.get_status(),
    }
}

/// Get extended debug status for the debug window.
pub fn get_debug_status(volume_id: &str) -> Result<IndexDebugStatusResponse, String> {
    let reg = INDEX_REGISTRY.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
    match reg.get(volume_id).map(|i| &i.phase) {
        None | Some(IndexPhase::ShuttingDown | IndexPhase::Failed { .. }) => {
            let base = disabled_status_response();
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
                huge_dirs_seen: DEBUG_STATS.huge_dirs_seen.load(Ordering::Relaxed),
                largest_dir_children: DEBUG_STATS.largest_dir_children.load(Ordering::Relaxed),
                verify_declined_dirs: DEBUG_STATS.verify_declined_dirs.load(Ordering::Relaxed),
                verify_truncated_dirs: DEBUG_STATS.verify_truncated_dirs.load(Ordering::Relaxed),
                db_main_size: None,
                db_wal_size: None,
                db_page_count: None,
                db_freelist_count: None,
            })
        }
        Some(IndexPhase::Initializing { store, .. }) => {
            let db_file_size = store.db_file_size().ok();
            let index_status = store.get_index_status().ok();
            let base = IndexStatusResponse {
                initialized: true,
                scanning: true,
                entries_scanned: 0,
                dirs_found: 0,
                bytes_scanned: 0,
                index_status,
                db_file_size,
                volume_used_bytes: None,
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
                huge_dirs_seen: DEBUG_STATS.huge_dirs_seen.load(Ordering::Relaxed),
                largest_dir_children: DEBUG_STATS.largest_dir_children.load(Ordering::Relaxed),
                verify_declined_dirs: DEBUG_STATS.verify_declined_dirs.load(Ordering::Relaxed),
                verify_truncated_dirs: DEBUG_STATS.verify_truncated_dirs.load(Ordering::Relaxed),
                db_main_size,
                db_wal_size,
                db_page_count,
                db_freelist_count,
            })
        }
        Some(IndexPhase::Running(mgr)) => mgr.get_debug_status(),
    }
}

/// Look up recursive stats for a single directory in a volume's index.
pub fn get_dir_stats_on_volume(volume_id: &str, path: &str) -> Result<Option<DirStats>, String> {
    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return Ok(None),
    };
    let normalized = firmlinks::normalize_path(path);
    // Map the mount-absolute path into the volume's index path space (no-op for
    // `root`, mount-relative for SMB). `None` ⇒ outside the mount ⇒ no stats.
    let index_path = match index_read_path(volume_id, &normalized) {
        Some(p) => p,
        None => return Ok(None),
    };

    pool.with_conn(|conn| {
        let entry_id =
            match store::resolve_path(conn, &index_path).map_err(|e| format!("Couldn't resolve path: {e}"))? {
                Some(id) => id,
                None => return Ok(None),
            };

        let stats =
            IndexStore::get_dir_stats_by_id(conn, entry_id).map_err(|e| format!("Couldn't get dir stats: {e}"))?;

        // Read the volume's current epoch on this same connection so the derived
        // honest-size booleans are consistent with the stats just read.
        let current_epoch = IndexStore::read_current_epoch(conn).unwrap_or(1);
        let pending = get_pending_sizes_for(volume_id).is_some_and(|t| t.is_pending(&normalized));
        Ok(stats.map(|s| dir_stats_from(normalized.clone(), &s, current_epoch, pending)))
    })?
}

/// Look up recursive stats for a single directory, resolving the owning volume
/// from the path. IPC stays path-based (see `commands/indexing.rs`); the volume
/// is resolved internally via [`volume_id_for_local_path`] (a registered external
/// mount routes to its own index; the boot disk to `root`).
pub fn get_dir_stats(path: &str) -> Result<Option<DirStats>, String> {
    get_dir_stats_on_volume(&volume_id_for_local_path(path), path)
}

/// Batch lookup of dir_stats for multiple paths on a volume.
pub fn get_dir_stats_batch_on_volume(volume_id: &str, paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return Ok(paths.iter().map(|_| None).collect()),
    };

    pool.with_conn(|conn| {
        let mut results = Vec::with_capacity(paths.len());
        let mut id_to_idx: Vec<(i64, usize, String)> = Vec::new();

        for (i, path) in paths.iter().enumerate() {
            let normalized = firmlinks::normalize_path(path);
            // Resolve in the volume's index path space (mount-relative for SMB),
            // but keep `normalized` (the mount-absolute path) as the returned
            // `path` and the pending-tracker key, since both are FE/write-side
            // keyed on the absolute path.
            let index_path = match index_read_path(volume_id, &normalized) {
                Some(p) => p,
                None => {
                    results.push(None);
                    continue;
                }
            };
            match store::resolve_path(conn, &index_path).map_err(|e| format!("Couldn't resolve path: {e}"))? {
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

            // Read the volume's current epoch once for the whole batch.
            let current_epoch = IndexStore::read_current_epoch(conn).unwrap_or(1);
            let tracker = get_pending_sizes_for(volume_id);
            for ((_, idx, normalized), stats_opt) in id_to_idx.into_iter().zip(stats_batch) {
                let pending = tracker.as_ref().is_some_and(|t| t.is_pending(&normalized));
                results[idx] = stats_opt.map(|s| dir_stats_from(normalized, &s, current_epoch, pending));
            }
        }

        Ok(results)
    })?
}

/// Build the path-keyed IPC `DirStats` from the integer-keyed stats, deriving
/// the FE-facing honest-size booleans (`recursive_size_complete` /
/// `recursive_size_stale`) from `min_subtree_epoch` vs `current_epoch`. Raw
/// epochs never cross IPC. Mirrors `enrichment::apply_dir_stats` for the
/// `FileEntry` read surface. See the "Honest sizes" model in DETAILS.
fn dir_stats_from(path: String, s: &store::DirStatsById, current_epoch: u64, pending: bool) -> DirStats {
    let complete = s.min_subtree_epoch > 0;
    DirStats {
        path,
        recursive_size: s.recursive_logical_size,
        recursive_physical_size: s.recursive_physical_size,
        recursive_file_count: s.recursive_file_count,
        recursive_dir_count: s.recursive_dir_count,
        recursive_has_symlinks: s.recursive_has_symlinks,
        recursive_size_pending: pending,
        recursive_size_complete: complete,
        recursive_size_stale: complete && s.min_subtree_epoch < current_epoch,
    }
}

/// List the immediate children of a directory from a volume's index (names,
/// folder/file, per-child size + mtime), resolving the owning volume from the
/// path. `Ok(None)` means the volume has no live index (no read pool) or the path
/// isn't in the index (unresolved) — the caller surfaces a typed "no index" /
/// "not found" rather than a misleading empty listing. Reads the index only; it
/// never touches the disk (so it's safe on a dead mount). Mirrors
/// [`get_dir_stats`]'s pool/resolve wiring.
pub fn list_dir_children(path: &str) -> Result<Option<Vec<store::EntryRow>>, String> {
    let volume_id = volume_id_for_local_path(path);
    let pool = match get_read_pool_for(&volume_id) {
        Some(p) => p,
        None => return Ok(None),
    };
    let normalized = firmlinks::normalize_path(path);
    let index_path = match index_read_path(&volume_id, &normalized) {
        Some(p) => p,
        None => return Ok(None),
    };
    pool.with_conn(|conn| {
        let entry_id =
            match store::resolve_path(conn, &index_path).map_err(|e| format!("Couldn't resolve path: {e}"))? {
                Some(id) => id,
                None => return Ok(None),
            };
        let children =
            IndexStore::list_children_on(entry_id, conn).map_err(|e| format!("Couldn't list children: {e}"))?;
        Ok(Some(children))
    })?
}

/// Batch lookup of dir_stats, resolving the owning volume from the paths. The
/// IPC `get_dir_stats_batch` sends one directory's children, which all live on
/// one volume; resolving from the first path is sufficient. Routes via
/// [`volume_id_for_local_path`] (a registered external mount → its own index; the
/// boot disk → `root`).
pub fn get_dir_stats_batch(paths: &[String]) -> Result<Vec<Option<DirStats>>, String> {
    let volume_id = paths
        .first()
        .map(|p| volume_id_for_local_path(p))
        .unwrap_or_else(|| ROOT_VOLUME_ID.to_string());
    get_dir_stats_batch_on_volume(&volume_id, paths)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;

    /// A mounted-but-unindexed external drive reports its OWN index status (`off` —
    /// no index registered under its id), not `root`'s. `cmdr://state`'s
    /// `indexStatus` reads through this resolution, so before the routing fix the
    /// drive inherited `root`'s id (and `root`'s freshness). The `volume_id` field
    /// is the proof the status now resolves to the drive, not `root`.
    #[test]
    fn volume_index_status_for_external_drive_resolves_to_the_drive_not_root() {
        #[cfg(target_os = "macos")]
        let ext_root = "/Volumes/StatusTestExt";
        #[cfg(not(target_os = "macos"))]
        let ext_root = "/media/StatusTestExt";

        let manager = get_volume_manager();
        let ext_id = "volumes-status-test-ext";
        manager.register(ext_id, Arc::new(LocalPosixVolume::new("Ext", ext_root)));

        let status = get_volume_index_status_for_path(&format!("{ext_root}/photos"));
        assert_eq!(status.volume_id, ext_id, "status resolves to the drive's own index id");
        // No index is registered for the drive, so it's off — not `root`'s freshness.
        assert!(
            !status.enabled,
            "an unindexed external drive reports off, not root's status"
        );
        assert!(status.freshness.is_none());

        manager.unregister(ext_id);
    }
}
