//! Reordering the line: moving a QUEUED operation earlier or later in the FIFO
//! `order` that admission walks. Running and paused operations hold lanes and
//! keep their place; only waiting ones trade places, and only with each other.
//!
//! Every test runs on a manager of its own (`TestOperations`): "the next queued
//! operation" is a question about the WHOLE line, which only a line no other
//! test queues on answers.

use super::*;

/// The ids of `mgr`'s live rows, in snapshot (= admission) order.
fn order_of(mgr: &OperationManager) -> Vec<OperationId> {
    mgr.list()
        .into_iter()
        .map(|row| OperationId::from(row.operation_id))
        .collect()
}

/// A held operation on `engine`: registered, and released (settled) when the
/// returned sender fires. Whether it runs or queues depends on its lanes.
struct Held {
    id: OperationId,
    started: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
}

fn hold(engine: &TestOperations, label: &str, lanes: Vec<&str>) -> Held {
    let id = OperationId::from(label);
    let (started_tx, started_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    engine.manager().spawn_managed(
        descriptor(&id, lanes),
        engine.new_state(),
        gated_deferred(id.clone(), started_tx, release_rx),
    );
    Held {
        id,
        started: started_rx,
        release: release_tx,
    }
}

/// One running op holding `lane`, then `waiting` ops queued behind it on the
/// same lane, in registration order.
async fn line(engine: &TestOperations, waiting: usize) -> (Held, Vec<Held>) {
    let lane = "lane".to_string();
    let mut running = hold(engine, "running", vec![&lane]);
    (&mut running.started).await.expect("the first op is admitted at once");
    let queued = (0..waiting)
        .map(|i| hold(engine, &format!("queued-{i}"), vec![&lane]))
        .collect::<Vec<_>>();
    for held in &queued {
        assert_eq!(
            engine.manager().lifecycle_status(&held.id),
            Some(LifecycleStatus::Queued)
        );
    }
    (running, queued)
}

/// Drops whatever is still queued, then lets the running op settle, so no test
/// leaves a task parked on a oneshot.
fn finish(engine: &TestOperations, running: Held, queued: Vec<Held>) {
    for held in &queued {
        engine.manager().cancel_if_queued(&held.id);
    }
    let _ = running.release.send(());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_the_last_waiting_op_earlier_trades_places_with_the_one_before_it() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 2).await;
    let (b, c) = (queued[0].id.clone(), queued[1].id.clone());

    let outcome = mgr.move_queued(&c, QueueMoveDirection::Earlier);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), vec![running.id.clone(), c, b]);
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moving_a_waiting_op_later_trades_places_with_the_one_after_it() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 3).await;
    let ids: Vec<OperationId> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.move_queued(&ids[0], QueueMoveDirection::Later);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 2,
            waiting: 3
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![running.id.clone(), ids[1].clone(), ids[0].clone(), ids[2].clone()]
    );
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_and_last_waiting_ops_stay_put_at_their_edge_and_say_where_they_are() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 2).await;
    let before = order_of(mgr);
    let emits = mgr.emit_count();

    assert_eq!(
        mgr.move_queued(&queued[0].id, QueueMoveDirection::Earlier),
        QueueMoveOutcome::AtEdge {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(
        mgr.move_queued(&queued[1].id, QueueMoveDirection::Later),
        QueueMoveOutcome::AtEdge {
            position: 2,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), before, "a move off the edge changes nothing");
    assert_eq!(
        mgr.emit_count(),
        emits,
        "and tells nobody about a change that didn't happen"
    );
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_or_unknown_op_does_not_move() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 1).await;
    let before = order_of(mgr);

    assert_eq!(
        mgr.move_queued(&running.id, QueueMoveDirection::Later),
        QueueMoveOutcome::NotQueued
    );
    assert_eq!(
        mgr.move_queued(&OperationId::from("no-such-op"), QueueMoveDirection::Earlier),
        QueueMoveOutcome::NotQueued
    );
    assert_eq!(order_of(mgr), before);
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn a_running_op_between_two_waiting_ones_keeps_its_place() {
    // [A running on L] [B waiting on L] [D running on M] [C waiting on L]:
    // C moving earlier passes D and trades with B, and D stays third.
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let lane = "lane".to_string();
    let other_lane = "other-lane".to_string();
    let mut a = hold(&engine, "a", vec![&lane]);
    (&mut a.started).await.expect("A admitted");
    let b = hold(&engine, "b", vec![&lane]);
    let mut d = hold(&engine, "d", vec![&other_lane]);
    (&mut d.started).await.expect("D admitted on its own lane");
    let c = hold(&engine, "c", vec![&lane]);

    let outcome = mgr.move_queued(&c.id, QueueMoveDirection::Earlier);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![a.id.clone(), c.id.clone(), d.id.clone(), b.id.clone()]
    );
    let _ = d.release.send(());
    finish(&engine, a, vec![b, c]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_tells_the_windows_once() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 2).await;
    let emits = mgr.emit_count();

    mgr.move_queued(&queued[1].id, QueueMoveDirection::Earlier);

    assert_eq!(mgr.emit_count(), emits + 1);
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_op_moved_to_the_front_is_the_one_admitted_next() {
    // The point of the whole feature: the line decides who goes next.
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, mut queued) = line(&engine, 2).await;
    let mut c = queued.pop().expect("two queued");
    let b = queued.pop().expect("two queued");

    mgr.move_queued(&c.id, QueueMoveDirection::Earlier);
    let _ = running.release.send(());

    tokio::time::timeout(WAIT, &mut c.started)
        .await
        .expect("the op moved to the front must be admitted when the lane frees")
        .expect("C started");
    assert_eq!(mgr.lifecycle_status(&b.id), Some(LifecycleStatus::Queued));

    mgr.cancel_if_queued(&b.id);
    let _ = c.release.send(());
}

// ── Placing a waiting op at a given place (a drag's drop) ──────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_the_last_waiting_op_first_shifts_the_others_back() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 3).await;
    let ids: Vec<OperationId> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.place_queued(&ids[2], 1);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 3
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![running.id.clone(), ids[2].clone(), ids[0].clone(), ids[1].clone()]
    );
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_the_first_waiting_op_last_shifts_the_others_forward() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 3).await;
    let ids: Vec<OperationId> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.place_queued(&ids[0], 3);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 3,
            waiting: 3
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![running.id.clone(), ids[1].clone(), ids[2].clone(), ids[0].clone()]
    );
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_place_past_the_end_lands_last() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 2).await;
    let ids: Vec<OperationId> = queued.iter().map(|h| h.id.clone()).collect();

    let outcome = mgr.place_queued(&ids[0], 99);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 2,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), vec![running.id.clone(), ids[1].clone(), ids[0].clone()]);
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_an_op_where_it_already_is_changes_nothing_and_tells_nobody() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 2).await;
    let before = order_of(mgr);
    let emits = mgr.emit_count();

    assert_eq!(
        mgr.place_queued(&queued[1].id, 2),
        QueueMoveOutcome::Unchanged {
            position: 2,
            waiting: 2
        }
    );
    assert_eq!(order_of(mgr), before);
    assert_eq!(mgr.emit_count(), emits);
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placing_a_running_or_unknown_op_does_nothing() {
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let (running, queued) = line(&engine, 1).await;
    let before = order_of(mgr);

    assert_eq!(mgr.place_queued(&running.id, 1), QueueMoveOutcome::NotQueued);
    assert_eq!(
        mgr.place_queued(&OperationId::from("no-such-op"), 1),
        QueueMoveOutcome::NotQueued
    );
    assert_eq!(order_of(mgr), before);
    finish(&engine, running, queued);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn placing_an_op_first_passes_a_running_op_without_moving_it() {
    // [A running on L] [B waiting on L] [D running on M] [C waiting on L]:
    // C placed first takes B's slot, B takes C's, and D stays third.
    let engine = TestOperations::new();
    let mgr = engine.manager();
    let lane = "lane".to_string();
    let other_lane = "other-lane".to_string();
    let mut a = hold(&engine, "a", vec![&lane]);
    (&mut a.started).await.expect("A admitted");
    let b = hold(&engine, "b", vec![&lane]);
    let mut d = hold(&engine, "d", vec![&other_lane]);
    (&mut d.started).await.expect("D admitted on its own lane");
    let c = hold(&engine, "c", vec![&lane]);

    let outcome = mgr.place_queued(&c.id, 1);

    assert_eq!(
        outcome,
        QueueMoveOutcome::Moved {
            position: 1,
            waiting: 2
        }
    );
    assert_eq!(
        order_of(mgr),
        vec![a.id.clone(), c.id.clone(), d.id.clone(), b.id.clone()]
    );
    let _ = d.release.send(());
    finish(&engine, a, vec![b, c]);
}
