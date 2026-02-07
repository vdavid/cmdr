//! Conflict resolution for volume-to-volume copy operations.
//!
//! Handles what to do when a destination file already exists:
//! - Stop: Emit conflict event, wait for user input via condvar
//! - Skip: Return None to skip this file
//! - Overwrite: Delete existing, return same path
//! - Rename: Find unique name like "file (1).txt"

// TODO: Remove this once volume_copy is integrated into Tauri commands (Phase 5)
#![allow(dead_code, reason = "Volume copy not yet integrated into Tauri commands")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::state::WriteOperationState;
use super::types::{ConflictResolution, VolumeCopyConfig, WriteConflictEvent, WriteOperationError};
use crate::file_system::volume::Volume;

/// Resolves a file conflict for volume-to-volume copy.
/// Returns None if file should be skipped, or Some(path) with the resolved destination path.
#[allow(
    clippy::too_many_arguments,
    reason = "Conflict resolution requires many context parameters"
)]
pub(super) fn resolve_volume_conflict(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
) -> Result<Option<PathBuf>, WriteOperationError> {
    use tauri::Emitter;

    // Determine effective conflict resolution
    let resolution = if let Some(saved_resolution) = apply_to_all_resolution {
        // Use saved "apply to all" resolution
        *saved_resolution
    } else {
        config.conflict_resolution
    };

    match resolution {
        ConflictResolution::Stop => {
            // Need to prompt user - gather metadata for the conflict event
            let source_scan = source_volume.scan_for_copy(source_path).ok();
            let source_size = source_scan.as_ref().map(|s| s.total_bytes).unwrap_or(0);

            // Try to get destination size by scanning (best effort)
            let dest_size = dest_volume
                .scan_for_copy(dest_path)
                .ok()
                .map(|s| s.total_bytes)
                .unwrap_or(0);

            // We can't easily get modification times from Volume trait, so use None
            let source_modified: Option<i64> = None;
            let destination_modified: Option<i64> = None;
            let destination_is_newer = false;
            let size_difference = dest_size as i64 - source_size as i64;

            let _ = app.emit(
                "write-conflict",
                WriteConflictEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source_path.display().to_string(),
                    destination_path: dest_path.display().to_string(),
                    source_size,
                    destination_size: dest_size,
                    source_modified,
                    destination_modified,
                    destination_is_newer,
                    size_difference,
                },
            );

            // Wait for user to call resolve_write_conflict
            let guard = state.conflict_mutex.lock().unwrap_or_else(|e| e.into_inner());
            let _guard = state
                .conflict_condvar
                .wait_while(guard, |_| {
                    // Keep waiting while:
                    // 1. No pending resolution
                    // 2. Not cancelled
                    let has_resolution = state.pending_resolution.read().map(|r| r.is_some()).unwrap_or(false);
                    let is_cancelled = state.cancelled.load(Ordering::Relaxed);
                    !has_resolution && !is_cancelled
                })
                .unwrap();

            // Check if cancelled
            if state.cancelled.load(Ordering::Relaxed) {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            // Get the resolution
            let response = state.pending_resolution.write().ok().and_then(|mut r| r.take());

            if let Some(response) = response {
                // Save for future conflicts if apply_to_all
                if response.apply_to_all {
                    *apply_to_all_resolution = Some(response.resolution);
                }

                // Apply the chosen resolution
                apply_volume_conflict_resolution(response.resolution, dest_volume, dest_path)
            } else {
                // No resolution provided, treat as error
                Err(WriteOperationError::DestinationExists {
                    path: dest_path.display().to_string(),
                })
            }
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            apply_volume_conflict_resolution(ConflictResolution::Overwrite, dest_volume, dest_path)
        }
        ConflictResolution::Rename => {
            apply_volume_conflict_resolution(ConflictResolution::Rename, dest_volume, dest_path)
        }
    }
}

/// Applies a specific conflict resolution for volume copy.
/// Returns None for Skip, or Some(path) with the path to write to.
fn apply_volume_conflict_resolution(
    resolution: ConflictResolution,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
) -> Result<Option<PathBuf>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Delete existing item first, then return the same path
            // Note: For directories, this will fail if not empty - that's expected behavior
            if let Err(e) = dest_volume.delete(dest_path) {
                log::warn!(
                    "Failed to delete existing item for overwrite: {} - {}",
                    dest_path.display(),
                    e
                );
                // Continue anyway - the copy might succeed if it's a file being overwritten
            }
            Ok(Some(dest_path.to_path_buf()))
        }
        ConflictResolution::Rename => {
            // Find a unique name - we need to check what exists on the volume
            let unique_path = find_unique_volume_name(dest_volume, dest_path);
            Ok(Some(unique_path))
        }
    }
}

/// Finds a unique filename on a volume by appending " (1)", " (2)", etc.
fn find_unique_volume_name(dest_volume: &Arc<dyn Volume>, path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());

    let mut counter = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);
        if !dest_volume.exists(&new_path) {
            return new_path;
        }
        counter += 1;

        // Safety limit to prevent infinite loop
        if counter > 1000 {
            // Just return with counter - extremely unlikely to happen
            let new_name = match &extension {
                Some(ext) => format!("{} ({}).{}", stem, counter, ext),
                None => format!("{} ({})", stem, counter),
            };
            return parent.join(new_name);
        }
    }
}
