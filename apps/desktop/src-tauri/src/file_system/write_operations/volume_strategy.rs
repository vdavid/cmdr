//! Copy strategy routing for volume-to-volume operations.
//!
//! Determines the appropriate copy method based on volume types:
//! - Both local: delegate to native copy (handled upstream in volume_copy)
//! - Local → MTP: dest.import_from_local()
//! - MTP → Local: source.export_to_local()
//! - MTP → MTP file: streaming transfer
//! - MTP → MTP directory: export to temp local, then import

// TODO: Remove this once volume_copy is integrated into Tauri commands (Phase 5)
#![allow(dead_code, reason = "Volume copy not yet integrated into Tauri commands")]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use uuid::Uuid;

use super::state::WriteOperationState;
use crate::file_system::volume::{Volume, VolumeError};

/// Checks if a volume is a real local filesystem (not MTP or other virtual volumes).
pub(super) fn is_local_volume(volume: &dyn Volume) -> bool {
    let root = volume.root();
    // Local volumes start with "/" but NOT "/mtp-volume/"
    root.starts_with("/") && !root.starts_with("/mtp-volume/")
}

/// Copies a single path from source volume to destination volume.
///
/// Determines the appropriate strategy based on volume types:
/// - If both are MTP and source is a file: Use streaming for direct transfer
/// - If both are MTP and source is a directory: Use temp local (export then import)
/// - If source is local: dest.import_from_local()
/// - If dest is local: source.export_to_local()
/// - Otherwise: Not supported
pub(super) fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<u64, VolumeError> {
    // Check cancellation
    if state.cancelled.load(Ordering::Relaxed) {
        return Err(VolumeError::IoError("Operation cancelled".to_string()));
    }

    let source_is_local = is_local_volume(source_volume.as_ref());
    let dest_is_local = is_local_volume(dest_volume.as_ref());

    // Handle non-local to non-local (e.g., MTP → MTP)
    if !source_is_local && !dest_is_local {
        // Check if source is a directory
        let is_dir = source_volume.is_directory(source_path).unwrap_or(false);

        if is_dir {
            // For directories, use temp local approach: export to temp, import from temp
            log::debug!(
                "copy_single_path: MTP→MTP directory copy via temp local: {} -> {}",
                source_path.display(),
                dest_path.display()
            );
            return copy_via_temp_local(source_volume, source_path, dest_volume, dest_path);
        }

        // For files, try streaming if both volumes support it
        if source_volume.supports_streaming() && dest_volume.supports_streaming() {
            log::debug!(
                "copy_single_path: using streaming for {} -> {}",
                source_path.display(),
                dest_path.display()
            );
            let stream = source_volume.open_read_stream(source_path)?;
            let size = stream.total_size();
            return dest_volume.write_from_stream(dest_path, size, stream);
        }

        // Neither supports streaming and it's not a directory - not supported
        return Err(VolumeError::NotSupported);
    }

    if source_is_local && !dest_is_local {
        // Source is local, dest is not (e.g., Local → MTP)
        // Use import_from_local on destination
        let local_source = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            source_volume.root().join(source_path)
        };
        dest_volume.import_from_local(&local_source, dest_path)
    } else if !source_is_local && dest_is_local {
        // Source is not local, dest is local (e.g., MTP → Local)
        // Use export_to_local on source
        let local_dest = if dest_path.is_absolute() {
            dest_path.to_path_buf()
        } else {
            dest_volume.root().join(dest_path)
        };
        source_volume.export_to_local(source_path, &local_dest)
    } else {
        // Both are local, use export which resolves paths internally
        // Note: export_to_local takes a path relative to the volume root for source,
        // and an absolute local path for destination
        let local_dest = if dest_path.is_absolute() {
            dest_path.to_path_buf()
        } else {
            dest_volume.root().join(dest_path)
        };
        source_volume.export_to_local(source_path, &local_dest)
    }
}

/// Copies a path between two non-local volumes via a temporary local directory.
///
/// This is used for MTP-to-MTP directory copies where streaming doesn't work.
/// The process:
/// 1. Export from source to a temp local directory
/// 2. Import from temp local to destination
/// 3. Clean up temp directory
fn copy_via_temp_local(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
) -> Result<u64, VolumeError> {
    // Create a temporary directory for the transfer
    let temp_dir = std::env::temp_dir().join(format!("cmdr_volume_copy_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| VolumeError::IoError(e.to_string()))?;

    // Determine the name of the item being copied
    let item_name = source_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".to_string());
    let temp_item_path = temp_dir.join(&item_name);

    log::debug!(
        "copy_via_temp_local: exporting {} to temp {}",
        source_path.display(),
        temp_item_path.display()
    );

    // Step 1: Export from source to temp local
    let bytes = source_volume.export_to_local(source_path, &temp_item_path)?;

    log::debug!(
        "copy_via_temp_local: importing from temp {} to {}",
        temp_item_path.display(),
        dest_path.display()
    );

    // Step 2: Import from temp local to destination
    let result = dest_volume.import_from_local(&temp_item_path, dest_path);

    // Step 3: Clean up temp directory (best effort)
    if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
        log::warn!("Failed to clean up temp directory {}: {}", temp_dir.display(), e);
    }

    // Return the bytes from export (import might report different due to protocol overhead)
    result.or(Ok(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};

    #[test]
    fn test_copy_single_path_local_to_local() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_copy_single_src");
        let dst_dir = std::env::temp_dir().join("cmdr_copy_single_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        fs::write(src_dir.join("source.txt"), "Source content").unwrap();

        let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
        let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

        let state = Arc::new(WriteOperationState {
            cancelled: Arc::new(AtomicBool::new(false)),
            skip_rollback: AtomicBool::new(false),
            progress_interval: Duration::from_millis(200),
            pending_resolution: std::sync::RwLock::new(None),
            conflict_condvar: std::sync::Condvar::new(),
            conflict_mutex: std::sync::Mutex::new(false),
        });

        let bytes = copy_single_path(&source, Path::new("source.txt"), &dest, Path::new("dest.txt"), &state).unwrap();

        assert_eq!(bytes, 14); // "Source content"
        assert_eq!(fs::read_to_string(dst_dir.join("dest.txt")).unwrap(), "Source content");

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }

    #[test]
    fn test_copy_single_path_cancelled() {
        use std::fs;

        let src_dir = std::env::temp_dir().join("cmdr_copy_cancel_src");
        let dst_dir = std::env::temp_dir().join("cmdr_copy_cancel_dst");
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();

        fs::write(src_dir.join("source.txt"), "Content").unwrap();

        let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Source", src_dir.to_str().unwrap()));
        let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.to_str().unwrap()));

        let state = Arc::new(WriteOperationState {
            cancelled: Arc::new(AtomicBool::new(true)), // Already cancelled
            skip_rollback: AtomicBool::new(false),
            progress_interval: Duration::from_millis(200),
            pending_resolution: std::sync::RwLock::new(None),
            conflict_condvar: std::sync::Condvar::new(),
            conflict_mutex: std::sync::Mutex::new(false),
        });

        let result = copy_single_path(&source, Path::new("source.txt"), &dest, Path::new("dest.txt"), &state);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), VolumeError::IoError(msg) if msg.contains("cancelled")));

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }
}
