//! Copy strategy routing for volume-to-volume operations.
//!
//! Determines the appropriate copy method based on volume types:
//! - Both local: delegate to native copy (handled upstream in volume_copy)
//! - Local → MTP: dest.import_from_local()
//! - MTP → Local: source.export_to_local()
//! - MTP → MTP file: streaming transfer
//! - MTP → MTP directory: export to temp local, then import

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;

use uuid::Uuid;

use super::state::WriteOperationState;
use crate::file_system::volume::{Volume, VolumeError};

/// Checks if a volume is a real local filesystem (not MTP, SMB, or other virtual volumes).
///
/// Uses `local_path()` which returns `Some` only for volumes where `std::fs` operations
/// work directly on the volume's paths. `SmbVolume` returns `None` (ops go through smb2),
/// `MtpVolume` returns `None` (ops go through USB), `LocalPosixVolume` returns `Some`.
pub(super) fn is_local_volume(volume: &dyn Volume) -> bool {
    volume.local_path().is_some()
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
    on_file_progress: &dyn Fn(u64, u64) -> ControlFlow<()>,
    on_file_complete: &dyn Fn(),
) -> Result<u64, VolumeError> {
    // Check cancellation
    if super::state::is_cancelled(&state.intent) {
        return Err(VolumeError::IoError {
            message: "Operation cancelled".to_string(),
            raw_os_error: None,
        });
    }

    let source_is_local = is_local_volume(source_volume.as_ref());
    let dest_is_local = is_local_volume(dest_volume.as_ref());

    // Handle non-local to non-local (like MTP → MTP)
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

        // Neither supports streaming — fall back to temp local (export then import)
        log::debug!(
            "copy_single_path: no streaming support, using temp local for {} -> {}",
            source_path.display(),
            dest_path.display()
        );
        return copy_via_temp_local(source_volume, source_path, dest_volume, dest_path);
    }

    if source_is_local && !dest_is_local {
        // Source is local, dest is not (like Local → SMB/MTP)
        let local_source = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            source_volume.root().join(source_path)
        };
        // For directories, walk the tree ourselves so we can check cancellation between files.
        // import_from_local(dir) would import everything in one shot with no cancellation.
        if local_source.is_dir() {
            import_directory_cancellable(
                &local_source,
                dest_path,
                dest_volume,
                state,
                on_file_progress,
                on_file_complete,
            )
        } else {
            let bytes = dest_volume.import_from_local_with_progress(&local_source, dest_path, on_file_progress)?;
            on_file_complete();
            Ok(bytes)
        }
    } else if !source_is_local && dest_is_local {
        // Source is not local, dest is local (like SMB/MTP → Local)
        let local_dest = if dest_path.is_absolute() {
            dest_path.to_path_buf()
        } else {
            dest_volume.root().join(dest_path)
        };
        // For directories, walk the tree ourselves for cancellation support.
        let is_dir = source_volume.is_directory(source_path).unwrap_or(false);
        if is_dir {
            export_directory_cancellable(
                source_path,
                &local_dest,
                source_volume,
                state,
                on_file_progress,
                on_file_complete,
            )
        } else {
            let bytes = source_volume.export_to_local_with_progress(source_path, &local_dest, on_file_progress)?;
            on_file_complete();
            Ok(bytes)
        }
    } else {
        // Both are local, use export which resolves paths internally
        let local_dest = if dest_path.is_absolute() {
            dest_path.to_path_buf()
        } else {
            dest_volume.root().join(dest_path)
        };
        let bytes = source_volume.export_to_local(source_path, &local_dest)?;
        on_file_complete();
        Ok(bytes)
    }
}

/// Recursively imports a local directory to a non-local volume, checking cancellation
/// between each file. This replaces `Volume::import_from_local(dir)` in the copy path
/// to ensure the user can cancel mid-directory.
fn import_directory_cancellable(
    local_source: &Path,
    dest_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    on_file_progress: &dyn Fn(u64, u64) -> ControlFlow<()>,
    on_file_complete: &dyn Fn(),
) -> Result<u64, VolumeError> {
    // Create the directory on the destination
    dest_volume.create_directory(dest_path)?;

    let read_dir = std::fs::read_dir(local_source).map_err(|e| VolumeError::IoError {
        message: e.to_string(),
        raw_os_error: None,
    })?;
    let mut total_bytes = 0u64;

    for dir_entry in read_dir {
        // Check cancellation between files
        if super::state::is_cancelled(&state.intent) {
            return Err(VolumeError::IoError {
                message: "Operation cancelled".to_string(),
                raw_os_error: None,
            });
        }

        let dir_entry = dir_entry.map_err(|e| VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: None,
        })?;
        let child_local = dir_entry.path();
        let child_name = dir_entry.file_name();
        let child_dest = dest_path.join(&child_name);

        if child_local.is_dir() {
            total_bytes += import_directory_cancellable(
                &child_local,
                &child_dest,
                dest_volume,
                state,
                on_file_progress,
                on_file_complete,
            )?;
        } else {
            total_bytes += dest_volume.import_from_local_with_progress(&child_local, &child_dest, on_file_progress)?;
            on_file_complete();
        }
    }

    Ok(total_bytes)
}

/// Recursively exports a non-local volume directory to local, checking cancellation
/// between each file.
fn export_directory_cancellable(
    source_path: &Path,
    local_dest: &Path,
    source_volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    on_file_progress: &dyn Fn(u64, u64) -> ControlFlow<()>,
    on_file_complete: &dyn Fn(),
) -> Result<u64, VolumeError> {
    // Create the local directory
    std::fs::create_dir_all(local_dest).map_err(|e| VolumeError::IoError {
        message: e.to_string(),
        raw_os_error: None,
    })?;

    // List the source directory via the Volume trait
    let entries = source_volume.list_directory(source_path)?;
    let mut total_bytes = 0u64;

    for entry in &entries {
        // Check cancellation between files
        if super::state::is_cancelled(&state.intent) {
            return Err(VolumeError::IoError {
                message: "Operation cancelled".to_string(),
                raw_os_error: None,
            });
        }

        let child_source = Path::new(&entry.path);
        let child_local = local_dest.join(&entry.name);

        if entry.is_directory {
            total_bytes += export_directory_cancellable(
                child_source,
                &child_local,
                source_volume,
                state,
                on_file_progress,
                on_file_complete,
            )?;
        } else {
            total_bytes += source_volume.export_to_local_with_progress(child_source, &child_local, on_file_progress)?;
            on_file_complete();
        }
    }

    Ok(total_bytes)
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
    std::fs::create_dir_all(&temp_dir).map_err(|e| VolumeError::IoError {
        message: e.to_string(),
        raw_os_error: None,
    })?;

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
    use std::sync::atomic::AtomicU8;
    use std::time::Duration;

    use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};

    fn no_progress(_bytes_done: u64, _bytes_total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

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
            intent: Arc::new(AtomicU8::new(0)),
            progress_interval: Duration::from_millis(200),
            pending_resolution: std::sync::RwLock::new(None),
            conflict_condvar: std::sync::Condvar::new(),
            conflict_mutex: std::sync::Mutex::new(false),
        });

        let bytes = copy_single_path(
            &source,
            Path::new("source.txt"),
            &dest,
            Path::new("dest.txt"),
            &state,
            &no_progress,
            &|| {},
        )
        .unwrap();

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
            intent: Arc::new(AtomicU8::new(2)), // Already cancelled (Stopped)
            progress_interval: Duration::from_millis(200),
            pending_resolution: std::sync::RwLock::new(None),
            conflict_condvar: std::sync::Condvar::new(),
            conflict_mutex: std::sync::Mutex::new(false),
        });

        let result = copy_single_path(
            &source,
            Path::new("source.txt"),
            &dest,
            Path::new("dest.txt"),
            &state,
            &no_progress,
            &|| {},
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), VolumeError::IoError { message, .. } if message.contains("cancelled")));

        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dst_dir);
    }
}
