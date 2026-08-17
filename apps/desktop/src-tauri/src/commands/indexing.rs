//! IPC commands for drive indexing.
//!
//! Thin wrappers around `indexing` module functions, exposed to the frontend via Tauri commands.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::index_host::index;
use cmdr_index::{
    IndexDebugStatusResponse, IndexStatusResponse, ROOT_VOLUME_ID, SmbIndexGateReason, StartOutcome, VolumeIndexStatus,
    store::DirStats,
};

/// The outcome of a per-drive "Turn on indexing" request.
///
/// The typed REFUSAL (an SMB volume that needs a direct-smb2 upgrade which can't
/// complete) rides the `Ok` channel as a variant the FE classifies by tag, never
/// by message substring — mirroring
/// `upgrade_to_smb_volume`'s `UpgradeResult`. A genuine internal failure (DB
/// open, manager spawn) is the command's `Err(String)` instead.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum EnableIndexingOutcome {
    /// Indexing started (a scan is now running or resuming) for the volume.
    Started,
    /// A search is walking this drive right now, so the full walk can't run yet.
    /// The index remembers the request and runs it when the walk ends, so the FE
    /// says "soon", ❌ never "nothing happened" — this is the variant that stops
    /// "Rescan now" from looking like a dead button.
    DeferredUntilSearchEnds,
    /// The same promise with the drive held by a full walk of its own (a scan, or
    /// the journal replay a launch does). The queued walk runs when that one ends,
    /// so the FE says the drive is already being indexed and this one is next.
    DeferredUntilScanEnds,
    /// The master drive-indexing switch is off, so no drive may index. Transport-
    /// neutral (the master switch outranks every per-transport gate), so the FE
    /// gets ONE shape to recognize whichever drive was asked for.
    IndexingDisabled,
    /// An SMB volume couldn't be indexed yet; `reason` says why (upgrade failed,
    /// credentials needed, disconnected). The FE shows an honest status and, for
    /// `credentials_needed`, can route into the reconnect/login flow.
    Refused { reason: SmbIndexGateReason },
}

impl From<StartOutcome> for EnableIndexingOutcome {
    /// The index's typed outcome becomes the frontend's, which is the only place
    /// the two shapes meet.
    fn from(outcome: StartOutcome) -> Self {
        match outcome {
            StartOutcome::Started => Self::Started,
            StartOutcome::DeferredUntilSearchEnds => Self::DeferredUntilSearchEnds,
            StartOutcome::DeferredUntilScanEnds => Self::DeferredUntilScanEnds,
            StartOutcome::IndexingDisabled => Self::IndexingDisabled,
            StartOutcome::Refused(reason) => Self::Refused { reason },
        }
    }
}

// These path-based IPC commands act on the local-disk `root` index: the
// index-status, scan, and clear commands resolve the volume internally (here,
// the constant `root`), so the frontend and `bindings.ts` stay path-based. The
// per-drive (volume-carrying) commands live further down.

#[tauri::command]
#[specta::specta]
pub async fn start_drive_index() -> Result<(), String> {
    // Already running: force a fresh full scan (for example, from the debug
    // "Start scan" button). Not running: the first scan IS the rescan.
    index()
        .rescan_volume(ROOT_VOLUME_ID)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn stop_drive_index() -> Result<(), String> {
    index().stop_scan(ROOT_VOLUME_ID).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_index_status() -> Result<IndexStatusResponse, String> {
    index().status(ROOT_VOLUME_ID).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_dir_stats(path: String) -> Result<Option<DirStats>, String> {
    index().dir_stats(&path).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_dir_stats_batch(paths: Vec<String>) -> Result<Vec<Option<DirStats>>, String> {
    index().dir_stats_batch(&paths).map_err(|e| e.to_string())
}

/// How much disk every drive's index takes up, in bytes, WAL sidecars included.
///
/// Deliberately not `get_index_status().db_file_size`, which reports the boot
/// disk's live instance and therefore reports nothing at all on a machine where
/// drive indexing is off — the machine most likely to have accumulated index
/// databases it never asked for, since a search walks whatever folder it's
/// pointed at. `0` means there's nothing on disk to clear.
#[tauri::command]
#[specta::specta]
pub async fn get_index_disk_usage() -> Result<u64, String> {
    Ok(index().disk_footprint())
}

/// Delete every drive's index (the settings screen's "Clear index").
///
/// Every volume, not just `root`: a search walks the drive it's pointed at, so
/// the disk this reclaims can belong to a share or an external drive the user
/// never turned indexing on for. Per-drive clearing has its own action
/// ([`forget_drive_index`], from the drive's badge menu).
#[tauri::command]
#[specta::specta]
pub async fn clear_drive_index() -> Result<(), String> {
    // Draining a running volume's writer blocks for up to five seconds each, so
    // it goes on a blocking thread rather than the IPC handler's.
    tauri::async_runtime::spawn_blocking(|| index().forget_all_volumes().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("Clearing the index didn't finish: {e}"))?
}

/// Extended debug status for the debug window (dev only).
#[tauri::command]
#[specta::specta]
pub async fn get_index_debug_status() -> Result<IndexDebugStatusResponse, String> {
    index().debug_status(ROOT_VOLUME_ID).map_err(|e| e.to_string())
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
    Ok(index().volume_status_for_path(&path))
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
    Ok(index().volume_status(&volume_id))
}

/// Apply the master drive-indexing switch (`indexing.enabled`), live.
///
/// The master switch gates EVERY drive, not only the boot disk: off stops every
/// running index (an SMB/MTP/local-external one included) and blocks every later
/// start, including the autonomous SMB reconnect resume. On restores the drives
/// whose PER-DRIVE intent says they should index, and only those: a drive the user
/// never turned on, or explicitly turned off, stays off. Neither direction writes
/// per-drive intent, so the choice survives any number of master toggles.
#[tauri::command]
#[specta::specta]
pub async fn set_indexing_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    // Move the gate FIRST in both directions: on, so the starts below pass it; off,
    // so a concurrent reconnect resume can't slip in behind the stop sweep.
    index().set_indexing_enabled(enabled);
    if enabled {
        for volume_id in index().drives_to_resume() {
            // Each drive routes through the normal per-drive enable, so its own gate
            // (the direct-smb2 upgrade, MTP device presence) still applies. A refusal
            // is expected here (a share that's offline right now) and only logged;
            // the reconnect resume picks it up when the drive comes back.
            match enable_drive_index(app.clone(), volume_id.clone()).await {
                Ok(EnableIndexingOutcome::Started) => {}
                Ok(other) => log::info!("set_indexing_enabled: '{volume_id}' not resumed: {other:?}"),
                Err(e) => log::warn!("set_indexing_enabled: resuming '{volume_id}' failed: {e}"),
            }
        }
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

    index()
        .start_volume(ROOT_VOLUME_ID)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
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
    // Kick mDNS first so a freshly-typed server name resolves during a share's
    // direct-session upgrade. Idempotent, and cheap enough not to branch on the
    // volume's kind (which is the index's business, not this command's).
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if volume_id != ROOT_VOLUME_ID {
        crate::network::ensure_mdns_started(app.clone());
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = &app;

    index()
        .start_volume(&volume_id)
        .await
        .map(EnableIndexingOutcome::from)
        .map_err(|e| e.to_string())
}

/// Turn off indexing for a specific drive.
///
/// Stops the scan and watcher and removes the volume's registry instance (so its
/// badge goes gray / not-indexed), but PRESERVES the DB on disk, so re-enabling
/// can resume rather than rescan from scratch. Also persists a sticky
/// `user_disabled` marker so a later SMB reconnect doesn't auto-resume what the
/// user turned off (`disable_drive_index_persist_intent`); re-enabling clears it,
/// "Forget this drive" deletes the whole DB. Local `root` disable/enable still
/// works (don't break it). A no-op if the drive isn't indexed.
#[tauri::command]
#[specta::specta]
pub async fn disable_drive_index(volume_id: String) -> Result<(), String> {
    index().disable_volume(&volume_id).map_err(|e| e.to_string())
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
    index().forget_volume(&volume_id).map_err(|e| e.to_string())
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
    // Not active: enabling is what triggers the (first) scan, so this is the same
    // call either way.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if volume_id != ROOT_VOLUME_ID {
        crate::network::ensure_mdns_started(app.clone());
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = &app;

    index()
        .rescan_volume(&volume_id)
        .await
        .map(EnableIndexingOutcome::from)
        .map_err(|e| e.to_string())
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
