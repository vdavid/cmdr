//! The snapshot's `conflict_policy`: what a live row reports, and the broadcast
//! a latched change earns (`conflict_policy_cell.rs`).

use super::*;
use crate::types::ConflictResolution;

/// The policy a row reports, read the way every window reads it.
fn policy_of(engine: &TestOperations, op: &OperationId) -> Option<ConflictResolution> {
    engine
        .manager()
        .list()
        .into_iter()
        .find(|row| row.operation_id == op.as_str())
        .and_then(|row| row.conflict_policy)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_row_reports_its_policy_and_a_latched_change_is_broadcast_once() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let op = OperationId::from("policy-op");
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(0))
            .with_conflict_policy(ConflictResolution::Stop),
    );
    mgr.spawn_managed(
        descriptor(&op, vec!["lane"]),
        Arc::clone(&state),
        gated_deferred(op.clone(), started_tx, release_rx),
    );
    started_rx.await.expect("started");
    assert_eq!(
        policy_of(&engine, &op),
        Some(ConflictResolution::Stop),
        "the row starts on the confirmed policy"
    );

    let base = mgr.emit_count();
    state.conflict_policy.latch(ConflictResolution::Overwrite);
    assert_eq!(
        policy_of(&engine, &op),
        Some(ConflictResolution::Overwrite),
        "an \"Overwrite all\" answer is what the row says from then on"
    );
    assert_eq!(mgr.emit_count(), base + 1, "and every window hears it");

    state.conflict_policy.latch(ConflictResolution::Overwrite);
    assert_eq!(mgr.emit_count(), base + 1, "a repeat of the same answer says nothing");

    let _ = release_tx.send(());
}

/// When it started running, for the progress dialog's "time taken" (#484): the
/// moment it was admitted, which a queued row doesn't have yet.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_row_says_when_it_started_and_a_queued_one_does_not() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = (OperationId::from("started-a"), OperationId::from("started-b"));
    let (a_started_tx, a_started_rx) = oneshot::channel();
    let (a_release_tx, a_release_rx) = oneshot::channel();
    let (b_started_tx, _b_started_rx) = oneshot::channel();
    let (b_release_tx, b_release_rx) = oneshot::channel();
    mgr.spawn_managed(
        descriptor(&running, vec!["lane"]),
        engine.new_state(),
        gated_deferred(running.clone(), a_started_tx, a_release_rx),
    );
    a_started_rx.await.expect("A started");
    mgr.spawn_managed(
        descriptor(&queued, vec!["lane"]),
        engine.new_state(),
        gated_deferred(queued.clone(), b_started_tx, b_release_rx),
    );

    let started_at = |op: &OperationId| {
        mgr.list()
            .into_iter()
            .find(|row| row.operation_id == op.as_str())
            .and_then(|row| row.started_at)
    };
    assert!(
        started_at(&running).is_some_and(|ms| ms > 0),
        "a running op knows when it started"
    );
    assert_eq!(started_at(&queued), None, "a queued one hasn't started");

    let _ = a_release_tx.send(());
    let _ = b_release_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_operation_with_no_policy_reports_none() {
    let engine = TestOperations::new();
    let op = OperationId::from("no-policy-op");
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    engine.manager().spawn_managed(
        descriptor(&op, vec!["lane"]),
        engine.new_state(),
        gated_deferred(op.clone(), started_tx, release_rx),
    );
    started_rx.await.expect("started");
    assert_eq!(
        policy_of(&engine, &op),
        None,
        "a delete or a trash has no policy to show"
    );
    let _ = release_tx.send(());
}
