//! The stop-and-pause machine's thread interleavings, explored with Shuttle: a
//! worker parked on the pause gate while a person pauses, resumes, and stops,
//! and quit's teardown stops too. "Cancellation wins over pause" has to hold
//! in every order, or a paused operation that someone cancelled sits parked
//! forever, holding its lane and its partial file.
//!
//! Built only under `--features shuttle`, like `in_flight_shuttle_tests.rs`;
//! the gate's flag, lock, and condvar and the intent's atomic come through
//! `sync_primitives.rs`. A worker that never wakes shows up as Shuttle's
//! `deadlock!`, with the schedule that caused it.

use std::sync::Arc;

use shuttle::thread;

use super::super::human_wait::HumanWaitClock;
use super::super::shuttle_support::explore;
use super::{IntentCell, OperationIntent, PauseGate, StopRequest};

/// A paused gate and a running intent, as an operation has them the moment a
/// person pauses it.
fn paused_operation() -> (Arc<PauseGate>, Arc<IntentCell>) {
    let gate = Arc::new(PauseGate::new(Arc::new(HumanWaitClock::new())));
    gate.pause();
    (gate, Arc::new(IntentCell::running()))
}

/// What `cancel_write_operation` does to a running operation, as far as the
/// gate and the intent go: the stop, then the wake. `cancel_all` stops for a
/// teardown in its place.
fn stop(gate: &PauseGate, intent: &IntentCell, teardown: bool) {
    let stopped = if teardown {
        intent.stop_for_teardown(|| {}).is_ok()
    } else {
        intent.request_stop(StopRequest::RollBack, || {}).is_ok()
    };
    if stopped {
        gate.wake();
    }
}

/// Files a worker gets through before it's done on its own. A few, so a worker
/// can still be mid-operation when the stop lands, and bounded, so a resumed
/// worker moves on rather than spinning on the gate.
const FILES: usize = 3;

/// The sync driver's loop top, once per file: stop when the intent says so,
/// else park while paused.
fn sync_worker(gate: &PauseGate, intent: &IntentCell) {
    for _ in 0..FILES {
        if intent.is_cancelled() {
            return;
        }
        gate.wait_while_paused_sync(intent);
    }
}

/// A worker on the sync gate (a `spawn_blocking` driver), a person flipping
/// pause, a person's Rollback, and quit's teardown, all at once. The worker
/// always gets out, and the intent ends `Stopped`: a teardown that lands after
/// the Rollback finishes it, one that lands before it refuses the Rollback.
#[test]
fn shuttle_a_stop_always_frees_a_worker_parked_on_the_sync_gate() {
    explore(|| {
        let (gate, intent) = paused_operation();
        let worker = {
            let (gate, intent) = (Arc::clone(&gate), Arc::clone(&intent));
            thread::spawn(move || sync_worker(&gate, &intent))
        };
        let person = {
            let (gate, intent) = (Arc::clone(&gate), Arc::clone(&intent));
            thread::spawn(move || {
                gate.resume();
                gate.pause();
                stop(&gate, &intent, false);
            })
        };
        let quit = {
            let (gate, intent) = (Arc::clone(&gate), Arc::clone(&intent));
            thread::spawn(move || stop(&gate, &intent, true))
        };

        person.join().expect("the person");
        quit.join().expect("the quit");
        worker.join().expect("the worker gets out");
        assert_eq!(intent.load(), OperationIntent::Stopped, "a teardown's stop is final");
    });
}

/// The same race for an async volume driver parked on the gate's `Notify`.
/// The task always finishes once the stop lands.
#[test]
fn shuttle_a_stop_always_frees_a_task_parked_on_the_async_gate() {
    explore(|| {
        let (gate, intent) = paused_operation();
        let task = {
            let (gate, intent) = (Arc::clone(&gate), Arc::clone(&intent));
            shuttle::future::spawn(async move {
                for _ in 0..FILES {
                    if intent.is_cancelled() {
                        return;
                    }
                    gate.wait_while_paused_until(&|| intent.is_cancelled()).await;
                }
            })
        };
        let person = {
            let (gate, intent) = (Arc::clone(&gate), Arc::clone(&intent));
            thread::spawn(move || {
                gate.resume();
                gate.pause();
                stop(&gate, &intent, false);
            })
        };

        person.join().expect("the person");
        shuttle::future::block_on(task).expect("the task gets out");
        assert!(intent.is_cancelled());
    });
}
