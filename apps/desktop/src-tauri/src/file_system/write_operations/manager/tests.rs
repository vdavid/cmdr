//! Unit tests for the operation manager's admission + lane logic.
//!
//! These drive `spawn_managed` with synthetic deferred starts (a oneshot-gated
//! future, no real I/O) so admission, lane reservation, FIFO ordering, settle
//! dequeue, queued-cancel, and the panic safety net are all observable and
//! deterministic. The manager is a process-global singleton, so every test uses
//! unique operation ids + lane keys to stay correct under nextest's in-process
//! parallelism.

use super::super::state::busy_volume_ids;
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

/// A descriptor for an instant op (`run_instant`): no lanes, the given op type
/// and busy `volume_ids`.
fn instant_descriptor(op_id: &str, op_type: WriteOperationType, volume_ids: Vec<String>) -> OperationDescriptor {
    OperationDescriptor {
        operation_id: op_id.to_string(),
        operation_type: op_type,
        lanes: vec![],
        volume_ids,
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
async fn set_paused_flips_running_op_to_paused_and_keeps_its_lane() {
    // Pausing a Running op flips its status to Paused but does NOT free its lane
    // slot (a paused Running op still occupies the resource). Resuming flips it
    // back and still holds the lane.
    let lane = unique("lane");
    let op = unique("pause-op");
    let (started_tx, started_rx) = oneshot::channel();
    let (rel_tx, rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op, vec![&lane]),
        fresh_state(),
        gated_deferred(op.clone(), started_tx, rel_rx),
    );
    started_rx.await.expect("started");
    assert_eq!(manager().status_of(&op), Some(LifecycleStatus::Running));
    assert_eq!(manager().lane_use_snapshot().get(&lane).copied(), Some(1));

    assert!(manager().set_paused(&op, true), "pausing a Running op flips it");
    assert_eq!(manager().status_of(&op), Some(LifecycleStatus::Paused));
    assert_eq!(
        manager().lane_use_snapshot().get(&lane).copied(),
        Some(1),
        "a paused Running op must keep holding its lane slot"
    );

    assert!(manager().set_paused(&op, false), "resuming a Paused op flips it back");
    assert_eq!(manager().status_of(&op), Some(LifecycleStatus::Running));
    assert_eq!(manager().lane_use_snapshot().get(&lane).copied(), Some(1));

    let _ = rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paused_running_op_does_not_admit_a_queued_same_lane_op() {
    // The whole point of keeping the lane while paused: a same-lane queued op
    // must NOT start (it would fight the paused op on resume).
    let lane = unique("lane");
    let op_a = unique("paused-holder");
    let op_b = unique("waiting");

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

    // Pause A. B must still be Queued (A kept the lane) — pause runs no admission.
    assert!(manager().set_paused(&op_a, true));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), b_started_rx)
            .await
            .is_err(),
        "a queued same-lane op must not start while the holder is merely paused"
    );
    assert_eq!(manager().status_of(&op_b), Some(LifecycleStatus::Queued));

    let _ = a_rel_tx.send(());
    let _ = b_rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_paused_is_noop_for_queued_or_absent_ops() {
    // A Queued op can't be "paused" in v1; an absent op is a no-op. Both return
    // false and leave status untouched.
    let lane = unique("lane");
    let op_a = unique("holder");
    let op_b = unique("queued");

    let (a_started_tx, a_started_rx) = oneshot::channel();
    let (a_rel_tx, a_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_a, vec![&lane]),
        fresh_state(),
        gated_deferred(op_a.clone(), a_started_tx, a_rel_rx),
    );
    a_started_rx.await.expect("A started");

    let (b_started_tx, _b_started_rx) = oneshot::channel();
    let (_b_rel_tx, b_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&op_b, vec![&lane]),
        fresh_state(),
        gated_deferred(op_b.clone(), b_started_tx, b_rel_rx),
    );
    assert_eq!(manager().status_of(&op_b), Some(LifecycleStatus::Queued));

    assert!(!manager().set_paused(&op_b, true), "pausing a Queued op is a no-op");
    assert_eq!(
        manager().status_of(&op_b),
        Some(LifecycleStatus::Queued),
        "a Queued op stays Queued (not Paused)"
    );
    assert!(!manager().set_paused("does-not-exist-zzz", true));

    let _ = a_rel_tx.send(());
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

// ============================================================================
// Managed instant ops (`run_instant`): scan-free, near-instant, result-returning
// metadata ops that register + mark-busy but NEVER reserve a lane or queue.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_instant_marks_volume_busy_and_registers_record_for_op_duration() {
    // (a) The volume is busy and the op shows as a Running record for the op's
    // whole duration, then both clear once it finishes; the result is returned.
    let vol = unique("inst-vol");
    let op = unique("inst-busy");
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let started_in = Arc::clone(&started);
    let release_in = Arc::clone(&release);

    let desc = instant_descriptor(&op, WriteOperationType::Rename, vec![vol.clone()]);
    let op_id = op.clone();
    let handle = tokio::spawn(async move {
        manager()
            .run_instant(desc, async move {
                started_in.notify_one();
                release_in.notified().await;
                7_u32
            })
            .await
    });

    // Wait until the op has started (registered + busy-marked + reached op.await).
    started.notified().await;
    assert!(
        busy_volume_ids().contains(&vol),
        "volume must be busy while the instant op runs"
    );
    assert!(
        manager()
            .list()
            .iter()
            .any(|o| o.operation_id == op_id && o.status == LifecycleStatus::Running),
        "the instant op must show as a Running record mid-flight"
    );

    // Release → the op completes and returns its value.
    release.notify_one();
    let result = handle.await.expect("task joins");
    assert_eq!(result, 7, "run_instant returns the op's own result");

    assert!(
        !busy_volume_ids().contains(&vol),
        "volume must no longer be busy after the op finishes"
    );
    assert!(
        !manager().list().iter().any(|o| o.operation_id == op),
        "the record must be removed after the op finishes"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_instant_marks_only_nonroot_volumes_busy() {
    // (b) A non-root volume is marked busy; the root volume is excluded.
    use crate::file_system::volume::DEFAULT_VOLUME_ID;

    // Non-root volume → busy while running.
    let vol = unique("inst-nonroot");
    let op = unique("inst-nonroot-op");
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (s_in, r_in) = (Arc::clone(&started), Arc::clone(&release));
    let desc = instant_descriptor(&op, WriteOperationType::CreateFolder, vec![vol.clone()]);
    let h = tokio::spawn(async move {
        manager()
            .run_instant(desc, async move {
                s_in.notify_one();
                r_in.notified().await;
            })
            .await
    });
    started.notified().await;
    assert!(
        busy_volume_ids().contains(&vol),
        "a non-root volume must be marked busy"
    );
    release.notify_one();
    h.await.expect("joins");
    assert!(!busy_volume_ids().contains(&vol));

    // Root volume → never in the busy set (root is excluded from the busy set).
    let op_root = unique("inst-root-op");
    let started2 = Arc::new(tokio::sync::Notify::new());
    let release2 = Arc::new(tokio::sync::Notify::new());
    let (s2, r2) = (Arc::clone(&started2), Arc::clone(&release2));
    let desc_root = instant_descriptor(
        &op_root,
        WriteOperationType::Rename,
        vec![DEFAULT_VOLUME_ID.to_string()],
    );
    let h2 = tokio::spawn(async move {
        manager()
            .run_instant(desc_root, async move {
                s2.notify_one();
                r2.notified().await;
            })
            .await
    });
    started2.notified().await;
    assert!(
        !busy_volume_ids().iter().any(|id| id == DEFAULT_VOLUME_ID),
        "the root volume must be excluded from the busy set even for an instant op"
    );
    assert!(
        manager().list().iter().any(|o| o.operation_id == op_root),
        "the root instant op still registers a Running record"
    );
    release2.notify_one();
    h2.await.expect("joins");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn run_instant_does_not_reserve_a_lane() {
    // (c) run_instant reserves no lane even when its descriptor names one, so a
    // real transfer on the SAME lane is admitted concurrently (Running, not
    // Queued).
    let lane = unique("inst-lane");
    let inst = unique("inst-op");
    let xfer = unique("xfer-op");

    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (s_in, r_in) = (Arc::clone(&started), Arc::clone(&release));
    // Descriptor deliberately names `lane` to prove run_instant ignores it.
    let desc = OperationDescriptor {
        operation_id: inst.clone(),
        operation_type: WriteOperationType::Rename,
        lanes: vec![LaneKey::new(lane.as_str())],
        volume_ids: vec![],
        summary: OperationSummaryText::default(),
    };
    let h = tokio::spawn(async move {
        manager()
            .run_instant(desc, async move {
                s_in.notify_one();
                r_in.notified().await;
            })
            .await
    });
    started.notified().await;

    assert!(
        !manager().lane_use_snapshot().contains_key(&lane),
        "run_instant must not reserve its descriptor's lanes"
    );

    // A real transfer on the same lane admits immediately (nothing holds it).
    let (x_started_tx, x_started_rx) = oneshot::channel();
    let (x_rel_tx, x_rel_rx) = oneshot::channel();
    manager().spawn_managed(
        descriptor(&xfer, vec![&lane]),
        fresh_state(),
        gated_deferred(xfer.clone(), x_started_tx, x_rel_rx),
    );
    tokio::time::timeout(Duration::from_secs(2), x_started_rx)
        .await
        .expect("a transfer on the same lane must admit despite the instant op")
        .expect("transfer started");
    assert_eq!(manager().status_of(&xfer), Some(LifecycleStatus::Running));

    release.notify_one();
    h.await.expect("joins");
    let _ = x_rel_tx.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_instant_releases_busy_and_record_when_dropped_midflight() {
    // (d, B1) Dropping the run_instant future mid-op.await (the command's IPC
    // timeout firing) MUST release the busy status and remove the record — else
    // the eject guard sticks ON forever and a phantom row lingers.
    let vol = unique("drop-vol");
    let op = unique("inst-drop");
    let started = Arc::new(tokio::sync::Notify::new());
    let parked = Arc::new(tokio::sync::Notify::new()); // never fired → op parks forever
    let (s_in, p_in) = (Arc::clone(&started), Arc::clone(&parked));
    let desc = instant_descriptor(&op, WriteOperationType::Rename, vec![vol.clone()]);

    let mut fut = Box::pin(manager().run_instant(desc, async move {
        s_in.notify_one();
        p_in.notified().await;
    }));

    // Poll the future until the op has started (registered + parked at op.await)
    // WITHOUT letting it complete.
    tokio::select! {
        _ = &mut fut => panic!("a parked instant op must not complete"),
        _ = started.notified() => {}
    }
    assert!(busy_volume_ids().contains(&vol), "volume busy while the op runs");
    assert!(
        manager().list().iter().any(|o| o.operation_id == op),
        "record present while running"
    );

    // Simulate the IPC timeout dropping the future mid-op.await.
    drop(fut);

    assert!(
        !busy_volume_ids().contains(&vol),
        "B1: a mid-flight drop must release the busy status (else eject stays disabled forever)"
    );
    assert!(
        !manager().list().iter().any(|o| o.operation_id == op),
        "B1: a mid-flight drop must remove the phantom record"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_instant_releases_busy_and_record_when_op_panics() {
    // (d, B1) A panic inside the awaited op MUST still release the busy status and
    // remove the record on unwind.
    let vol = unique("panic-vol");
    let op = unique("inst-panic");
    let desc = instant_descriptor(&op, WriteOperationType::CreateFile, vec![vol.clone()]);

    let handle = tokio::spawn(async move {
        manager()
            .run_instant(desc, async { panic!("simulated instant-op panic") })
            .await
    });
    assert!(handle.await.is_err(), "the panic propagates out of the awaited op");

    assert!(
        !busy_volume_ids().contains(&vol),
        "B1: a panicking instant op must release the busy status on unwind"
    );
    assert!(
        !manager().list().iter().any(|o| o.operation_id == op),
        "B1: a panicking instant op must remove its record on unwind"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_instant_returns_the_ops_result_unchanged() {
    // (e) Both Ok and Err propagate to the caller verbatim, and the record is
    // cleaned up either way.
    let op_ok = unique("inst-ok");
    let ok: Result<u32, String> = manager()
        .run_instant(
            instant_descriptor(&op_ok, WriteOperationType::CreateFolder, vec![]),
            async { Ok(11) },
        )
        .await;
    assert_eq!(ok, Ok(11), "Ok result propagates unchanged");
    assert!(
        !manager().list().iter().any(|o| o.operation_id == op_ok),
        "record removed after a successful instant op"
    );

    let op_err = unique("inst-err");
    let err: Result<u32, String> = manager()
        .run_instant(instant_descriptor(&op_err, WriteOperationType::Rename, vec![]), async {
            Err("boom".to_string())
        })
        .await;
    assert_eq!(err, Err("boom".to_string()), "Err result propagates unchanged");
    assert!(
        !manager().list().iter().any(|o| o.operation_id == op_err),
        "record removed after a failed instant op"
    );
}
