//! Retained-failure tests: the bounded, out-of-band list that outlives an op's
//! record, so a failure landing while the queue window is closed isn't lost.
//!
//! Sits beside the admission/lane suite in `tests.rs` and reuses its fixtures
//! (`unique`, `descriptor`, `gated_deferred`(`_on`), `WAIT`) through
//! `use super::*`.

use super::*;

/// A representative real failure.
fn io_error(path: &str) -> WriteOperationError {
    WriteOperationError::IoError {
        path: path.to_string(),
        message: "disk went away".to_string(),
    }
}

/// The rows of `mgr`'s snapshot carrying `op_id`. Tests on the global manager
/// pass `manager()` and scope themselves to their own unique id; tests on a
/// private manager pass that.
fn rows_for(mgr: &OperationManager, op_id: &str) -> Vec<OperationSnapshot> {
    mgr.list().into_iter().filter(|row| row.operation_id == op_id).collect()
}

/// A manager instance the test OWNS, for the assertions whose subject is the
/// failure list itself (eviction depth, "dismiss all" emptying it). Those can't
/// run against the global singleton without reaching into a sibling test's
/// retained failures — the same reasoning behind `TestOperationGuard`.
fn private_manager() -> OperationManager {
    OperationManager::new()
}

/// A private manager that also satisfies `spawn_managed`'s `&'static self`, so
/// a test can drive a real spawn-to-settle lifecycle in isolation. Needed by
/// any assertion on `emit_count()`, which counts a WHOLE manager's broadcasts:
/// on the global one, a sibling test spawning or settling an op moves it
/// underneath the assertion.
///
/// Leaking is the honest price of that signature (an admitted op's task
/// outlives every borrow), and it's bounded: one small struct per test that
/// calls this, reclaimed at process exit.
fn leaked_manager() -> &'static OperationManager {
    Box::leak(Box::new(OperationManager::new()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_failure_stays_hidden_until_the_record_settles() {
    // `emit_error` runs inside the op's own task, BEFORE `on_settled` removes
    // the record. So for a moment the op is both live and failed, and a visible
    // failure row would put one operationId in the snapshot twice — which throws
    // in the queue window's keyed `{#each}`.
    //
    // On a PRIVATE manager: the emit-silence assertion below is an equality on
    // `emit_count()`, and that counter belongs to a whole manager, not to one
    // op. Every broadcast this one makes is this test's own.
    let mgr = leaked_manager();
    let op = unique("fail-live");
    let lane = unique("lane");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();

    let mut desc = descriptor(&op, vec![&lane]);
    desc.summary = OperationSummaryText {
        source: Some("/Users/me/photos".to_string()),
        destination: Some("Naspolya".to_string()),
    };
    mgr.spawn_managed(
        desc,
        fresh_state(),
        gated_deferred_on(mgr, op.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("started");

    let emits_before = mgr.emit_count();
    mgr.record_failure(&op, WriteOperationType::Copy, &io_error("/Users/me/photos/a.raw"));

    let live = rows_for(mgr, &op);
    assert_eq!(live.len(), 1, "the retained failure must not duplicate a live op's id");
    assert_eq!(live[0].status, LifecycleStatus::Running, "the live row is unchanged");
    assert!(live[0].error.is_none(), "a live row never carries an error");
    assert_eq!(
        mgr.emit_count(),
        emits_before,
        "record_failure must not emit; on_settled's existing emit is the correct moment"
    );

    // Settle → the record goes and the retained failure takes its place.
    let _ = rel_tx.send(());
    wait_until_async(WAIT, "the failed op to settle and surface its retained row", || {
        rows_for(mgr, &op)
            .iter()
            .any(|row| row.status == LifecycleStatus::Failed)
    })
    .await;

    assert_eq!(
        mgr.lifecycle_status(&op),
        None,
        "the record itself is still removed on settle"
    );
    let retained = rows_for(mgr, &op);
    assert_eq!(retained.len(), 1, "exactly one row per operation id, always");
    assert!(
        matches!(retained[0].error, Some(WriteOperationError::IoError { .. })),
        "the retained row carries the typed error, got {:?}",
        retained[0].error
    );
    assert_eq!(
        retained[0].source.as_deref(),
        Some("/Users/me/photos"),
        "the retained row keeps the live record's summary, so the queue row still reads like the others"
    );
    assert_eq!(retained[0].destination.as_deref(), Some("Naspolya"));
    assert!(
        !retained[0].supports_rollback,
        "a settled failure offers no rollback from this row"
    );
    // No `dismiss_failure` cleanup: the retained row lives on this test's own
    // manager, where no sibling can ever see it.
}

#[test]
fn a_second_failure_for_one_operation_keeps_the_first_error() {
    // `emit_error` can fire twice for one op (an inner handler emits and returns
    // `Err`, then `mod.rs`'s safety net emits again). The first error is the one
    // that actually stopped it.
    let mgr = private_manager();
    let first = WriteOperationError::ReadOnlyDevice {
        path: "/Volumes/stick/a.txt".to_string(),
        device_name: Some("stick".to_string()),
    };
    mgr.record_failure("op-double", WriteOperationType::Delete, &first);
    mgr.record_failure(
        "op-double",
        WriteOperationType::Delete,
        &io_error("/Volumes/stick/a.txt"),
    );

    let rows = mgr.list();
    assert_eq!(rows.len(), 1, "a double emit must not double the row");
    assert!(
        matches!(rows[0].error, Some(WriteOperationError::ReadOnlyDevice { .. })),
        "first write wins, got {:?}",
        rows[0].error
    );
    // No live record exists here, so the row falls back to the event's own type
    // and reports no summary rather than inventing one.
    assert_eq!(rows[0].operation_type, WriteOperationType::Delete);
    assert_eq!(rows[0].source, None);
    assert_eq!(rows[0].destination, None);
}

#[test]
fn cancels_and_password_prompts_are_not_retained_as_failures() {
    // Both reach `emit_error` without being failures: a cancel is the user's own
    // doing (some volume paths emit it), and a password prompt is recoverable —
    // the FE sets the password and retries. Excluded by typed variant, never by
    // message text.
    let mgr = private_manager();
    mgr.record_failure(
        "op-cancelled",
        WriteOperationType::Copy,
        &WriteOperationError::Cancelled {
            message: "cancelled".to_string(),
        },
    );
    mgr.record_failure(
        "op-password",
        WriteOperationType::Copy,
        &WriteOperationError::ArchiveNeedsPassword {
            path: "/Users/me/locked.zip".to_string(),
            wrong_attempt: false,
        },
    );
    assert!(
        mgr.list().is_empty(),
        "neither a cancel nor a password prompt is a failure"
    );

    // ...and a real failure alongside them still lands, so this is a filter and
    // not a blanket refusal.
    mgr.record_failure("op-real", WriteOperationType::Copy, &io_error("/Users/me/a.txt"));
    assert_eq!(mgr.list().len(), 1);
    assert_eq!(mgr.list()[0].operation_id, "op-real");
}

#[test]
fn a_failure_with_no_live_record_broadcasts_itself() {
    // The other branch: the record is already gone, so no `on_settled` emit is
    // coming to carry this row out. Emitting here is safe precisely because the
    // record is absent — the duplicate-id hazard needs a LIVE record.
    let mgr = private_manager();
    let emits_before = mgr.emit_count();

    mgr.record_failure("op-orphan", WriteOperationType::Copy, &io_error("/a"));

    assert!(
        mgr.emit_count() > emits_before,
        "an unbroadcast row means no toast, no chip, and nothing until the queue window next opens"
    );
}

#[test]
fn retained_failures_evict_the_oldest_past_capacity() {
    let mgr = private_manager();
    for i in 0..(FAILURE_CAPACITY + 5) {
        mgr.record_failure(&format!("op-{i}"), WriteOperationType::Copy, &io_error("/x"));
    }
    let rows = mgr.list();
    assert_eq!(rows.len(), FAILURE_CAPACITY, "the list stays bounded");
    assert_eq!(rows[0].operation_id, "op-5", "the five oldest were evicted");
    assert_eq!(
        rows[rows.len() - 1].operation_id,
        format!("op-{}", FAILURE_CAPACITY + 4),
        "the newest failure is kept"
    );
}

#[test]
fn dismissing_one_failure_removes_exactly_it_and_re_emits() {
    let mgr = private_manager();
    mgr.record_failure("op-a", WriteOperationType::Copy, &io_error("/a"));
    mgr.record_failure("op-b", WriteOperationType::Move, &io_error("/b"));

    let emits_before = mgr.emit_count();
    mgr.dismiss_failure("op-a");
    let ids: Vec<String> = mgr.list().into_iter().map(|row| row.operation_id).collect();
    assert_eq!(ids, vec!["op-b".to_string()], "only the dismissed row goes");
    assert!(
        mgr.emit_count() > emits_before,
        "both windows have to be told the row is gone"
    );

    mgr.dismiss_failure("op-never-existed");
    assert_eq!(mgr.list().len(), 1, "dismissing an unknown id changes nothing");
}

#[test]
fn dismissing_all_failures_empties_the_list_and_re_emits() {
    let mgr = private_manager();
    mgr.record_failure("op-a", WriteOperationType::Copy, &io_error("/a"));
    mgr.record_failure("op-b", WriteOperationType::Trash, &io_error("/b"));

    let emits_before = mgr.emit_count();
    mgr.dismiss_all_failures();
    assert!(mgr.list().is_empty(), "Dismiss all clears every retained failure");
    assert!(mgr.emit_count() > emits_before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_op_frees_its_lane_and_admits_the_next_exactly_as_before() {
    // The discipline this milestone must not disturb: retention is out-of-band,
    // so `free_and_remove` still removes the record and releases the lane, and
    // `on_settled` still admits the next op.
    let lane = unique("lane");
    let op_a = unique("fail-lane-a");
    let op_b = unique("fail-lane-b");

    let (a_started_tx, a_started_rx) = oneshot::channel();
    let (a_rel_tx, a_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_a, vec![&lane]),
        fresh_state(),
        gated_deferred(op_a.clone(), a_started_tx, a_rel_rx),
    );
    a_started_rx.await.expect("A started");

    let (b_started_tx, b_started_rx) = oneshot::channel();
    let (b_rel_tx, b_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_b, vec![&lane]),
        fresh_state(),
        gated_deferred(op_b.clone(), b_started_tx, b_rel_rx),
    );
    assert_eq!(manager().lifecycle_status(&op_b), Some(LifecycleStatus::Queued));

    manager().record_failure(&op_a, WriteOperationType::Copy, &io_error("/a"));
    let _ = a_rel_tx.send(());

    tokio::time::timeout(Duration::from_secs(2), b_started_rx)
        .await
        .expect("a FAILED op must free its lane and admit the next one, exactly as a clean settle does")
        .expect("B started");
    assert_eq!(manager().lifecycle_status(&op_b), Some(LifecycleStatus::Running));
    assert_eq!(
        manager().lane_use_snapshot().get(&lane).copied(),
        Some(1),
        "the lane is held by B alone — the failed op's slot was released, not leaked"
    );
    assert_eq!(
        manager().lifecycle_status(&op_a),
        None,
        "the failed op's RECORD is gone; only the out-of-band failure row remains"
    );
    assert_eq!(
        rows_for(manager(), &op_a).first().map(|row| row.status),
        Some(LifecycleStatus::Failed)
    );

    let _ = b_rel_tx.send(());
    wait_until_async(WAIT, "B to settle and free the lane", || {
        manager().lifecycle_status(&op_b).is_none()
    })
    .await;
    assert!(
        !manager().lane_use_snapshot().contains_key(&lane),
        "the lane is free once B settles too"
    );
    manager().dismiss_failure(&op_a);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_successful_operation_settles_clean_and_retains_nothing() {
    let op = unique("ok-settle");
    let lane = unique("lane");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op, vec![&lane]),
        fresh_state(),
        gated_deferred(op.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("started");
    let _ = rel_tx.send(());

    wait_until_async(WAIT, "the op to settle and leave the registry", || {
        manager().lifecycle_status(&op).is_none()
    })
    .await;
    assert!(
        rows_for(manager(), &op).is_empty(),
        "a clean settle leaves no row behind — retention is failures-only"
    );
    assert!(
        !manager().lane_use_snapshot().contains_key(&lane),
        "and its lane is freed as before"
    );
}
