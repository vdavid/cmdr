//! Targeted unit tests covering survivors from `cargo mutants` on this
//! module (state machine transitions and status-cache CRUD). The
//! `OperationIntent` / `PauseGate` state-machine tests live in
//! `operation_intent.rs`; the `FileInfo` sort-key and scan-result TTL tests
//! live in `scan_cache.rs`; `CopyTransaction`'s live in `ledger_tests.rs`.
//!
//! Every test that touches a registry or a status cache drives an engine of its
//! own (`TestOperations`), so a walk over all operations meets only its own.
use super::*;
use crate::Operations;
use crate::test_support::placeholder_conflict;
use crate::testing::{RegisteredOperation, TestOperations};
use crate::types::{
    ConflictId, ConflictResolution, ConflictResolutionOutcome, TransferActivity, TransferWaitReason,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};
use cmdr_operation_log::types::OperationId;
use std::sync::atomic::Ordering;

// ---- the wait classifier ----

/// A progress event mid-copy, for the activity tests below.
fn copying_event(operation_id: &str) -> WriteProgressEvent {
    WriteProgressEvent::new(
        operation_id.to_owned(),
        WriteOperationType::Copy,
        WriteOperationPhase::Copying,
        Some("dsc-1.raw".to_owned()),
        5,
        764,
        83_650_000,
        900_000_000,
    )
}

#[test]
fn enrich_progress_keeps_an_activity_the_caller_already_decided() {
    // The stall watchdog emits from the probe it is stepping, and its copy is the
    // one that just decided the transfer is wedged. A second lookup here would
    // cost the re-emitted event the very activity it exists to carry: this
    // operation keeps no in-flight table, so the lookup answers nothing.
    let engine = TestOperations::new();
    let op = install_state(&engine, "keeps-callers-activity", OperationIntent::Running);
    let mut event = copying_event(op.id().as_str());
    let wedged = TransferActivity {
        in_flight: 3,
        still_for_seconds: 310,
        waiting_on: TransferWaitReason::Unknown,
        opening_source: false,
        source_inbound_bytes_per_second: None,
        reconnecting_volume: None,
    };
    event.activity = Some(wedged.clone());

    op.state().enrich_progress(&mut event);

    assert_eq!(event.activity, Some(wedged), "the caller's classification survives");
}

#[test]
fn enrich_progress_classifies_a_wait_the_caller_left_open() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "classifies-open-wait", OperationIntent::Running);
    let mut event = copying_event(op.id().as_str());
    let (tx, _rx) = tokio::sync::oneshot::channel();
    op.state().conflict_slot.arm(tx, placeholder_conflict);

    op.state().enrich_progress(&mut event);

    assert_eq!(
        event.activity.expect("a parked operation names its wait").waiting_on,
        TransferWaitReason::Conflict
    );
}

#[test]
fn an_operation_with_nothing_to_say_reports_no_activity() {
    // No in-flight table and nobody parked on a decision. Absent is the honest
    // answer; a stand-in `moving` is an invention a poller would act on.
    let engine = TestOperations::new();
    let op = install_state(&engine, "nothing-to-say", OperationIntent::Running);
    let mut event = copying_event(op.id().as_str());

    op.state().enrich_progress(&mut event);

    assert!(event.activity.is_none());
}

/// A panic that poisoned the ETA estimator costs one sample at most: the next
/// event still carries a rate and is still kept as the newest tick, which the
/// stall watchdog judges movement by.
#[test]
fn a_poisoned_estimator_still_rates_and_keeps_the_event() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "poisoned-estimator", OperationIntent::Running);
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = op.state().estimator.lock_ignore_poison();
        panic!("an estimator update panicked, on purpose");
    }));
    assert!(poisoned.is_err() && op.state().estimator.is_poisoned());
    let mut event = copying_event(op.id().as_str());

    op.state().enrich_progress(&mut event);

    assert!(event.bytes_per_second.is_some(), "the event still gets a rate");
    assert_eq!(
        op.state().last_progress_bytes(),
        Some(event.bytes_done),
        "and is kept as the newest tick"
    );
}

// ---- cancel_write_operation state-machine transitions ----
//
// Helper: register a fresh state in the test's engine with the given intent;
// each test then runs the cancellation and reads back the resulting intent.

fn install_state(engine: &TestOperations, label: &str, initial: OperationIntent) -> RegisteredOperation {
    let op = engine.register(label);
    op.state().intent().set_for_test(initial);
    op
}

#[test]
fn cancel_running_with_rollback_goes_to_rolling_back() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-running-rollback", OperationIntent::Running);
    engine.cancel_write_operation(op.id(), true);
    assert_eq!(op.state().intent().load(), OperationIntent::RollingBack);
}

#[test]
fn cancel_running_without_rollback_goes_to_stopped() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-running-stop", OperationIntent::Running);
    engine.cancel_write_operation(op.id(), false);
    assert_eq!(op.state().intent().load(), OperationIntent::Stopped);
}

#[test]
fn cancel_rolling_back_with_rollback_is_a_noop() {
    // Only RollingBack → Stopped is valid; RollingBack → RollingBack is a no-op.
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-rb-rb", OperationIntent::RollingBack);
    engine.cancel_write_operation(op.id(), true);
    assert_eq!(
        op.state().intent().load(),
        OperationIntent::RollingBack,
        "RollingBack → RollingBack is not a valid transition; intent must not change"
    );
}

#[test]
fn cancel_rolling_back_without_rollback_goes_to_stopped() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-rb-stop", OperationIntent::RollingBack);
    engine.cancel_write_operation(op.id(), false);
    assert_eq!(op.state().intent().load(), OperationIntent::Stopped);
}

#[test]
fn cancel_stopped_is_terminal_for_any_target() {
    // Stopped is terminal; no transition is valid from it.
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-stopped", OperationIntent::Stopped);
    engine.cancel_write_operation(op.id(), true);
    assert_eq!(op.state().intent().load(), OperationIntent::Stopped);
    engine.cancel_write_operation(op.id(), false);
    assert_eq!(op.state().intent().load(), OperationIntent::Stopped);
}

#[test]
fn cancel_drops_the_conflict_resolution_sender() {
    // After cancel, any pending receiver should observe a closed channel.
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-drops-tx", OperationIntent::Running);
    let (tx, mut rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    op.state().conflict_slot.arm(tx, placeholder_conflict);
    engine.cancel_write_operation(op.id(), false);
    // The receiver should now be closed (sender dropped).
    match rx.try_recv() {
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {} // good
        other => panic!("expected sender to be dropped, got {other:?}"),
    }
}

// ---- backend_cancel flag flipping ---------------------------------------

#[test]
fn backend_cancel_starts_unset_on_fresh_state() {
    let state = TestOperations::new().new_state();
    assert!(!state.backend_cancel.is_cancelled());
}

#[test]
fn cancel_write_operation_flips_backend_cancel_to_stopped() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-flips-backend-stopped", OperationIntent::Running);
    assert!(!op.state().backend_cancel.is_cancelled());
    engine.cancel_write_operation(op.id(), false);
    assert!(
        op.state().backend_cancel.is_cancelled(),
        "cancel → Stopped must also flip backend_cancel so in-flight USB ops bail"
    );
}

#[test]
fn cancel_write_operation_flips_backend_cancel_to_rolling_back() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-flips-backend-rb", OperationIntent::Running);
    engine.cancel_write_operation(op.id(), true);
    assert!(
        op.state().backend_cancel.is_cancelled(),
        "cancel → RollingBack must also flip backend_cancel — the user wants the wire activity stopped, even though we're going to delete created files"
    );
}

#[test]
fn cancel_stopped_is_noop_for_backend_cancel_too() {
    // Stopped → anything is terminal, so backend_cancel state must not
    // change either. This guards against a subtle regression where the
    // token gets cancelled before the validity check. A freshly registered
    // op starts with an un-cancelled token, so this observes the flip alone.
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-stopped-noop", OperationIntent::Stopped);
    engine.cancel_write_operation(op.id(), true);
    assert!(
        !op.state().backend_cancel.is_cancelled(),
        "Stopped is terminal: invalid transition must not flip backend_cancel"
    );
}

// ---- two stops racing on one intent ----
//
// A Rollback click reads `Running`, and before it writes `RollingBack` a second
// stop lands `Stopped`. Whatever the second stop is (quit's teardown walk, a
// Cancel click from another window or over MCP), `Stopped` is terminal: the
// Rollback must not overwrite it, or "keep the partial files" becomes "delete
// them". The gap seam parks the Rollback in exactly that window.

/// Registers a state whose first intent writer parks in the read-to-write gap,
/// and returns it with the two edges of the park: `parked` fires once the writer
/// is in the gap, and sending on the returned sender lets it continue.
fn op_with_a_parked_intent_writer(
    engine: &TestOperations,
    label: &str,
) -> (
    RegisteredOperation,
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::Sender<()>,
) {
    let (parked_tx, parked) = std::sync::mpsc::channel();
    let (release, release_rx) = std::sync::mpsc::channel::<()>();
    let state = engine
        .new_operation_state(Duration::from_millis(50))
        .with_intent_write_gap(move || {
            parked_tx.send(()).expect("the test waits for the park");
            release_rx.recv().expect("the test releases the park");
        });
    (engine.register_state(label, Arc::new(state)), parked, release)
}

/// Starts a Rollback click on its own thread and returns once it's parked in the
/// gap, having already read `Running`.
fn rollback_parked_in_the_gap(
    engine: &Operations,
    op: &RegisteredOperation,
    parked: &std::sync::mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    let id = op.id().to_owned();
    let engine = engine.clone();
    let rollback = std::thread::spawn(move || engine.cancel_write_operation(&id, true));
    parked
        .recv_timeout(Duration::from_secs(5))
        .expect("the Rollback reaches the gap between its read and its write");
    rollback
}

#[test]
fn a_rollback_click_racing_quit_keeps_the_partial_files_quit_meant_to_keep() {
    let engine = TestOperations::new();
    let (op, parked, release) = op_with_a_parked_intent_writer(&engine, "race-rollback-vs-quit");
    let quit_walk = WriteOperationRegistry::new();
    quit_walk.insert(op.id().to_owned(), Arc::clone(op.state()));

    let rollback = rollback_parked_in_the_gap(&engine, &op, &parked);
    quit_walk.cancel_all();
    release.send(()).expect("the Rollback is still parked");
    rollback.join().expect("the Rollback returns");

    assert_eq!(
        op.state().intent().load(),
        OperationIntent::Stopped,
        "quit stopped the op to keep its partials; a Rollback that read `Running` before quit landed must not turn that into a delete"
    );
}

#[test]
fn a_rollback_click_racing_a_cancel_click_leaves_the_cancel_standing() {
    let engine = TestOperations::new();
    let (op, parked, release) = op_with_a_parked_intent_writer(&engine, "race-rollback-vs-cancel");

    let rollback = rollback_parked_in_the_gap(&engine, &op, &parked);
    engine.cancel_write_operation(op.id(), false);
    release.send(()).expect("the Rollback is still parked");
    rollback.join().expect("the Rollback returns");

    assert_eq!(
        op.state().intent().load(),
        OperationIntent::Stopped,
        "`Stopped` is terminal, whichever stop got there first"
    );
}

#[test]
fn cancel_unknown_operation_is_a_silent_noop() {
    // No installed state; must not panic, must not affect anything.
    let engine = TestOperations::new();
    engine.cancel_write_operation(&OperationId::from("does-not-exist-xyzzy"), true);
    engine.cancel_write_operation(&OperationId::from("does-not-exist-xyzzy"), false);
}

// ---- cancel_all: the frontend-teardown safety net -------------------------
//
// `cancel_all` is a WALK over every operation in a registry, so these drive the
// registry of an engine the test OWNS: on a registry other tests share, it
// would stop whatever they have in flight. The public function is a one-line
// delegation to this method, pinned by
// `cancel_all_write_operations_walks_the_engines_registry` below.

/// A state registered in `engine`, returned for direct inspection. The engine
/// owns the entry, so there's nothing to clean up.
fn registered_in(engine: &TestOperations, label: &str, initial: OperationIntent) -> Arc<WriteOperationState> {
    Arc::clone(install_state(engine, label, initial).state())
}

#[test]
fn cancel_all_stops_running_but_does_not_re_stop_already_stopped() {
    // Pins the `current != OperationIntent::Stopped` guard. If the guard
    // flips to `==`, running operations would NOT be stopped; they'd
    // remain running.
    let engine = TestOperations::new();
    let running = registered_in(&engine, "cancel-all-running", OperationIntent::Running);
    let stopped = registered_in(&engine, "cancel-all-stopped", OperationIntent::Stopped);
    let rb = registered_in(&engine, "cancel-all-rb", OperationIntent::RollingBack);

    engine.registry().cancel_all();

    assert_eq!(running.intent().load(), OperationIntent::Stopped);
    assert_eq!(stopped.intent().load(), OperationIntent::Stopped);
    assert_eq!(
        rb.intent().load(),
        OperationIntent::Stopped,
        "RollingBack should also be force-stopped on teardown"
    );
    assert!(
        !stopped.backend_cancel.is_cancelled(),
        "an already-Stopped op is skipped entirely: the guard, not just the intent store"
    );
}

#[test]
fn cancel_all_flips_backend_cancel() {
    let engine = TestOperations::new();
    let running = registered_in(&engine, "cancel-all-flips-backend", OperationIntent::Running);
    assert!(!running.backend_cancel.is_cancelled());

    engine.registry().cancel_all();

    assert!(
        running.backend_cancel.is_cancelled(),
        "cancel_all must flip backend_cancel so teardown also stops the wire activity"
    );
}

#[test]
fn cancel_all_drops_pending_conflict_senders() {
    // Teardown must unblock an op parked on a Stop-mode conflict prompt:
    // nobody is left to answer it.
    let engine = TestOperations::new();
    let state = registered_in(&engine, "cancel-all-conflict", OperationIntent::Running);
    let (tx, mut rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    state.conflict_slot.arm(tx, placeholder_conflict);

    engine.registry().cancel_all();

    match rx.try_recv() {
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {} // good
        other => panic!("teardown must drop the pending conflict sender, got {other:?}"),
    }
}

#[test]
fn cancel_all_wakes_a_paused_parked_op() {
    // Cancellation wins over pause on the teardown path too: without the
    // `wake()`, an op parked on the condvar would sit there forever while the
    // frontend that could resume it is being torn down.
    let engine = TestOperations::new();
    let state = registered_in(&engine, "cancel-all-paused", OperationIntent::Running);
    state.pause_gate.pause();

    let woke = Arc::new(AtomicBool::new(false));
    let state_t = Arc::clone(&state);
    let woke_t = Arc::clone(&woke);
    let handle = std::thread::spawn(move || {
        state_t.pause_gate.wait_while_paused_sync(state_t.intent());
        woke_t.store(true, Ordering::SeqCst);
    });

    // The condvar park has no "parked now" signal, so hold a window to prove
    // the worker is really parked before the teardown lands. Otherwise a
    // worker that never parked would pass this test.
    // allowed-test-sleep: negative assertion over a window; the condvar park has nothing to await.
    std::thread::sleep(Duration::from_millis(50));
    assert!(!woke.load(Ordering::SeqCst), "still parked before teardown");

    engine.registry().cancel_all();

    cmdr_fs::testing::wait_until(
        Duration::from_secs(2),
        "the parked worker to observe the teardown cancel",
        || woke.load(Ordering::SeqCst),
    );
    handle.join().expect("worker joins");
    assert!(
        state.pause_gate.is_paused(),
        "teardown wakes without resuming: the waiter returned because cancel won"
    );
}

#[test]
fn cancel_all_write_operations_walks_the_engines_registry() {
    // The public function's one job is to point at the engine's registry;
    // everything it then does is pinned above against that registry directly.
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-all-wiring", OperationIntent::Running);

    engine.cancel_all_write_operations();

    assert_eq!(
        op.state().intent().load(),
        OperationIntent::Stopped,
        "the public teardown entry point must reach ops in the engine's registry"
    );
}

// ---- the hard-abort tier --------------------------------------------
//
// Tier 2 exists so a deadline holder can stop WAITING for a backend that isn't
// answering. Its whole safety story is that it is unreachable from anything a
// user clicks, so the negatives here matter more than the positives.

#[test]
fn backend_abort_starts_unset_on_fresh_state() {
    let state = TestOperations::new().new_state();
    assert!(!state.backend_abort.is_cancelled());
}

/// The invariant the whole two-tier split rests on: an ordinary cancel — the one
/// every Cancel button, every queue-window stop, and the frontend teardown net
/// fire — must NEVER reach tier 2. Tier 2 skips each backend's own partial
/// cleanup, so a cancel that leaked into it would trade a clean wind-down for
/// litter on every single cancel.
#[test]
fn an_ordinary_cancel_never_fires_the_hard_abort() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-never-aborts", OperationIntent::Running);
    engine.cancel_write_operation(op.id(), false);
    assert!(
        !op.state().backend_abort.is_cancelled(),
        "a user's Cancel must stay in tier 1: the backend deletes its own partial"
    );

    let rb = install_state(&engine, "rollback-never-aborts", OperationIntent::Running);
    engine.cancel_write_operation(rb.id(), true);
    assert!(
        !rb.state().backend_abort.is_cancelled(),
        "a Rollback is a user decision too, and stays in tier 1"
    );
}

/// The same negative for the teardown walk, which is the one a wrong wiring
/// would most plausibly route through tier 2 ("we're going away anyway").
#[test]
fn cancel_all_never_fires_the_hard_abort() {
    let engine = TestOperations::new();
    let running = registered_in(&engine, "cancel-all-never-aborts", OperationIntent::Running);

    engine.registry().cancel_all();

    assert!(running.backend_cancel.is_cancelled(), "tier 1 fires");
    assert!(
        !running.backend_abort.is_cancelled(),
        "tier 2 is for a deadline holder, ❌ never for a teardown that can still afford to wait"
    );
}

/// Aborting one operation fires BOTH tiers: an abort is a cancel that ran out of
/// patience, so the backend still gets its cooperative signal first.
#[test]
fn abort_write_operation_fires_both_tiers() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "abort-one", OperationIntent::Running);

    engine.abort_write_operation(op.id());

    assert_eq!(op.state().intent().load(), OperationIntent::Stopped);
    assert!(op.state().backend_cancel.is_cancelled(), "tier 1 first");
    assert!(op.state().backend_abort.is_cancelled(), "then tier 2");
}

/// An abort must reach an op that was ALREADY cancelled — that is the whole quit
/// sequence (cancel, wait a beat, abort whatever is left). `cancel_write_operation`
/// no-ops on an already-`Stopped` op, so an abort that rode entirely on it would
/// silently do nothing exactly when it is needed.
#[test]
fn abort_reaches_an_operation_that_was_already_cancelled() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "abort-after-cancel", OperationIntent::Running);
    engine.cancel_write_operation(op.id(), false);

    engine.abort_write_operation(op.id());

    assert!(
        op.state().backend_abort.is_cancelled(),
        "the quit path cancels first and aborts second; the second step has to land"
    );
}

#[test]
fn abort_unknown_operation_is_a_silent_noop() {
    let engine = TestOperations::new();
    engine.abort_write_operation(&OperationId::from("does-not-exist-xyzzy"));
}

#[test]
fn abort_all_write_operations_walks_the_engines_registry() {
    // Same shape and same reason as `cancel_all_write_operations_walks_the_engines_registry`:
    // the public function's one job is to point at the engine's registry.
    let engine = TestOperations::new();
    let op = install_state(&engine, "abort-all-wiring", OperationIntent::Running);

    engine.abort_all_write_operations();

    assert!(
        op.state().backend_abort.is_cancelled(),
        "the public quit-deadline entry point must reach ops in the engine's registry"
    );
}

#[test]
fn abort_all_fires_both_tiers_on_every_live_operation() {
    let engine = TestOperations::new();
    let running = registered_in(&engine, "abort-all-running", OperationIntent::Running);
    // Already stopped: `cancel_all` skips it, but the deadline still has to stop
    // waiting for whatever it left in flight.
    let stopped = registered_in(&engine, "abort-all-stopped", OperationIntent::Stopped);

    engine.registry().abort_all();

    assert!(running.backend_cancel.is_cancelled());
    assert!(running.backend_abort.is_cancelled());
    assert!(
        stopped.backend_abort.is_cancelled(),
        "an op cancelled a moment ago is exactly the one still holding the quit"
    );
}

// Panic-safe cache + lane cleanup is now `manager::ManagedTaskGuard`; its
// panic-unwind pin lives in `manager::tests`.

// ---- resolve_write_conflict ----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resolve_write_conflict_delivers_response_to_waiter() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "resolve-conflict", OperationIntent::Running);

    let (tx, rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let clash = op.state().conflict_slot.arm(tx, placeholder_conflict).conflict_id;

    assert_eq!(
        engine.resolve_write_conflict(op.id(), clash, ConflictResolution::Overwrite, true),
        ConflictResolutionOutcome::Resolved
    );

    let resp = rx.await.expect("sender should have delivered the response");
    assert_eq!(resp.resolution, ConflictResolution::Overwrite);
    assert!(resp.apply_to_all);
}

/// Two surfaces answer the same prompt. The first answer is the one the
/// operation acts on; the second is told so, and changes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_answer_to_one_conflict_is_reported_as_already_resolved() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "resolve-conflict-twice", OperationIntent::Running);

    let (tx, rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let clash = op.state().conflict_slot.arm(tx, placeholder_conflict).conflict_id;

    assert_eq!(
        engine.resolve_write_conflict(op.id(), clash, ConflictResolution::Overwrite, false),
        ConflictResolutionOutcome::Resolved
    );
    assert_eq!(
        engine.resolve_write_conflict(op.id(), clash, ConflictResolution::Skip, true),
        ConflictResolutionOutcome::AlreadyResolved
    );

    let resp = rx.await.expect("sender should have delivered the response");
    assert_eq!(
        resp.resolution,
        ConflictResolution::Overwrite,
        "the operation carries on with the answer that won"
    );
    assert!(!resp.apply_to_all);
    assert!(!op.state().conflict_slot.is_awaiting());
}

/// An answer for a clash the operation has already left behind, arriving after
/// it parked on the next one. The whole point of the id: this answer decides
/// nothing, and the caller is told that rather than being told it won.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_answer_for_a_retired_conflict_is_refused_and_reported_as_stale() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "resolve-conflict-stale", OperationIntent::Running);

    let (first_tx, first_rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let first = op.state().conflict_slot.arm(first_tx, placeholder_conflict).conflict_id;
    assert_eq!(
        engine.resolve_write_conflict(op.id(), first, ConflictResolution::Overwrite, false),
        ConflictResolutionOutcome::Resolved
    );
    first_rx.await.expect("the first clash gets its own answer");

    // The operation moved on and parked on the next clash.
    let (second_tx, mut second_rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    op.state().conflict_slot.arm(second_tx, placeholder_conflict);

    assert_eq!(
        engine.resolve_write_conflict(op.id(), first, ConflictResolution::Skip, true),
        ConflictResolutionOutcome::StaleAnswer,
        "an answer naming the retired clash must not be reported as resolving anything"
    );
    assert!(
        second_rx.try_recv().is_err(),
        "the live clash is still waiting for an answer of its own"
    );
    assert!(op.state().conflict_slot.is_awaiting());
}

#[test]
fn resolve_write_conflict_on_an_operation_with_no_conflict_says_so() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "resolve-no-conflict", OperationIntent::Running);
    // The operation is live but has never raised a conflict.
    assert_eq!(
        engine.resolve_write_conflict(op.id(), ConflictId(1), ConflictResolution::Skip, false),
        ConflictResolutionOutcome::NoPendingConflict
    );
}

#[test]
fn resolve_write_conflict_on_an_unknown_operation_says_so() {
    // Nothing registered under this id: it settled, or it never existed. Not the
    // same thing as a live operation that isn't asking anything.
    let engine = TestOperations::new();
    assert_eq!(
        engine.resolve_write_conflict(
            &OperationId::from("resolve-gone"),
            ConflictId(1),
            ConflictResolution::Skip,
            false
        ),
        ConflictResolutionOutcome::UnknownOperation
    );
}

#[test]
fn cancelling_takes_the_pending_conflict_away() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "resolve-after-cancel", OperationIntent::Running);
    let (tx, _rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let clash = op.state().conflict_slot.arm(tx, placeholder_conflict).conflict_id;

    engine.cancel_write_operation(op.id(), false);

    assert!(!op.state().conflict_slot.is_awaiting());
    assert_eq!(
        engine.resolve_write_conflict(op.id(), clash, ConflictResolution::Skip, false),
        ConflictResolutionOutcome::NoPendingConflict
    );
}

// ── Busy-volumes set (drives "disable Eject while an op touches a device") ──
// Each runs on an engine of its own, so the busy set holds its volumes alone.

#[test]
fn busy_volume_ids_reflects_registered_volumes() {
    let engine = TestOperations::new();
    let op = "busy-op".to_string();
    let usb = "usb".to_string();
    let mtp = "mtp".to_string();

    engine
        .status()
        .register(&op, WriteOperationType::Copy, vec![usb.clone(), mtp.clone()]);
    let busy = engine.busy_volume_ids();
    assert!(busy.contains(&usb), "source volume should be busy");
    assert!(busy.contains(&mtp), "dest volume should be busy");

    engine.status().unregister(&op);
    let after = engine.busy_volume_ids();
    assert!(!after.contains(&usb), "volume should clear once the op finishes");
    assert!(!after.contains(&mtp), "volume should clear once the op finishes");
}

#[test]
fn busy_volume_ids_excludes_root() {
    let engine = TestOperations::new();
    let op = "busy-root-op".to_string();
    let usb = "usb".to_string();

    let root = cmdr_fs::volume::DEFAULT_VOLUME_ID.to_string();
    engine
        .status()
        .register(&op, WriteOperationType::Move, vec![root.clone(), usb.clone()]);
    let busy = engine.busy_volume_ids();
    assert!(busy.contains(&usb), "the ejectable volume should be busy");
    assert!(
        !busy.contains(&root),
        "root is never ejectable, so it must not appear in the busy set"
    );

    engine.status().unregister(&op);
}

#[test]
fn busy_volume_ids_stays_busy_until_all_overlapping_ops_finish() {
    // Two concurrent transfers touch the same device; it must stay busy
    // until BOTH finish (refcount-by-membership, no manual counter).
    let engine = TestOperations::new();
    let op_a = "overlap-a".to_string();
    let op_b = "overlap-b".to_string();
    let dev = "device".to_string();

    engine
        .status()
        .register(&op_a, WriteOperationType::Copy, vec![dev.clone()]);
    engine
        .status()
        .register(&op_b, WriteOperationType::Copy, vec![dev.clone()]);
    assert!(engine.busy_volume_ids().contains(&dev));

    engine.status().unregister(&op_a);
    assert!(
        engine.busy_volume_ids().contains(&dev),
        "still busy while the second op runs"
    );

    engine.status().unregister(&op_b);
    assert!(
        !engine.busy_volume_ids().contains(&dev),
        "clears only after the last op finishes"
    );
}

#[test]
fn busy_volume_ids_clears_on_panic_unwind_via_unregister() {
    // A panicking op must not leave its volume stuck "busy" forever. In
    // production `manager::ManagedTaskGuard` unregisters the op's status on
    // unwind; this pins that the status cache's `unregister` itself clears the
    // busy mark when invoked from a `Drop` during a panic.
    struct UnregisterOnDrop(Operations, String);
    impl Drop for UnregisterOnDrop {
        fn drop(&mut self) {
            self.0.status().unregister(&self.1);
        }
    }

    let engine = TestOperations::new();
    let op = "panic-op".to_string();
    let dev = "panic-device".to_string();
    let op_for_thread = op.clone();
    let dev_for_thread = dev.clone();
    let engine_for_thread = Operations::clone(&engine);

    let handle = std::thread::spawn(move || {
        engine_for_thread
            .status()
            .register(&op_for_thread, WriteOperationType::Delete, vec![dev_for_thread]);
        let _guard = UnregisterOnDrop(engine_for_thread, op_for_thread.clone());
        panic!("simulated op panic while the device is busy");
    });
    assert!(handle.join().is_err(), "thread should have panicked");

    assert!(
        !engine.busy_volume_ids().contains(&dev),
        "unregister on unwind must clear the busy mark"
    );
}

// ---- pause / resume on the live state ------------------------------------
// The pure `PauseGate` mechanics (sync/async parking, cancel-wins) are
// tested in `operation_intent.rs`; these pin the registry
// lookup wiring of `pause_write_operation` / `resume_write_operation`.

#[test]
fn pause_resume_write_operation_flip_the_live_gate() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "pause-live", OperationIntent::Running);
    assert!(!op.state().pause_gate.is_paused());

    assert!(engine.pause_write_operation(op.id()), "should find the live state");
    assert!(op.state().pause_gate.is_paused(), "pause must set the gate flag");

    assert!(engine.resume_write_operation(op.id()), "should find the live state");
    assert!(!op.state().pause_gate.is_paused(), "resume must clear the gate flag");
}

#[test]
fn pause_resume_unknown_operation_returns_false() {
    let engine = TestOperations::new();
    assert!(!engine.pause_write_operation(&OperationId::from("does-not-exist-pause")));
    assert!(!engine.resume_write_operation(&OperationId::from("does-not-exist-pause")));
}

// ---- the newest tick, for a window that opens late -------------------------

#[test]
fn latest_progress_hands_a_late_window_the_tick_a_paused_operation_last_emitted() {
    // A paused operation emits nothing, so a queue window opened while it's
    // parked would have no bars and no tree until somebody pressed Resume. This
    // is the one tick that window can start from, with the wait named NOW: the
    // stored tick was measured while the copy was still moving.
    let engine = TestOperations::new();
    let op = install_state(&engine, "latest-progress-paused", OperationIntent::Running);
    let mut event = copying_event(op.id().as_str());
    op.state().enrich_progress(&mut event);
    op.state().pause_gate.pause();

    let latest = engine.registry().latest_progress();
    let tick = latest
        .iter()
        .find(|tick| tick.operation_id == op.id().as_str())
        .expect("a paused operation that emitted a tick hands it over");

    assert_eq!(
        (tick.files_done, tick.bytes_done),
        (5, 83_650_000),
        "the counters are the last ones it emitted"
    );
    assert_eq!(
        tick.activity.as_ref().map(|activity| activity.waiting_on),
        Some(TransferWaitReason::Paused),
        "the wait is classified at read time, not replayed"
    );
}

#[test]
fn latest_progress_skips_an_operation_that_has_emitted_nothing_yet() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "latest-progress-silent", OperationIntent::Running);

    let latest = engine.registry().latest_progress();

    assert!(
        latest.iter().all(|tick| tick.operation_id != op.id().as_str()),
        "no tick, no row to seed"
    );
}

// ---- the stop-work token --------------------------------------------
//
// One token says "stop starting work" for both reasons a concurrent copy stops:
// a person's stop, and its own first failure. Only the intent tells them apart.

#[test]
fn a_cancel_stops_work() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "cancel-stops-work", OperationIntent::Running);
    assert!(!op.state().stop_work.is_cancelled(), "a fresh operation is working");

    engine.cancel_write_operation(op.id(), false);

    assert!(
        op.state().stop_work.is_cancelled(),
        "every stop fires the stop-work token, through `backend_cancel`"
    );
    assert!(op.state().work_stopped());
}

/// The failure drain fires ONLY the stop-work token: the intent stays
/// `Running` (so the failure stays the operation's error, never a cancel), and
/// `backend_cancel` stays quiet (MTP bridges it into the device's own cancel).
#[test]
fn a_failure_drain_stops_work_without_reading_as_a_cancel() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "failure-drain-stops-work", OperationIntent::Running);

    op.state().stop_work_after_failure();

    assert!(
        op.state().stop_work.is_cancelled(),
        "the drain fires the stop-work token"
    );
    assert!(op.state().work_stopped());
    assert_eq!(
        op.state().intent().load(),
        OperationIntent::Running,
        "❗ a failure is not a person's stop: the intent says nothing of it"
    );
    assert!(
        !op.state().backend_cancel.is_cancelled(),
        "❌ the drain never reaches a backend's own cancel token"
    );
    assert!(!op.state().backend_abort.is_cancelled());
}

/// What a drain does to a Stop-mode prompt still up for a sibling: drops it, the
/// way a cancel does, so its waiter reads a stop instead of holding the drain
/// open on a question nobody will answer.
#[test]
fn a_failure_drain_drops_the_pending_conflict_prompt() {
    let engine = TestOperations::new();
    let op = install_state(&engine, "failure-drain-drops-prompt", OperationIntent::Running);
    let (tx, mut rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    op.state().conflict_slot.arm(tx, placeholder_conflict);

    op.state().stop_work_after_failure();

    match rx.try_recv() {
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {}
        other => panic!("expected the prompt's sender to be dropped, got {other:?}"),
    }
}

/// A loop boundary stops on the drain instead of starting the next item.
#[tokio::test]
async fn a_loop_boundary_stops_after_a_failure_drain() {
    let state = TestOperations::new().new_state();
    assert!(!state.stop_or_park_async().await, "a working operation carries on");

    state.stop_work_after_failure();

    assert!(state.stop_or_park_async().await, "the async boundary stops");
    assert!(state.stop_or_park_sync(), "and the sync one");
    assert!(
        state.is_stopping_or_paused(),
        "and the cheap probe says there's something to ask"
    );
}

/// A task parked on Pause when a sibling fails is woken to stop, so it closes
/// its handle and deletes its own partial instead of being dropped at the drain
/// deadline. The pause flag stays set: nothing starts again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_drain_wakes_a_task_parked_on_pause() {
    let engine = TestOperations::new();
    let state = registered_in(&engine, "failure-drain-wakes-paused", OperationIntent::Running);
    state.pause_gate.pause();

    let parked = tokio::spawn({
        let state = Arc::clone(&state);
        async move { state.stop_or_park_async().await }
    });
    // allowed-test-sleep: negative assertion over a window; the park has nothing to await.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!parked.is_finished(), "still parked before the drain");

    state.stop_work_after_failure();

    let stopped = tokio::time::timeout(Duration::from_secs(2), parked)
        .await
        .expect("the parked task must observe the drain")
        .expect("the task joins");
    assert!(stopped, "it wakes to STOP, not to carry on");
    assert!(state.pause_gate.is_paused(), "the drain wakes without resuming");
}
