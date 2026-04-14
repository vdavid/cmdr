//! Unified volume copy operations.
//!
//! This module provides copy operations that work across different volume types.
//! It abstracts the differences between local and MTP volumes, providing a unified
//! interface for file copying regardless of source or destination type.
//!
//! Copy operation flow:
//! 1. Scan source files for count and total bytes
//! 2. Check destination space availability
//! 3. Scan for conflicts at destination
//! 4. Execute copy with progress reporting
//!
//! For cross-volume copies:
//! - Local → Local: Uses existing efficient file copy
//! - Local → MTP: Uses volume.import_from_local()
//! - MTP → Local: Uses volume.export_to_local()

use std::cell::Cell;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::scan::take_cached_scan_result;
use super::state::{
    OperationIntent, WRITE_OPERATION_STATE, WriteOperationState, is_cancelled, load_intent, register_operation_status,
    unregister_operation_status, update_operation_status,
};
use super::types::{
    ConflictResolution, VolumeCopyConfig, VolumeCopyScanResult, WriteCancelledEvent, WriteCompleteEvent,
    WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationStartResult,
    WriteOperationType, WriteProgressEvent,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_strategy::copy_single_path;
use crate::file_system::volume::{SourceItemInfo, Volume, VolumeError};

/// Starts a copy operation between two volumes.
///
/// This is the unified entry point for all copy operations:
/// - Local → Local
/// - Local → MTP
/// - MTP → Local
///
/// The function determines the appropriate copy strategy based on volume types
/// and handles progress reporting, conflict detection, and cancellation.
///
/// # Arguments
///
/// * `app` - Tauri app handle for event emission
/// * `source_volume` - The source volume to copy from
/// * `source_paths` - Paths of files/directories to copy (relative to source volume root)
/// * `dest_volume` - The destination volume to copy to
/// * `dest_path` - Destination directory path (relative to dest volume root)
/// * `config` - Copy operation configuration
///
/// # Events emitted
///
/// * `write-progress` - Every progress_interval_ms with WriteProgressEvent
/// * `write-complete` - On success with WriteCompleteEvent
/// * `write-error` - On error with WriteErrorEvent
/// * `write-cancelled` - If cancelled with WriteCancelledEvent
pub async fn copy_between_volumes(
    app: tauri::AppHandle,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Validate that volumes support the required operations
    if !source_volume.supports_export() {
        return Err(WriteOperationError::IoError {
            path: String::new(),
            message: format!("Source volume '{}' does not support export", source_volume.name()),
        });
    }

    // Optimization: If both volumes are local filesystem paths, use the battle-tested
    // copy.rs implementation which has proper cancellation support via macOS copyfile API.
    if let (Some(src_root), Some(dest_root)) = (source_volume.local_path(), dest_volume.local_path()) {
        log::debug!(
            "copy_between_volumes: both volumes are local, delegating to native copy (src={}, dest={})",
            src_root.display(),
            dest_root.display()
        );

        // Convert relative paths to absolute paths
        let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();
        let absolute_dest = dest_root.join(dest_path.strip_prefix("/").unwrap_or(&dest_path));

        // Convert VolumeCopyConfig to WriteOperationConfig, preserving preview_id
        let write_config = WriteOperationConfig {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
            preview_id: config.preview_id,
            ..Default::default()
        };

        // Delegate to the existing copy implementation with full cancellation support
        return super::copy_files_start(app, absolute_sources, absolute_dest, write_config).await;
    }

    let operation_id = Uuid::new_v4().to_string();
    log::info!(
        "copy_between_volumes: operation_id={}, source_volume={}, dest_volume={}, {} sources, dest={}",
        operation_id,
        source_volume.name(),
        dest_volume.name(),
        source_paths.len(),
        dest_path.display()
    );

    let state = Arc::new(WriteOperationState {
        intent: Arc::new(AtomicU8::new(0)),
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
            copy_volumes_with_progress(
                &app,
                &operation_id_for_spawn,
                &state,
                source_volume,
                &source_paths,
                dest_volume,
                &dest_path,
                &config,
            )
        })
        .await;

        // Clean up state
        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        // Handle task result - both panics and operation errors
        use tauri::Emitter;
        match result {
            Ok(Ok(())) => {
                // Success - write-complete event already emitted by copy_volumes_with_progress
            }
            Ok(Err(write_err)) => {
                // Operation returned an error (not a panic)
                log::error!(
                    "copy_between_volumes: operation {} failed with error: {:?}",
                    operation_id_for_cleanup,
                    write_err
                );
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent {
                        operation_id: operation_id_for_cleanup,
                        operation_type: WriteOperationType::Copy,
                        error: write_err,
                    },
                );
            }
            Err(e) => {
                // Task panicked
                log::error!(
                    "copy_between_volumes: operation {} panicked: {}",
                    operation_id_for_cleanup,
                    e
                );
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
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Copy,
    })
}

/// Performs a pre-flight scan for volume copy without executing.
///
/// This scans the source files and checks destination for conflicts and space.
/// Use this to show the user what will happen before starting the copy.
///
/// # Arguments
///
/// * `source_volume` - The source volume to scan
/// * `source_paths` - Paths of files/directories to copy
/// * `dest_volume` - The destination volume
/// * `dest_path` - Destination directory path
/// * `max_conflicts` - Maximum number of conflicts to return
pub fn scan_for_volume_copy(
    source_volume: &dyn Volume,
    source_paths: &[PathBuf],
    dest_volume: &dyn Volume,
    dest_path: &Path,
    max_conflicts: usize,
) -> Result<VolumeCopyScanResult, VolumeError> {
    // Scan source for total bytes and file count
    let mut total_files = 0;
    let mut total_dirs = 0;
    let mut total_bytes = 0u64;
    let mut source_items: Vec<SourceItemInfo> = Vec::new();

    for source_path in source_paths {
        let scan = source_volume.scan_for_copy(source_path)?;
        total_files += scan.file_count;
        total_dirs += scan.dir_count;
        total_bytes += scan.total_bytes;

        // Collect source item info for conflict detection
        // For now, we just use the top-level item name
        if let Some(name) = source_path.file_name() {
            let metadata = source_volume.get_metadata(source_path).ok();
            source_items.push(SourceItemInfo {
                name: name.to_string_lossy().to_string(),
                size: metadata.as_ref().and_then(|m| m.size).unwrap_or(0),
                modified: metadata
                    .as_ref()
                    .and_then(|m| m.modified_at.map(|ms| (ms / 1000) as i64)),
            });
        }
    }

    // Get destination space info
    let dest_space = dest_volume.get_space_info()?;

    // Check if there's enough space
    if dest_space.available_bytes < total_bytes {
        return Err(VolumeError::IoError {
            message: format!(
                "Not enough space: need {} bytes, only {} available",
                total_bytes, dest_space.available_bytes
            ),
            raw_os_error: None,
        });
    }

    // Scan for conflicts at destination
    let all_conflicts = dest_volume.scan_for_conflicts(&source_items, dest_path)?;

    // Limit the number of conflicts returned
    let conflicts = if all_conflicts.len() > max_conflicts {
        all_conflicts.into_iter().take(max_conflicts).collect()
    } else {
        all_conflicts
    };

    Ok(VolumeCopyScanResult {
        file_count: total_files,
        dir_count: total_dirs,
        total_bytes,
        dest_space,
        conflicts,
    })
}

/// Internal function that performs the actual copy with progress reporting.
#[allow(
    clippy::too_many_arguments,
    reason = "Volume copy requires passing multiple context parameters"
)]
fn copy_volumes_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    source_volume: Arc<dyn Volume>,
    source_paths: &[PathBuf],
    dest_volume: Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    log::debug!(
        "copy_volumes_with_progress: starting operation_id={}, {} sources",
        operation_id,
        source_paths.len()
    );

    // Phase 1: Scan sources (or reuse cached scan from preview)
    let mut total_files;
    let mut total_bytes;

    if let Some(cached) = config.preview_id.as_deref().and_then(take_cached_scan_result) {
        total_files = cached.file_count;
        total_bytes = cached.total_bytes;
        log::debug!(
            "copy_volumes_with_progress: reused cached scan for operation_id={}, files={}, bytes={}",
            operation_id,
            total_files,
            total_bytes
        );
    } else {
        log::debug!(
            "copy_volumes_with_progress: scanning sources for operation_id={}",
            operation_id
        );

        let _ = app.emit(
            "write-progress",
            WriteProgressEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Copy,
                phase: WriteOperationPhase::Scanning,
                current_file: None,
                files_done: 0,
                files_total: 0,
                bytes_done: 0,
                bytes_total: 0,
            },
        );

        total_files = 0;
        total_bytes = 0u64;
        let mut total_dirs = 0;

        for source_path in source_paths {
            if is_cancelled(&state.intent) {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            let scan = source_volume
                .scan_for_copy(source_path)
                .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;
            total_files += scan.file_count;
            total_dirs += scan.dir_count;
            total_bytes += scan.total_bytes;
        }

        log::debug!(
            "copy_volumes_with_progress: scan complete for operation_id={}, files={}, dirs={}, bytes={}",
            operation_id,
            total_files,
            total_dirs,
            total_bytes
        );
    }

    // Phase 2: Check destination space
    let dest_space = dest_volume
        .get_space_info()
        .map_err(|e| map_volume_error(&dest_path.display().to_string(), e))?;
    if dest_space.available_bytes < total_bytes {
        return Err(WriteOperationError::InsufficientSpace {
            required: total_bytes,
            available: dest_space.available_bytes,
            volume_name: Some(dest_volume.name().to_string()),
        });
    }

    // Phase 3: Copy files with progress
    // files_done tracks individual files (updated by on_file_complete callback from recursive copy).
    // total_files is the recursive count from the scan.
    let files_done_atomic = AtomicUsize::new(0);
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();
    let progress_interval = Duration::from_millis(config.progress_interval_ms);

    // Emit initial copying phase event
    let _ = app.emit(
        "write-progress",
        WriteProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            phase: WriteOperationPhase::Copying,
            current_file: None,
            files_done: 0,
            files_total: total_files,
            bytes_done: 0,
            bytes_total: total_bytes,
        },
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Copying,
        None,
        0,
        total_files,
        0,
        total_bytes,
    );

    // Track "apply to all" resolution for conflicts
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    // Track successfully copied destination paths for rollback/cleanup
    let mut copied_paths: Vec<PathBuf> = Vec::new();
    // Track the last destination path being copied (for partial-file cleanup on cancel)
    let mut last_dest_path: Option<PathBuf> = None;
    let mut copy_error: Option<WriteOperationError> = None;

    for source_path in source_paths {
        // Check cancellation
        if is_cancelled(&state.intent) {
            break;
        }

        let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
        let mut dest_item_path = if let Some(name) = source_path.file_name() {
            dest_path.join(name)
        } else {
            dest_path.to_path_buf()
        };

        // Check for conflict: does destination already exist?
        // Use get_metadata() once to avoid redundant list_directory calls.
        // On MTP, exists() + is_directory() would each list the parent directory
        // separately; get_metadata() does it once (and the listing cache covers
        // the source side too).
        if let Ok(dest_meta) = dest_volume.get_metadata(&dest_item_path) {
            let source_is_dir = source_volume.is_directory(source_path).unwrap_or(false);
            let dest_is_dir = dest_meta.is_directory;

            if source_is_dir && dest_is_dir {
                // Both are directories - this is a merge, not a conflict
                // Continue with the copy (contents will be merged)
                log::debug!(
                    "copy_volumes_with_progress: merging directories {} -> {}",
                    source_path.display(),
                    dest_item_path.display()
                );
            } else {
                // Either both are files, or there's a type mismatch - this is a conflict
                log::debug!(
                    "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                    dest_item_path.display(),
                    source_is_dir,
                    dest_is_dir
                );

                // Resolve the conflict
                let resolved = resolve_volume_conflict(
                    &source_volume,
                    source_path,
                    &dest_volume,
                    &dest_item_path,
                    config,
                    app,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )?;

                match resolved {
                    None => {
                        // Skip this file
                        log::debug!(
                            "copy_volumes_with_progress: skipping {} due to conflict resolution",
                            source_path.display()
                        );
                        continue;
                    }
                    Some(resolved_path) => {
                        dest_item_path = resolved_path;
                    }
                }
            }
        }

        log::debug!(
            "copy_volumes_with_progress: copying {} -> {}",
            source_path.display(),
            dest_item_path.display()
        );

        // Build a per-file progress callback that updates overall bytes_done and
        // emits throttled write-progress events. Uses AtomicU64 + Cell so the
        // closure is Fn (required by the Volume trait's dyn Fn signature).
        let atomic_bytes_done = AtomicU64::new(bytes_done);
        let last_file_bytes = AtomicU64::new(0);
        let last_progress_cell = Cell::new(last_progress_time);
        let file_name_for_cb = file_name.clone();

        let on_file_progress = |file_bytes_done: u64, _file_bytes_total: u64| -> ControlFlow<()> {
            // Check cancellation
            if is_cancelled(&state.intent) {
                return ControlFlow::Break(());
            }

            // Update overall bytes_done: add the delta since last callback
            let prev = last_file_bytes.swap(file_bytes_done, Ordering::Relaxed);
            let delta = file_bytes_done.saturating_sub(prev);
            let current_total = atomic_bytes_done.fetch_add(delta, Ordering::Relaxed) + delta;

            // Read current files_done from the atomic (updated by on_file_complete)
            let current_files_done = files_done_atomic.load(Ordering::Relaxed);

            // Throttled progress emission
            let last = last_progress_cell.get();
            if last.elapsed() >= progress_interval {
                last_progress_cell.set(Instant::now());
                let _ = app.emit(
                    "write-progress",
                    WriteProgressEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Copy,
                        phase: WriteOperationPhase::Copying,
                        current_file: file_name_for_cb.clone(),
                        files_done: current_files_done,
                        files_total: total_files,
                        bytes_done: current_total,
                        bytes_total: total_bytes,
                    },
                );
                update_operation_status(
                    operation_id,
                    WriteOperationPhase::Copying,
                    file_name_for_cb.clone(),
                    current_files_done,
                    total_files,
                    current_total,
                    total_bytes,
                );
            }

            ControlFlow::Continue(())
        };

        // Build the on_file_complete callback that increments the atomic file counter
        let on_file_complete = || {
            files_done_atomic.fetch_add(1, Ordering::Relaxed);
        };

        // Remember the destination path before copying (for partial-file cleanup)
        last_dest_path = Some(dest_item_path.clone());

        match copy_single_path(
            &source_volume,
            source_path,
            &dest_volume,
            &dest_item_path,
            state,
            &on_file_progress,
            &on_file_complete,
        ) {
            Ok(bytes_copied) => {
                // Copy succeeded — record destination path for potential rollback
                copied_paths.push(dest_item_path);
                last_dest_path = None;

                // Sync files_done from the atomic (updated by on_file_complete during recursive copy)
                files_done = files_done_atomic.load(Ordering::Relaxed);
                // Sync bytes_done from the atomic (the callback may have updated it mid-file)
                bytes_done = atomic_bytes_done.load(Ordering::Relaxed);
                // If the volume didn't call the progress callback (default impl), add bytes_copied
                if last_file_bytes.load(Ordering::Relaxed) == 0 && bytes_copied > 0 {
                    bytes_done += bytes_copied;
                }
                last_progress_time = last_progress_cell.get();
            }
            Err(e) => {
                // Sync bytes_done before handling the error
                bytes_done = atomic_bytes_done.load(Ordering::Relaxed);
                copy_error = Some(map_volume_error(&source_path.display().to_string(), e));
                break;
            }
        }
    }

    // Post-loop: handle success, cancellation, or error
    let intent = load_intent(&state.intent);

    if copy_error.is_none() && !is_cancelled(&state.intent) {
        // All files copied successfully
        log::info!(
            "copy_volumes_with_progress: completed op={} files={} bytes={}",
            operation_id,
            files_done,
            bytes_done
        );

        let _ = app.emit(
            "write-complete",
            WriteCompleteEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Copy,
                files_processed: files_done,
                bytes_processed: bytes_done,
            },
        );

        return Ok(());
    }

    // Cancelled or errored — decide between rollback and cancel
    if intent == OperationIntent::RollingBack {
        // Include the last in-progress item in rollback (it was partially created)
        if let Some(partial_path) = last_dest_path.take() {
            copied_paths.push(partial_path);
        }

        // User requested rollback — delete all copied files in reverse order with progress
        log::info!(
            "copy_volumes_with_progress: rolling back op={}, {} paths to delete",
            operation_id,
            copied_paths.len()
        );

        let rollback_completed = volume_rollback_with_progress(
            &dest_volume,
            &copied_paths,
            app,
            operation_id,
            state,
            files_done,
            bytes_done,
            total_files,
            total_bytes,
        );

        let _ = app.emit(
            "write-cancelled",
            WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Copy,
                files_processed: files_done,
                rolled_back: rollback_completed,
            },
        );
    } else {
        // Stopped or error — keep completed files, clean up only the last partial file
        if let Some(partial_path) = &last_dest_path {
            log::debug!(
                "copy_volumes_with_progress: cleaning up partial file {} for op={}",
                partial_path.display(),
                operation_id,
            );
            if let Err(e) = delete_volume_path_recursive(&dest_volume, partial_path) {
                log::warn!(
                    "copy_volumes_with_progress: failed to clean up partial file {}: {:?}",
                    partial_path.display(),
                    e
                );
            }
        }

        if copy_error.is_none() {
            // Pure cancellation (Stopped)
            log::info!(
                "copy_volumes_with_progress: cancelled op={}, keeping {} copied files",
                operation_id,
                copied_paths.len()
            );
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    files_processed: files_done,
                    rolled_back: false,
                },
            );
        }
    }

    if let Some(err) = copy_error {
        return Err(err);
    }

    Err(WriteOperationError::Cancelled {
        message: "Operation cancelled by user".to_string(),
    })
}

// ============================================================================
// Volume rollback helpers
// ============================================================================

/// Rolls back copied files on a volume with progress events, matching the local copy's
/// `rollback_with_progress` pattern. Deletes paths in reverse order so that files inside
/// directories are removed before the directories themselves.
///
/// Returns `true` if rollback completed fully, `false` if the user cancelled it.
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
fn volume_rollback_with_progress(
    volume: &Arc<dyn Volume>,
    copied_paths: &[PathBuf],
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> bool {
    use tauri::Emitter;

    let paths_to_delete = copied_paths.len();
    let mut paths_deleted = 0usize;
    let mut last_progress_time = Instant::now();

    // Emit initial rollback phase event
    let _ = app.emit(
        "write-progress",
        WriteProgressEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            phase: WriteOperationPhase::RollingBack,
            current_file: None,
            files_done: files_at_cancel,
            files_total,
            bytes_done: bytes_at_cancel,
            bytes_total,
        },
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::RollingBack,
        None,
        files_at_cancel,
        files_total,
        bytes_at_cancel,
        bytes_total,
    );

    // Delete in reverse order (newest first)
    for path in copied_paths.iter().rev() {
        // Check if user cancelled the rollback (RollingBack → Stopped)
        if load_intent(&state.intent) == OperationIntent::Stopped {
            log::info!(
                "volume_rollback_with_progress: rollback cancelled at {}/{} paths, keeping remaining",
                paths_deleted,
                paths_to_delete,
            );
            return false;
        }

        // Each copied path may be a file or a directory tree — delete recursively
        if let Err(e) = delete_volume_path_recursive(volume, path) {
            log::warn!(
                "volume_rollback_with_progress: failed to delete {}: {:?}",
                path.display(),
                e
            );
        }
        paths_deleted += 1;

        // Throttled progress events with decreasing values
        if last_progress_time.elapsed() >= state.progress_interval {
            let remaining_files = files_at_cancel.saturating_sub(paths_deleted);
            let remaining_bytes = if paths_to_delete > 0 {
                bytes_at_cancel - (bytes_at_cancel as f64 * paths_deleted as f64 / paths_to_delete as f64) as u64
            } else {
                0
            };

            let current_file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    phase: WriteOperationPhase::RollingBack,
                    current_file: Some(current_file_name.clone()),
                    files_done: remaining_files,
                    files_total,
                    bytes_done: remaining_bytes,
                    bytes_total,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::RollingBack,
                Some(current_file_name),
                remaining_files,
                files_total,
                remaining_bytes,
                bytes_total,
            );
            last_progress_time = Instant::now();
        }
    }

    true
}

/// Recursively deletes a file or directory on a volume.
///
/// For files: calls `volume.delete()` directly.
/// For directories: lists contents, deletes children (files first, then subdirs),
/// then deletes the directory itself. Best-effort — logs errors but continues.
fn delete_volume_path_recursive(volume: &Arc<dyn Volume>, path: &Path) -> Result<(), VolumeError> {
    let is_dir = match volume.is_directory(path) {
        Ok(true) => true,
        Ok(false) => false,
        Err(_) => {
            // Path may not exist (already deleted or never fully created) — nothing to do
            return Ok(());
        }
    };

    if !is_dir {
        return volume.delete(path);
    }

    // List directory contents and delete children first
    let children = volume.list_directory(path)?;

    // Delete files first, then recurse into subdirectories
    for child in &children {
        let child_path = PathBuf::from(&child.path);
        if child.is_directory {
            if let Err(e) = delete_volume_path_recursive(volume, &child_path) {
                log::warn!(
                    "delete_volume_path_recursive: failed to delete subdirectory {}: {:?}",
                    child_path.display(),
                    e
                );
            }
        } else if let Err(e) = volume.delete(&child_path) {
            log::warn!(
                "delete_volume_path_recursive: failed to delete file {}: {:?}",
                child_path.display(),
                e
            );
        }
    }

    // Delete the now-empty directory
    volume.delete(path)
}

/// Maps VolumeError to WriteOperationError, attaching path context where the original error lacks one.
pub(super) fn map_volume_error(context_path: &str, e: VolumeError) -> WriteOperationError {
    match e {
        VolumeError::NotFound(path) => WriteOperationError::SourceNotFound { path },
        VolumeError::PermissionDenied(msg) => WriteOperationError::PermissionDenied {
            path: context_path.to_string(),
            message: msg,
        },
        VolumeError::AlreadyExists(path) => WriteOperationError::DestinationExists { path },
        VolumeError::NotSupported => WriteOperationError::IoError {
            path: context_path.to_string(),
            message: "Operation not supported by this volume type".to_string(),
        },
        VolumeError::DeviceDisconnected(_) => WriteOperationError::DeviceDisconnected {
            path: context_path.to_string(),
        },
        VolumeError::ReadOnly(_) => WriteOperationError::ReadOnlyDevice {
            path: context_path.to_string(),
            device_name: None,
        },
        VolumeError::StorageFull { .. } => WriteOperationError::InsufficientSpace {
            required: 0,
            available: 0,
            volume_name: None,
        },
        VolumeError::ConnectionTimeout(_) => WriteOperationError::ConnectionInterrupted {
            path: context_path.to_string(),
        },
        VolumeError::Cancelled(_) => WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        },
        VolumeError::IoError { message, .. } => WriteOperationError::IoError {
            path: context_path.to_string(),
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};

    #[test]
    fn test_volume_copy_config_default() {
        let config = VolumeCopyConfig::default();
        assert_eq!(config.progress_interval_ms, 200);
        assert_eq!(config.max_conflicts_to_show, 100);
    }

    #[test]
    fn test_scan_for_volume_copy_empty_source_returns_error_without_space_info() {
        // InMemoryVolume without configured space_info returns NotSupported for get_space_info
        let source = InMemoryVolume::new("Source");
        let dest = InMemoryVolume::new("Dest");

        let result = scan_for_volume_copy(&source, &[], &dest, Path::new("/"), 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_for_volume_copy_with_in_memory_volumes() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source.create_file(Path::new("/file1.txt"), b"Hello").unwrap();
        source.create_file(Path::new("/file2.txt"), b"World").unwrap();

        let dest = InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000);

        let result = scan_for_volume_copy(
            &source,
            &[PathBuf::from("/file1.txt"), PathBuf::from("/file2.txt")],
            &dest,
            Path::new("/"),
            10,
        )
        .unwrap();

        assert_eq!(result.file_count, 2);
        assert_eq!(result.total_bytes, 10); // "Hello" + "World"
        assert!(result.conflicts.is_empty());
        assert!(result.dest_space.available_bytes >= result.total_bytes);
    }

    #[test]
    fn test_scan_for_volume_copy_detects_conflicts_in_memory() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source.create_file(Path::new("/report.txt"), b"new content").unwrap();

        let dest = InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000);
        dest.create_file(Path::new("/report.txt"), b"old content").unwrap();

        let result = scan_for_volume_copy(&source, &[PathBuf::from("/report.txt")], &dest, Path::new("/"), 10).unwrap();

        assert_eq!(result.file_count, 1);
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].source_path, "report.txt");
    }

    #[test]
    fn test_scan_for_volume_copy_insufficient_space() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source.create_file(Path::new("/big.bin"), &vec![0u8; 1000]).unwrap();

        // Dest has only 500 bytes available
        let dest = InMemoryVolume::new("Dest").with_space_info(1000, 500);

        let result = scan_for_volume_copy(&source, &[PathBuf::from("/big.bin")], &dest, Path::new("/"), 10);

        assert!(result.is_err());
    }

    #[test]
    fn test_scan_for_volume_copy_directory_tree() {
        let source = InMemoryVolume::new("Source").with_space_info(1_000_000, 500_000);
        source.create_directory(Path::new("/docs")).unwrap();
        source.create_file(Path::new("/docs/readme.txt"), b"Read me").unwrap();
        source.create_file(Path::new("/docs/notes.txt"), b"Notes here").unwrap();

        let dest = InMemoryVolume::new("Dest").with_space_info(1_000_000, 900_000);

        let result = scan_for_volume_copy(&source, &[PathBuf::from("/docs")], &dest, Path::new("/"), 10).unwrap();

        assert_eq!(result.file_count, 2);
        assert_eq!(result.total_bytes, 17); // 7 + 10
    }

    #[test]
    fn test_map_volume_error_not_found() {
        let err = map_volume_error("/ctx", VolumeError::NotFound("/test/path".to_string()));
        assert!(matches!(err, WriteOperationError::SourceNotFound { path } if path == "/test/path"));
    }

    #[test]
    fn test_map_volume_error_permission_denied() {
        let err = map_volume_error("/ctx", VolumeError::PermissionDenied("Access denied".to_string()));
        assert!(
            matches!(err, WriteOperationError::PermissionDenied { path, message } if message == "Access denied" && path == "/ctx")
        );
    }

    #[test]
    fn test_map_volume_error_already_exists() {
        let err = map_volume_error("/ctx", VolumeError::AlreadyExists("/existing".to_string()));
        assert!(matches!(err, WriteOperationError::DestinationExists { path } if path == "/existing"));
    }

    #[test]
    fn test_map_volume_error_not_supported() {
        let err = map_volume_error("/ctx", VolumeError::NotSupported);
        assert!(
            matches!(err, WriteOperationError::IoError { path, message } if message.contains("not supported") && path == "/ctx")
        );
    }

    // ========================================
    // LocalPosixVolume integration tests
    // ========================================

    #[test]
    fn test_scan_for_volume_copy_with_local_volumes() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_volume_scan_src");
        let dst_dir = std::env::temp_dir().join("cmdr_volume_scan_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        // Create source files
        fs::write(src_dir.join("file1.txt"), "Hello").unwrap();
        fs::write(src_dir.join("file2.txt"), "World").unwrap();

        let source = LocalPosixVolume::new("Source", src_dir.to_str().unwrap());
        let dest = LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap());

        let result = scan_for_volume_copy(
            &source,
            &[PathBuf::from("file1.txt"), PathBuf::from("file2.txt")],
            &dest,
            Path::new(""),
            10,
        );

        let scan = result.unwrap();
        assert_eq!(scan.file_count, 2);
        assert_eq!(scan.total_bytes, 10); // "Hello" + "World"
        assert!(scan.conflicts.is_empty());
        assert!(scan.dest_space.total_bytes > 0);

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn test_scan_for_volume_copy_detects_conflicts() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_volume_conflict_src");
        let dst_dir = std::env::temp_dir().join("cmdr_volume_conflict_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        // Create source file
        fs::write(src_dir.join("conflict.txt"), "New content").unwrap();

        // Create existing file at destination
        fs::write(dst_dir.join("conflict.txt"), "Old content").unwrap();

        let source = LocalPosixVolume::new("Source", src_dir.to_str().unwrap());
        let dest = LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap());

        let result = scan_for_volume_copy(&source, &[PathBuf::from("conflict.txt")], &dest, Path::new(""), 10);

        let scan = result.unwrap();
        assert_eq!(scan.file_count, 1);
        assert_eq!(scan.conflicts.len(), 1);
        assert_eq!(scan.conflicts[0].source_path, "conflict.txt");
        assert_eq!(scan.conflicts[0].source_size, 11); // "New content"
        assert_eq!(scan.conflicts[0].dest_size, 11); // "Old content"

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn test_scan_for_volume_copy_max_conflicts() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_volume_max_conflicts_src");
        let dst_dir = std::env::temp_dir().join("cmdr_volume_max_conflicts_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        // Create 5 conflicting files
        let mut source_paths = Vec::new();
        for i in 0..5 {
            let name = format!("file{}.txt", i);
            fs::write(src_dir.join(&name), "new").unwrap();
            fs::write(dst_dir.join(&name), "old").unwrap();
            source_paths.push(PathBuf::from(&name));
        }

        let source = LocalPosixVolume::new("Source", src_dir.to_str().unwrap());
        let dest = LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap());

        // Request max 3 conflicts
        let result = scan_for_volume_copy(&source, &source_paths, &dest, Path::new(""), 3);

        let scan = result.unwrap();
        assert_eq!(scan.conflicts.len(), 3); // Limited to max

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }
}
