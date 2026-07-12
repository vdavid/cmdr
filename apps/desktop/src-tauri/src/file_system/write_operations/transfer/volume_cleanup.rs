//! Volume cleanup / rollback helpers for the volume-aware copy and move paths.
//!
//! `volume_rollback_with_progress` reverses copied files (with reverse-progress
//! events) on cancel/failure, and `delete_volume_path_recursive` clears a file
//! or directory tree off a volume. Both are shared by `volume_copy` and
//! `volume_move`, so they live here rather than inside either operation module.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::super::state::{OperationIntent, WriteOperationState, load_intent, update_operation_status};
use super::super::types::{OperationEventSink, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use crate::file_system::volume::{Volume, VolumeError};

/// Rolls back copied files on a volume with progress events, matching the local copy's
/// `rollback_with_progress` pattern. Deletes paths in reverse order so that files inside
/// directories are removed before the directories themselves.
///
/// `copied_paths` are the individual destination FILES the operation wrote (never a merged
/// directory root). After deleting them, `created_dirs` — the directories this operation
/// NEWLY created — are removed deepest-first with a non-recursive, empty-only delete. A
/// directory that still holds a pre-existing sibling (a dest-only file the user already had,
/// or a kept-partial under cancel) is left in place, so rollback never destroys data this
/// operation didn't write.
///
/// Returns `true` if rollback completed fully, `false` if the user cancelled it.
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
pub(super) async fn volume_rollback_with_progress(
    volume: &Arc<dyn Volume>,
    copied_paths: &[PathBuf],
    created_dirs: &[PathBuf],
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> bool {
    let paths_to_delete = copied_paths.len();
    let mut paths_deleted = 0usize;
    let mut last_progress_time = Instant::now();

    // Emit initial rollback phase event
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Copy,
            WriteOperationPhase::RollingBack,
            None,
            files_at_cancel,
            files_total,
            bytes_at_cancel,
            bytes_total,
        ),
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

        // Each copied path may be a file or a directory tree, so delete recursively
        if let Err(e) = delete_volume_path_recursive(volume, path).await {
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
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Copy,
                    WriteOperationPhase::RollingBack,
                    Some(current_file_name.clone()),
                    remaining_files,
                    files_total,
                    remaining_bytes,
                    bytes_total,
                ),
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

    // Prune the directories this operation newly created, deepest-first, with a
    // non-recursive empty-only delete. `created_dirs` is in creation order
    // (shallowest first), so iterating in reverse hits leaves before their
    // parents. A directory that still holds a pre-existing sibling (a dest-only
    // file the user already had) won't be empty, so its `delete` fails with
    // NotFound/IoError on real backends and we leave it standing — exactly the
    // protection that keeps rollback from destroying untouched user data. We
    // deliberately do NOT use `delete_volume_path_recursive` here: that would
    // recurse into and delete those pre-existing siblings.
    for dir in created_dirs.iter().rev() {
        if load_intent(&state.intent) == OperationIntent::Stopped {
            return false;
        }
        if let Err(e) = volume.delete(dir).await {
            log::debug!(
                "volume_rollback_with_progress: not removing created dir {} (likely non-empty, kept): {:?}",
                dir.display(),
                e
            );
        }
    }

    true
}

/// Recursively deletes a file or directory on a volume.
///
/// For files: calls `volume.delete()` directly.
/// For directories: lists contents, deletes children (files first, then subdirs),
/// then deletes the directory itself. Best-effort: logs errors but continues.
pub(in crate::file_system::write_operations) async fn delete_volume_path_recursive(
    volume: &Arc<dyn Volume>,
    path: &Path,
) -> Result<(), VolumeError> {
    let is_dir = match volume.is_directory(path).await {
        Ok(true) => true,
        Ok(false) => false,
        Err(_) => {
            // Path may not exist (already deleted or never fully created). Nothing to do.
            return Ok(());
        }
    };

    if !is_dir {
        return volume.delete(path).await;
    }

    // List directory contents and delete children first
    let children = volume.list_directory(path, None).await?;

    // Delete files first, then recurse into subdirectories
    for child in &children {
        let child_path = PathBuf::from(&child.path);
        if child.is_directory {
            if let Err(e) = Box::pin(delete_volume_path_recursive(volume, &child_path)).await {
                log::warn!(
                    "delete_volume_path_recursive: failed to delete subdirectory {}: {:?}",
                    child_path.display(),
                    e
                );
            }
        } else if let Err(e) = volume.delete(&child_path).await {
            log::warn!(
                "delete_volume_path_recursive: failed to delete file {}: {:?}",
                child_path.display(),
                e
            );
        }
    }

    // Delete the now-empty directory
    volume.delete(path).await
}
