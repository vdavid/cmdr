//! Tests for the operation status cache and the busy-volume set it drives.
//!
//! Each test keys its cache entries per test (via `unique_id`) so they don't
//! collide when nextest runs them in one process against the module's
//! process-global `OPERATION_STATUS_CACHE`.

use super::*;
use crate::file_system::write_operations::state::OperationIntent;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{WriteOperationPhase, WriteOperationType};
use std::sync::atomic::Ordering;

/// A cache key nothing else in the suite will collide with.
fn unique_id(label: &str) -> String {
    use std::sync::atomic::AtomicU64;
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("test-status-cache-{label}-{n}-{:?}", std::thread::current().id())
}

/// Installs a live operation so `get_operation_status`'s `is_running` has
/// something to read. The guard removes the entry on drop, so a failing
/// assertion can't leak it into another test.
fn install_state(label: &str, initial: OperationIntent) -> TestOperationGuard {
    let op = TestOperationGuard::register(label);
    op.state().intent.store(initial as u8, Ordering::Relaxed);
    op
}

// ---- register / update / unregister + list / get ----

#[test]
fn register_then_get_status_roundtrip() {
    let op = install_state("reg-get", OperationIntent::Running);
    let id = op.id().to_string();
    register_operation_status(&id, WriteOperationType::Copy, vec![]);

    let status = get_operation_status(&id).expect("operation should be in cache");
    assert_eq!(status.operation_id, id);
    assert_eq!(status.operation_type, WriteOperationType::Copy);
    assert_eq!(status.phase, WriteOperationPhase::Scanning);
    assert!(
        status.is_running,
        "is_running must reflect WRITE_OPERATION_STATE presence"
    );
    assert_eq!(status.files_done, 0);
    assert_eq!(status.files_total, 0);
    assert_eq!(status.bytes_done, 0);
    assert_eq!(status.bytes_total, 0);

    // is_running flips when the WRITE_OPERATION_STATE entry is removed.
    drop(op);
    let status = get_operation_status(&id).expect("status cache still has it");
    assert!(!status.is_running);

    unregister_operation_status(&id);
    assert!(get_operation_status(&id).is_none());
}

#[test]
fn update_operation_status_overwrites_fields() {
    let id = unique_id("update");
    register_operation_status(&id, WriteOperationType::Move, vec![]);
    update_operation_status(
        &id,
        WriteOperationPhase::Copying,
        Some("a.txt".into()),
        3,
        10,
        500,
        1000,
    );
    let status = get_operation_status(&id).unwrap();
    assert_eq!(status.phase, WriteOperationPhase::Copying);
    assert_eq!(status.current_file.as_deref(), Some("a.txt"));
    assert_eq!(status.files_done, 3);
    assert_eq!(status.files_total, 10);
    assert_eq!(status.bytes_done, 500);
    assert_eq!(status.bytes_total, 1000);
    unregister_operation_status(&id);
}

#[test]
fn update_unknown_id_is_a_silent_noop() {
    // Pins the `&& get_mut` short-circuit. If `&&` becomes `||`, this would
    // dereference a None and panic.
    update_operation_status("no-such-op-xyzzy", WriteOperationPhase::Copying, None, 0, 0, 0, 0);
}

#[test]
fn list_active_operations_percent_uses_bytes_when_available() {
    // bytes_total > 0 → percent comes from bytes axis, not files.
    let id = unique_id("list-bytes");
    register_operation_status(&id, WriteOperationType::Copy, vec![]);
    update_operation_status(
        &id,
        WriteOperationPhase::Copying,
        None,
        1,    // files_done
        100,  // files_total (would give 1% if used)
        500,  // bytes_done
        1000, // bytes_total → 50%
    );
    let summary = list_active_operations()
        .into_iter()
        .find(|s| s.operation_id == id)
        .expect("operation present in summary");
    assert_eq!(
        summary.percent_complete, 50,
        "percent must be derived from bytes axis when bytes_total > 0"
    );
    unregister_operation_status(&id);
}

#[test]
fn list_active_operations_percent_falls_back_to_files() {
    // bytes_total == 0, files_total > 0 → use files axis.
    let id = unique_id("list-files");
    register_operation_status(&id, WriteOperationType::Delete, vec![]);
    update_operation_status(&id, WriteOperationPhase::Deleting, None, 3, 4, 0, 0);
    let summary = list_active_operations()
        .into_iter()
        .find(|s| s.operation_id == id)
        .unwrap();
    assert_eq!(summary.percent_complete, 75);
    unregister_operation_status(&id);
}

#[test]
fn list_active_operations_percent_is_zero_when_nothing_known() {
    // Both totals == 0 → percent_complete == 0 (not the files-axis path).
    let id = unique_id("list-zero");
    register_operation_status(&id, WriteOperationType::Copy, vec![]);
    let summary = list_active_operations()
        .into_iter()
        .find(|s| s.operation_id == id)
        .unwrap();
    assert_eq!(summary.percent_complete, 0);
    unregister_operation_status(&id);
}

#[test]
fn list_active_operations_percent_clamps_to_100() {
    // Pin the `.min(100.0)` clamp. If bytes_done > bytes_total (which can
    // happen in flight due to over-counting), the UI must never see > 100.
    let id = unique_id("list-clamp");
    register_operation_status(&id, WriteOperationType::Copy, vec![]);
    update_operation_status(&id, WriteOperationPhase::Copying, None, 0, 0, 1500, 1000);
    let summary = list_active_operations()
        .into_iter()
        .find(|s| s.operation_id == id)
        .unwrap();
    assert_eq!(summary.percent_complete, 100);
    unregister_operation_status(&id);
}
