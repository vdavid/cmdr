//! Trash implementation for write operations.
//!
//! Provides `move_to_trash_sync()` (reusable core) and `trash_files_with_progress()`
//! (batch operation with progress, cancellation, and partial failure support).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::helpers::spawn_async_sync;
use super::state::{WriteOperationState, update_operation_status};
use super::types::{
    WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError, WriteOperationPhase,
    WriteOperationType, WriteProgressEvent,
};

// ============================================================================
// Core trash function (reusable by commands/rename.rs and batch trash)
// ============================================================================

/// Synchronous trash implementation using macOS NSFileManager.trashItem.
///
/// Uses `symlink_metadata()` for existence checks so dangling symlinks
/// are handled correctly (the link itself exists even if its target doesn't).
#[cfg(target_os = "macos")]
pub fn move_to_trash_sync(path: &Path) -> Result<(), String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    if fs::symlink_metadata(path).is_err() {
        return Err(format!("'{}' doesn't exist", path.display()));
    }

    let path_str = path.to_string_lossy();
    let ns_path = NSString::from_str(&path_str);
    let url = NSURL::fileURLWithPath(&ns_path);
    let file_manager = NSFileManager::defaultManager();

    // trashItemAtURL:resultingItemURL:error: moves the item to Trash.
    // We pass None for resultingItemURL since we don't need the trash location.
    file_manager
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| format!("Failed to move to trash: {}", e))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn move_to_trash_sync(path: &Path) -> Result<(), String> {
    Err(format!(
        "Moving to trash is not supported on this platform for '{}'",
        path.display()
    ))
}

// ============================================================================
// Batch trash with progress
// ============================================================================

/// Per-item error from a trash operation.
#[derive(Debug, Clone)]
pub struct TrashItemError {
    pub path: PathBuf,
    pub message: String,
}

/// Moves files to trash with progress reporting, cancellation, and partial failure.
///
/// Iterates top-level items, calling `move_to_trash_sync()` for each.
/// Unlike permanent delete, trash doesn't need a recursive scan phase because
/// `trashItemAtURL` is atomic per top-level item (the OS moves the entire tree).
///
/// # Arguments
/// * `app` - Tauri app handle for event emission
/// * `operation_id` - Unique operation ID for event correlation
/// * `state` - Shared state with cancellation flag and progress interval
/// * `sources` - Top-level items to trash
/// * `item_sizes` - Optional per-item sizes for byte-level progress (from scan
///   preview or drive index). When `None`, bytes progress is not reported.
pub(super) fn trash_files_with_progress(
    app: &tauri::AppHandle,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    item_sizes: Option<&[u64]>,
) -> Result<(), WriteOperationError> {
    use tauri::Emitter;

    let items_total = sources.len();
    let bytes_total: u64 = item_sizes.map(|s| s.iter().sum()).unwrap_or(0);

    let mut items_done = 0usize;
    let mut bytes_done = 0u64;
    let mut errors: Vec<TrashItemError> = Vec::new();
    let mut last_progress_time = Instant::now();

    for (i, source) in sources.iter().enumerate() {
        // Check cancellation between items
        if state.cancelled.load(Ordering::Relaxed) {
            let _ = app.emit(
                "write-cancelled",
                WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Trash,
                    files_processed: items_done,
                    rolled_back: false, // Trash is recoverable, no rollback needed
                },
            );
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Check existence using symlink_metadata (handles dangling symlinks)
        if fs::symlink_metadata(source).is_err() {
            errors.push(TrashItemError {
                path: source.clone(),
                message: format!("'{}' no longer exists", source.display()),
            });
            continue;
        }

        // Attempt to trash the item
        match move_to_trash_sync(source) {
            Ok(()) => {
                items_done += 1;
                if let Some(sizes) = item_sizes
                    && let Some(&size) = sizes.get(i)
                {
                    bytes_done += size;
                }
            }
            Err(e) => {
                errors.push(TrashItemError {
                    path: source.clone(),
                    message: e,
                });
                continue;
            }
        }

        // Emit throttled progress
        if last_progress_time.elapsed() >= state.progress_interval {
            let current_file = source.file_name().map(|n| n.to_string_lossy().to_string());
            let _ = app.emit(
                "write-progress",
                WriteProgressEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Trash,
                    phase: WriteOperationPhase::Trashing,
                    current_file: current_file.clone(),
                    files_done: items_done,
                    files_total: items_total,
                    bytes_done,
                    bytes_total,
                },
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::Trashing,
                current_file,
                items_done,
                items_total,
                bytes_done,
                bytes_total,
            );
            last_progress_time = Instant::now();
        }
    }

    // Spawn async sync for durability (non-blocking)
    spawn_async_sync();

    // If all items failed, emit error
    if items_done == 0 && !errors.is_empty() {
        let error_summary = errors
            .iter()
            .map(|e| format!("{}: {}", e.path.display(), e.message))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = app.emit(
            "write-error",
            WriteErrorEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Trash,
                error: WriteOperationError::IoError {
                    path: String::new(),
                    message: error_summary,
                },
            },
        );
        return Err(WriteOperationError::IoError {
            path: String::new(),
            message: format!(
                "Couldn't move {} to trash",
                if errors.len() == 1 {
                    format!("'{}'", errors[0].path.display())
                } else {
                    format!("{} items", errors.len())
                }
            ),
        });
    }

    // Emit completion (may include partial errors)
    let _ = app.emit(
        "write-complete",
        WriteCompleteEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Trash,
            files_processed: items_done,
            bytes_processed: bytes_done,
        },
    );

    // Log partial failures
    if !errors.is_empty() {
        log::warn!(
            "Trash operation {} completed with {} errors out of {} items",
            operation_id,
            errors.len(),
            items_total
        );
        for error in &errors {
            log::warn!("  Failed: {} — {}", error.path.display(), error.message);
        }
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;

    fn create_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_trash_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("Failed to create test directory");
        dir
    }

    fn cleanup_test_dir(path: &PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    // ========================================================================
    // move_to_trash_sync tests
    // ========================================================================

    #[cfg(target_os = "macos")]
    #[test]
    fn test_move_to_trash_sync_file() {
        let tmp = create_test_dir("trash_sync_file");
        let file = tmp.join("test.txt");
        fs::write(&file, "content").unwrap();
        assert!(fs::symlink_metadata(&file).is_ok());

        let result = move_to_trash_sync(&file);
        assert!(result.is_ok());
        assert!(fs::symlink_metadata(&file).is_err());
        cleanup_test_dir(&tmp);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_move_to_trash_sync_directory() {
        let tmp = create_test_dir("trash_sync_dir");
        let dir = tmp.join("subdir");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("inner.txt"), "data").unwrap();

        let result = move_to_trash_sync(&dir);
        assert!(result.is_ok());
        assert!(fs::symlink_metadata(&dir).is_err());
        cleanup_test_dir(&tmp);
    }

    #[test]
    fn test_move_to_trash_sync_nonexistent() {
        let result = move_to_trash_sync(Path::new("/nonexistent_12345/file.txt"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("doesn't exist"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_move_to_trash_sync_dangling_symlink() {
        let tmp = create_test_dir("trash_sync_dangling");
        let target = tmp.join("target.txt");
        let link = tmp.join("link.txt");
        fs::write(&target, "data").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        // Remove the target, leaving a dangling symlink
        fs::remove_file(&target).unwrap();

        // The link itself still exists (symlink_metadata succeeds)
        assert!(fs::symlink_metadata(&link).is_ok());
        // But path.exists() would return false (follows symlink)
        assert!(!link.exists());

        // move_to_trash_sync should handle this correctly
        let result = move_to_trash_sync(&link);
        assert!(result.is_ok());
        assert!(fs::symlink_metadata(&link).is_err());
        cleanup_test_dir(&tmp);
    }

    // ========================================================================
    // trash_files_with_progress tests (using mock AppHandle)
    // ========================================================================

    // Note: Full integration tests for trash_files_with_progress require a
    // tauri::AppHandle, which needs the Tauri runtime. Unit-level tests for
    // the core move_to_trash_sync function above cover the ObjC logic.
    // The cancellation and progress patterns are structurally identical to
    // delete_files_with_progress, which is tested via integration tests.

    #[test]
    fn test_trash_item_error_captures_path_and_message() {
        let error = TrashItemError {
            path: PathBuf::from("/some/file.txt"),
            message: "Permission denied".to_string(),
        };
        assert_eq!(error.path.display().to_string(), "/some/file.txt");
        assert_eq!(error.message, "Permission denied");
    }

    #[test]
    fn test_cancellation_flag_checked_by_state() {
        let state = Arc::new(WriteOperationState {
            cancelled: Arc::new(AtomicBool::new(false)),
            skip_rollback: AtomicBool::new(false),
            progress_interval: Duration::from_millis(200),
            pending_resolution: RwLock::new(None),
            conflict_condvar: std::sync::Condvar::new(),
            conflict_mutex: std::sync::Mutex::new(false),
        });

        assert!(!state.cancelled.load(Ordering::Relaxed));
        state.cancelled.store(true, Ordering::Relaxed);
        assert!(state.cancelled.load(Ordering::Relaxed));
    }
}
