//! Tests for write operations (copy, move, delete).

use super::write_operations::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
    });

    assert!(!state.cancelled.load(Ordering::Relaxed));
    state.cancelled.store(true, Ordering::Relaxed);
    assert!(state.cancelled.load(Ordering::Relaxed));
}

// ============================================================================
// Config tests
// ============================================================================

#[test]
fn test_default_config() {
    let config = WriteOperationConfig::default();
    assert_eq!(config.progress_interval_ms, 200);
    assert!(!config.overwrite);
}

#[test]
fn test_config_serialization() {
    let config = WriteOperationConfig {
        progress_interval_ms: 500,
        overwrite: true,
    };

    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("progressIntervalMs"));
    assert!(json.contains("500"));
    assert!(json.contains("overwrite"));
    assert!(json.contains("true"));
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
}

// ============================================================================
// Error serialization tests
// ============================================================================

#[test]
fn test_error_serialization_source_not_found() {
    let error = WriteOperationError::SourceNotFound {
        path: "/test/path".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"source_not_found\""));
    assert!(json.contains("/test/path"));
}

#[test]
fn test_error_serialization_destination_exists() {
    let error = WriteOperationError::DestinationExists {
        path: "/dest/path".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"destination_exists\""));
}

#[test]
fn test_error_serialization_permission_denied() {
    let error = WriteOperationError::PermissionDenied {
        path: "/protected".to_string(),
        message: "Access denied".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"permission_denied\""));
    assert!(json.contains("Access denied"));
}

#[test]
fn test_error_serialization_destination_inside_source() {
    let error = WriteOperationError::DestinationInsideSource {
        source: "/foo".to_string(),
        destination: "/foo/bar".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"destination_inside_source\""));
}

#[test]
fn test_error_serialization_same_location() {
    let error = WriteOperationError::SameLocation {
        path: "/same/path".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"same_location\""));
}

#[test]
fn test_error_serialization_insufficient_space() {
    let error = WriteOperationError::InsufficientSpace {
        required: 1024,
        available: 512,
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"insufficient_space\""));
    assert!(json.contains("\"required\":1024"));
    assert!(json.contains("\"available\":512"));
}

#[test]
fn test_error_serialization_cancelled() {
    let error = WriteOperationError::Cancelled {
        message: "User cancelled".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"cancelled\""));
    assert!(json.contains("User cancelled"));
}

#[test]
fn test_error_serialization_io_error() {
    let error = WriteOperationError::IoError {
        path: "/some/file".to_string(),
        message: "Read failed".to_string(),
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("\"type\":\"io_error\""));
    assert!(json.contains("Read failed"));
}

// ============================================================================
// Event serialization tests
// ============================================================================

#[test]
fn test_progress_event_serialization() {
    let event = WriteProgressEvent {
        operation_id: "test-id".to_string(),
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        current_file: Some("file.txt".to_string()),
        files_done: 5,
        files_total: 10,
        bytes_done: 1024,
        bytes_total: 2048,
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("operationId"));
    assert!(json.contains("test-id"));
    assert!(json.contains("operationType"));
    assert!(json.contains("copy"));
    assert!(json.contains("phase"));
    assert!(json.contains("copying"));
    assert!(json.contains("currentFile"));
    assert!(json.contains("file.txt"));
    assert!(json.contains("filesDone"));
    assert!(json.contains("filesTotal"));
    assert!(json.contains("bytesDone"));
    assert!(json.contains("bytesTotal"));
}

#[test]
fn test_progress_event_with_null_current_file() {
    let event = WriteProgressEvent {
        operation_id: "test-id".to_string(),
        operation_type: WriteOperationType::Delete,
        phase: WriteOperationPhase::Scanning,
        current_file: None,
        files_done: 0,
        files_total: 0,
        bytes_done: 0,
        bytes_total: 0,
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"currentFile\":null"));
}

#[test]
fn test_complete_event_serialization() {
    let event = WriteCompleteEvent {
        operation_id: "test-id".to_string(),
        operation_type: WriteOperationType::Move,
        files_processed: 100,
        bytes_processed: 1048576,
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"operationType\":\"move\""));
    assert!(json.contains("\"filesProcessed\":100"));
    assert!(json.contains("\"bytesProcessed\":1048576"));
}

#[test]
fn test_cancelled_event_serialization() {
    let event = WriteCancelledEvent {
        operation_id: "test-id".to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: 50,
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"operationType\":\"delete\""));
    assert!(json.contains("\"filesProcessed\":50"));
}

#[test]
fn test_error_event_serialization() {
    let event = WriteErrorEvent {
        operation_id: "test-id".to_string(),
        operation_type: WriteOperationType::Copy,
        error: WriteOperationError::SourceNotFound {
            path: "/missing".to_string(),
        },
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"operationId\":\"test-id\""));
    assert!(json.contains("\"operationType\":\"copy\""));
    assert!(json.contains("\"type\":\"source_not_found\""));
}

// ============================================================================
// Start result tests
// ============================================================================

#[test]
fn test_start_result_serialization() {
    let result = WriteOperationStartResult {
        operation_id: "uuid-here".to_string(),
        operation_type: WriteOperationType::Copy,
    };

    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("operationId"));
    assert!(json.contains("uuid-here"));
    assert!(json.contains("operationType"));
    assert!(json.contains("copy"));
}

// ============================================================================
// Operation type tests
// ============================================================================

#[test]
fn test_operation_type_equality() {
    assert_eq!(WriteOperationType::Copy, WriteOperationType::Copy);
    assert_ne!(WriteOperationType::Copy, WriteOperationType::Move);
    assert_ne!(WriteOperationType::Move, WriteOperationType::Delete);
}

#[test]
fn test_operation_type_serialization() {
    assert_eq!(
        serde_json::to_string(&WriteOperationType::Copy).unwrap(),
        "\"copy\""
    );
    assert_eq!(
        serde_json::to_string(&WriteOperationType::Move).unwrap(),
        "\"move\""
    );
    assert_eq!(
        serde_json::to_string(&WriteOperationType::Delete).unwrap(),
        "\"delete\""
    );
}

#[test]
fn test_operation_phase_equality() {
    assert_eq!(WriteOperationPhase::Scanning, WriteOperationPhase::Scanning);
    assert_ne!(WriteOperationPhase::Scanning, WriteOperationPhase::Copying);
    assert_ne!(WriteOperationPhase::Copying, WriteOperationPhase::Deleting);
}

#[test]
fn test_operation_phase_serialization() {
    assert_eq!(
        serde_json::to_string(&WriteOperationPhase::Scanning).unwrap(),
        "\"scanning\""
    );
    assert_eq!(
        serde_json::to_string(&WriteOperationPhase::Copying).unwrap(),
        "\"copying\""
    );
    assert_eq!(
        serde_json::to_string(&WriteOperationPhase::Deleting).unwrap(),
        "\"deleting\""
    );
}

// ============================================================================
// IO Error conversion tests
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
