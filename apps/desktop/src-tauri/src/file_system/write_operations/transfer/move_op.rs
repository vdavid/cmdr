//! Move implementation for write operations.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::helpers::{
    is_same_filesystem, path_exists_or_is_symlink, remove_dir_all_in_background, resolve_conflict, spawn_async_sync,
};
use super::super::scan::{SourceItemTracker, handle_dry_run, scan_sources, take_cached_scan_result};
use super::super::state::{
    CopyTransaction, OperationIntent, WriteOperationState, load_intent, update_operation_status,
};
use super::super::types::{
    ConflictResolution, IoResultExt, OperationEventSink, TauriEventSink, WriteCancelledEvent, WriteCompleteEvent,
    WriteErrorEvent, WriteOperationConfig, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent, WriteSourceItemDoneEvent,
};
use super::copy::copy_single_item;

// ============================================================================
// Move rollback tracking
// ============================================================================

/// Tracks renames performed during same-FS move for rollback on cancellation.
/// Each entry is `(original_source, moved_to_dest)`. Rollback reverses them.
struct MoveTransaction {
    renames: Vec<(PathBuf, PathBuf)>,
}

impl MoveTransaction {
    fn new() -> Self {
        Self { renames: Vec::new() }
    }

    fn record(&mut self, source: PathBuf, dest: PathBuf) {
        self.renames.push((source, dest));
    }

    /// Reverses all recorded renames (dest → source) in reverse order.
    /// Same-FS rename is instant, so this runs synchronously.
    fn rollback(&self) {
        for (original_source, moved_to_dest) in self.renames.iter().rev() {
            if let Err(e) = fs::rename(moved_to_dest, original_source) {
                log::warn!(
                    "move rollback: failed to rename {} back to {}: {}",
                    moved_to_dest.display(),
                    original_source.display(),
                    e
                );
            }
        }
    }
}

// ============================================================================
// Move implementation
// ============================================================================

pub(in crate::file_system::write_operations) fn move_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    let events = TauriEventSink::new(app.clone());
    move_files_with_progress_inner(&events, operation_id, state, sources, destination, config)
}

pub(super) fn move_files_with_progress_inner(
    events: &dyn OperationEventSink,
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
        events,
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
        move_with_rename(events, operation_id, state, sources, destination, config)
    } else {
        // Use atomic staging pattern for cross-filesystem move
        move_with_staging(events, operation_id, state, sources, destination, config)
    }
}

fn move_with_rename(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    let mut files_done = 0;
    let mut files_skipped = 0usize;
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;
    let mut move_tx = MoveTransaction::new();

    let result: Result<(), WriteOperationError> = (|| {
        for source in sources {
            // Check cancellation
            if super::super::state::is_cancelled(&state.intent) {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            let file_name = source.file_name().ok_or_else(|| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "Invalid source path".to_string(),
            })?;
            let dest_path = destination.join(file_name);

            // When both source and dest are directories, merge recursively
            // instead of replacing (which would destroy dest-only files).
            if source.is_dir() && dest_path.exists() && dest_path.is_dir() {
                merge_move_directory(
                    source,
                    &dest_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                    &mut move_tx,
                    &mut files_skipped,
                )?;
            } else if path_exists_or_is_symlink(&dest_path) {
                // File-to-file (or type mismatch) conflict
                match resolve_conflict(
                    source,
                    &dest_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )? {
                    Some(resolved) => {
                        // Register both halves with the downloads watcher's
                        // ignore set: destination so rename-arrival is
                        // suppressed, source so a Cmdr move OUT of Downloads
                        // is also suppressed. No-ops outside ~/Downloads.
                        crate::downloads::note_pending_write_for_cmdr(source);
                        crate::downloads::note_pending_write_for_cmdr(&resolved.path);
                        fs::rename(source, &resolved.path).with_path(source)?;
                        move_tx.record(source.clone(), resolved.path);
                    }
                    None => {
                        // Skip this file
                        files_skipped += 1;
                        continue;
                    }
                }
            } else {
                // No conflict, so just rename
                crate::downloads::note_pending_write_for_cmdr(source);
                crate::downloads::note_pending_write_for_cmdr(&dest_path);
                fs::rename(source, &dest_path).with_path(source)?;
                move_tx.record(source.clone(), dest_path);
            }

            files_done += 1;

            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
            });
        }
        Ok(())
    })();

    // Handle cancellation: emit write-cancelled so the frontend can close the dialog.
    // The outer start_write_operation wrapper treats Cancelled as "already handled",
    // so we must emit the event here.
    if let Err(WriteOperationError::Cancelled { .. }) = &result {
        let rolled_back = match load_intent(&state.intent) {
            OperationIntent::RollingBack => {
                move_tx.rollback();
                true
            }
            _ => false,
        };

        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Move,
            files_processed: files_done,
            rolled_back,
        });
        return result;
    }

    result?;

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion (instant, no progress needed)
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done,
        files_skipped,
        bytes_processed: 0, // Rename doesn't track bytes
    });

    Ok(())
}

/// Recursively merges a source directory into an existing destination directory
/// using rename() for individual files. Dest-only files are preserved.
/// After all contents are moved, removes the now-empty source directory.
///
/// Note: This duplicates the recursive-merge-with-conflict-resolution pattern from `copy.rs`.
/// The two look similar in structure but differ in every detail (copy has progress tracking,
/// symlink handling, byte counting, transaction recording, strategy selection). A shared
/// abstraction would be forced and fragile. See `copy.rs` `copy_single_item` for the copy side.
#[allow(clippy::too_many_arguments, reason = "intentional; see doc comment above")]
fn merge_move_directory(
    source_dir: &Path,
    dest_dir: &Path,
    config: &WriteOperationConfig,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
    move_tx: &mut MoveTransaction,
    files_skipped: &mut usize,
) -> Result<(), WriteOperationError> {
    let entries = fs::read_dir(source_dir).with_path(source_dir)?;

    for entry in entries {
        let entry = entry.with_path(source_dir)?;
        let source_child = entry.path();
        let file_name = match source_child.file_name() {
            Some(n) => n.to_owned(),
            None => continue,
        };
        let dest_child = dest_dir.join(&file_name);

        // Check cancellation
        if super::super::state::is_cancelled(&state.intent) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        if source_child.is_dir() && dest_child.exists() && dest_child.is_dir() {
            // Both are directories, recurse
            merge_move_directory(
                &source_child,
                &dest_child,
                config,
                events,
                operation_id,
                state,
                apply_to_all_resolution,
                move_tx,
                files_skipped,
            )?;
        } else if path_exists_or_is_symlink(&dest_child) {
            // File conflict (or type mismatch)
            match resolve_conflict(
                &source_child,
                &dest_child,
                config,
                events,
                operation_id,
                state,
                apply_to_all_resolution,
            )? {
                Some(resolved) => {
                    // Hook the downloads watcher's ignore set for both
                    // halves of the rename; no-ops outside ~/Downloads.
                    crate::downloads::note_pending_write_for_cmdr(&source_child);
                    crate::downloads::note_pending_write_for_cmdr(&resolved.path);
                    fs::rename(&source_child, &resolved.path).with_path(&source_child)?;
                    move_tx.record(source_child, resolved.path);
                }
                None => {
                    // Skip: source file stays in place
                    *files_skipped += 1;
                    continue;
                }
            }
        } else {
            // No conflict, just rename
            crate::downloads::note_pending_write_for_cmdr(&source_child);
            crate::downloads::note_pending_write_for_cmdr(&dest_child);
            fs::rename(&source_child, &dest_child).with_path(&source_child)?;
            move_tx.record(source_child, dest_child);
        }
    }

    // Remove the source directory if it's now empty
    if fs::read_dir(source_dir)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
    {
        let _ = fs::remove_dir(source_dir);
    }

    Ok(())
}

/// Performs cross-filesystem move using atomic staging pattern.
/// This ensures source files remain intact if the operation fails.
fn move_with_staging(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    destination: &Path,
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    // Phase 1: Scan (or reuse cached preview results)
    let scan_result = if let Some(preview_id) = &config.preview_id {
        // Volume scans cache aggregate stats with an empty `files` list; the
        // per-file move loop needs the file list, so treat an empty-files
        // cache hit the same as a miss and fall through to a fresh local scan.
        if let Some(cached) = take_cached_scan_result(preview_id).filter(|c| !c.files.is_empty()) {
            log::debug!(
                "move_with_staging: reusing cached scan for operation_id={}, preview_id={}, files={}, bytes={}",
                operation_id,
                preview_id,
                cached.file_count,
                cached.total_bytes
            );
            cached
        } else {
            log::warn!(
                "preview_id={} cache miss despite frontend coordination, starting fresh scan for operation_id={}",
                preview_id,
                operation_id
            );
            scan_sources(
                sources,
                state,
                events,
                operation_id,
                WriteOperationType::Move,
                config.sort_column,
                config.sort_order,
            )?
        }
    } else {
        scan_sources(
            sources,
            state,
            events,
            operation_id,
            WriteOperationType::Move,
            config.sort_column,
            config.sort_order,
        )?
    };

    // Create staging directory
    let staging_dir = destination.join(format!(".cmdr-staging-{}", operation_id));
    fs::create_dir(&staging_dir).map_err(|e| WriteOperationError::IoError {
        path: staging_dir.display().to_string(),
        message: format!("Failed to create staging directory: {}", e),
    })?;

    // Phase 2: Copy files to staging directory (using scan results, same as copy operation)
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut files_skipped = 0usize;
    let mut apply_to_all_resolution: Option<ConflictResolution> = None;
    let mut created_dirs: HashSet<PathBuf> = HashSet::new();

    // Emit initial copying phase event
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            WriteOperationPhase::Copying,
            None,
            0,
            scan_result.file_count,
            0,
            scan_result.total_bytes,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::Copying,
        None,
        0,
        scan_result.file_count,
        0,
        scan_result.total_bytes,
    );

    log::debug!(
        "move_with_staging: starting copy loop for operation_id={}, {} files",
        operation_id,
        scan_result.files.len()
    );

    let mut tracker = SourceItemTracker::new(&scan_result.files);

    let copy_result: Result<(), WriteOperationError> = (|| {
        for file_info in &scan_result.files {
            log::debug!(
                "move_with_staging: copying file {} ({} bytes) to staging",
                file_info.path.display(),
                file_info.size
            );
            // Copy to staging directory instead of final destination
            copy_single_item(
                &file_info.path,
                file_info.dest_path(&staging_dir),
                file_info.is_symlink,
                // Write footprint: a cross-FS move stages a full copy of every
                // file (including hardlink dupes) before deleting the sources.
                file_info.size,
                &mut files_done,
                &mut bytes_done,
                scan_result.file_count,
                scan_result.total_bytes,
                state,
                events,
                operation_id,
                WriteOperationType::Move,
                &state.progress_interval,
                config,
                &mut transaction,
                &mut apply_to_all_resolution,
                &mut created_dirs,
            )?;

            if let Some(source_path) = tracker.record(file_info) {
                events.emit_source_item_done(WriteSourceItemDoneEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source_path.display().to_string(),
                });
            }
        }
        Ok(())
    })();

    if let Err(e) = copy_result {
        // Cleanup staging directory in background (may block on network mounts)
        remove_dir_all_in_background(staging_dir.clone());
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            e.clone(),
        ));
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

            // When both staged and final are directories, merge recursively.
            // No MoveTransaction needed here: staging cleanup handles rollback.
            let mut staging_move_tx = MoveTransaction::new();
            if staged_path.is_dir() && final_path.exists() && final_path.is_dir() {
                merge_move_directory(
                    &staged_path,
                    &final_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                    &mut staging_move_tx,
                    &mut files_skipped,
                )?;
            } else if final_path.exists() {
                // File conflict (or type mismatch)
                match resolve_conflict(
                    source,
                    &final_path,
                    config,
                    events,
                    operation_id,
                    state,
                    &mut apply_to_all_resolution,
                )? {
                    Some(resolved) => {
                        // Cross-FS move: stage→final lands the file at its
                        // final visible name. Register so the watcher
                        // suppresses; no-ops outside ~/Downloads.
                        crate::downloads::note_pending_write_for_cmdr(&resolved.path);
                        fs::rename(&staged_path, &resolved.path).map_err(|e| WriteOperationError::IoError {
                            path: staged_path.display().to_string(),
                            message: format!("Failed to move from staging: {}", e),
                        })?;
                    }
                    None => {
                        // Skip - remove from staging
                        if staged_path.is_dir() {
                            let _ = fs::remove_dir_all(&staged_path);
                        } else {
                            let _ = fs::remove_file(&staged_path);
                        }
                        files_skipped += 1;
                        continue;
                    }
                }
            } else {
                // No conflict, just rename from staging to final
                crate::downloads::note_pending_write_for_cmdr(&final_path);
                fs::rename(&staged_path, &final_path).map_err(|e| WriteOperationError::IoError {
                    path: staged_path.display().to_string(),
                    message: format!("Failed to move from staging: {}", e),
                })?;
            }
        }
        Ok(())
    })();

    if let Err(e) = rename_result {
        // Cleanup staging directory in background (may block on network mounts)
        remove_dir_all_in_background(staging_dir);
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Move,
            e.clone(),
        ));
        return Err(e);
    }

    // Phase 4: Delete source files (only after successful copy+rename)
    delete_sources_after_move(events, operation_id, state, sources, files_done)?;

    // Phase 5: Remove empty staging directory
    let _ = fs::remove_dir(&staging_dir);

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: files_done,
        files_skipped,
        bytes_processed: bytes_done,
    });

    Ok(())
}

fn delete_sources_after_move(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    files_done: usize,
) -> Result<(), WriteOperationError> {
    for source in sources {
        // Check cancellation
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Move,
                files_processed: files_done,
                rolled_back: false, // Source deletion phase - nothing to rollback
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Use symlink_metadata to check if it still exists
        if fs::symlink_metadata(source).is_ok() {
            if source.is_dir() {
                fs::remove_dir_all(source).with_path(source)?;
            } else {
                fs::remove_file(source).with_path(source)?;
            }

            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: source.display().to_string(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "move_op_tests.rs"]
mod tests;
