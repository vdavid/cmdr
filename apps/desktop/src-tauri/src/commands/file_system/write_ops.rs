//! Tauri commands for write operations (create, copy, move, delete, trash) and scan preview.

use crate::file_system::write_operations::{
    ConflictResolution, ScanPreviewStartResult, cancel_scan_preview as ops_cancel_scan_preview,
    create_directory_managed as ops_create_directory_managed, create_file_managed as ops_create_file_managed,
    get_scan_preview_totals as ops_get_scan_preview_totals, resolve_write_conflict as ops_resolve_write_conflict,
    start_scan_preview as ops_start_scan_preview,
};
use crate::file_system::{
    OperationEventSink, OperationSnapshot, OperationStatus, OperationSummary, SortColumn, SortOrder, TauriEventSink,
    WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
    cancel_all_write_operations as ops_cancel_all_write_operations, cancel_operation as ops_cancel_operation,
    cancel_operations as ops_cancel_operations, cancel_write_operation as ops_cancel_write_operation,
    copy_files_start as ops_copy_files_start, delete_files_start as ops_delete_files_start,
    get_operation_status as ops_get_operation_status, get_volume_manager,
    list_active_operations as ops_list_active_operations, list_operations as ops_list_operations,
    move_files_start as ops_move_files_start, pause_all as ops_pause_all, pause_operation as ops_pause_operation,
    resume_all as ops_resume_all, resume_operation as ops_resume_operation, trash_files_start as ops_trash_files_start,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Duration;

use crate::commands::util::IpcError;
use crate::operation_log::types::Initiator;
use crate::file_system::Volume;
use crate::file_system::volume::backends::archive;

use super::expand_tilde;

/// Picks the source volume for a scan preview.
///
/// Routes an archive-inner source to its `ArchiveVolume` so the preview scan reads
/// entries from INSIDE the zip. Without this, a `.zip`-crossing source (whose
/// display volume id is the local parent drive, `"root"`) would take the local
/// `std::fs` scan path and find 0 files (inner paths aren't real FS paths), cache
/// a 0-file preview, and the copy would reuse it — the extract-out "cancelled
/// after 0 files" stall. `resolve` does no I/O unless a path component carries a
/// `.zip` extension, so ordinary local/remote scans are unaffected.
///
/// `None` means "scan the local filesystem directly" (the `std::fs` fast path);
/// `Some` means scan through the `Volume` trait.
async fn scan_preview_source_volume(volume_id: &str, first_source: Option<&PathBuf>) -> Option<Arc<dyn Volume>> {
    // Route to the ArchiveVolume only for a source INSIDE the archive. The `.zip`
    // file itself is scanned as a plain file (one entry), not its contents. Only a
    // non-empty inner component can be archive-inner; the pure string pre-filter
    // gates the parent-aware resolve, which confirms a REMOTE zip too.
    let is_inner_candidate = first_source
        .and_then(|first| archive::archive_boundary_candidate(first))
        .is_some_and(|(_zip, inner)| !inner.as_os_str().is_empty());
    let archive_source = if is_inner_candidate {
        let resolved = get_volume_manager()
            .resolve(volume_id, first_source.expect("candidate implies a source"))
            .await;
        // `is_archive` gates whether we actually got the ArchiveVolume (a mislabeled
        // `.zip` falls through to the parent, which the branches below handle).
        resolved.is_archive.then_some(resolved.volume).flatten()
    } else {
        None
    };
    if archive_source.is_some() {
        archive_source
    } else if volume_id == "root" {
        None
    } else {
        get_volume_manager().get(volume_id)
    }
}

/// Rejects a local write op that touches a path INSIDE an archive. Archives are
/// read-only until zip mutation lands, and the real extract-out path goes through
/// `copy_between_volumes` (which routes to the `ArchiveVolume`), never this local
/// `std::fs` fast-path. A backend safety net behind the frontend's read-only
/// capability gating; this seam turns into archive-edit routing when mutation
/// lands.
fn reject_if_archive_inner<'a>(paths: impl IntoIterator<Item = &'a PathBuf>) -> Result<(), WriteOperationError> {
    for path in paths {
        // Only a path INSIDE an archive is read-only. The `.zip` file itself is a
        // regular file — copying/moving/deleting/trashing it must work.
        if archive::path_is_inside_archive(path) {
            return Err(WriteOperationError::ReadOnlyDevice {
                path: path.to_string_lossy().into_owned(),
                device_name: None,
            });
        }
    }
    Ok(())
}

/// Creates a folder and returns its new path. Thin pass-through to the managed
/// create op (`write_operations::create`): expand tilde (root only), wrap in the
/// 5 s write timeout, map to `IpcError`.
#[tauri::command]
#[specta::specta]
pub async fn create_directory(
    volume_id: Option<String>,
    parent_path: String,
    name: String,
) -> Result<String, IpcError> {
    let expanded_parent = expand_parent(volume_id.as_deref(), &parent_path);
    tokio::time::timeout(
        Duration::from_secs(5),
        ops_create_directory_managed(volume_id, expanded_parent, name),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(IpcError::from_err)
}

/// Creates an empty file and returns its new path. Same shape as
/// [`create_directory`].
#[tauri::command]
#[specta::specta]
pub async fn create_file(volume_id: Option<String>, parent_path: String, name: String) -> Result<String, IpcError> {
    let expanded_parent = expand_parent(volume_id.as_deref(), &parent_path);
    tokio::time::timeout(
        Duration::from_secs(5),
        ops_create_file_managed(volume_id, expanded_parent, name),
    )
    .await
    .map_err(|_| IpcError::timeout())?
    .map_err(IpcError::from_err)
}

/// Expands tilde for local (`root`) parents only; volume paths are
/// volume-relative and must never be tilde-expanded.
fn expand_parent(volume_id: Option<&str>, parent_path: &str) -> String {
    if volume_id.unwrap_or("root") == "root" {
        expand_tilde(parent_path)
    } else {
        parent_path.to_string()
    }
}

// ============================================================================
// Write operations (copy, move, delete)
// ============================================================================

/// Emits write-progress, write-complete, write-error, write-cancelled.
#[tauri::command]
#[specta::specta]
pub async fn copy_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    // A copy INTO or OUT of an archive doesn't belong on the local fast-path
    // (extract-out routes through `copy_between_volumes`; write-in is read-only).
    reject_if_archive_inner(sources.iter().chain(std::iter::once(&destination)))?;

    // The unified transfer dialog routes every cross-device copy through
    // `copy_between_volumes`; this plain command is the same-`root` local path,
    // so no ejectable volume is involved (empty busy set).
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_copy_files_start(events, sources, destination, config, vec![], None, initiator.unwrap_or(Initiator::User)).await
}

/// Uses rename() for same-filesystem (instant), copy+delete for cross-filesystem.
/// Same events as `copy_files`.
#[tauri::command]
#[specta::specta]
pub async fn move_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    destination: String,
    config: Option<WriteOperationConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    // A move touching an archive doesn't belong on the local fast-path (moving
    // into or out of a zip is read-only until mutation lands).
    reject_if_archive_inner(sources.iter().chain(std::iter::once(&destination)))?;

    // Same-`root` local move (the FE uses `move_between_volumes` whenever the
    // source and destination volumes differ), so no ejectable volume here.
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_move_files_start(events, sources, destination, config, vec![], None, initiator.unwrap_or(Initiator::User)).await
}

/// Recursively deletes files and directories. Same events as `copy_files`.
/// When `volume_id` is provided and is not "root", routes through the Volume trait.
#[tauri::command]
#[specta::specta]
pub async fn delete_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    volume_id: Option<String>,
    config: Option<WriteOperationConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let is_local = volume_id.as_deref().unwrap_or("root") == "root";
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };
    let config = config.unwrap_or_default();

    // Deleting an entry INSIDE an archive routes to the managed archive-edit
    // driver inside `delete_files_start` (a `{ delete }` changeset), so no
    // rejection here. The `.zip` file itself deletes on the normal path.
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_delete_files_start(events, sources, config, volume_id, initiator.unwrap_or(Initiator::User)).await
}

/// Moves files to macOS Trash. Same events as `copy_files` but with `operationType: trash`.
#[tauri::command]
#[specta::specta]
pub async fn trash_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    item_sizes: Option<Vec<u64>>,
    config: Option<WriteOperationConfig>,
    initiator: Option<Initiator>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let config = config.unwrap_or_default();

    // Trashing an entry inside an archive is a mutation (read-only for now).
    reject_if_archive_inner(sources.iter())?;

    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_trash_files_start(events, sources, item_sizes, config, initiator.unwrap_or(Initiator::User)).await
}

#[tauri::command]
#[specta::specta]
pub fn cancel_write_operation(operation_id: String, rollback: bool) {
    ops_cancel_write_operation(&operation_id, rollback);
}

#[tauri::command]
#[specta::specta]
pub fn cancel_all_write_operations() {
    ops_cancel_all_write_operations();
}

// ============================================================================
// Scan preview (for Copy dialog live stats)
// ============================================================================

/// Scans source files for Copy dialog stats. Results are cached for reuse by the actual copy.
/// Emits scan-preview-progress, scan-preview-complete, scan-preview-error, scan-preview-cancelled.
///
/// When `source_volume_id` is provided and is not "root", the scan uses the Volume trait
/// (enabling MTP and other non-local volumes). Otherwise, uses `std::fs` for local scanning.
#[tauri::command]
#[specta::specta]
pub async fn start_scan_preview(
    app: tauri::AppHandle,
    sources: Vec<String>,
    source_volume_id: Option<String>,
    sort_column: SortColumn,
    sort_order: SortOrder,
    progress_interval_ms: Option<u64>,
    // Compress-mode scans set this so the local walk samples a compressed-size
    // estimate. Ignored for remote sources (never sampled). `None` == false.
    sample_for_estimate: Option<bool>,
) -> ScanPreviewStartResult {
    let volume_id = source_volume_id.unwrap_or_else(|| "root".to_string());
    let is_local = volume_id == "root";

    // Only expand tilde for local paths
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };

    let source_volume = scan_preview_source_volume(&volume_id, sources.first()).await;

    let progress_interval = progress_interval_ms.unwrap_or(500);
    ops_start_scan_preview(
        app,
        sources,
        source_volume,
        volume_id,
        sort_column,
        sort_order,
        progress_interval,
        sample_for_estimate.unwrap_or(false),
    )
}

#[tauri::command]
#[specta::specta]
pub fn cancel_scan_preview(preview_id: String) {
    ops_cancel_scan_preview(&preview_id);
}

/// Returns the cached totals from a completed scan preview, or `null` while the
/// scan is still running / cancelled / errored. The FE uses the presence of a
/// value both as a "scan done" signal and to repopulate display state when its
/// listeners missed the events (a watcher-backed oracle can finish before the
/// FE finishes the `startScanPreview()` IPC round-trip).
#[tauri::command]
#[specta::specta]
pub fn check_scan_preview_status(
    preview_id: String,
) -> Option<crate::file_system::write_operations::ScanPreviewTotals> {
    ops_get_scan_preview_totals(&preview_id)
}

/// In Stop mode, the operation pauses on conflict and waits for this call to proceed.
#[tauri::command]
#[specta::specta]
pub fn resolve_write_conflict(operation_id: String, resolution: ConflictResolution, apply_to_all: bool) {
    ops_resolve_write_conflict(&operation_id, resolution, apply_to_all);
}

#[tauri::command]
#[specta::specta]
pub fn list_active_operations() -> Vec<OperationSummary> {
    ops_list_active_operations()
}

#[tauri::command]
#[specta::specta]
pub fn get_operation_status(operation_id: String) -> Option<OperationStatus> {
    ops_get_operation_status(&operation_id)
}

// ============================================================================
// Operation manager (queue + lifecycle)
// ============================================================================

/// Returns the thin operation registry snapshot (membership + lifecycle
/// status) for the queue window. Live per-row progress comes from the separate
/// `write-progress` stream; this snapshot stays thin.
#[tauri::command]
#[specta::specta]
pub fn list_operations() -> Vec<OperationSnapshot> {
    ops_list_operations()
}

/// Cancels one operation, keeping already-copied files. A Queued op is dropped
/// without ever spawning; a Running/Paused op routes through the existing
/// keep-partials cancel path.
#[tauri::command]
#[specta::specta]
pub fn cancel_operation(operation_id: String) {
    ops_cancel_operation(&operation_id);
}

/// Cancels several operations (keep-partials each). Backs the queue window's
/// "Cancel selected".
#[tauri::command]
#[specta::specta]
pub fn cancel_operations(operation_ids: Vec<String>) {
    ops_cancel_operations(&operation_ids);
}

/// Pauses one Running operation. It parks at the next between-files boundary and
/// its lifecycle status flips to `paused` in `operations-changed`. A paused op
/// keeps holding its lane slots. Pausing a Queued/Done op is a no-op.
#[tauri::command]
#[specta::specta]
pub fn pause_operation(operation_id: String) {
    ops_pause_operation(&operation_id);
}

/// Resumes one paused operation: it continues from where it parked and its
/// status flips back to `running`. Resuming a non-paused op is a no-op.
#[tauri::command]
#[specta::specta]
pub fn resume_operation(operation_id: String) {
    ops_resume_operation(&operation_id);
}

/// Pauses every currently-running operation. Backs the queue window's global
/// "Pause all".
#[tauri::command]
#[specta::specta]
pub fn pause_all() {
    ops_pause_all();
}

/// Resumes every currently-paused operation. Backs "Resume all".
#[tauri::command]
#[specta::specta]
pub fn resume_all() {
    ops_resume_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_if_archive_inner_flags_a_path_inside_a_zip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        // A zip start-of-file signature is enough for the boundary magic check.
        std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");

        // A path INSIDE the archive is refused with a typed read-only error...
        let inner = zip.join("inner.txt");
        let err = reject_if_archive_inner(std::iter::once(&inner)).expect_err("archive-inner path must be refused");
        assert!(
            matches!(err, WriteOperationError::ReadOnlyDevice { .. }),
            "expected ReadOnlyDevice, got {err:?}"
        );

        // ...while a plain local sibling passes (proves the guard, not a blanket reject).
        let plain = dir.path().join("plain.txt");
        assert!(reject_if_archive_inner(std::iter::once(&plain)).is_ok());

        // ...AND the `.zip` FILE ITSELF passes: copying/moving/deleting/trashing a
        // zip file is a normal file op, not a write INSIDE the archive.
        assert!(
            reject_if_archive_inner(std::iter::once(&zip)).is_ok(),
            "the .zip file itself must not be refused"
        );
    }

    #[tokio::test]
    async fn scan_preview_routes_an_archive_source_to_the_archive_volume() {
        use crate::file_system::volume::InMemoryVolume;

        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04rest").expect("write zip magic");

        // resolve needs the parent drive registered to build the ArchiveVolume.
        // The `.zip` is a real temp file, so the parent is LOCAL (std::fs confirm).
        // (nextest runs each test in its own process, so this global is isolated.)
        get_volume_manager().register("root", Arc::new(InMemoryVolume::new("Root").with_local_fs_access()));

        // An archive-inner source resolves to the ArchiveVolume (its root() is the
        // `.zip`), so the preview scans INSIDE the zip instead of via `std::fs`
        // (which would find 0 files and stall extract-out).
        let inner = zip.join("inner.txt");
        let source = scan_preview_source_volume("root", Some(&inner))
            .await
            .expect("archive source volume");
        assert_eq!(source.root(), zip);

        // A plain local source stays `None` — the `std::fs` fast path.
        let plain = dir.path().join("plain.txt");
        assert!(scan_preview_source_volume("root", Some(&plain)).await.is_none());
    }
}
