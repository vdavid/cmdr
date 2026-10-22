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
