//! What a cancel does to a delete: the loop stops, the directory phase stops,
//! and the cancelled event says honestly that nothing came back.
//!
//! A delete can't be undone, so its reversal outcome is always "nothing was
//! rolled back". The three-state outcome exists so the dialog can tell that
//! apart from a partial reversal, and that's what these pin.
//!
//! A `#[path]` child of `delete/mod.rs`, so `super::` here is `delete` and
//! `super::super::` is `write_operations`.

use super::super::state::{OperationIntent, WriteOperationState, is_cancelled};
use super::super::types::{CancelRollback, CancelRollbackOutcome, WriteCancelledEvent, WriteOperationType};
use crate::test_support::TestDir;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Creates a temporary test directory with a unique name.
fn create_temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("write_test_{}", name))
}

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

/// A delete can't be undone, so its cancel always reports that nothing came back.
#[test]
fn a_cancelled_delete_reports_no_reversal() {
    let event = WriteCancelledEvent {
        operation_id: "delete-test".to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: 3,
        rollback: CancelRollback::none(),
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(
        json.contains("\"outcome\":\"notRolledBack\""),
        "a cancelled delete reverses nothing, got {json}"
    );
    assert!(
        json.contains("\"operationType\":\"delete\""),
        "Should serialize as delete operation type"
    );
}

/// The cancelled event carries the three-state reversal outcome over the wire,
/// so the dialog can tell "nothing was undone" from "some of it was".
#[test]
fn a_cancelled_event_carries_what_the_reversal_managed() {
    let fully_reversed = WriteCancelledEvent {
        operation_id: "test-123".to_string(),
        operation_type: WriteOperationType::Copy,
        files_processed: 5,
        rollback: CancelRollback {
            outcome: CancelRollbackOutcome::RolledBack,
            reversed: 5,
            skips: Vec::new(),
            staged_leftovers: None,
            originals_still_in_place: None,
        },
    };

    let json = serde_json::to_string(&fully_reversed).unwrap();
    assert!(json.contains("\"outcome\":\"rolledBack\""), "got {json}");
    assert!(json.contains("\"reversed\":5"), "got {json}");

    let nothing_reversed = WriteCancelledEvent {
        operation_id: "test-456".to_string(),
        operation_type: WriteOperationType::Copy,
        files_processed: 3,
        rollback: CancelRollback::none(),
    };

    let json = serde_json::to_string(&nothing_reversed).unwrap();
    assert!(json.contains("\"outcome\":\"notRolledBack\""), "got {json}");
    assert!(json.contains("\"skips\":[]"), "got {json}");
}
