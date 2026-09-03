//! `WriteOperationState::stop_or_park_sync` / `stop_or_park_async`: the one
//! boundary question every serial loop in this module asks.
//!
//! The contract these pin is an ORDERING, which is why it lives in one primitive
//! rather than at each loop: cancel outranks pause, so a stopping operation never
//! parks on its way out, and a cancel that lands while a loop is already parked is
//! answered at that same boundary instead of one item later. `DETAILS.md`
//! § "Pause / resume" for where it's asked and why only there.
//!
//! `PauseGate`'s own state machine is tested in `operation_intent.rs`; the rest of
//! `WriteOperationState` in `state_tests.rs`.

use super::state::WriteOperationState;
use crate::file_system::write_operations::state::OperationIntent;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A live operation is told to carry on, and nothing parks it.
#[test]
fn stop_or_park_lets_a_live_operation_carry_on() {
    let state = WriteOperationState::new(Duration::from_millis(0));
    assert!(
        !state.stop_or_park_sync(),
        "a Running, unpaused op carries on — and returns instantly, or this test hangs"
    );
}

/// Cancel outranks pause: an op that is BOTH paused and cancelled is told to
/// stop, and never parks on the way out. Without that ordering the loop would
/// sit on the gate holding a cancel nobody reads.
#[test]
fn stop_or_park_says_stop_without_parking_a_cancelled_op() {
    let state = WriteOperationState::new(Duration::from_millis(0));
    state.pause_gate.pause();
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);

    assert!(
        state.stop_or_park_sync(),
        "cancel wins over pause, and answering it must not park"
    );
}

/// The park itself: a paused loop holds until somebody resumes it, then carries on.
#[test]
fn stop_or_park_holds_a_paused_operation_until_it_resumes() {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    state.pause_gate.pause();

    let passed = Arc::new(AtomicBool::new(false));
    let state_t = Arc::clone(&state);
    let passed_t = Arc::clone(&passed);
    let worker = std::thread::spawn(move || {
        let stop = state_t.stop_or_park_sync();
        passed_t.store(true, Ordering::SeqCst);
        stop
    });

    // The condvar park has no "parked now" signal, so hold a window open: an
    // ungated boundary would have gone through it many times over.
    // allowed-test-sleep: negative assertion over a window; the park has nothing to await.
    std::thread::sleep(Duration::from_millis(100));
    assert!(!passed.load(Ordering::SeqCst), "a paused op holds at the boundary");

    state.pause_gate.resume();
    assert!(
        !worker.join().expect("the worker joins"),
        "a resumed op is told to carry on"
    );
}

/// A cancel landing while parked is answered at once, at the boundary the loop
/// is already standing on, rather than one item later.
#[test]
fn stop_or_park_says_stop_when_cancel_lands_while_parked() {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    state.pause_gate.pause();

    let state_t = Arc::clone(&state);
    let worker = std::thread::spawn(move || state_t.stop_or_park_sync());

    // allowed-test-sleep: let the worker reach the park before the cancel lands.
    std::thread::sleep(Duration::from_millis(50));
    // Exactly what `cancel_write_operation` does: flip the intent, then wake.
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    state.pause_gate.wake();

    assert!(
        worker.join().expect("the worker joins"),
        "the parked loop is told to stop, so it bails at this boundary"
    );
}

/// The async twin answers the same way; the drivers that park a task rather
/// than a blocking-pool thread rely on it.
#[tokio::test]
async fn stop_or_park_async_says_stop_when_cancel_lands_while_parked() {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    state.pause_gate.pause();

    let state_t = Arc::clone(&state);
    let worker = tokio::spawn(async move { state_t.stop_or_park_async().await });

    // allowed-test-sleep: let the task reach the park before the cancel lands.
    tokio::time::sleep(Duration::from_millis(50)).await;
    state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
    state.pause_gate.wake();

    let stop = tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("the parked task unblocks on cancel")
        .expect("the task joins");
    assert!(stop, "the parked task is told to stop");
}

/// And it carries on when nothing is asking it to stop.
#[tokio::test]
async fn stop_or_park_async_lets_a_live_operation_carry_on() {
    let state = WriteOperationState::new(Duration::from_millis(0));
    let stop = tokio::time::timeout(Duration::from_secs(5), state.stop_or_park_async())
        .await
        .expect("an unpaused op never parks");
    assert!(!stop, "a Running, unpaused op carries on");
}
