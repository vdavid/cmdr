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

// TODO: Remove this once volume_copy is integrated into Tauri commands (Phase 5)
#![allow(dead_code, reason = "Volume copy not yet integrated into Tauri commands")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::state::{
    WRITE_OPERATION_STATE, WriteOperationState, register_operation_status, unregister_operation_status,
    update_operation_status,
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
        log::info!(
            "copy_between_volumes: both volumes are local, delegating to native copy (src={}, dest={})",
            src_root.display(),
            dest_root.display()
        );

        // Convert relative paths to absolute paths
        let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();
        let absolute_dest = dest_root.join(&dest_path);

        // Convert VolumeCopyConfig to WriteOperationConfig
        let write_config = WriteOperationConfig {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
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
        return Err(VolumeError::IoError(format!(
            "Not enough space: need {} bytes, only {} available",
            total_bytes, dest_space.available_bytes
        )));
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

    // Phase 1: Scan sources
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

    let mut total_files = 0;
    let mut total_dirs = 0;
    let mut total_bytes = 0u64;

    for source_path in source_paths {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
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

    log::info!(
        "copy_volumes_with_progress: scan complete for operation_id={}, files={}, dirs={}, bytes={}",
        operation_id,
        total_files,
        total_dirs,
        total_bytes
    );

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

    for source_path in source_paths {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    files_processed: files_done,
                    rolled_back: false, // Volume copies don't have rollback yet
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
        let mut dest_item_path = if let Some(name) = source_path.file_name() {
            dest_path.join(name)
        } else {
            dest_path.to_path_buf()
        };

        // Check for conflict: does destination already exist?
        if dest_volume.exists(&dest_item_path) {
            // Check if both source and destination are directories - directories merge, not conflict
            let source_is_dir = source_volume.is_directory(source_path).unwrap_or(false);
            let dest_is_dir = dest_volume.is_directory(&dest_item_path).unwrap_or(false);

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

        let bytes_copied = copy_single_path(&source_volume, source_path, &dest_volume, &dest_item_path, state)
            .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;

        files_done += 1;
        bytes_done += bytes_copied;

        // Emit progress
        if last_progress_time.elapsed() >= progress_interval {
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Copy,
                    phase: WriteOperationPhase::Copying,
                    current_file: file_name.clone(),
                    files_done,
                    files_total: total_files,
                    bytes_done,
                    bytes_total: total_bytes,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Copying,
                file_name,
                files_done,
                total_files,
                bytes_done,
                total_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Success
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

    Ok(())
}

/// Maps VolumeError to WriteOperationError, attaching path context where the original error lacks one.
fn map_volume_error(context_path: &str, e: VolumeError) -> WriteOperationError {
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
        VolumeError::IoError(msg) => WriteOperationError::IoError {
            path: context_path.to_string(),
            message: msg,
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
    fn test_scan_for_volume_copy_empty_source_returns_error_for_in_memory() {
        // InMemoryVolume doesn't support get_space_info, so scan_for_volume_copy
        // will return an error. This is expected behavior.
        let source = InMemoryVolume::new("Source");
        let dest = InMemoryVolume::new("Dest");

        let result = scan_for_volume_copy(&source, &[], &dest, Path::new("/"), 10);
        // InMemoryVolume doesn't support get_space_info, so this should fail
        assert!(result.is_err());
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
