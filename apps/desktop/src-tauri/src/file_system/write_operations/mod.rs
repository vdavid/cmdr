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
pub use scan::{cancel_scan_preview, start_scan_preview};
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

/// Starts a copy operation in the background.
///
/// # Arguments
/// * `app` - Tauri app handle for event emission
/// * `sources` - List of source file/directory paths (absolute)
/// * `destination` - Destination directory path (absolute)
/// * `config` - Operation configuration
///
/// # Events emitted
/// * `write-progress` - Every progress_interval_ms with WriteProgressEvent
/// * `write-complete` - On success with WriteCompleteEvent
/// * `write-error` - On error with WriteErrorEvent
/// * `write-cancelled` - If cancelled with WriteCancelledEvent
/// * `write-conflict` - When Stop mode encounters a conflict
pub async fn copy_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Validate inputs
    validate_sources(&sources)?;
    validate_destination(&destination)?;
    validate_destination_writable(&destination)?;
    validate_not_same_location(&sources, &destination)?;
    validate_destination_not_inside_source(&sources, &destination)?;

    let operation_id = Uuid::new_v4().to_string();
    log::info!(
        "copy_files_start: operation_id={}, sources={:?}, destination={:?}, dry_run={}",
        operation_id,
        sources,
        destination,
        config.dry_run
    );
    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: std::sync::RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Copy);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            copy_files_with_progress(&app, &operation_id_for_spawn, &state, &sources, &destination, &config)
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task panic
        if let Err(e) = result {
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id_for_cleanup,
                    operation_type: WriteOperationType::Copy,
                    error: WriteOperationError::IoError {
                        path: String::new(),
                        message: format!("Task failed: {}", e),
                    },
                },
            );
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Copy,
    })
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
    // Validate inputs
    validate_sources(&sources)?;
    validate_destination(&destination)?;
    validate_destination_writable(&destination)?;
    validate_not_same_location(&sources, &destination)?;
    validate_destination_not_inside_source(&sources, &destination)?;

    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: std::sync::RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Move);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            move_files_with_progress(&app, &operation_id_for_spawn, &state, &sources, &destination, &config)
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task panic
        if let Err(e) = result {
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id_for_cleanup,
                    operation_type: WriteOperationType::Move,
                    error: WriteOperationError::IoError {
                        path: String::new(),
                        message: format!("Task failed: {}", e),
                    },
                },
            );
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
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
    // Resolve volume for non-default volume IDs
    let volume_id_str = volume_id.unwrap_or_else(|| "root".to_string());
    let volume = if volume_id_str != "root" {
        Some(
            crate::file_system::get_volume_manager()
                .get(&volume_id_str)
                .ok_or_else(|| WriteOperationError::IoError {
                    path: volume_id_str.clone(),
                    message: format!("Volume '{}' not found", volume_id_str),
                })?,
        )
    } else {
        // Only validate sources on local filesystem — MTP paths can't be checked with symlink_metadata
        validate_sources(&sources)?;
        None
    };

    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: std::sync::RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Delete);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            if let Some(vol) = volume {
                delete_volume_files_with_progress(vol, &app, &operation_id_for_spawn, &state, &sources, &config)
            } else {
                delete_files_with_progress(&app, &operation_id_for_spawn, &state, &sources, &config)
            }
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task panic
        if let Err(e) = result {
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id_for_cleanup,
                    operation_type: WriteOperationType::Delete,
                    error: WriteOperationError::IoError {
                        path: String::new(),
                        message: format!("Task failed: {}", e),
                    },
                },
            );
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Delete,
    })
}

/// Starts a trash operation in the background.
///
/// Moves top-level items to the macOS Trash via `NSFileManager.trashItemAtURL`.
/// Supports cancellation between items and partial failure (some items may fail
/// while others succeed).
///
/// # Arguments
/// * `app` - Tauri app handle for event emission
/// * `sources` - Top-level items to trash
/// * `item_sizes` - Optional per-item sizes for byte-level progress
/// * `config` - Operation configuration (only `progress_interval_ms` is used)
pub async fn trash_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    item_sizes: Option<Vec<u64>>,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Validate inputs
    validate_sources(&sources)?;

    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: std::sync::RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Store state for cancellation
    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }

    // Register operation status for query APIs
    register_operation_status(&operation_id, WriteOperationType::Trash);

    let operation_id_for_spawn = operation_id.clone();

    // Spawn background task
    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            trash_files_with_progress(&app, &operation_id_for_spawn, &state, &sources, item_sizes.as_deref())
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task panic
        if let Err(e) = result {
            use tauri::Emitter;
            let _ = app_for_error.emit(
                "write-error",
                WriteErrorEvent {
                    operation_id: operation_id_for_cleanup,
                    operation_type: WriteOperationType::Trash,
                    error: WriteOperationError::IoError {
                        path: String::new(),
                        message: format!("Task failed: {}", e),
                    },
                },
            );
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Trash,
    })
}

#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod tests;
