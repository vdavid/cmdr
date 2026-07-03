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

use super::expand_tilde;

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
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    // The unified transfer dialog routes every cross-device copy through
    // `copy_between_volumes`; this plain command is the same-`root` local path,
    // so no ejectable volume is involved (empty busy set).
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_copy_files_start(events, sources, destination, config, vec![], None).await
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
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let destination = PathBuf::from(expand_tilde(&destination));
    let config = config.unwrap_or_default();

    // Same-`root` local move (the FE uses `move_between_volumes` whenever the
    // source and destination volumes differ), so no ejectable volume here.
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_move_files_start(events, sources, destination, config, vec![], None).await
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
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let is_local = volume_id.as_deref().unwrap_or("root") == "root";
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };
    let config = config.unwrap_or_default();

    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_delete_files_start(events, sources, config, volume_id).await
}

/// Moves files to macOS Trash. Same events as `copy_files` but with `operationType: trash`.
#[tauri::command]
#[specta::specta]
pub async fn trash_files(
    app: tauri::AppHandle,
    sources: Vec<String>,
    item_sizes: Option<Vec<u64>>,
    config: Option<WriteOperationConfig>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let sources: Vec<PathBuf> = sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect();
    let config = config.unwrap_or_default();

    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    ops_trash_files_start(events, sources, item_sizes, config).await
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
) -> ScanPreviewStartResult {
    let volume_id = source_volume_id.unwrap_or_else(|| "root".to_string());
    let is_local = volume_id == "root";

    // Only expand tilde for local paths
    let sources: Vec<PathBuf> = if is_local {
        sources.iter().map(|s| PathBuf::from(expand_tilde(s))).collect()
    } else {
        sources.iter().map(PathBuf::from).collect()
    };

    let source_volume = if is_local {
        None
    } else {
        get_volume_manager().get(&volume_id)
    };

    let progress_interval = progress_interval_ms.unwrap_or(500);
    ops_start_scan_preview(
        app,
        sources,
        source_volume,
        volume_id,
        sort_column,
        sort_order,
        progress_interval,
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
