//! Trash implementation for write operations.
//!
//! Provides `move_to_trash_sync()` (reusable core) and `trash_files_with_progress()`
//! (batch operation with progress, cancellation, and partial failure support).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use super::super::state::{WriteOperationState, update_operation_status};
use super::super::types::{
    OperationEventSink, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};

// ============================================================================
// Core trash function (reusable by commands/rename.rs and batch trash)
// ============================================================================

/// Synchronous trash implementation using macOS NSFileManager.trashItem.
///
/// Returns the item's **in-trash location** (`Some` on macOS, where the OS
/// reports it): the journal records it as the trash row's dest so a later restore
/// knows where the OS put the item (the M3 trash rollback depends on it). `None`
/// means "trashed, but no restore location recorded" (Linux, or the rare case the
/// OS omitted the URL).
///
/// Uses `symlink_metadata()` for existence checks so dangling symlinks
/// are handled correctly (the link itself exists even if its target doesn't).
///
/// The macOS in-trash URL comes from `trashItemAtURL:resultingItemURL:error:`,
/// which populates the out-param with the final URL inside the user's Trash
/// (verified on macOS 15, live trash of a temp file returns a `~/.Trash/…` path,
/// 2026-07-10). NSFileManager may de-duplicate the name (`file 2.txt`) if the
/// Trash already holds one, so the returned location is the authoritative one.
#[cfg(target_os = "macos")]
pub fn move_to_trash_sync(path: &Path) -> Result<Option<PathBuf>, String> {
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    if fs::symlink_metadata(path).is_err() {
        return Err(format!("'{}' doesn't exist", path.display()));
    }

    // Drain autoreleased ObjC objects (NSURL, NSString, NSFileManager internals).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let path_str = path.to_string_lossy();
        let ns_path = NSString::from_str(&path_str);
        let url = NSURL::fileURLWithPath(&ns_path);
        let file_manager = NSFileManager::defaultManager();

        // Capture resultingItemURL (the final location inside Trash) so the
        // journal can record where to restore from (M3 trash rollback).
        let mut resulting: Option<Retained<NSURL>> = None;
        file_manager
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
            .map_err(|e| format!("Failed to move to trash: {}", e))?;
        let in_trash = resulting.and_then(|u| u.path()).map(|p| PathBuf::from(p.to_string()));
        Ok(in_trash)
    })
}

#[cfg(target_os = "linux")]
pub fn move_to_trash_sync(path: &Path) -> Result<Option<PathBuf>, String> {
    if fs::symlink_metadata(path).is_err() {
        return Err(format!("'{}' doesn't exist", path.display()));
    }

    trash::delete(path).map_err(|e| format!("Failed to move to trash: {}", e))?;
    // The `trash` crate doesn't surface the in-trash location, so no restore
    // location is recorded (trash rollback is then unavailable on Linux, M3).
    Ok(None)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn move_to_trash_sync(path: &Path) -> Result<Option<PathBuf>, String> {
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
/// * `events` - Event sink for `write-progress`, `write-complete`, `write-cancelled`,
///   `write-error`, and `write-source-item-done` emits. Production wraps a Tauri AppHandle
///   in `TauriEventSink`; tests use `CollectorEventSink`.
/// * `operation_id` - Unique operation ID for event correlation
/// * `state` - Shared state with cancellation flag and progress interval
/// * `sources` - Top-level items to trash
/// * `item_sizes` - Optional per-item sizes for byte-level progress (from scan preview or drive
///   index). When `None`, bytes progress is not reported.
pub(in crate::file_system::write_operations) fn trash_files_with_progress(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    item_sizes: Option<&[u64]>,
) -> Result<(), WriteOperationError> {
    let items_total = sources.len();
    let bytes_total: u64 = item_sizes.map(|s| s.iter().sum()).unwrap_or(0);

    let mut items_done = 0usize;
    let mut bytes_done = 0u64;
    let mut errors: Vec<TrashItemError> = Vec::new();
    let mut last_progress_time = Instant::now();

    for (i, source) in sources.iter().enumerate() {
        // Check cancellation between items
        if super::super::state::is_cancelled(&state.intent) {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Trash,
                files_processed: items_done,
                rolled_back: false, // Trash is recoverable, no rollback needed
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        // Check existence using symlink_metadata (handles dangling symlinks).
        // Keep the metadata: it's the free snapshot (kind + mtime) the journal
        // records for the top-level item, stat'd BEFORE the OS moves it to trash.
        let source_meta = match fs::symlink_metadata(source) {
            Ok(m) => m,
            Err(_) => {
                errors.push(TrashItemError {
                    path: source.clone(),
                    message: format!("'{}' no longer exists", source.display()),
                });
                continue;
            }
        };

        // Defensive: register with the downloads watcher's ignore set so a
        // future "deleted from Downloads" event source wouldn't surprise us.
        // No-ops outside ~/Downloads.
        crate::downloads::note_pending_write_for_cmdr(source);

        // Attempt to trash the item
        match move_to_trash_sync(source) {
            Ok(in_trash) => {
                items_done += 1;
                let item_size = item_sizes.and_then(|s| s.get(i).copied());
                if let Some(size) = item_size {
                    bytes_done += size;
                }

                // Journal the trashed top-level item as the rollback unit (one
                // restore-from-trash reverses the whole subtree). The in-trash
                // location (`resultingItemURL`) is the row's dest so M3 restore
                // knows where to move it back FROM. The subtree's `search_only`
                // leaves are enumerated from the drive index (M2e).
                let entry_type = if source_meta.is_dir() {
                    crate::operation_log::types::EntryType::Dir
                } else {
                    crate::operation_log::types::EntryType::File
                };
                super::super::journal::record_local_leaf(
                    operation_id,
                    entry_type,
                    source,
                    in_trash.as_deref(),
                    item_size.map(|s| s as i64).or(Some(source_meta.len() as i64)),
                    super::super::journal::mtime_secs(&source_meta),
                    false,
                    crate::operation_log::types::ItemOutcome::Done,
                );

                events.emit_source_item_done(WriteSourceItemDoneEvent {
                    operation_id: operation_id.to_string(),
                    source_path: source.display().to_string(),
                });
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
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Trash,
                    WriteOperationPhase::Trashing,
                    current_file.clone(),
                    items_done,
                    items_total,
                    bytes_done,
                    bytes_total,
                ),
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

    // No fsync after trashing: like delete, a non-durable trash fails
    // annoyance-class (a trashed file could reappear after a crash, the user
    // re-trashes; never data loss), so targeted fsync isn't worth its cost, and
    // dropping the old whole-machine global sync (`sync(2)`) removes the stall
    // it caused on unrelated apps. See `CLAUDE.md` § "Durability".

    // If all items failed, emit error
    if items_done == 0 && !errors.is_empty() {
        let error_summary = errors
            .iter()
            .map(|e| format!("{}: {}", e.path.display(), e.message))
            .collect::<Vec<_>>()
            .join("; ");
        events.emit_error(WriteErrorEvent::new(
            operation_id.to_string(),
            WriteOperationType::Trash,
            WriteOperationError::IoError {
                path: String::new(),
                message: error_summary,
            },
        ));
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
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Trash,
        files_processed: items_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
    });

    // Log partial failures
    if !errors.is_empty() {
        log::warn!(
            "Trash operation {} completed with {} errors out of {} items",
            operation_id,
            errors.len(),
            items_total
        );
        for error in &errors {
            log::warn!("  Failed: {}: {}", error.path.display(), error.message);
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
    use std::sync::Arc;
    use std::time::Duration;

    #[cfg(target_os = "macos")]
    fn create_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_trash_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("Failed to create test directory");
        dir
    }

    #[cfg(target_os = "macos")]
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

        // The in-trash location (resultingItemURL) is captured and points into
        // the user's Trash — the M3 restore depends on this dest.
        let in_trash = result.unwrap().expect("macOS reports an in-trash location");
        assert!(
            in_trash.components().any(|c| c.as_os_str() == ".Trash"),
            "expected a ~/.Trash path, got {}",
            in_trash.display()
        );
        assert!(fs::symlink_metadata(&in_trash).is_ok(), "the item exists in Trash");
        let _ = fs::remove_file(&in_trash);
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
    // trash_files_with_progress tests (via CollectorEventSink)
    // ========================================================================

    use crate::file_system::write_operations::types::CollectorEventSink;

    /// Empty source list short-circuits: no destructive work, but a
    /// `write-complete` event still fires so the FE dialog closes cleanly.
    #[test]
    fn trash_empty_sources_emits_complete_via_sink() {
        let events = Arc::new(CollectorEventSink::new());
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

        let result = trash_files_with_progress(&*events, "op-trash-empty", &state, &[], None);
        assert!(result.is_ok(), "expected Ok, got {:?}", result);

        let complete = events.complete.lock().unwrap();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].files_processed, 0);
        assert_eq!(complete[0].bytes_processed, 0);
        assert!(events.cancelled.lock().unwrap().is_empty());
        assert!(events.errors.lock().unwrap().is_empty());
    }

    /// Pre-cancel: `Stopped` set before the loop's first iteration. Trash
    /// emits `write-cancelled` via the sink and returns
    /// `WriteOperationError::Cancelled` without invoking `move_to_trash_sync`.
    /// Source path is intentionally bogus — the cancel check fires first, so
    /// the path is never stat'd.
    #[test]
    fn trash_pre_cancel_emits_cancelled_via_sink() {
        let events = Arc::new(CollectorEventSink::new());
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        state.intent.store(2u8, std::sync::atomic::Ordering::Relaxed); // Stopped

        let sources = [PathBuf::from("/nonexistent_trash_test_12345/file.txt")];
        let result = trash_files_with_progress(&*events, "op-trash-cancel", &state, &sources, None);
        assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));

        let cancelled = events.cancelled.lock().unwrap();
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0].files_processed, 0);
        assert!(!cancelled[0].rolled_back);
        assert!(events.complete.lock().unwrap().is_empty());
    }

    /// All sources missing: trash emits `write-error` via the sink and
    /// returns `IoError`. Tests the all-failed branch without invoking
    /// `move_to_trash_sync` (the missing-source branch short-circuits
    /// before the trash call).
    #[test]
    fn trash_all_sources_missing_emits_error_via_sink() {
        let events = Arc::new(CollectorEventSink::new());
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

        let sources = [
            PathBuf::from("/nonexistent_trash_test_aaa/x.txt"),
            PathBuf::from("/nonexistent_trash_test_bbb/y.txt"),
        ];
        let result = trash_files_with_progress(&*events, "op-trash-all-missing", &state, &sources, None);
        assert!(matches!(result, Err(WriteOperationError::IoError { .. })));

        let errors = events.errors.lock().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(events.complete.lock().unwrap().is_empty());
    }

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
        let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

        assert!(!crate::file_system::write_operations::is_cancelled(&state.intent));
        state.intent.store(2u8, std::sync::atomic::Ordering::Relaxed);
        assert!(crate::file_system::write_operations::is_cancelled(&state.intent));
    }
}
