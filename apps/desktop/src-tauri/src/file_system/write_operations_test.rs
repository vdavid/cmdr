//! Tests for write operations (copy, move, delete).
//!
//! Note: Serialization tests were removed - serde derive macros are well-tested.
//! We keep deserialization tests as they verify the API contract with the frontend.

use super::write_operations::*;
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
        cancelled: AtomicBool::new(false),
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
