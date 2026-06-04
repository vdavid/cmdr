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

mod cancellable;
mod conflict;
mod delete;
mod durability;
mod eta;
mod overwrite;
mod scan;
mod scan_preview;
mod state;
mod transfer;
mod types;
mod validation;

// Re-export `macos_copy` at this level so existing call sites
// (`crate::file_system::write_operations::macos_copy`) keep compiling.
#[cfg(target_os = "macos")]
pub(crate) use transfer::macos_copy;

// Re-export `trash` at this level so `crate::file_system::write_operations::trash`
// keeps resolving (used by `commands/rename.rs`).
pub(crate) use delete::trash;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use delete::{delete_files_with_progress, delete_volume_files_with_progress};
#[cfg(not(test))]
use state::WriteOperationState;
use state::{
    OperationStateGuard, WRITE_OPERATION_STATE, WriteSettledGuard, register_operation_status,
    unregister_operation_status,
};
use transfer::copy::copy_files_with_progress_inner;
use transfer::move_op::move_files_with_progress;
use trash::trash_files_with_progress;
#[cfg(not(test))]
use validation::{
    validate_destination, validate_destination_not_inside_source, validate_destination_writable,
    validate_not_same_location, validate_sources,
};

// Re-export public types
pub use scan_preview::{cancel_scan_preview, get_scan_preview_totals, start_scan_preview};
pub use state::{
    busy_volume_ids, cancel_all_write_operations, cancel_write_operation, get_operation_status,
    init_busy_volume_emitter, list_active_operations, resolve_write_conflict,
};
#[allow(unused_imports, reason = "Public API re-exports for consumers of this module")]
pub use types::{
    ConflictInfo, ConflictResolution, DryRunResult, OperationStatus, OperationSummary, ScanPreviewCancelledEvent,
    ScanPreviewCompleteEvent, ScanPreviewErrorEvent, ScanPreviewProgressEvent, ScanPreviewStartResult,
    ScanPreviewTotals, ScanProgressEvent, SortColumn, SortOrder, WriteCancelledEvent, WriteCompleteEvent,
    WriteConflictEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase,
    WriteOperationStartResult, WriteOperationType, WriteProgressEvent, WriteSettledEvent,
};

// Re-export for tests (these are pub(crate) in validation.rs and state.rs)
#[cfg(test)]
pub(crate) use state::{CopyTransaction, OperationIntent, WriteOperationState, is_cancelled, load_intent};
#[cfg(test)]
#[allow(unused_imports, reason = "Re-exports for test modules in file_system")]
pub(crate) use validation::{
    is_same_file, is_same_filesystem, validate_destination, validate_destination_not_inside_source,
    validate_destination_writable, validate_disk_space, validate_not_same_location, validate_path_length,
    validate_sources,
};
// Exposed for cross-module integration tests (for example the SMB
// concurrent-copy cross-contamination test in
// `file_system::volume::smb`) that drive `copy_volumes_with_progress`
// directly against a real SMB backend instead of the full Tauri path.
#[cfg(test)]
#[allow(unused_imports, reason = "Used by SMB integration tests in file_system::volume::smb")]
pub(crate) use transfer::volume_copy::copy_volumes_with_progress;
#[cfg(test)]
#[allow(unused_imports, reason = "Used by SMB integration tests in file_system::volume::smb")]
pub(crate) use types::CollectorEventSink;

// Re-export volume copy types and functions
pub use transfer::volume_copy::{copy_between_volumes, scan_for_volume_copy};
pub use transfer::volume_move::move_between_volumes;
pub use types::{VolumeCopyConfig, VolumeCopyScanResult};

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
    volume_ids: Vec<String>,
    handler: F,
) -> Result<WriteOperationStartResult, WriteOperationError>
where
    F: FnOnce(tauri::AppHandle, String, Arc<WriteOperationState>) -> Result<(), WriteOperationError> + Send + 'static,
{
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }
    register_operation_status(&operation_id, operation_type, volume_ids);

    let operation_id_for_spawn = operation_id.clone();

    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();
        // RAII guard: emits `write-settled` when this task exits, no matter
        // how (handler success, error, cancel, or panic via JoinError). FE
        // gates the "Cancelling…" dialog close on this event so the user
        // can't dispatch a new op against a still-tearing-down volume.
        let _settled_guard = WriteSettledGuard::new(
            app_for_error.clone(),
            operation_id_for_cleanup.clone(),
            operation_type,
            None,
        );

        let result = tokio::task::spawn_blocking(move || handler(app, operation_id_for_spawn, state)).await;

        use tauri::Emitter;
        match result {
            Ok(Ok(())) => {} // Handler already emitted write-complete or write-cancelled
            Ok(Err(ref e)) if matches!(e, WriteOperationError::Cancelled { .. }) => {
                // Handler already emitted write-cancelled
            }
            Ok(Err(e)) => {
                // Handler error (validation, I/O, etc.): emit write-error as safety net
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent::new(operation_id_for_cleanup.clone(), operation_type, e),
                );
            }
            Err(join_error) => {
                // Panic/abort in spawn_blocking
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent::new(
                        operation_id_for_cleanup.clone(),
                        operation_type,
                        WriteOperationError::IoError {
                            path: String::new(),
                            message: format!("Task failed: {}", join_error),
                        },
                    ),
                );
            }
        }

        // Cache cleanup happens AFTER terminal events are emitted, BEFORE the
        // settle guard drops (the guard is the very last thing in scope).
        // Order: terminal event → cache removal → `write-settled` via Drop.
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type,
    })
}

/// Starts a copy operation in the background.
///
/// `volume_ids` lists the volumes this copy touches (source + destination), so
/// an ejectable USB / DMG / SMB volume is marked busy while the copy runs. Pass
/// an empty `Vec` for a same-`root` local copy (root is never ejectable).
pub async fn copy_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
    volume_ids: Vec<String>,
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
        volume_ids,
        move |app, op_id, state| {
            validate_sources(&sources)?;
            validate_destination(&destination)?;
            validate_destination_writable(&destination)?;
            validate_not_same_location(&sources, &destination)?;
            validate_destination_not_inside_source(&sources, &destination)?;
            let events = types::TauriEventSink::new(app.clone());
            copy_files_with_progress_inner(&events, &op_id, &state, &sources, &destination, &config)
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
    volume_ids: Vec<String>,
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
        volume_ids,
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

    if volume_id_str != "root" {
        // Volume-aware delete (async): bypass start_write_operation since the handler is async
        let operation_id = Uuid::new_v4().to_string();
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(
            config.progress_interval_ms,
        )));

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.insert(operation_id.clone(), Arc::clone(&state));
        }
        register_operation_status(&operation_id, WriteOperationType::Delete, vec![volume_id_str.clone()]);

        let operation_id_for_spawn = operation_id.clone();
        tokio::spawn(async move {
            let operation_id_for_cleanup = operation_id_for_spawn.clone();
            let app_for_error = app.clone();
            // RAII settle guard: fires `write-settled` when the task exits.
            // Drop runs last in scope so the FE sees the terminal event
            // (write-cancelled / write-error / write-complete) first, then
            // settle, then is free to clear the dialog.
            let _settled_guard = WriteSettledGuard::new(
                app_for_error.clone(),
                operation_id_for_cleanup.clone(),
                WriteOperationType::Delete,
                Some(volume_id_str.clone()),
            );
            // RAII guard: removes the op from `WRITE_OPERATION_STATE` and
            // `OPERATION_STATUS_CACHE` on every exit path, including a panic
            // inside `delete_volume_files_with_progress` that the runtime
            // catches (the cleanup lives after the `.await`, so an unwind would
            // otherwise skip it and leak both map entries). Drops before
            // `_settled_guard`, so cache removal runs before `write-settled`,
            // matching the `start_write_operation` ordering.
            let _state_guard = OperationStateGuard::new(operation_id_for_cleanup.clone());

            let volume = match crate::file_system::get_volume_manager().get(&volume_id_str) {
                Some(v) => v,
                None => {
                    use tauri::Emitter;
                    let _ = app_for_error.emit(
                        "write-error",
                        WriteErrorEvent::new(
                            operation_id_for_cleanup.clone(),
                            WriteOperationType::Delete,
                            WriteOperationError::IoError {
                                path: volume_id_str.clone(),
                                message: format!("Volume '{}' not found", volume_id_str),
                            },
                        ),
                    );
                    return;
                }
            };

            let result = delete_volume_files_with_progress(
                volume,
                &volume_id_str,
                &app,
                &operation_id_for_spawn,
                &state,
                &sources,
                &config,
            )
            .await;

            use tauri::Emitter;
            match result {
                Ok(()) => {}
                Err(ref e) if matches!(e, WriteOperationError::Cancelled { .. }) => {}
                Err(e) => {
                    let _ = app_for_error.emit(
                        "write-error",
                        WriteErrorEvent::new(operation_id_for_cleanup.clone(), WriteOperationType::Delete, e),
                    );
                }
            }
        });

        Ok(WriteOperationStartResult {
            operation_id,
            operation_type: WriteOperationType::Delete,
        })
    } else {
        // Local same-`root` delete: no ejectable volume involved.
        start_write_operation(
            app,
            WriteOperationType::Delete,
            config.progress_interval_ms,
            vec![],
            move |app, op_id, state| {
                validate_sources(&sources)?;
                delete_files_with_progress(&app, &op_id, &state, &sources, &config)
            },
        )
        .await
    }
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

    // Trash always targets the local macOS Trash; no ejectable volume involved.
    start_write_operation(
        app,
        WriteOperationType::Trash,
        config.progress_interval_ms,
        vec![],
        move |app, op_id, state| {
            validate_sources(&sources)?;
            let events: Arc<dyn types::OperationEventSink> = Arc::new(types::TauriEventSink::new(app));
            trash_files_with_progress(&*events, &op_id, &state, &sources, item_sizes.as_deref())
        },
    )
    .await
}

#[cfg(test)]
mod scan_preview_listing_progress_tests;
#[cfg(test)]
mod scan_preview_oracle_tests;
#[cfg(test)]
mod settle_event_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation_integration_test;
