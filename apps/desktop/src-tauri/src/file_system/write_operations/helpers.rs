//! Helper functions for write operations.
//!
//! Contains validation, conflict resolution, and utility functions.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use uuid::Uuid;

#[cfg(target_os = "macos")]
use crate::file_system::macos_copy::{CopyProgressContext, copy_single_file_native};

use super::state::WriteOperationState;
use super::types::{ConflictInfo, ConflictResolution, WriteConflictEvent, WriteOperationConfig, WriteOperationError};

// ============================================================================
// Validation helpers
// ============================================================================

pub(crate) fn validate_sources(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    for source in sources {
        // Use symlink_metadata to check existence without following symlinks
        if fs::symlink_metadata(source).is_err() {
            return Err(WriteOperationError::SourceNotFound {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination(destination: &Path) -> Result<(), WriteOperationError> {
    // Destination must exist and be a directory
    if !destination.exists() {
        return Err(WriteOperationError::SourceNotFound {
            path: destination.display().to_string(),
        });
    }
    if !destination.is_dir() {
        return Err(WriteOperationError::IoError {
            path: destination.display().to_string(),
            message: "Destination must be a directory".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn validate_not_same_location(sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
    for source in sources {
        if let Some(parent) = source.parent()
            && parent == destination
        {
            return Err(WriteOperationError::SameLocation {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination_not_inside_source(
    sources: &[PathBuf],
    destination: &Path,
) -> Result<(), WriteOperationError> {
    for source in sources {
        if source.is_dir() && destination.starts_with(source) {
            return Err(WriteOperationError::DestinationInsideSource {
                source: source.display().to_string(),
                destination: destination.display().to_string(),
            });
        }
    }
    Ok(())
}

// ============================================================================
// Symlink loop detection
// ============================================================================

/// Checks if a path creates a symlink loop.
pub(super) fn is_symlink_loop(path: &Path, visited: &HashSet<PathBuf>) -> bool {
    if let Ok(canonical) = path.canonicalize() {
        visited.contains(&canonical)
    } else {
        false
    }
}

// ============================================================================
// Filesystem detection
// ============================================================================

/// Checks if two paths are on the same filesystem using device IDs.
#[cfg(unix)]
pub(crate) fn is_same_filesystem(source: &Path, destination: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let source_meta = fs::metadata(source)?;
    let dest_meta = fs::metadata(destination)?;

    Ok(source_meta.dev() == dest_meta.dev())
}

#[cfg(not(unix))]
pub(crate) fn is_same_filesystem(_source: &Path, _destination: &Path) -> std::io::Result<bool> {
    // On non-Unix, assume different filesystem to be safe (will use copy+delete)
    Ok(false)
}

// ============================================================================
// Async sync for durability
// ============================================================================

/// Spawns a background thread to call sync() for durability.
/// This ensures writes are flushed to disk without blocking the completion event.
pub(super) fn spawn_async_sync() {
    std::thread::spawn(|| {
        // On Unix, call sync() to flush all filesystem buffers
        #[cfg(unix)]
        unsafe {
            libc::sync();
        }
        // On other platforms, this is a no-op (sync is not easily available)
    });
}

// ============================================================================
// Conflict handling helpers
// ============================================================================

/// Result of applying a conflict resolution.
#[derive(Debug)]
pub(super) struct ResolvedDestination {
    /// The path to write to
    pub path: PathBuf,
    /// Whether this is an overwrite that needs safe handling
    pub needs_safe_overwrite: bool,
}

/// Resolves a file conflict based on the configured resolution mode.
/// Returns the resolved destination info, or None if the file should be skipped.
/// Also returns whether the resolution should be applied to all future conflicts.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
pub(super) fn resolve_conflict(
    source: &Path,
    dest_path: &Path,
    config: &WriteOperationConfig,
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut Option<ConflictResolution>,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    use tauri::Emitter;

    // Determine effective conflict resolution
    let resolution = if let Some(saved_resolution) = apply_to_all_resolution {
        // Use saved "apply to all" resolution
        *saved_resolution
    } else if config.overwrite {
        ConflictResolution::Overwrite
    } else {
        config.conflict_resolution
    };

    match resolution {
        ConflictResolution::Stop => {
            // Emit conflict event for frontend to handle
            let source_meta = fs::metadata(source).ok();
            let dest_meta = fs::metadata(dest_path).ok();

            let destination_is_newer = match (&source_meta, &dest_meta) {
                (Some(s), Some(d)) => {
                    let src_time = s.modified().ok();
                    let dst_time = d.modified().ok();
                    matches!((src_time, dst_time), (Some(src), Some(dst)) if dst > src)
                }
                _ => false,
            };

            let source_size = source_meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let destination_size = dest_meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let size_difference = destination_size as i64 - source_size as i64;

            let source_modified = source_meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            let destination_modified = dest_meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);

            let _ = app.emit(
                "write-conflict",
                WriteConflictEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source.display().to_string(),
                    destination_path: dest_path.display().to_string(),
                    source_size,
                    destination_size,
                    source_modified,
                    destination_modified,
                    destination_is_newer,
                    size_difference,
                },
            );

            // Wait for user to call resolve_write_conflict
            let guard = state.conflict_mutex.lock().unwrap();
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

                // Now apply the chosen resolution
                apply_resolution(response.resolution, dest_path)
            } else {
                // No resolution provided, treat as error
                Err(WriteOperationError::DestinationExists {
                    path: dest_path.display().to_string(),
                })
            }
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => apply_resolution(ConflictResolution::Overwrite, dest_path),
        ConflictResolution::Rename => apply_resolution(ConflictResolution::Rename, dest_path),
    }
}

/// Applies a specific conflict resolution to a destination path.
/// Returns None for Skip, or ResolvedDestination with path and overwrite flag.
fn apply_resolution(
    resolution: ConflictResolution,
    dest_path: &Path,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Don't delete here - the copy function will use safe overwrite pattern
            Ok(Some(ResolvedDestination {
                path: dest_path.to_path_buf(),
                needs_safe_overwrite: true,
            }))
        }
        ConflictResolution::Rename => {
            // Find a unique name by appending " (1)", " (2)", etc.
            let unique_path = find_unique_name(dest_path);
            Ok(Some(ResolvedDestination {
                path: unique_path,
                needs_safe_overwrite: false,
            }))
        }
    }
}

/// Finds a unique filename by appending " (1)", " (2)", etc.
pub(super) fn find_unique_name(path: &Path) -> PathBuf {
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
        if !new_path.exists() {
            return new_path;
        }
        counter += 1;
    }
}

// ============================================================================
// Safe overwrite helpers
// ============================================================================

/// Performs a safe overwrite using temp+rename pattern.
/// This ensures the original file is preserved if the copy fails.
///
/// Steps:
/// 1. Copy source to `dest.cmdr-tmp-{uuid}` (temp file in same directory)
/// 2. Rename original dest to `dest.cmdr-backup-{uuid}`
/// 3. Rename temp to final dest path
/// 4. Delete backup
///
/// If any step fails before step 3 completes, the original dest is intact.
pub(super) fn safe_overwrite_file(
    source: &Path,
    dest: &Path,
    #[cfg(target_os = "macos")] context: Option<&CopyProgressContext>,
) -> Result<u64, WriteOperationError> {
    let uuid = Uuid::new_v4();
    let parent = dest.parent().unwrap_or(Path::new("."));
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let temp_path = parent.join(format!("{}.cmdr-tmp-{}", file_name, uuid));
    let backup_path = parent.join(format!("{}.cmdr-backup-{}", file_name, uuid));

    // Step 1: Copy source to temp
    #[cfg(target_os = "macos")]
    let bytes = copy_single_file_native(source, &temp_path, false, context)?;
    #[cfg(not(target_os = "macos"))]
    let bytes = fs::copy(source, &temp_path).map_err(|e| WriteOperationError::IoError {
        path: source.display().to_string(),
        message: e.to_string(),
    })?;

    // Step 2: Rename original dest to backup
    if let Err(e) = fs::rename(dest, &backup_path) {
        // Failed to backup - clean up temp and return error
        let _ = fs::remove_file(&temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to backup existing file: {}", e),
        });
    }

    // Step 3: Rename temp to final dest
    if let Err(e) = fs::rename(&temp_path, dest) {
        // Failed to rename - restore backup and clean up
        let _ = fs::rename(&backup_path, dest);
        let _ = fs::remove_file(&temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to finalize overwrite: {}", e),
        });
    }

    // Step 4: Delete backup (non-critical, ignore errors)
    let _ = fs::remove_file(&backup_path);

    Ok(bytes)
}

/// Performs a safe overwrite for directories using temp+rename pattern.
pub(super) fn safe_overwrite_dir(dest: &Path) -> Result<PathBuf, WriteOperationError> {
    let uuid = Uuid::new_v4();
    let parent = dest.parent().unwrap_or(Path::new("."));
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let backup_path = parent.join(format!("{}.cmdr-backup-{}", file_name, uuid));

    // Rename original dest to backup
    fs::rename(dest, &backup_path).map_err(|e| WriteOperationError::IoError {
        path: dest.display().to_string(),
        message: format!("Failed to backup existing directory: {}", e),
    })?;

    // Return the backup path so caller can delete it after successful copy
    Ok(backup_path)
}

// ============================================================================
// Conflict info helpers
// ============================================================================

/// Calculates destination path for a source file relative to source root.
pub(super) fn calculate_dest_path(
    path: &Path,
    source_root: &Path,
    dest_root: &Path,
) -> Result<PathBuf, WriteOperationError> {
    // If path is the source root itself, use the file name in dest_root
    if path == source_root {
        let file_name = path.file_name().ok_or_else(|| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Invalid source path".to_string(),
        })?;
        return Ok(dest_root.join(file_name));
    }

    // Otherwise, strip the source root's parent and join with dest_root
    let source_parent = source_root.parent().unwrap_or(source_root);
    let relative = path
        .strip_prefix(source_parent)
        .map_err(|_| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Failed to calculate relative path".to_string(),
        })?;

    Ok(dest_root.join(relative))
}

/// Creates ConflictInfo for a source/destination pair.
pub(super) fn create_conflict_info(
    source: &Path,
    dest: &Path,
    source_metadata: &fs::Metadata,
) -> Result<Option<ConflictInfo>, WriteOperationError> {
    let dest_metadata = match fs::symlink_metadata(dest) {
        Ok(m) => m,
        Err(_) => return Ok(None), // No conflict if dest doesn't exist
    };

    let source_modified = source_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let dest_modified = dest_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let destination_is_newer = match (source_modified, dest_modified) {
        (Some(s), Some(d)) => d > s,
        _ => false,
    };

    Ok(Some(ConflictInfo {
        source_path: source.display().to_string(),
        destination_path: dest.display().to_string(),
        source_size: source_metadata.len(),
        destination_size: dest_metadata.len(),
        source_modified,
        destination_modified: dest_modified,
        destination_is_newer,
        is_directory: source_metadata.is_dir(),
    }))
}

/// Samples conflicts if there are too many, using reservoir sampling.
pub(super) fn sample_conflicts(conflicts: Vec<ConflictInfo>, max_count: usize) -> (Vec<ConflictInfo>, bool) {
    if conflicts.len() <= max_count {
        return (conflicts, false);
    }

    // Use reservoir sampling for uniform random selection
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut sampled: Vec<ConflictInfo> = conflicts.iter().take(max_count).cloned().collect();

    for (i, conflict) in conflicts.iter().enumerate().skip(max_count) {
        // Deterministic "random" based on path hash for reproducibility
        let mut hasher = DefaultHasher::new();
        conflict.source_path.hash(&mut hasher);
        i.hash(&mut hasher);
        let hash = hasher.finish();
        let j = (hash as usize) % (i + 1);

        if j < max_count {
            sampled[j] = conflict.clone();
        }
    }

    (sampled, true)
}
