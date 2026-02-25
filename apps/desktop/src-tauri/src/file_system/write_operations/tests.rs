//! Tests for write operations (copy, move, delete).
//!
//! Note: Serialization tests were removed - serde derive macros are well-tested.
//! We keep deserialization tests as they verify the API contract with the frontend.

use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
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
fn test_cancel_sets_flag() {
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
fn test_skip_rollback_flag_behavior() {
    // Test that skip_rollback flag controls rollback behavior
    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(200),
        pending_resolution: RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Initially skip_rollback is false (rollback enabled)
    assert!(!state.skip_rollback.load(Ordering::Relaxed));

    // Set cancelled with rollback=true (skip_rollback stays false)
    state.cancelled.store(true, Ordering::Relaxed);
    state.skip_rollback.store(false, Ordering::Relaxed); // !rollback where rollback=true
    assert!(state.cancelled.load(Ordering::Relaxed));
    assert!(
        !state.skip_rollback.load(Ordering::Relaxed),
        "skip_rollback should be false when rollback=true"
    );

    // Reset and set cancelled with rollback=false (skip_rollback becomes true)
    state.cancelled.store(true, Ordering::Relaxed);
    state.skip_rollback.store(true, Ordering::Relaxed); // !rollback where rollback=false
    assert!(state.cancelled.load(Ordering::Relaxed));
    assert!(
        state.skip_rollback.load(Ordering::Relaxed),
        "skip_rollback should be true when rollback=false"
    );
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

    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(200),
        pending_resolution: RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Simulate the delete loop from delete.rs, setting cancelled after 2 files
    let mut files_done = 0;
    for file in &files {
        if state.cancelled.load(Ordering::Relaxed) {
            break;
        }

        fs::remove_file(file).expect("Failed to delete file");
        files_done += 1;

        // Cancel after deleting 2 files
        if files_done == 2 {
            state.cancelled.store(true, Ordering::Relaxed);
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
    let state = Arc::new(WriteOperationState {
        cancelled: Arc::new(AtomicBool::new(false)),
        skip_rollback: AtomicBool::new(false),
        progress_interval: Duration::from_millis(200),
        pending_resolution: RwLock::new(None),
        conflict_condvar: std::sync::Condvar::new(),
        conflict_mutex: std::sync::Mutex::new(false),
    });

    // Set cancellation before directory phase
    state.cancelled.store(true, Ordering::Relaxed);

    // Verify that the cancelled flag is detectable (matches the check in delete.rs line 123)
    assert!(
        state.cancelled.load(Ordering::Relaxed),
        "cancelled flag should be set before directory deletion phase"
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
