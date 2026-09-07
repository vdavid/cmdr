//! The vocabulary every write operation shares: the cancel intent machine, the
//! config the frontend sends, and how an `io::Error` becomes a typed
//! `WriteOperationError`.
//!
//! Everything with a subject of its own lives next to that subject:
//! `overwrite_tests.rs`, `ledger_tests.rs` (`CopyTransaction`), and
//! `delete/delete_cancel_tests.rs`.
//!
//! Deserialization is tested and serialization isn't: the derive macros are
//! well covered upstream, and it's the DEserialization that pins the API
//! contract with the frontend.

use super::*;
use crate::test_support::TestDir;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Creates a temporary test directory with a unique name.
fn create_temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("write_test_{}", name))
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
    assert_eq!(config.conflict_resolution, ConflictResolution::Stop);
    assert!(!config.dry_run);
}

#[test]
fn test_config_default_values_deserialization() {
    let json = r#"{}"#;
    let config: WriteOperationConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.progress_interval_ms, 200);
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
    // The subject here is the fixture lifecycle itself, so the drop has to be
    // explicit: cleanup is no longer a call the test makes, it's `TestDir`'s
    // `Drop`. Capture the path first, since the handle is what owns it.
    let temp_dir = create_temp_dir("helper_test");
    let path = temp_dir.to_path_buf();
    assert!(temp_dir.exists());
    assert!(temp_dir.is_dir());

    drop(temp_dir);
    assert!(!path.exists(), "dropping the handle must remove the directory");
}

// ============================================================================
// OperationIntent transitions
// ============================================================================

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
// Whole-module guard: no global sync anywhere under `write_operations/`
// ============================================================================

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
    fn walk(dir: &Path, hits: &mut Vec<String>) {
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

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
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
