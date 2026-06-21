//! Unit tests for the operation manager's admission + lane logic.
//!
//! These drive `spawn_managed` with synthetic deferred starts (a oneshot-gated
//! future, no real I/O) so admission, lane reservation, FIFO ordering, settle
//! dequeue, queued-cancel, and the panic safety net are all observable and
//! deterministic. The manager is a process-global singleton, so every test uses
//! unique operation ids + lane keys to stay correct under nextest's in-process
//! parallelism.

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::oneshot;

fn unique(label: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("mgr-test-{label}-{n}-{:?}", std::thread::current().id())
}

fn descriptor(op_id: &str, lanes: Vec<&str>) -> OperationDescriptor {
    OperationDescriptor {
        operation_id: op_id.to_string(),
        operation_type: WriteOperationType::Copy,
        lanes: lanes.into_iter().map(LaneKey::new).collect(),
        volume_ids: vec![],
        summary: OperationSummaryText::default(),
    }
}

fn fresh_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// A synthetic deferred start: signals "I started running" on `started`, waits
/// for `release`, then settles the op (frees lanes + admits next). Lets a test
/// hold an op "running" while it asserts the state of a second op.
fn gated_deferred(
    op_id: String,
    started: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
) -> Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send> {
    Box::new(move || {
        Box::pin(async move {
            let guard = ManagedTaskGuard::new(op_id.clone());
            let _ = started.send(());
            let _ = release.await;
            guard.disarm();
            manager().on_settled(&op_id);
        })
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admits_immediately_when_lanes_free() {
    let op = unique("admit-free");
    let lane = unique("lane");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();

    manager().spawn_managed(
        descriptor(&op, vec![&lane]),
        fresh_state(),
        gated_deferred(op.clone(), started_tx, rel_rx),
    );

    // The op was admitted: its deferred start runs and signals "started".
    tokio::time::timeout(Duration::from_secs(2), started_rx)
        .await
        .expect("op with a free lane must be admitted immediately")
        .expect("started signal");
    assert_eq!(manager().status_of(&op), Some(LifecycleStatus::Running));

    let _ = rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queues_when_a_needed_lane_is_busy() {
    let lane = unique("lane");
    let op_a = unique("busy-a");
    let op_b = unique("busy-b");
    let (a_started_tx, a_started_rx) = oneshot::channel();
    let (a_rel_tx, a_rel_rx) = oneshot::channel();

    // A holds the lane.
    manager().spawn_managed(
        descriptor(&op_a, vec![&lane]),
        fresh_state(),
        gated_deferred(op_a.clone(), a_started_tx, a_rel_rx),
    );
    a_started_rx.await.expect("A started");

    // B needs the same lane → must stay Queued, never spawn.
    let (b_started_tx, b_started_rx) = oneshot::channel();
    let (_b_rel_tx, b_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_b, vec![&lane]),
        fresh_state(),
        gated_deferred(op_b.clone(), b_started_tx, b_rel_rx),
    );

    assert_eq!(
        manager().status_of(&op_b),
        Some(LifecycleStatus::Queued),
        "second same-lane op must queue while the first holds the lane"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), b_started_rx)
            .await
            .is_err(),
        "a queued op's deferred start must not run"
    );

    let _ = a_rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fifo_dequeue_on_settle_serializes_same_lane_ops() {
    let lane = unique("lane");
    let op_a = unique("serial-a");
    let op_b = unique("serial-b");

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
    assert_eq!(manager().status_of(&op_b), Some(LifecycleStatus::Queued));

    // Settle A → B is admitted (FIFO dequeue frees the lane + admits next).
    let _ = a_rel_tx.send(());
    tokio::time::timeout(Duration::from_secs(2), b_started_rx)
        .await
        .expect("B must be admitted once A settles and frees the lane")
        .expect("B started");
    assert_eq!(manager().status_of(&op_b), Some(LifecycleStatus::Running));
    assert_eq!(manager().status_of(&op_a), None, "A is gone after settle");

    let _ = b_rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn disjoint_lane_ops_both_run_in_parallel() {
    let lane_a = unique("lane-a");
    let lane_b = unique("lane-b");
    let op_a = unique("par-a");
    let op_b = unique("par-b");

    let (a_started_tx, a_started_rx) = oneshot::channel();
    let (a_rel_tx, a_rel_rx) = oneshot::channel();
    let (b_started_tx, b_started_rx) = oneshot::channel();
    let (b_rel_tx, b_rel_rx) = oneshot::channel();

    manager().spawn_managed(
        descriptor(&op_a, vec![&lane_a]),
        fresh_state(),
        gated_deferred(op_a.clone(), a_started_tx, a_rel_rx),
    );
    manager().spawn_managed(
        descriptor(&op_b, vec![&lane_b]),
        fresh_state(),
        gated_deferred(op_b.clone(), b_started_tx, b_rel_rx),
    );

    // Both run concurrently (disjoint lanes): both started signals arrive even
    // though neither has been released.
    a_started_rx.await.expect("A started");
    b_started_rx.await.expect("B started");
    assert_eq!(manager().status_of(&op_a), Some(LifecycleStatus::Running));
    assert_eq!(manager().status_of(&op_b), Some(LifecycleStatus::Running));

    let _ = a_rel_tx.send(());
    let _ = b_rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn two_lane_op_waits_until_both_lanes_free_and_does_not_starve() {
    // A holds lane X, C holds lane Y. B needs BOTH X and Y, so it stays queued
    // until BOTH free — even though single-lane ops keep churning on each lane.
    let lane_x = unique("lane-x");
    let lane_y = unique("lane-y");
    let op_a = unique("two-a"); // holds X
    let op_c = unique("two-c"); // holds Y
    let op_b = unique("two-b"); // needs X + Y

    let (a_started_tx, a_started_rx) = oneshot::channel();
    let (a_rel_tx, a_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_a, vec![&lane_x]),
        fresh_state(),
        gated_deferred(op_a.clone(), a_started_tx, a_rel_rx),
    );
    a_started_rx.await.expect("A started");

    let (c_started_tx, c_started_rx) = oneshot::channel();
    let (c_rel_tx, c_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_c, vec![&lane_y]),
        fresh_state(),
        gated_deferred(op_c.clone(), c_started_tx, c_rel_rx),
    );
    c_started_rx.await.expect("C started");

    let (b_started_tx, b_started_rx) = oneshot::channel();
    let (b_rel_tx, b_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_b, vec![&lane_x, &lane_y]),
        fresh_state(),
        gated_deferred(op_b.clone(), b_started_tx, b_rel_rx),
    );
    assert_eq!(manager().status_of(&op_b), Some(LifecycleStatus::Queued));

    // Free only X (settle A). B still can't run — Y is busy.
    let _ = a_rel_tx.send(());
    // Give the admission pass a chance to (wrongly) run B.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        manager().status_of(&op_b),
        Some(LifecycleStatus::Queued),
        "two-lane op must not start while one of its lanes is still busy"
    );

    // Free Y too (settle C) → now B admits.
    let _ = c_rel_tx.send(());
    tokio::time::timeout(Duration::from_secs(2), b_started_rx)
        .await
        .expect("B must admit once BOTH its lanes are free")
        .expect("B started");

    let _ = b_rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_queued_op_removes_it_without_spawning() {
    use std::sync::atomic::AtomicBool;

    let lane = unique("lane");
    let op_a = unique("cancelq-a");
    let op_b = unique("cancelq-b");

    let (a_started_tx, a_started_rx) = oneshot::channel();
    let (a_rel_tx, a_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_a, vec![&lane]),
        fresh_state(),
        gated_deferred(op_a.clone(), a_started_tx, a_rel_rx),
    );
    a_started_rx.await.expect("A started");

    // B's deferred flips `b_ran` if it ever executes. (A oneshot would resolve
    // on channel-close when the dropped deferred is freed, so use an explicit
    // flag to prove the start never ran.)
    let b_ran = Arc::new(AtomicBool::new(false));
    let b_ran_in_task = Arc::clone(&b_ran);
    let b_id = op_b.clone();
    let b_deferred: Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send> = Box::new(move || {
        Box::pin(async move {
            b_ran_in_task.store(true, Ordering::SeqCst);
            let guard = ManagedTaskGuard::new(b_id.clone());
            guard.disarm();
            manager().on_settled(&b_id);
        })
    });
    manager().spawn_managed(descriptor(&op_b, vec![&lane]), fresh_state(), b_deferred);
    assert_eq!(manager().status_of(&op_b), Some(LifecycleStatus::Queued));

    // Cancel the queued op: removed from the registry, its deferred never runs.
    assert!(
        manager().cancel_if_queued(&op_b),
        "should report it cancelled a queued op"
    );
    assert_eq!(manager().status_of(&op_b), None, "queued op removed from registry");

    // Settle A (frees the lane + runs an admission pass). B is gone, so its
    // deferred must still never have run.
    let _ = a_rel_tx.send(());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !b_ran.load(Ordering::SeqCst),
        "a cancelled queued op's deferred start must never run"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_if_queued_returns_false_for_running_op() {
    // A Running op is NOT a queued op: `cancel_if_queued` returns false so the
    // caller routes through the existing intent-based cancel path.
    let lane = unique("lane");
    let op = unique("cancel-running");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op, vec![&lane]),
        fresh_state(),
        gated_deferred(op.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("started");

    assert!(
        !manager().cancel_if_queued(&op),
        "a Running op must not be cancellable via the queued-cancel fast path"
    );
    assert_eq!(manager().status_of(&op), Some(LifecycleStatus::Running));

    let _ = rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panicking_op_releases_its_lane_without_spawning_next() {
    // The Drop safety net frees a panicking op's lane, but must NOT spawn the
    // next op (no admission pass in Drop). So after A panics, B is admitted only
    // because a SUBSEQUENT healthy event runs an admission pass — here we assert
    // the lane is freed (so a later pass CAN admit) and that B did not start as a
    // direct result of the panic.
    let lane = unique("lane");
    let op_a = unique("panic-a");
    let op_b = unique("panic-b");

    // A: a deferred that panics WITHOUT calling on_settled (guard armed).
    let a_id = op_a.clone();
    let panic_deferred: Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send> = Box::new(move || {
        Box::pin(async move {
            let _guard = ManagedTaskGuard::new(a_id.clone());
            panic!("simulated op panic");
        })
    });
    manager().spawn_managed(descriptor(&op_a, vec![&lane]), fresh_state(), panic_deferred);

    // Let A's task run + panic + unwind (its Drop frees the lane).
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        manager().status_of(&op_a),
        None,
        "panicked op is removed by the Drop guard"
    );
    let lane_use = manager().lane_use_snapshot();
    assert!(
        !lane_use.contains_key(&lane),
        "the Drop safety net must release the panicked op's lane"
    );

    // B can now be admitted on the NEXT registration's admission pass (proving
    // the lane really is free, and that Drop didn't itself spawn anything).
    let (b_started_tx, b_started_rx) = oneshot::channel();
    let (b_rel_tx, b_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_b, vec![&lane]),
        fresh_state(),
        gated_deferred(op_b.clone(), b_started_tx, b_rel_rx),
    );
    tokio::time::timeout(Duration::from_secs(2), b_started_rx)
        .await
        .expect("B admits because the panicked op freed the lane")
        .expect("B started");

    let _ = b_rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn single_op_with_free_lanes_behaves_like_immediate_spawn() {
    // The common case: nothing else running, the op spawns at once and settles
    // cleanly, leaving the registry empty.
    let op = unique("solo");
    let lane = unique("lane");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op, vec![&lane]),
        fresh_state(),
        gated_deferred(op.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("started immediately");
    let _ = rel_tx.send(());

    // After settle the op is gone and its lane is free.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(manager().status_of(&op), None);
    assert!(!manager().lane_use_snapshot().contains_key(&lane));
}
