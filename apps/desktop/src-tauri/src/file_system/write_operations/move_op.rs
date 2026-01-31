//! Move implementation for write operations.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::copy::copy_path_recursive;
use super::helpers::{is_same_filesystem, resolve_conflict, safe_overwrite_dir, spawn_async_sync};
use super::scan::{handle_dry_run, scan_sources};
use super::state::{CopyTransaction, WriteOperationState};
use super::types::{
    ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationType,
};

// ============================================================================
// Move implementation
// ============================================================================

pub(super) fn move_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    // Handle dry-run mode
    if handle_dry_run(
        config.dry_run,
        sources,
        destination,
        state,
        app,
        operation_id,
        WriteOperationType::Move,
        state.progress_interval,
        config.max_conflicts_to_show,
    )? {
        return Ok(());
    }

    // Check if all sources are on the same filesystem as destination
    let same_fs = sources
        .iter()
        .all(|s| is_same_filesystem(s, destination).unwrap_or(false));

    if same_fs {
        // Use instant rename for each source
        move_with_rename(app, operation_id, state, sources, destination, config)
    } else {
        // Use atomic staging pattern for cross-filesystem move
        move_with_staging(app, operation_id, state, sources, destination, config)
    }
}

fn move_with_rename(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    let mut files_done = 0;
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    for source in sources {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Move,
                    files_processed: files_done,
                    rolled_back: false, // Same-filesystem moves are atomic, nothing to rollback
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
            path: source.display().to_string(),
            message: "Invalid source path".to_string(),
        })?;
        let dest_path = destination.join(file_name);

        // Handle conflicts
        let (actual_dest, needs_safe_overwrite) = if dest_path.exists() {
            match resolve_conflict(
                source,
                &dest_path,
                config,
                app,
                operation_id,
                state,
                &mut apply_to_all_resolution,
            )? {
                Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                None => {
                    // Skip this file
                    continue;
                }
            }
        } else {
            (dest_path, false)
        };

        // For same-FS move with overwrite:
        // - For files: rename() atomically replaces the destination
        // - For directories: need to remove dest first (rename fails on non-empty dirs)
        if needs_safe_overwrite && actual_dest.is_dir() {
            // Safe directory overwrite: backup, then rename
            let backup_path = safe_overwrite_dir(&actual_dest)?;
            if let Err(e) = fs::rename(source, &actual_dest) {
                // Restore backup on failure
                let _ = fs::rename(&backup_path, &actual_dest);
                return Err(WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                });
            }
            // Remove backup
            let _ = fs::remove_dir_all(&backup_path);
        } else {
            // For files or non-overwrite: rename() handles it (atomic for files)
            fs::rename(source, &actual_dest).map_err(|e| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: e.to_string(),
            })?;
        }

        files_done += 1;
    }

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion (instant, no progress needed)
    let _ = app.emit(
        "write-complete",
        WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            bytes_processed: 0, // Rename doesn't track bytes
        },
    );

    Ok(())
}

/// Performs cross-filesystem move using atomic staging pattern.
/// This ensures source files remain intact if the operation fails.
fn move_with_staging(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    // Phase 1: Scan (move uses default sorting - order doesn't matter much for move)
    let scan_result = scan_sources(
        sources,
        state,
        app,
        operation_id,
        WriteOperationType::Move,
        config.sort_column,
        config.sort_order,
    )?;

    // Create staging directory
    let staging_dir = destination.join(format!(".cmdr-staging-{}", operation_id));
    fs::create_dir(&staging_dir).map_err(|e| WriteOperationError::IoError {
        path: staging_dir.display().to_string(),
        message: format!("Failed to create staging directory: {}", e),
    })?;

    // Phase 2: Copy to staging directory
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;

    let copy_result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            copy_path_recursive(
                source,
                &staging_dir,
                &mut files_done,
                &mut bytes_done,
                scan_result.file_count,
                scan_result.total_bytes,
                state,
                app,
                operation_id,
                &state.progress_interval,
                &mut last_progress_time,
                config,
                &mut transaction,
                &mut apply_to_all_resolution,
            )?;
        }
        Ok(())
    })();

    if let Err(e) = copy_result {
        // Cleanup staging directory on failure
        let _ = fs::remove_dir_all(&staging_dir);
        let _ = app.emit(
            "write-error",
            WriteErrorEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                error: e.clone(),
            },
        );
        return Err(e);
    }

    // Phase 3: Atomic rename from staging to final destination
    let rename_result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "Invalid source path".to_string(),
            })?;

            let staged_path = staging_dir.join(file_name);
            let final_path = destination.join(file_name);

            // Handle conflicts at final destination
            let (actual_dest, needs_safe_overwrite) = if final_path.exists() {
                match resolve_conflict(
                    source,
                    &final_path,
                    config,
                    app,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )? {
                    Some(resolved) => (resolved.path, resolved.needs_safe_overwrite),
                    None => {
                        // Skip - remove from staging
                        if staged_path.is_dir() {
                            let _ = fs::remove_dir_all(&staged_path);
                        } else {
                            let _ = fs::remove_file(&staged_path);
                        }
                        continue;
                    }
                }
            } else {
                (final_path, false)
            };

            // Rename from staging to final (atomic on same filesystem)
            // For overwrite of directories, need to backup first
            if needs_safe_overwrite && actual_dest.is_dir() {
                let backup_path = safe_overwrite_dir(&actual_dest)?;
                if let Err(e) = fs::rename(&staged_path, &actual_dest) {
                    // Restore backup on failure
                    let _ = fs::rename(&backup_path, &actual_dest);
                    return Err(WriteOperationError::IoError {
                        path: staged_path.display().to_string(),
                        message: format!("Failed to move from staging: {}", e),
                    });
                }
                let _ = fs::remove_dir_all(&backup_path);
            } else {
                // For files: rename atomically replaces
                fs::rename(&staged_path, &actual_dest).map_err(|e| WriteOperationError::IoError {
                    path: staged_path.display().to_string(),
                    message: format!("Failed to move from staging: {}", e),
                })?;
            }
        }
        Ok(())
    })();

    if let Err(e) = rename_result {
        // Cleanup staging directory on failure
        let _ = fs::remove_dir_all(&staging_dir);
        let _ = app.emit(
            "write-error",
            WriteErrorEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                error: e.clone(),
            },
        );
        return Err(e);
    }

    // Phase 4: Delete source files (only after successful copy+rename)
    delete_sources_after_move(app, operation_id, state, sources, files_done)?;

    // Phase 5: Remove empty staging directory
    let _ = fs::remove_dir(&staging_dir);

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion
    let _ = app.emit(
        "write-complete",
        WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            bytes_processed: bytes_done,
        },
    );

    Ok(())
}

fn delete_sources_after_move(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    files_done: usize,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    for source in sources {
        // Check cancellation
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Move,
                    files_processed: files_done,
                    rolled_back: false, // Source deletion phase - nothing to rollback
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Use symlink_metadata to check if it still exists
        if fs::symlink_metadata(source).is_ok() {
            if source.is_dir() {
                fs::remove_dir_all(source).map_err(|e| WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                })?;
            } else {
                fs::remove_file(source).map_err(|e| WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: e.to_string(),
                })?;
            }
        }
    }

    Ok(())
}
