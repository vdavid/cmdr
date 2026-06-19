//! IPC commands for drive indexing.
//!
//! Thin wrappers around `indexing` module functions, exposed to the frontend via Tauri commands.

use tauri::AppHandle;

use crate::indexing::{
    self, IndexDebugStatusResponse, IndexStatusResponse, ROOT_VOLUME_ID, VolumeIndexStatus, store::DirStats,
};

// IPC stays path-based and single-volume in M1: the index-status, scan, and
// clear commands all act on the local-disk `root` index. The backend resolves
// the volume internally (here, the constant `root`), so the frontend and
// `bindings.ts` are unchanged. M2+ will widen these to carry a volume.

#[tauri::command]
#[specta::specta]
pub async fn start_drive_index(app: AppHandle) -> Result<(), String> {
    if indexing::is_active(ROOT_VOLUME_ID) {
        // Already running: force a fresh full scan (for example, from the debug "Start scan" button)
        indexing::force_scan(ROOT_VOLUME_ID)
    } else {
        indexing::start_indexing(&app)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn stop_drive_index() -> Result<(), String> {
    indexing::stop_scan(ROOT_VOLUME_ID)
}

#[tauri::command]
#[specta::specta]
pub async fn get_index_status() -> Result<IndexStatusResponse, String> {
    indexing::get_status(ROOT_VOLUME_ID)
}

#[tauri::command]
#[specta::specta]
pub async fn get_dir_stats(path: String) -> Result<Option<DirStats>, String> {
    indexing::get_dir_stats(&path)
}

#[tauri::command]
#[specta::specta]
pub async fn get_dir_stats_batch(paths: Vec<String>) -> Result<Vec<Option<DirStats>>, String> {
    indexing::get_dir_stats_batch(&paths)
}

#[tauri::command]
#[specta::specta]
pub async fn clear_drive_index() -> Result<(), String> {
    indexing::clear_index(ROOT_VOLUME_ID)
}

/// Extended debug status for the debug window (dev only).
#[tauri::command]
#[specta::specta]
pub async fn get_index_debug_status() -> Result<IndexDebugStatusResponse, String> {
    indexing::get_debug_status(ROOT_VOLUME_ID)
}

/// Per-volume index status for the freshness badge (M3's per-drive UX).
///
/// Returns the volume's freshness color plus the last completed scan's facts
/// (`scan_completed_at`, `scan_duration_ms`). Resolves the owning volume from
/// the path so the FE can pass a listing path; an SMB path maps to its SMB
/// volume id, everything else to `root`. A not-indexed volume reports
/// `enabled: false`, `freshness: None` (gray).
#[tauri::command]
#[specta::specta]
pub async fn get_volume_index_status(path: String) -> Result<VolumeIndexStatus, String> {
    Ok(indexing::get_volume_index_status_for_path(&path))
}

/// Toggle drive indexing on/off based on the user's setting.
#[tauri::command]
#[specta::specta]
pub async fn set_indexing_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        if !indexing::is_active(ROOT_VOLUME_ID) {
            indexing::start_indexing(&app)?;
        }
    } else {
        indexing::stop_indexing(ROOT_VOLUME_ID)?;
    }
    Ok(())
}

/// Apply the user's FDA decision: clear the gate, start the MTP watcher
/// (deferred at launch to avoid the MacDroid File Provider prompt during
/// onboarding), and start the indexer.
///
/// Three things happen at the gate boundary:
/// 1. Clear the FDA-pending atomic (`crate::fda_gate::set_fda_pending(false)`) so subsequent code
///    paths can run normally. The deny path runs in the same process; the allow path restarts the
///    app, which re-enters `setup()` and sets the atomic via the OS probe.
/// 2. Start the MTP hotplug watcher. MTP is opt-in per device; the watcher itself doesn't trigger
///    TCC.
/// 3. Start the drive indexer. On the Deny path this is what surfaces the "individual Allow/Deny
///    prompts" the user signed up for by denying FDA: the scan walks protected folders, macOS fires
///    one TCC popup per folder, the user grants or denies each. Folders that get denied stay
///    unindexed (size shows as `<dir>`); the rest get indexed normally.
///
/// **No proactive `volumes-changed` re-emission.** Emitting here would
/// refire every per-folder TCC prompt at once via NSWorkspace icon
/// resolution, on TOP of the per-folder prompts the indexer is already
/// generating. The sidebar keeps the icon-less favorites it got during
/// onboarding; the next listing-driven flow refreshes them naturally.
///
/// At app launch, indexing is skipped when the FDA choice is `NotAskedYet`
/// AND the OS reports FDA as not granted (see `should_auto_start_indexing`).
/// The frontend calls this command after the user clicks "Deny" so the
/// indexer starts within the same session. The "Allow" path needs no call:
/// the user restarts the app, and the launch-time gate passes via the OS
/// check.
///
/// Idempotent: a no-op when indexing is already running or initializing.
#[tauri::command]
#[specta::specta]
pub async fn start_indexing_after_fda_decision(app: AppHandle) -> Result<(), String> {
    crate::fda_gate::set_fda_pending(false);

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    crate::mtp::start_mtp_watcher(&app);

    if indexing::is_active(ROOT_VOLUME_ID) {
        return Ok(());
    }
    indexing::start_indexing(&app)
}
