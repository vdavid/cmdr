//! IPC commands for drive indexing.
//!
//! Thin wrappers around `indexing` module functions, exposed to the frontend via Tauri commands.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::indexing::SmbIndexGateReason;
use crate::indexing::{
    self, IndexDebugStatusResponse, IndexStatusResponse, ROOT_VOLUME_ID, VolumeIndexStatus, store::DirStats,
};

/// The outcome of a per-drive "Turn on indexing" request.
///
/// The typed REFUSAL (an SMB volume that needs a direct-smb2 upgrade which can't
/// complete) rides the `Ok` channel as a variant the FE classifies by tag, never
/// by message substring (`.claude/rules/no-string-matching.md`) — mirroring
/// `upgrade_to_smb_volume`'s `UpgradeResult`. A genuine internal failure (DB
/// open, manager spawn) is the command's `Err(String)` instead.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum EnableIndexingOutcome {
    /// Indexing started (a scan is now running or resuming) for the volume.
    Started,
    /// An SMB volume couldn't be indexed yet; `reason` says why (upgrade failed,
    /// credentials needed, disconnected). The FE shows an honest status and, for
    /// `credentials_needed`, can route into the reconnect/login flow.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    Refused { reason: SmbIndexGateReason },
}

// These path-based IPC commands act on the local-disk `root` index: the
// index-status, scan, and clear commands resolve the volume internally (here,
// the constant `root`), so the frontend and `bindings.ts` stay path-based. The
// per-drive (volume-carrying) commands live further down.

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

/// Per-volume index status for the freshness badge (the per-drive freshness UX).
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

/// Per-volume index status keyed by volume id (the per-drive badge surface).
///
/// The dropdown renders one badge per drive ROW, and the FE identifies drives by
/// `volume.id` (`"root"`, `smb-…`, `mtp-…`), not by a path. This is the id-keyed
/// sibling of `get_volume_index_status` (which takes a listing path for the
/// always-visible active-drive badge). Both return the same [`VolumeIndexStatus`]
/// shape; a not-indexed volume reports `enabled: false`, `freshness: None` (gray).
#[tauri::command]
#[specta::specta]
pub async fn get_volume_index_status_by_id(volume_id: String) -> Result<VolumeIndexStatus, String> {
    Ok(indexing::get_volume_index_status(&volume_id))
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

// ── Per-drive enable / disable / rescan (the per-drive badge menu) ───
//
// These are the typed, per-volume controls the freshness UX drives: "Turn on
// indexing for this drive", "Turn off indexing for this drive", and "Rescan
// now". Thin pass-throughs to the `indexing` module (smart backend / thin
// frontend). SMB enable is FDA-independent by design (network paths aren't
// TCC-protected) and triggers the direct-smb2 upgrade when needed, surfacing a
// TYPED `SmbIndexGateReason` on refusal.

/// Turn on indexing for a specific drive.
///
/// - `root` (local disk): starts the local indexer (same as `start_drive_index`,
///   FDA-gated at launch elsewhere; an explicit user enable here is honored).
/// - An SMB volume: gates on a direct smb2 connection, upgrading from `os_mount`
///   if needed, then scans over the `Volume` trait. A refusal (upgrade failed,
///   credentials needed, disconnected) returns `Refused { reason }` so the UI
///   classifies it by typed variant. FDA-independent.
///
/// Idempotent: a no-op (`Started`) if the drive's index is already active.
#[tauri::command]
#[specta::specta]
pub async fn enable_drive_index(app: AppHandle, volume_id: String) -> Result<EnableIndexingOutcome, String> {
    if indexing::is_active(&volume_id) {
        return Ok(EnableIndexingOutcome::Started);
    }

    if volume_id == ROOT_VOLUME_ID {
        indexing::start_indexing(&app)?;
        return Ok(EnableIndexingOutcome::Started);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // MTP: no connection-upgrade gate (one USB session, FDA-independent), so
        // it can't return a typed `SmbIndexGateReason`. Route it to the MTP enable
        // path; a plain string error (device not connected / internal start
        // failure) surfaces as a command error.
        if crate::mtp::identity::is_mtp_volume_id(&volume_id) {
            indexing::start_indexing_for_mtp(app, volume_id)?;
            return Ok(EnableIndexingOutcome::Started);
        }

        // Local external drive (USB stick, SD card, extra disk, mounted disk
        // image): the LOCAL jwalk + FSEvents pipeline, mount-rooted, with NO
        // connection gate (a local mount is already directly readable). Classify
        // by typed volume facts; a network mount (SMB os-mount, NFS, ...) is NOT
        // this branch and falls through to the SMB gate below. This is the branch
        // whose absence refused a healthy local drive as `NotAnSmbVolume`.
        match indexing::start_indexing_for_local_external(app.clone(), volume_id.clone()).await? {
            indexing::LocalExternalEnable::Started => return Ok(EnableIndexingOutcome::Started),
            indexing::LocalExternalEnable::NotLocalExternal => {}
        }

        // SMB: gate on the direct-smb2 connection. Kick mDNS first so a
        // freshly-typed server name resolves during the upgrade, then start. The
        // typed gate reason is the refusal surface for the UI.
        crate::network::ensure_mdns_started(app.clone());
        match indexing::start_indexing_for_smb(app, volume_id).await {
            Ok(()) => Ok(EnableIndexingOutcome::Started),
            Err(reason) => Ok(EnableIndexingOutcome::Refused { reason }),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = app;
        Err(format!(
            "Indexing for volume '{volume_id}' is not supported on this platform"
        ))
    }
}

/// Turn off indexing for a specific drive.
///
/// Stops the scan and watcher and removes the volume's registry instance (so its
/// badge goes gray / not-indexed), but PRESERVES the DB on disk, so re-enabling
/// can resume rather than rescan from scratch. Local `root` disable/enable still
/// works (don't break it). A no-op if the drive isn't indexed.
#[tauri::command]
#[specta::specta]
pub async fn disable_drive_index(volume_id: String) -> Result<(), String> {
    indexing::stop_indexing(&volume_id)
}

/// Forget a drive's index entirely: stop it, DELETE its index DB (plus WAL/SHM
/// sidecars), and drop its registry instance, so its badge goes gray and a
/// future enable does a clean fresh scan rather than resuming a stale DB.
///
/// This is the per-volume sibling of `clear_drive_index` (which is `root`-only):
/// the user-facing "forget this drive" action for an external (SMB/MTP) index
/// that's accumulating on disk. Unlike `disable_drive_index` (which preserves the
/// DB for a fast resume), forget reclaims the disk. A no-op if not indexed. Since
/// removal drops the instance, a Stale badge transitions to gray (not a dangling
/// Stale) automatically — `get_freshness` returns `None` once the key is gone.
#[tauri::command]
#[specta::specta]
pub async fn forget_drive_index(volume_id: String) -> Result<(), String> {
    indexing::clear_index(&volume_id)
}

/// Force a fresh full rescan of a drive (the menu's "Rescan now").
///
/// - An ALREADY-active drive: kicks off a fresh full scan (Stale ⇒ Scanning ⇒
///   Fresh on clean completion), truncating and rebuilding its index.
/// - An SMB drive that's NOT active (e.g. a persisted Stale index loaded on
///   launch but never re-enabled this session): enable it, which scans. Returns
///   the typed refusal if the direct-smb2 gate blocks it.
/// - `root` that's not active: starts the local indexer.
#[tauri::command]
#[specta::specta]
pub async fn rescan_drive_index(app: AppHandle, volume_id: String) -> Result<EnableIndexingOutcome, String> {
    if indexing::is_active(&volume_id) {
        indexing::force_scan(&volume_id)?;
        return Ok(EnableIndexingOutcome::Started);
    }
    // Not active: enabling is what triggers the (first) scan.
    enable_drive_index(app, volume_id).await
}

// ── App handle for handle-free callers (the MCP `indexing` tool) ─────
//
// `enable`/`rescan` need a concrete `AppHandle` (they spawn the indexer and emit
// events), but the MCP tool executor is generic over `Runtime` and can't supply
// one. So we stash the concrete handle at startup and expose handle-free
// wrappers, mirroring the `upgrade_to_smb_volume_inner` / `space_poller`
// pattern. `disable`/`forget` need no handle and are called directly.

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Cache the concrete `AppHandle` for handle-free callers. Called once from
/// `setup()`.
pub fn set_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

fn app_handle() -> Result<AppHandle, String> {
    APP_HANDLE
        .get()
        .cloned()
        .ok_or_else(|| "Indexing app handle isn't ready yet".to_string())
}

/// Handle-free `enable_drive_index` for the MCP `indexing` tool.
pub async fn enable_drive_index_via_handle(volume_id: String) -> Result<EnableIndexingOutcome, String> {
    enable_drive_index(app_handle()?, volume_id).await
}

/// Handle-free `rescan_drive_index` for the MCP `indexing` tool.
pub async fn rescan_drive_index_via_handle(volume_id: String) -> Result<EnableIndexingOutcome, String> {
    rescan_drive_index(app_handle()?, volume_id).await
}
