//! Volume move operations.
//!
//! Move operations across different volume types:
//! - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
//! - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
//! - Cross-volume: copy to destination then delete sources

use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::time::{Duration, Instant};
use uuid::Uuid;

use super::state::{
    WRITE_OPERATION_STATE, WriteOperationState, is_cancelled, register_operation_status, unregister_operation_status,
};
use super::types::{
    ConflictResolution, VolumeCopyConfig, WriteCompleteEvent, WriteErrorEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType, WriteProgressEvent,
};
use super::volume_conflict::resolve_volume_conflict;
use super::volume_copy::map_volume_error;
use super::volume_strategy::copy_single_path;
use crate::file_system::volume::Volume;

/// Unified move across volume types.
///
/// Determines the best strategy based on volume relationship:
/// - Same volume (same Arc): `volume.rename()` per file (instant for MTP MoveObject)
/// - Both local: delegates to `move_files_start` (handles same-fs rename optimization)
/// - Cross-volume: copy to destination then delete sources
///
/// Emits the standard write events (`write-progress`, `write-complete`, `write-error`).
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Same volume — use native rename/move (instant for MTP)
    if Arc::ptr_eq(&source_volume, &dest_volume) {
        return move_within_same_volume(app, source_volume, source_paths, dest_path, config).await;
    }

    // Both local — delegate to the battle-tested move implementation
    if let (Some(src_root), Some(dest_root)) = (source_volume.local_path(), dest_volume.local_path()) {
        log::debug!(
            "move_between_volumes: both volumes are local, delegating to native move (src={}, dest={})",
            src_root.display(),
            dest_root.display()
        );

        let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();
        let absolute_dest = dest_root.join(dest_path.strip_prefix("/").unwrap_or(&dest_path));

        let write_config = WriteOperationConfig {
            progress_interval_ms: config.progress_interval_ms,
            conflict_resolution: config.conflict_resolution,
            max_conflicts_to_show: config.max_conflicts_to_show,
            preview_id: config.preview_id,
            ..Default::default()
        };

        return super::move_files_start(app, absolute_sources, absolute_dest, write_config).await;
    }

    // Cross-volume — copy each file to destination, then delete source
    log::info!(
        "move_between_volumes: cross-volume move, {} -> {}, {} sources",
        source_volume.name(),
        dest_volume.name(),
        source_paths.len()
    );

    let operation_id = Uuid::new_v4().to_string();
    let operation_id_for_spawn = operation_id.clone();

    let state = Arc::new(WriteOperationState {
        intent: Arc::new(AtomicU8::new(0)),
        progress_interval: Duration::from_millis(config.progress_interval_ms),
        pending_resolution: std::sync::RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }
    register_operation_status(&operation_id, WriteOperationType::Move);

    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            use tauri::Emitter;

            let total_files = source_paths.len();
            let mut files_done = 0usize;
            let mut bytes_done = 0u64;
            let mut last_progress_time = Instant::now();
            let progress_interval = state.progress_interval;

            // Copy+delete per file: on partial failure, already-moved files exist at dest,
            // remaining files stay at source. No data loss, but the move is partial.
            let mut apply_to_all_resolution: Option<ConflictResolution> = None;

            for source_path in &source_paths {
                if is_cancelled(&state.intent) {
                    return Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".to_string(),
                    });
                }

                let file_name = source_path.file_name().ok_or_else(|| WriteOperationError::IoError {
                    path: source_path.display().to_string(),
                    message: "Invalid source path".to_string(),
                })?;

                let mut dest_item = dest_path.join(file_name);

                // Check for conflict: does destination already exist?
                if let Ok(dest_meta) = dest_volume.get_metadata(&dest_item) {
                    let source_is_dir = source_volume.is_directory(source_path).unwrap_or(false);
                    let dest_is_dir = dest_meta.is_directory;

                    if source_is_dir && dest_is_dir {
                        // Both are directories — merge, not a conflict
                        log::debug!(
                            "move_between_volumes: merging directories {} -> {}",
                            source_path.display(),
                            dest_item.display()
                        );
                    } else {
                        log::debug!(
                            "move_between_volumes: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                            dest_item.display(),
                            source_is_dir,
                            dest_is_dir
                        );

                        let resolved = resolve_volume_conflict(
                            &source_volume,
                            source_path,
                            &dest_volume,
                            &dest_item,
                            &config,
                            &app,
                            &operation_id_for_spawn,
                            &state,
                            &mut apply_to_all_resolution,
                        )?;

                        match resolved {
                            None => {
                                // Skip — don't copy and don't delete source
                                log::debug!(
                                    "move_between_volumes: skipping {} due to conflict resolution",
                                    source_path.display()
                                );
                                continue;
                            }
                            Some(resolved_path) => {
                                dest_item = resolved_path;
                            }
                        }
                    }
                }

                // Copy to destination (no per-file progress for moves — total_bytes is 0)
                let no_progress = |_: u64, _: u64| -> ControlFlow<()> {
                    if is_cancelled(&state.intent) {
                        return ControlFlow::Break(());
                    }
                    ControlFlow::Continue(())
                };
                let bytes = copy_single_path(
                    &source_volume,
                    source_path,
                    &dest_volume,
                    &dest_item,
                    &state,
                    &no_progress,
                    &|| {},
                )
                .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;

                // Delete source
                source_volume
                    .delete(source_path)
                    .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;

                files_done += 1;
                bytes_done += bytes;

                if last_progress_time.elapsed() >= progress_interval {
                    let _ = app.emit(
                        "write-progress",
                        WriteProgressEvent {
                            operation_id: operation_id_for_spawn.clone(),
                            operation_type: WriteOperationType::Move,
                            phase: WriteOperationPhase::Copying,
                            current_file: Some(file_name.to_string_lossy().to_string()),
                            files_done,
                            files_total: total_files,
                            bytes_done,
                            bytes_total: 0,
                        },
                    );
                    last_progress_time = Instant::now();
                }
            }

            log::info!(
                "move_between_volumes: completed op={}, files={}, bytes={}",
                operation_id_for_spawn,
                files_done,
                bytes_done
            );

            let _ = app.emit(
                "write-complete",
                WriteCompleteEvent {
                    operation_id: operation_id_for_spawn,
                    operation_type: WriteOperationType::Move,
                    files_processed: files_done,
                    bytes_processed: bytes_done,
                },
            );

            Ok(())
        })
        .await;

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        use tauri::Emitter;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent {
                        operation_id: operation_id_for_cleanup,
                        operation_type: WriteOperationType::Move,
                        error: e,
                    },
                );
            }
            Err(e) => {
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
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
}

/// Moves files within the same volume using native `Volume::rename`.
///
/// For MTP, this uses MTP MoveObject — a single USB command per file.
/// Runs as a background task with operation registration, progress events, and cancellation.
async fn move_within_same_volume(
    app: tauri::AppHandle,
    volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_path: PathBuf,
    config: VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let operation_id = Uuid::new_v4().to_string();
    let operation_id_for_spawn = operation_id.clone();

    log::info!(
        "move_within_same_volume: operation_id={}, volume={}, {} sources, dest={}",
        operation_id,
        volume.name(),
        source_paths.len(),
        dest_path.display()
    );

    let progress_interval_ms = config.progress_interval_ms;

    let state = Arc::new(WriteOperationState {
        intent: Arc::new(AtomicU8::new(0)),
        progress_interval: Duration::from_millis(progress_interval_ms),
        pending_resolution: std::sync::RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
        cache.insert(operation_id.clone(), Arc::clone(&state));
    }
    register_operation_status(&operation_id, WriteOperationType::Move);

    tokio::spawn(async move {
        let operation_id_for_cleanup = operation_id_for_spawn.clone();
        let app_for_error = app.clone();

        let result = tokio::task::spawn_blocking(move || {
            use tauri::Emitter;

            let total_files = source_paths.len();
            let mut files_moved = 0usize;
            let mut bytes_moved = 0u64;
            let mut last_progress_time = Instant::now();
            let progress_interval = Duration::from_millis(progress_interval_ms);
            let mut apply_to_all_resolution: Option<ConflictResolution> = None;

            for source_path in &source_paths {
                if is_cancelled(&state.intent) {
                    return Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".to_string(),
                    });
                }

                let file_name = source_path.file_name().ok_or_else(|| WriteOperationError::IoError {
                    path: source_path.display().to_string(),
                    message: "Invalid source path".to_string(),
                })?;

                let mut dest_item = dest_path.join(file_name);

                // Check for conflict: does destination already exist?
                if let Ok(dest_meta) = volume.get_metadata(&dest_item) {
                    let source_is_dir = volume.is_directory(source_path).unwrap_or(false);
                    let dest_is_dir = dest_meta.is_directory;

                    if source_is_dir && dest_is_dir {
                        // Both are directories — merge, not a conflict
                        log::debug!(
                            "move_within_same_volume: merging directories {} -> {}",
                            source_path.display(),
                            dest_item.display()
                        );
                    } else {
                        log::debug!(
                            "move_within_same_volume: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                            dest_item.display(),
                            source_is_dir,
                            dest_is_dir
                        );

                        let resolved = resolve_volume_conflict(
                            &volume,
                            source_path,
                            &volume,
                            &dest_item,
                            &config,
                            &app,
                            &operation_id_for_spawn,
                            &state,
                            &mut apply_to_all_resolution,
                        )?;

                        match resolved {
                            None => {
                                // Skip — don't move this file
                                log::debug!(
                                    "move_within_same_volume: skipping {} due to conflict resolution",
                                    source_path.display()
                                );
                                continue;
                            }
                            Some(resolved_path) => {
                                dest_item = resolved_path;
                            }
                        }
                    }
                }

                let size = volume.get_metadata(source_path).ok().and_then(|m| m.size).unwrap_or(0);

                volume
                    .rename(source_path, &dest_item, false)
                    .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?;

                files_moved += 1;
                bytes_moved += size;

                if last_progress_time.elapsed() >= progress_interval {
                    let _ = app.emit(
                        "write-progress",
                        WriteProgressEvent {
                            operation_id: operation_id_for_spawn.clone(),
                            operation_type: WriteOperationType::Move,
                            phase: WriteOperationPhase::Copying,
                            current_file: Some(file_name.to_string_lossy().to_string()),
                            files_done: files_moved,
                            files_total: total_files,
                            bytes_done: bytes_moved,
                            bytes_total: 0, // Not known upfront for rename-based moves
                        },
                    );
                    last_progress_time = Instant::now();
                }
            }

            log::info!(
                "move_within_same_volume: completed op={}, files={}, bytes={}",
                operation_id_for_spawn,
                files_moved,
                bytes_moved
            );

            let _ = app.emit(
                "write-complete",
                WriteCompleteEvent {
                    operation_id: operation_id_for_spawn,
                    operation_type: WriteOperationType::Move,
                    files_processed: files_moved,
                    bytes_processed: bytes_moved,
                },
            );

            Ok(())
        })
        .await;

        if let Ok(mut cache) = WRITE_OPERATION_STATE.write() {
            cache.remove(&operation_id_for_cleanup);
        }
        unregister_operation_status(&operation_id_for_cleanup);

        use tauri::Emitter;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = app_for_error.emit(
                    "write-error",
                    WriteErrorEvent {
                        operation_id: operation_id_for_cleanup,
                        operation_type: WriteOperationType::Move,
                        error: e,
                    },
                );
            }
            Err(e) => {
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
        }
    });

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Move,
    })
}
