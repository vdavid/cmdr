//! Tests for the operation status cache and the busy-volume set it drives.
//!
//! Each test keys its cache entries per test (via `unique_id`) so they don't
//! collide when nextest runs them in one process against the module's
//! process-global `OPERATION_STATUS_CACHE`.

use super::*;
use crate::file_system::volume::LaneKey;
use crate::file_system::write_operations::manager::{
    LifecycleStatus, OperationDescriptor, OperationSummaryText, PauseOutcome, manager,
};
use crate::file_system::write_operations::state::{OperationIntent, WRITE_OPERATION_STATE, WriteOperationState};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{WriteOperationPhase, WriteOperationType};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::oneshot;

/// A cache key nothing else in the suite will collide with.
fn unique_id(label: &str) -> String {
    use std::sync::atomic::AtomicU64;
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("test-status-cache-{label}-{n}-{:?}", std::thread::current().id())
}

/// Installs a live `WRITE_OPERATION_STATE` entry. The guard removes it on drop,
/// so a failing assertion can't leak it into another test.
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
    // This op was never registered with the MANAGER, only with the state map, so
    // there is no lifecycle record to report. That is the honest answer: the
    // status cache does not invent one.
    assert_eq!(status.lifecycle, None);
    assert_eq!(status.files_done, 0);
    assert_eq!(status.files_total, 0);
    assert_eq!(status.bytes_done, 0);
    assert_eq!(status.bytes_total, 0);

    drop(op);
    unregister_operation_status(&id);
    assert!(get_operation_status(&id).is_none());
}

/// A real managed operation on a lane nothing else touches, so admission runs it
/// immediately and the manager holds a `Running` record for it. Its status-cache
/// row is registered too, which is what `get_operation_status` reads.
///
/// Releases on drop so the lane frees for the rest of the process.
struct ManagedOperationFixture {
    id: String,
    lane: LaneKey,
    release: Option<oneshot::Sender<()>>,
}

impl ManagedOperationFixture {
    fn start(tag: &str) -> Self {
        let lane = LaneKey::new(format!("{}-lane", unique_id(tag)));
        Self::start_on_lane(tag, lane)
    }

    /// Same, on a caller-supplied lane, so a second operation can be made to
    /// queue behind the first.
    fn start_on_lane(tag: &str, lane: LaneKey) -> Self {
        let id = unique_id(tag);
        let (release_tx, release_rx) = oneshot::channel();
        let settle_id = id.clone();
        manager().spawn_managed(
            OperationDescriptor {
                operation_id: id.clone(),
                operation_type: WriteOperationType::Copy,
                lanes: vec![lane.clone()],
                volume_ids: vec![],
                summary: OperationSummaryText::default(),
                supports_rollback: false,
                preview_id: None,
            },
            Arc::new(WriteOperationState::new(Duration::from_millis(50))),
            Box::new(move || {
                Box::pin(async move {
                    let _ = release_rx.await;
                    manager().on_settled(&settle_id);
                })
            }),
        );
        register_operation_status(&id, WriteOperationType::Copy, vec![]);
        Self {
            id,
            lane,
            release: Some(release_tx),
        }
    }
}

impl Drop for ManagedOperationFixture {
    fn drop(&mut self) {
        unregister_operation_status(&self.id);
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

/// The bug the `lifecycle` field exists to prevent: a paused operation keeps its
/// `WRITE_OPERATION_STATE` entry (it still holds its lane slots and parks between
/// files), so the presence test this replaced answered "running" while parked.
/// Every surface that steers off the answer — the pause/resume toggle above all
/// — then tried to pause what was already parked.
#[test]
fn a_paused_operation_reports_paused_not_running() {
    let op = ManagedOperationFixture::start("paused-lifecycle");
    assert_eq!(
        manager().lifecycle_status(&op.id),
        Some(LifecycleStatus::Running),
        "precondition: a fresh lane admits the operation immediately"
    );

    assert_eq!(manager().set_paused(&op.id, true), PauseOutcome::Applied);

    let status = get_operation_status(&op.id).expect("a paused operation is still in the status cache");
    assert_eq!(status.lifecycle, Some(LifecycleStatus::Paused));
    // The state-map entry is still there, which is exactly why the old boolean
    // got this backwards.
    assert!(WRITE_OPERATION_STATE.contains(&op.id));
}

/// The other half of the same bug, and the one that shows how far the boolean
/// missed: `spawn_managed` inserts the state entry BEFORE admission, so an
/// operation still waiting for its lane — one that has not written a byte and
/// may never be admitted — was reported as running too.
#[test]
fn a_queued_operation_reports_queued() {
    let holder = ManagedOperationFixture::start("queued-holder");
    // A second operation on the SAME lane can't be admitted until the first frees it.
    let queued = ManagedOperationFixture::start_on_lane("queued-waiter", holder.lane.clone());

    assert_eq!(manager().lifecycle_status(&queued.id), Some(LifecycleStatus::Queued));
    let status = get_operation_status(&queued.id).expect("a queued operation has a status-cache row");
    assert_eq!(status.lifecycle, Some(LifecycleStatus::Queued));
    assert!(
        WRITE_OPERATION_STATE.contains(&queued.id),
        "the state entry lands at spawn, not at admission — which is exactly why a presence test called this one running"
    );
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
