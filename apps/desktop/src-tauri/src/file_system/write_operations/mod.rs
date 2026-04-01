//! Write operations (copy, move, delete) with streaming progress.
//!
//! All operations run in background tasks and emit progress events at configurable intervals.
//! Operations support batch processing (multiple source files) and cancellation.
//!
//! Safety features:
//! - Path canonicalization to prevent ".." and symlink bypass of recursion checks
//! - Destination writability check before starting
//! - Pre-flight disk space validation after scan
//! - Inode identity check to prevent copy-over-self via symlinks/hard links
//! - Path/name length validation (255-byte name, 1024-byte path)
//! - Special file filtering (skips sockets, FIFOs, devices)
//! - macOS copyfile(3) for full metadata preservation (xattrs, ACLs, resource forks)
//! - Symlink preservation (not dereferenced)
//! - Symlink loop detection to prevent infinite recursion
//! - Copy rollback on failure (CopyTransaction)
//! - Atomic cross-filesystem moves using staging directory

mod chunked_copy;
mod copy;
mod copy_strategy;
mod delete;
mod helpers;
#[cfg(target_os = "linux")]
mod linux_copy;
#[cfg(target_os = "macos")]
pub(crate) mod macos_copy;
mod move_op;
mod scan;
mod state;
pub(crate) mod trash;
mod types;
mod volume_conflict;
mod volume_copy;
mod volume_strategy;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use uuid::Uuid;

use copy::copy_files_with_progress;
use delete::{delete_files_with_progress, delete_volume_files_with_progress};
#[cfg(not(test))]
use helpers::{
    validate_destination, validate_destination_not_inside_source, validate_destination_writable,
    validate_not_same_location, validate_sources,
};
use move_op::move_files_with_progress;
#[cfg(not(test))]
use state::WriteOperationState;
use state::{WRITE_OPERATION_STATE, register_operation_status, unregister_operation_status};
use trash::trash_files_with_progress;

// Re-export public types
pub use scan::{cancel_scan_preview, is_scan_preview_complete, start_scan_preview};
pub use state::{
    cancel_all_write_operations, cancel_write_operation, get_operation_status, list_active_operations,
    resolve_write_conflict,
};
#[allow(unused_imports, reason = "Public API re-exports for consumers of this module")]
pub use types::{
    ConflictInfo, ConflictResolution, DryRunResult, OperationStatus, OperationSummary, ScanPreviewCancelledEvent,
    ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent, ScanPreviewStartResult,
    ScanProgressEvent, SortColumn, SortOrder, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent,
    WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult,
    WriteOperationType, WriteProgressEvent,
};

// Re-export for tests (these are pub(crate) in helpers.rs and state.rs)
#[cfg(test)]
#[allow(unused_imports, reason = "Re-exports for test modules in file_system")]
pub(crate) use helpers::{
    is_same_file, is_same_filesystem, validate_destination, validate_destination_not_inside_source,
    validate_destination_writable, validate_disk_space, validate_not_same_location, validate_path_length,
    validate_sources,
};
#[cfg(test)]
pub(crate) use state::{CopyTransaction, WriteOperationState};

// Re-export volume copy types and functions
pub use types::{VolumeCopyConfig, VolumeCopyScanResult};
pub use volume_copy::{copy_between_volumes, scan_for_volume_copy};

// ============================================================================
// Public API functions
// ============================================================================

/// Spawns a write operation in the background with state management and panic handling.
///
/// Creates `WriteOperationState`, registers the operation, spawns `tokio::spawn` +
/// `spawn_blocking`, and handles cleanup and panic recovery. Callers do validation
/// and logging before calling this, then pass a closure for the actual work.
async fn start_write_operation<F>(
    app: tauri::AppHandle,
    operation_type: WriteOperationType,
    progress_interval_ms: u64,
    handler: F,
) -> Result<WriteOperationStartResult, WriteOperationError>
where
    F: FnOnce(tauri::AppHandle, String, Arc<WriteOperationState>) -> Result<(), WriteOperationError> + Send + 'static,
{
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(progress_interval_ms),
        pending_resolution: std::sync::RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }
    register_operation_status(&operation_id, operation_type);

    let operation_id_for_spawn = operation_id.clone();

    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || handler(app, operation_id_for_spawn, state)).await;

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        use tauri::Emitter;
        match result {
            Ok(Ok(())) => {} // Handler already emitted write-complete or write-cancelled
            Ok(Err(ref e)) if matches!(e, WriteOperationError::Cancelled { .. }) => {
                // Handler already emitted write-cancelled
            }
            Ok(Err(e)) => {
                // Handler error (validation, I/O, etc.) — emit write-error as safety net
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent {
                        operation_id: operation_id_for_cleanup,
                        operation_type,
                        error: e,
                    },
                );
            }
            Err(join_error) => {
                // Panic/abort in spawn_blocking
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent {
                        operation_id: operation_id_for_cleanup,
                        operation_type,
                        error: WriteOperationError::IoError {
                            path: String::new(),
                            message: format!("Task failed: {}", join_error),
                        },
                    },
                );
            }
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type,
    })
}

/// Starts a copy operation in the background.
pub async fn copy_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!(
        "copy_files_start: sources={:?}, destination={:?}, dry_run={}",
        sources,
        destination,
        config.dry_run
    );

    start_write_operation(
        app,
        WriteOperationType::Copy,
        config.progress_interval_ms,
        move |app, op_id, state| {
            validate_sources(&sources)?;
            validate_destination(&destination)?;
            validate_destination_writable(&destination)?;
            validate_not_same_location(&sources, &destination)?;
            validate_destination_not_inside_source(&sources, &destination)?;
            copy_files_with_progress(&app, &op_id, &state, &sources, &destination, &config)
        },
    )
    .await
}

/// Starts a move operation in the background.
///
/// Uses instant rename() for same-filesystem moves.
/// Uses atomic staging pattern for cross-filesystem moves.
pub async fn move_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!(
        "move_files_start: sources={:?}, destination={:?}, dry_run={}",
        sources,
        destination,
        config.dry_run
    );

    start_write_operation(
        app,
        WriteOperationType::Move,
        config.progress_interval_ms,
        move |app, op_id, state| {
            validate_sources(&sources)?;
            validate_destination(&destination)?;
            validate_destination_writable(&destination)?;
            validate_not_same_location(&sources, &destination)?;
            validate_destination_not_inside_source(&sources, &destination)?;
            move_files_with_progress(&app, &op_id, &state, &sources, &destination, &config)
        },
    )
    .await
}

/// Starts a delete operation in the background.
///
/// Recursively deletes files and directories. When `volume_id` is provided and
/// is not the default volume, routes through `delete_volume_files_with_progress`
/// which uses the Volume trait (needed for MTP and other non-local volumes).
pub async fn delete_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    config: WriteOperationConfig,
    volume_id: Option<String>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let volume_id_str = volume_id.unwrap_or_else(|| "root".to_string());

    log::info!(
        "delete_files_start: sources={:?}, volume={}, dry_run={}",
        sources,
        volume_id_str,
        config.dry_run
    );

    start_write_operation(
        app,
        WriteOperationType::Delete,
        config.progress_interval_ms,
        move |app, op_id, state| {
            if volume_id_str != "root" {
                let volume = crate::file_system::get_volume_manager()
                    .get(&volume_id_str)
                    .ok_or_else(|| WriteOperationError::IoError {
                        path: volume_id_str.clone(),
                        message: format!("Volume '{}' not found", volume_id_str),
                    })?;
                delete_volume_files_with_progress(volume, &app, &op_id, &state, &sources, &config)
            } else {
                validate_sources(&sources)?;
                delete_files_with_progress(&app, &op_id, &state, &sources, &config)
            }
        },
    )
    .await
}

/// Starts a trash operation in the background.
///
/// Moves top-level items to the macOS Trash via `NSFileManager.trashItemAtURL`.
/// Supports cancellation between items and partial failure (some items may fail
/// while others succeed).
pub async fn trash_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    item_sizes: Option<Vec<u64>>,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!("trash_files_start: sources={:?}", sources);

    start_write_operation(
        app,
        WriteOperationType::Trash,
        config.progress_interval_ms,
        move |app, op_id, state| {
            validate_sources(&sources)?;
            trash_files_with_progress(&app, &op_id, &state, &sources, item_sizes.as_deref())
        },
    )
    .await
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod tests;
