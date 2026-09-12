//! Tauri commands for cross-volume copy/move operations.
//!
//! These are pass-throughs: they build the `TauriEventSink` at the edge and hand
//! the whole routed transfer to `write_operations::routing`, which owns the volume
//! and destination-path resolution and the archive forks. That split is what lets
//! a backend caller start the same transfer with its own injected sink.

use crate::file_system::{
    CONFLICT_CHECK_BUDGET, OperationEventSink, ScanConflict, SourceItemInput, TauriEventSink, VolumeCopyConfig,
    VolumeCopyScanResult, VolumeScanError, WriteOperationError, WriteOperationStartResult, resolve_dest_path,
    resolve_source_volume, scan_for_volume_copy as ops_scan_for_volume_copy, scan_volume_for_conflicts_within,
    start_volume_compress, start_volume_copy, start_volume_move,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::time::Duration;

use crate::commands::util::{Deadline, timeout_detached_typed};
use crate::file_system::volume::manager::get_volume_manager;
use crate::operation_log::types::Initiator;

/// Unified copy across volume types (local, MTP, extract out of a `.zip`).
/// Same events as `copy_files`.
#[tauri::command]
#[specta::specta]
pub async fn copy_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    start_volume_copy(
        events,
        source_volume_id,
        source_paths.iter().map(PathBuf::from).collect(),
        dest_volume_id,
        dest_path,
        config.unwrap_or_default(),
        initiator.unwrap_or(Initiator::User),
        // No source binding: the user picked these in the pane they are looking at.
        None,
    )
    .await
}

/// Unified move across volume types. Handles same-volume (native rename/move),
/// both-local (native move), cross-volume (copy+delete), and both directions
/// across a `.zip` boundary.
#[tauri::command]
#[specta::specta]
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    start_volume_move(
        events,
        source_volume_id,
        source_paths.iter().map(PathBuf::from).collect(),
        dest_volume_id,
        dest_path,
        config.unwrap_or_default(),
        initiator.unwrap_or(Initiator::User),
        // No source binding: the user picked these in the pane they are looking at.
        None,
    )
    .await
}

/// Compresses `source_paths` into a NEW zip at `dest_zip_path` on `dest_volume_id`.
/// Same events as `copy_between_volumes`. The destination may be LOCAL or REMOTE
/// (SMB/MTP).
#[tauri::command]
#[specta::specta]
pub async fn compress_files(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_zip_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    start_volume_compress(
        events,
        source_volume_id,
        source_paths.iter().map(PathBuf::from).collect(),
        dest_volume_id,
        dest_zip_path,
        config.unwrap_or_default(),
        initiator.unwrap_or(Initiator::User),
    )
    .await
}

/// Pre-flight scan: total count/bytes, available space, conflicts. Doesn't copy anything.
#[tauri::command]
#[specta::specta]
pub async fn scan_volume_for_copy(
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    max_conflicts: Option<usize>,
) -> Result<VolumeCopyScanResult, VolumeScanError> {
    let source_paths: Vec<PathBuf> = source_paths.iter().map(PathBuf::from).collect();
    let dest_path = PathBuf::from(dest_path);

    // Resolve both so an archive-inner source scans through its ArchiveVolume
    // (sizing an extract-out) and the dest routes consistently with the copy op.
    let Some((source_volume, _)) = resolve_source_volume(&source_volume_id, source_paths.first()).await else {
        return Err(VolumeScanError::source_missing(source_volume_id).await);
    };

    let Some(dest_volume) = get_volume_manager().resolve(&dest_volume_id, &dest_path).await.volume else {
        return Err(VolumeScanError::destination_missing(dest_volume_id).await);
    };

    let max_conflicts = max_conflicts.unwrap_or(100);
    // Same anchoring the copy op applies, so the scan sizes and counts conflicts
    // at the folder the copy will actually write to.
    let dest_path = resolve_dest_path(&dest_volume, dest_path.to_string_lossy().into_owned());

    // Run scan (now async). Detached: a copy scan of an MTP source is a recursive
    // listing that outlives 30 s on any photo-heavy folder, and dropping it
    // mid-`GetObjectInfo` wedges the phone.
    timeout_detached_typed(
        Duration::from_secs(30),
        || VolumeScanError::TimedOut,
        |detail| VolumeScanError::Unexpected { detail },
        async move {
            ops_scan_for_volume_copy(&*source_volume, &source_paths, &*dest_volume, &dest_path, max_conflicts)
                .await
                .map_err(|error| VolumeScanError::Volume { error })
        },
    )
    .await
}

/// Whether the transfer dialog's destination folder takes writes, for the notice
/// under its path box. Resolved and anchored the way the copy op does it
/// (`resolve_dest_path` expands a local `~`), and bounded: a volume that isn't
/// registered (a phone nobody dialed) or doesn't answer in 2 s is `Unknown`, which
/// shows nothing. ❌ Never a refusal of its own: the transfer asks again before it
/// writes, and that answer is the one that refuses.
#[tauri::command]
#[specta::specta]
pub async fn destination_write_access(dest_volume_id: String, dest_path: String) -> cmdr_fs::volume::WriteAccess {
    use cmdr_fs::volume::WriteAccess;

    let Some(dest_volume) = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_path))
        .await
        .volume
    else {
        return WriteAccess::Unknown;
    };
    let dest_path = resolve_dest_path(&dest_volume, dest_path);
    // Detached, like the scan: dropping a probe mid-round-trip wedges an MTP phone.
    timeout_detached_typed(
        Duration::from_secs(2),
        || WriteAccess::Unknown,
        |_| WriteAccess::Unknown,
        async move { Ok::<_, WriteAccess>(dest_volume.write_access_at(&dest_path).await) },
    )
    .await
    .unwrap_or_else(|unknown| unknown)
}

/// Checks which source items already exist at the destination. Returns conflict details for UI.
///
/// When `source_volume_id` and `source_paths` are both provided, each item's
/// `is_directory` and `size` are resolved authoritatively on the source volume
/// via one `get_metadata` per top-level path (see `stat_source_paths`: strictly
/// O(top-level items), never a subtree walk), overriding whatever the caller
/// passed in `source_items`. This lets the dialog classify dir-vs-dir collisions
/// as silent merges without the FE having to plumb per-item types. Callers that
/// don't pass the source volume keep the legacy name-only behavior.
///
/// A thin wrapper: the budgeted scan itself is
/// `write_operations::conflict_preflight::scan_volume_for_conflicts_within`,
/// which is what a test hands a budget it can wait out.
#[tauri::command]
#[specta::specta]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
    source_volume_id: Option<String>,
    source_paths: Option<Vec<String>>,
) -> Result<Vec<ScanConflict>, VolumeScanError> {
    scan_volume_for_conflicts_within(
        Deadline::new(CONFLICT_CHECK_BUDGET),
        volume_id,
        source_items,
        dest_path,
        source_volume_id,
        source_paths,
    )
    .await
}
