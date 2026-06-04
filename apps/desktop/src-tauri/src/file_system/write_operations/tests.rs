//! Tests for write operations (copy, move, delete).
//!
//! Note: Serialization tests were removed - serde derive macros are well-tested.
//! We keep deserialization tests as they verify the API contract with the frontend.

use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Creates a temporary test directory with a unique name.
fn create_temp_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!("cmdr_write_test_{}", name));
    let _ = fs::remove_dir_all(&temp_dir); // Clean up any previous run
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
    temp_dir
}

/// Cleans up a test directory.
fn cleanup_temp_dir(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

// ============================================================================
// Cancellation state tests
// ============================================================================

#[test]
fn test_cancel_sets_intent() {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    assert!(!is_cancelled(&state.intent));
    assert_eq!(load_intent(&state.intent), OperationIntent::Running);

    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    assert!(is_cancelled(&state.intent));
    assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
}

// ============================================================================
// Config tests - deserialization verifies API contract with frontend
// ============================================================================

#[test]
fn test_default_config() {
    let config = WriteOperationConfig::default();
    assert_eq!(config.progress_interval_ms, 200);
    assert!(!config.overwrite);
    assert_eq!(config.conflict_resolution, ConflictResolution::Stop);
    assert!(!config.dry_run);
}

#[test]
fn test_config_deserialization() {
    let json = r#"{"progressIntervalMs": 100, "overwrite": true}"#;
    let config: WriteOperationConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.progress_interval_ms, 100);
    assert!(config.overwrite);
}

#[test]
fn test_config_default_values_deserialization() {
    let json = r#"{}"#;
    let config: WriteOperationConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.progress_interval_ms, 200);
    assert!(!config.overwrite);
    assert!(!config.dry_run);
}

#[test]
fn test_config_conflict_resolution_deserialization() {
    let json = r#"{"conflictResolution": "skip"}"#;
    let config: WriteOperationConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.conflict_resolution, ConflictResolution::Skip);

    let json = r#"{"conflictResolution": "overwrite"}"#;
    let config: WriteOperationConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.conflict_resolution, ConflictResolution::Overwrite);

    let json = r#"{"conflictResolution": "rename"}"#;
    let config: WriteOperationConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.conflict_resolution, ConflictResolution::Rename);
}

#[test]
fn test_config_dry_run_deserialization() {
    let json = r#"{"dryRun": true}"#;
    let config: WriteOperationConfig = serde_json::from_str(json).unwrap();
    assert!(config.dry_run);
}

// ============================================================================
// IO Error conversion tests - actual logic
// ============================================================================

#[test]
fn test_io_error_not_found_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let write_err: WriteOperationError = io_err.into();

    assert!(matches!(write_err, WriteOperationError::SourceNotFound { .. }));
}

#[test]
fn test_io_error_permission_denied_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let write_err: WriteOperationError = io_err.into();

    assert!(matches!(write_err, WriteOperationError::PermissionDenied { .. }));
}

#[test]
fn test_io_error_already_exists_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "file exists");
    let write_err: WriteOperationError = io_err.into();

    assert!(matches!(write_err, WriteOperationError::DestinationExists { .. }));
}

#[test]
fn test_io_error_other_conversion() {
    let io_err = std::io::Error::other("some error");
    let write_err: WriteOperationError = io_err.into();

    assert!(matches!(write_err, WriteOperationError::IoError { .. }));
}

// ============================================================================
// Temp directory helper tests
// ============================================================================

#[test]
fn test_create_and_cleanup_temp_dir() {
    let temp_dir = create_temp_dir("helper_test");
    assert!(temp_dir.exists());
    assert!(temp_dir.is_dir());

    cleanup_temp_dir(&temp_dir);
    assert!(!temp_dir.exists());
}

// ============================================================================
// CopyTransaction rollback tests
// ============================================================================

#[test]
fn test_copy_transaction_rollback_deletes_files() {
    let temp_dir = create_temp_dir("rollback_files");

    // Create some files to simulate a partial copy
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");
    let file3 = temp_dir.join("subdir").join("file3.txt");

    fs::write(&file1, "content1").expect("Failed to create file1");
    fs::write(&file2, "content2").expect("Failed to create file2");
    fs::create_dir_all(file3.parent().unwrap()).expect("Failed to create subdir");
    fs::write(&file3, "content3").expect("Failed to create file3");

    // Verify files exist before rollback
    assert!(file1.exists(), "file1 should exist before rollback");
    assert!(file2.exists(), "file2 should exist before rollback");
    assert!(file3.exists(), "file3 should exist before rollback");

    // Record files in transaction
    let mut transaction = CopyTransaction::new();
    transaction.record_file(file1.clone());
    transaction.record_file(file2.clone());
    transaction.record_file(file3.clone());
    transaction.record_dir(temp_dir.join("subdir"));

    // Rollback should delete all recorded files and directories
    transaction.rollback();

    // Verify files are deleted
    assert!(!file1.exists(), "file1 should be deleted after rollback");
    assert!(!file2.exists(), "file2 should be deleted after rollback");
    assert!(!file3.exists(), "file3 should be deleted after rollback");
    assert!(
        !temp_dir.join("subdir").exists(),
        "subdir should be deleted after rollback"
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_commit_keeps_files() {
    let temp_dir = create_temp_dir("commit_files");

    // Create some files to simulate a partial copy
    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");

    fs::write(&file1, "content1").expect("Failed to create file1");
    fs::write(&file2, "content2").expect("Failed to create file2");

    // Record files in transaction
    let mut transaction = CopyTransaction::new();
    transaction.record_file(file1.clone());
    transaction.record_file(file2.clone());

    // Commit should NOT delete files (they are kept)
    transaction.commit();

    // Verify files still exist
    assert!(file1.exists(), "file1 should still exist after commit");
    assert!(file2.exists(), "file2 should still exist after commit");

    // Verify content is intact
    assert_eq!(fs::read_to_string(&file1).unwrap(), "content1");
    assert_eq!(fs::read_to_string(&file2).unwrap(), "content2");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_rollback_handles_already_deleted_files() {
    let temp_dir = create_temp_dir("rollback_missing");

    // Create a file
    let file1 = temp_dir.join("file1.txt");
    fs::write(&file1, "content1").expect("Failed to create file1");

    // Record in transaction
    let mut transaction = CopyTransaction::new();
    transaction.record_file(file1.clone());

    // Delete file before rollback (simulates external deletion)
    fs::remove_file(&file1).expect("Failed to delete file1");

    // Rollback should not panic even if files are already gone
    transaction.rollback(); // Should not panic

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_operation_intent_transitions() {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    // Running → RollingBack
    assert_eq!(load_intent(&state.intent), OperationIntent::Running);
    state
        .intent
        .store(OperationIntent::RollingBack as u8, Ordering::Relaxed);
    assert_eq!(load_intent(&state.intent), OperationIntent::RollingBack);
    assert!(is_cancelled(&state.intent), "RollingBack should count as cancelled");

    // RollingBack → Stopped
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
    assert!(is_cancelled(&state.intent), "Stopped should count as cancelled");

    // Running → Stopped (direct, no rollback)
    state.intent.store(OperationIntent::Running as u8, Ordering::Relaxed);
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    assert_eq!(load_intent(&state.intent), OperationIntent::Stopped);
}

// ============================================================================
// Delete cancellation tests
// ============================================================================

#[test]
fn test_cancel_flag_stops_delete_loop() {
    let temp_dir = create_temp_dir("cancel_delete_loop");

    // Create several files
    let mut files = Vec::new();
    for i in 0..5 {
        let file = temp_dir.join(format!("file{}.txt", i));
        fs::write(&file, format!("content{}", i)).expect("Failed to create file");
        files.push(file);
    }

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    // Simulate the delete loop from delete.rs, setting cancelled after 2 files
    let mut files_done = 0;
    for file in &files {
        if is_cancelled(&state.intent) {
            break;
        }

        fs::remove_file(file).expect("Failed to delete file");
        files_done += 1;

        // Cancel after deleting 2 files
        if files_done == 2 {
            state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
        }
    }

    // 2 files deleted, 3 remaining
    assert_eq!(files_done, 2);
    assert!(!files[0].exists(), "file0 should be deleted");
    assert!(!files[1].exists(), "file1 should be deleted");
    assert!(files[2].exists(), "file2 should still exist");
    assert!(files[3].exists(), "file3 should still exist");
    assert!(files[4].exists(), "file4 should still exist");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_cancel_during_directory_deletion_phase() {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));

    // Set cancellation before directory phase
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);

    // Verify that the intent is detectable (matches the check in delete.rs)
    assert!(
        is_cancelled(&state.intent),
        "intent should indicate cancellation before directory deletion phase"
    );
}

#[test]
fn test_delete_cancelled_event_has_no_rollback() {
    // Delete operations set rolled_back: false because deletions can't be undone
    let event = WriteCancelledEvent {
        operation_id: "delete-test".to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: 3,
        rolled_back: false,
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(
        json.contains("\"rolledBack\":false"),
        "Delete cancelled events should always have rolledBack:false"
    );
    assert!(
        json.contains("\"operationType\":\"delete\""),
        "Should serialize as delete operation type"
    );
}

#[test]
fn test_cancelled_event_rolled_back_field_serialization() {
    // Test that WriteCancelledEvent serializes correctly with rolled_back field
    let event_with_rollback = WriteCancelledEvent {
        operation_id: "test-123".to_string(),
        operation_type: WriteOperationType::Copy,
        files_processed: 5,
        rolled_back: true,
    };

    let json = serde_json::to_string(&event_with_rollback).unwrap();
    assert!(
        json.contains("\"rolledBack\":true"),
        "JSON should contain rolledBack:true"
    );

    let event_without_rollback = WriteCancelledEvent {
        operation_id: "test-456".to_string(),
        operation_type: WriteOperationType::Copy,
        files_processed: 3,
        rolled_back: false,
    };

    let json = serde_json::to_string(&event_without_rollback).unwrap();
    assert!(
        json.contains("\"rolledBack\":false"),
        "JSON should contain rolledBack:false"
    );
}

// ============================================================================
// CopyTransaction Drop auto-rollback tests
// ============================================================================

#[test]
fn test_copy_transaction_drop_without_commit_rolls_back() {
    let temp_dir = create_temp_dir("drop_rollback");

    let file1 = temp_dir.join("file1.txt");
    let file2 = temp_dir.join("file2.txt");
    fs::write(&file1, "content1").expect("Failed to create file1");
    fs::write(&file2, "content2").expect("Failed to create file2");

    // Create a transaction, record files, then drop without committing
    {
        let mut transaction = CopyTransaction::new();
        transaction.record_file(file1.clone());
        transaction.record_file(file2.clone());
        // transaction drops here without commit()
    }

    // Files should be rolled back (deleted)
    assert!(!file1.exists(), "file1 should be deleted by Drop rollback");
    assert!(!file2.exists(), "file2 should be deleted by Drop rollback");

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_copy_transaction_commit_prevents_drop_rollback() {
    let temp_dir = create_temp_dir("commit_no_rollback");

    let file1 = temp_dir.join("file1.txt");
    fs::write(&file1, "content1").expect("Failed to create file1");

    {
        let mut transaction = CopyTransaction::new();
        transaction.record_file(file1.clone());
        transaction.commit(); // should prevent Drop from rolling back
    }

    assert!(file1.exists(), "file1 should still exist after commit + drop");

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// safe_overwrite_file tests
// ============================================================================

use super::overwrite::safe_overwrite_file;

#[test]
fn test_safe_overwrite_basic() {
    let temp_dir = create_temp_dir("safe_overwrite_basic");
    let source = temp_dir.join("source.txt");
    let dest = temp_dir.join("dest.txt");

    fs::write(&source, "new-data!!").unwrap();
    fs::write(&dest, "old-data").unwrap();

    #[cfg(target_os = "macos")]
    let result = safe_overwrite_file(&source, &dest, None);
    #[cfg(not(target_os = "macos"))]
    let result = safe_overwrite_file(&source, &dest);

    let bytes = result.expect("safe_overwrite_file should succeed");
    assert_eq!(bytes, 10, "should report 10 bytes copied");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "new-data!!");
    assert!(source.exists(), "source should still exist");

    // No leftover temp / set-aside files
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(!name.contains(".cmdr-tmp-"), "temp file should be cleaned up: {name}");
        assert!(!name.contains(".cmdr-temp-"), "aside file should be cleaned up: {name}");
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_safe_overwrite_preserves_dest_on_missing_source() {
    let temp_dir = create_temp_dir("safe_overwrite_missing_src");
    let source = temp_dir.join("nonexistent.txt");
    let dest = temp_dir.join("dest.txt");

    fs::write(&dest, "old-data").unwrap();

    #[cfg(target_os = "macos")]
    let result = safe_overwrite_file(&source, &dest, None);
    #[cfg(not(target_os = "macos"))]
    let result = safe_overwrite_file(&source, &dest);

    assert!(result.is_err(), "should fail when source doesn't exist");
    assert_eq!(
        fs::read_to_string(&dest).unwrap(),
        "old-data",
        "original dest content must be untouched"
    );

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_safe_overwrite_dest_has_new_content_after_completion() {
    let temp_dir = create_temp_dir("safe_overwrite_atomic");
    let source = temp_dir.join("source.txt");
    let dest = temp_dir.join("dest.txt");

    let new_content = "replacement-content-here";
    fs::write(&source, new_content).unwrap();
    fs::write(&dest, "original").unwrap();

    #[cfg(target_os = "macos")]
    let result = safe_overwrite_file(&source, &dest, None);
    #[cfg(not(target_os = "macos"))]
    let result = safe_overwrite_file(&source, &dest);

    result.expect("safe_overwrite_file should succeed");

    // After completion, reading dest returns the full new content (no partial writes)
    assert_eq!(fs::read_to_string(&dest).unwrap(), new_content);

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_safe_overwrite_different_sizes() {
    let temp_dir = create_temp_dir("safe_overwrite_sizes");

    // Case 1: source much larger than dest
    let source_large = temp_dir.join("large_source.txt");
    let dest_small = temp_dir.join("small_dest.txt");
    let large_content = "x".repeat(100_000);
    fs::write(&source_large, &large_content).unwrap();
    fs::write(&dest_small, "tiny").unwrap();

    #[cfg(target_os = "macos")]
    let result = safe_overwrite_file(&source_large, &dest_small, None);
    #[cfg(not(target_os = "macos"))]
    let result = safe_overwrite_file(&source_large, &dest_small);

    result.expect("large-to-small overwrite should succeed");
    assert_eq!(fs::read_to_string(&dest_small).unwrap(), large_content);

    // Case 2: source much smaller than dest
    let source_small = temp_dir.join("small_source.txt");
    let dest_large = temp_dir.join("large_dest.txt");
    let large_dest_content = "y".repeat(100_000);
    fs::write(&source_small, "tiny").unwrap();
    fs::write(&dest_large, &large_dest_content).unwrap();

    #[cfg(target_os = "macos")]
    let result = safe_overwrite_file(&source_small, &dest_large, None);
    #[cfg(not(target_os = "macos"))]
    let result = safe_overwrite_file(&source_small, &dest_large);

    result.expect("small-to-large overwrite should succeed");
    assert_eq!(fs::read_to_string(&dest_large).unwrap(), "tiny");

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// safe_overwrite_file: cross-type overwrites (file source → folder dest)
// ============================================================================

#[test]
fn test_safe_overwrite_file_replaces_existing_folder() {
    // Source = file. Dest = existing folder with contents. After overwrite the
    // dest path holds the source's bytes and the old folder tree is gone, with
    // no stray cmdr-temp artifacts left behind. Pre-fix the caller did a direct
    // `fs::remove_dir_all` before the copy, so a crash mid-delete would lose
    // the folder forever.
    let temp_dir = create_temp_dir("safe_overwrite_file_over_folder");
    let source = temp_dir.join("source.txt");
    let dest = temp_dir.join("dest");

    fs::write(&source, "I am a file now").unwrap();
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("inner-a.txt"), "inner a").unwrap();
    fs::write(dest.join("inner-b.txt"), "inner b").unwrap();
    fs::create_dir_all(dest.join("sub")).unwrap();
    fs::write(dest.join("sub").join("deep.txt"), "deep").unwrap();

    #[cfg(target_os = "macos")]
    let result = safe_overwrite_file(&source, &dest, None);
    #[cfg(not(target_os = "macos"))]
    let result = safe_overwrite_file(&source, &dest);

    result.expect("safe_overwrite_file should succeed when dest is an existing folder");

    // Dest is now a file with the source's contents
    let dest_meta = fs::symlink_metadata(&dest).unwrap();
    assert!(dest_meta.is_file(), "dest should be a file after overwrite");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "I am a file now");

    // No cmdr-temp / cmdr-tmp artifacts remain
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-tmp-") && !name.contains(".cmdr-temp-"),
            "no temp / aside artifacts should remain: {name}"
        );
    }

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// safe_overwrite_dir tests (folder materialized over existing file or folder)
// ============================================================================

use super::overwrite::safe_overwrite_dir;
use super::types::WriteOperationError;

#[test]
fn test_safe_overwrite_dir_materializes_folder_over_existing_file() {
    // Source intent = folder. Dest = existing file. After materialization the
    // dest path is a folder containing the materialized contents, and the
    // original file is gone with no cmdr-temp artifact left.
    let temp_dir = create_temp_dir("safe_overwrite_dir_over_file");
    let dest = temp_dir.join("dest");
    fs::write(&dest, "I am the existing file").unwrap();

    let result = safe_overwrite_dir(&dest, |target| {
        fs::create_dir_all(target).map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("create_dir_all: {e}"),
        })?;
        fs::write(target.join("a.txt"), "a").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write a: {e}"),
        })?;
        fs::write(target.join("b.txt"), "b").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write b: {e}"),
        })?;
        Ok(())
    });
    result.expect("safe_overwrite_dir should materialize the folder");

    let dest_meta = fs::symlink_metadata(&dest).unwrap();
    assert!(dest_meta.is_dir(), "dest should be a directory after overwrite");
    assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "a");
    assert_eq!(fs::read_to_string(dest.join("b.txt")).unwrap(), "b");

    // No cmdr-temp artifacts remain in parent
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-temp-") && !name.contains(".cmdr-tmp-"),
            "no aside artifacts should remain: {name}"
        );
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_safe_overwrite_dir_restores_original_on_materialize_failure() {
    // Cancellation / materialize failure must restore the original dest. No
    // data loss when the closure returns an error after the rename-aside step.
    let temp_dir = create_temp_dir("safe_overwrite_dir_restore");
    let dest = temp_dir.join("dest_folder");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("keep-me.txt"), "do not lose this").unwrap();
    fs::write(dest.join("also-keep.txt"), "also important").unwrap();

    let result: Result<(), WriteOperationError> = safe_overwrite_dir(&dest, |target| {
        // Pretend the caller got partway through and then was cancelled.
        fs::create_dir_all(target).ok();
        fs::write(target.join("partial.txt"), "half-written").ok();
        Err(WriteOperationError::Cancelled {
            message: "user cancelled".to_string(),
        })
    });

    assert!(result.is_err(), "should propagate the materialize failure");
    assert!(
        matches!(result.unwrap_err(), WriteOperationError::Cancelled { .. }),
        "should preserve the Cancelled variant"
    );

    // Original dest survives untouched
    let dest_meta = fs::symlink_metadata(&dest).unwrap();
    assert!(dest_meta.is_dir(), "dest should still be the original folder");
    assert_eq!(
        fs::read_to_string(dest.join("keep-me.txt")).unwrap(),
        "do not lose this"
    );
    assert_eq!(
        fs::read_to_string(dest.join("also-keep.txt")).unwrap(),
        "also important"
    );
    assert!(
        !dest.join("partial.txt").exists(),
        "partial materialize artifact should be gone after restore"
    );

    // No cmdr-temp aside remains
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-temp-"),
            "aside should be rolled back, not left on disk: {name}"
        );
    }

    cleanup_temp_dir(&temp_dir);
}

#[test]
fn test_safe_overwrite_dir_over_folder_dest_replaces_contents() {
    // Source intent = folder. Dest = existing folder with different contents.
    // After successful materialization, the dest path holds the newly
    // materialized contents (no merge), and the original folder is gone with
    // no cmdr-temp artifacts left.
    let temp_dir = create_temp_dir("safe_overwrite_dir_over_folder");
    let dest = temp_dir.join("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("old.txt"), "old").unwrap();

    let result = safe_overwrite_dir(&dest, |target| {
        fs::create_dir_all(target).map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("create_dir_all: {e}"),
        })?;
        fs::write(target.join("new.txt"), "new").map_err(|e| WriteOperationError::IoError {
            path: target.display().to_string(),
            message: format!("write new: {e}"),
        })?;
        Ok(())
    });
    result.expect("safe_overwrite_dir should succeed over existing folder");

    assert!(dest.is_dir(), "dest should be a directory");
    assert!(dest.join("new.txt").exists(), "new file should be present");
    assert!(!dest.join("old.txt").exists(), "old file should be gone (no merge)");

    for entry in fs::read_dir(&temp_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().to_string();
        assert!(
            !name.contains(".cmdr-temp-"),
            "no aside artifacts should remain: {name}"
        );
    }

    cleanup_temp_dir(&temp_dir);
}

/// Delete and trash deliberately don't flush after deleting, and copy/move
/// flush their own destinations (`durability::flush_created_destinations`) rather
/// than firing a whole-machine `libc::sync()`. This pins that there are no
/// remaining `spawn_async_sync` callers and no raw `libc::sync()` anywhere in
/// the `write_operations` module, so the global-sync approach can't creep back
/// in. (A non-durable delete fails annoyance-class — a deleted file reappears
/// after a crash, the user re-deletes; never data loss — so targeted fsync
/// isn't worth its cost there.)
#[test]
fn no_global_sync_or_spawn_async_sync_in_write_operations() {
    fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, hits);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // Skip this very file: it names the forbidden symbols in prose.
            if path.file_name().and_then(|n| n.to_str()) == Some("tests.rs")
                && path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) == Some("write_operations")
            {
                continue;
            }
            let src = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            // `spawn_async_sync` (the deleted helper) and a raw whole-machine
            // `libc::sync()` are both banned.
            if src.contains("spawn_async_sync") || src.contains("libc::sync(") {
                hits.push(path.display().to_string());
            }
        }
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("file_system")
        .join("write_operations");
    let mut hits = Vec::new();
    walk(&root, &mut hits);
    assert!(
        hits.is_empty(),
        "found banned global-sync references (spawn_async_sync / libc::sync()) in: {hits:?}"
    );
}
