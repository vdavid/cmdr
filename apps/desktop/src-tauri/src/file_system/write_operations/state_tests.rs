//! Targeted unit tests covering survivors from `cargo mutants` on this
//! module (state machine transitions, status-cache CRUD, and
//! CopyTransaction commit/rollback/Drop). The `OperationIntent` /
//! `PauseGate` state-machine tests live in `operation_intent.rs`; the
//! `FileInfo` sort-key and scan-result TTL tests live in `scan_cache.rs`.
//!
//! Tests that touch the global `WRITE_OPERATION_STATE` /
//! `OPERATION_STATUS_CACHE` caches key their entries per test, so they don't
//! collide with concurrent test runs in the same process. `WRITE_OPERATION_STATE`
//! entries go through `TestOperationGuard`, which also removes them on unwind.
use super::*;
use crate::file_system::write_operations::test_support::{TestOperationGuard, placeholder_conflict};
use crate::file_system::write_operations::types::{
    ConflictId, ConflictResolution, ConflictResolutionOutcome, TransferActivity, TransferWaitReason,
    WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};
use std::sync::atomic::Ordering;

fn unique_id(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("test-state-{label}-{n}-{:?}", std::thread::current().id())
}

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
    let op = install_state("keeps-callers-activity", OperationIntent::Running);
    let mut event = copying_event(op.id());
    let wedged = TransferActivity {
        in_flight: 3,
        still_for_seconds: 310,
        waiting_on: TransferWaitReason::Unknown,
    };
    event.activity = Some(wedged);

    op.state().enrich_progress(&mut event);

    assert_eq!(event.activity, Some(wedged), "the caller's classification survives");
}

#[test]
fn enrich_progress_classifies_a_wait_the_caller_left_open() {
    let op = install_state("classifies-open-wait", OperationIntent::Running);
    let mut event = copying_event(op.id());
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
    let op = install_state("nothing-to-say", OperationIntent::Running);
    let mut event = copying_event(op.id());

    op.state().enrich_progress(&mut event);

    assert!(event.activity.is_none());
}

// ---- cancel_write_operation state-machine transitions ----
//
// Helper: install a fresh state into the global cache under a unique op id,
// run the cancellation, then read back the resulting intent. The guard removes
// the entry when it drops, so a failing assertion can't leak it.

fn install_state(label: &str, initial: OperationIntent) -> TestOperationGuard {
    let op = TestOperationGuard::register(label);
    op.state().intent.store(initial as u8, Ordering::Relaxed);
    op
}

#[test]
fn cancel_running_with_rollback_goes_to_rolling_back() {
    let op = install_state("cancel-running-rollback", OperationIntent::Running);
    cancel_write_operation(op.id(), true);
    assert_eq!(load_intent(&op.state().intent), OperationIntent::RollingBack);
}

#[test]
fn cancel_running_without_rollback_goes_to_stopped() {
    let op = install_state("cancel-running-stop", OperationIntent::Running);
    cancel_write_operation(op.id(), false);
    assert_eq!(load_intent(&op.state().intent), OperationIntent::Stopped);
}

#[test]
fn cancel_rolling_back_with_rollback_is_a_noop() {
    // Only RollingBack → Stopped is valid; RollingBack → RollingBack is a no-op.
    let op = install_state("cancel-rb-rb", OperationIntent::RollingBack);
    cancel_write_operation(op.id(), true);
    assert_eq!(
        load_intent(&op.state().intent),
        OperationIntent::RollingBack,
        "RollingBack → RollingBack is not a valid transition; intent must not change"
    );
}

#[test]
fn cancel_rolling_back_without_rollback_goes_to_stopped() {
    let op = install_state("cancel-rb-stop", OperationIntent::RollingBack);
    cancel_write_operation(op.id(), false);
    assert_eq!(load_intent(&op.state().intent), OperationIntent::Stopped);
}

#[test]
fn cancel_stopped_is_terminal_for_any_target() {
    // Stopped is terminal; no transition is valid from it.
    let op = install_state("cancel-stopped", OperationIntent::Stopped);
    cancel_write_operation(op.id(), true);
    assert_eq!(load_intent(&op.state().intent), OperationIntent::Stopped);
    cancel_write_operation(op.id(), false);
    assert_eq!(load_intent(&op.state().intent), OperationIntent::Stopped);
}

#[test]
fn cancel_drops_the_conflict_resolution_sender() {
    // After cancel, any pending receiver should observe a closed channel.
    let op = install_state("cancel-drops-tx", OperationIntent::Running);
    let (tx, mut rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    op.state().conflict_slot.arm(tx, placeholder_conflict);
    cancel_write_operation(op.id(), false);
    // The receiver should now be closed (sender dropped).
    match rx.try_recv() {
        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {} // good
        other => panic!("expected sender to be dropped, got {other:?}"),
    }
}

// ---- backend_cancel flag flipping ---------------------------------------

#[test]
fn backend_cancel_starts_unset_on_fresh_state() {
    let state = WriteOperationState::new(Duration::from_millis(50));
    assert!(!state.backend_cancel.is_cancelled());
}

#[test]
fn cancel_write_operation_flips_backend_cancel_to_stopped() {
    let op = install_state("cancel-flips-backend-stopped", OperationIntent::Running);
    assert!(!op.state().backend_cancel.is_cancelled());
    cancel_write_operation(op.id(), false);
    assert!(
        op.state().backend_cancel.is_cancelled(),
        "cancel → Stopped must also flip backend_cancel so in-flight USB ops bail"
    );
}

#[test]
fn cancel_write_operation_flips_backend_cancel_to_rolling_back() {
    let op = install_state("cancel-flips-backend-rb", OperationIntent::Running);
    cancel_write_operation(op.id(), true);
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
    let op = install_state("cancel-stopped-noop", OperationIntent::Stopped);
    cancel_write_operation(op.id(), true);
    assert!(
        !op.state().backend_cancel.is_cancelled(),
        "Stopped is terminal: invalid transition must not flip backend_cancel"
    );
}

#[test]
fn cancel_unknown_operation_is_a_silent_noop() {
    // No installed state; must not panic, must not affect anything.
    cancel_write_operation("does-not-exist-xyzzy", true);
    cancel_write_operation("does-not-exist-xyzzy", false);
}

// ---- cancel_all: the frontend-teardown safety net -------------------------
//
// `cancel_all` is a WALK, so these drive a `WriteOperationRegistry` the test
// OWNS. Calling the global `cancel_all_write_operations()` here would stop
// every operation every OTHER concurrently-running test has in flight — the
// defect these tests used to be. The code under test is the same either way:
// the public function is a one-line delegation to this method, pinned by
// `cancel_all_write_operations_walks_the_global_registry` below.

/// A state registered in `registry` under a unique id, returned for direct
/// inspection. The registry owns the entry, so there's nothing to clean up.
fn registered_in(registry: &WriteOperationRegistry, label: &str, initial: OperationIntent) -> Arc<WriteOperationState> {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    state.intent.store(initial as u8, Ordering::Relaxed);
    registry.insert(unique_id(label), Arc::clone(&state));
    state
}

#[test]
fn cancel_all_stops_running_but_does_not_re_stop_already_stopped() {
    // Pins the `current != OperationIntent::Stopped` guard. If the guard
    // flips to `==`, running operations would NOT be stopped; they'd
    // remain running.
    let registry = WriteOperationRegistry::new();
    let running = registered_in(&registry, "cancel-all-running", OperationIntent::Running);
    let stopped = registered_in(&registry, "cancel-all-stopped", OperationIntent::Stopped);
    let rb = registered_in(&registry, "cancel-all-rb", OperationIntent::RollingBack);

    registry.cancel_all();

    assert_eq!(load_intent(&running.intent), OperationIntent::Stopped);
    assert_eq!(load_intent(&stopped.intent), OperationIntent::Stopped);
    assert_eq!(
        load_intent(&rb.intent),
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
    let registry = WriteOperationRegistry::new();
    let running = registered_in(&registry, "cancel-all-flips-backend", OperationIntent::Running);
    assert!(!running.backend_cancel.is_cancelled());

    registry.cancel_all();

    assert!(
        running.backend_cancel.is_cancelled(),
        "cancel_all must flip backend_cancel so teardown also stops the wire activity"
    );
}

#[test]
fn cancel_all_drops_pending_conflict_senders() {
    // Teardown must unblock an op parked on a Stop-mode conflict prompt:
    // nobody is left to answer it.
    let registry = WriteOperationRegistry::new();
    let state = registered_in(&registry, "cancel-all-conflict", OperationIntent::Running);
    let (tx, mut rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    state.conflict_slot.arm(tx, placeholder_conflict);

    registry.cancel_all();

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
    let registry = WriteOperationRegistry::new();
    let state = registered_in(&registry, "cancel-all-paused", OperationIntent::Running);
    state.pause_gate.pause();

    let woke = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let state_t = Arc::clone(&state);
    let woke_t = Arc::clone(&woke);
    let handle = std::thread::spawn(move || {
        state_t.pause_gate.wait_while_paused_sync(&state_t.intent);
        woke_t.store(true, Ordering::SeqCst);
    });

    // The condvar park has no "parked now" signal, so hold a window to prove
    // the worker is really parked before the teardown lands. Otherwise a
    // worker that never parked would pass this test.
    // allowed-test-sleep: negative assertion over a window; the condvar park has nothing to await.
    std::thread::sleep(Duration::from_millis(50));
    assert!(!woke.load(Ordering::SeqCst), "still parked before teardown");

    registry.cancel_all();

    crate::test_support::wait_until(
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
fn cancel_all_write_operations_walks_the_global_registry() {
    // The public function's one job is to point at the process-global
    // registry; everything it then does is pinned above against a private
    // one. This is the only write-op test that drives a process-global
    // mutator, so it runs ONLY when it has the process to itself — under
    // plain `cargo test` it would stop every other test's operations, which
    // is the defect this suite was restructured to remove.
    if !crate::file_system::write_operations::test_support::one_test_per_process() {
        return;
    }
    let op = install_state("cancel-all-global-wiring", OperationIntent::Running);

    cancel_all_write_operations();

    assert_eq!(
        load_intent(&op.state().intent),
        OperationIntent::Stopped,
        "the public teardown entry point must reach ops in the global registry"
    );
}

// ---- the hard-abort tier --------------------------------------------
//
// Tier 2 exists so a deadline holder can stop WAITING for a backend that isn't
// answering. Its whole safety story is that it is unreachable from anything a
// user clicks, so the negatives here matter more than the positives.

#[test]
fn backend_abort_starts_unset_on_fresh_state() {
    let state = WriteOperationState::new(Duration::from_millis(50));
    assert!(!state.backend_abort.is_cancelled());
}

/// The invariant the whole two-tier split rests on: an ordinary cancel — the one
/// every Cancel button, every queue-window stop, and the frontend teardown net
/// fire — must NEVER reach tier 2. Tier 2 skips each backend's own partial
/// cleanup, so a cancel that leaked into it would trade a clean wind-down for
/// litter on every single cancel.
#[test]
fn an_ordinary_cancel_never_fires_the_hard_abort() {
    let op = install_state("cancel-never-aborts", OperationIntent::Running);
    cancel_write_operation(op.id(), false);
    assert!(
        !op.state().backend_abort.is_cancelled(),
        "a user's Cancel must stay in tier 1: the backend deletes its own partial"
    );

    let rb = install_state("rollback-never-aborts", OperationIntent::Running);
    cancel_write_operation(rb.id(), true);
    assert!(
        !rb.state().backend_abort.is_cancelled(),
        "a Rollback is a user decision too, and stays in tier 1"
    );
}

/// The same negative for the teardown walk, which is the one a wrong wiring
/// would most plausibly route through tier 2 ("we're going away anyway").
#[test]
fn cancel_all_never_fires_the_hard_abort() {
    let registry = WriteOperationRegistry::new();
    let running = registered_in(&registry, "cancel-all-never-aborts", OperationIntent::Running);

    registry.cancel_all();

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
    let op = install_state("abort-one", OperationIntent::Running);

    abort_write_operation(op.id());

    assert_eq!(load_intent(&op.state().intent), OperationIntent::Stopped);
    assert!(op.state().backend_cancel.is_cancelled(), "tier 1 first");
    assert!(op.state().backend_abort.is_cancelled(), "then tier 2");
}

/// An abort must reach an op that was ALREADY cancelled — that is the whole quit
/// sequence (cancel, wait a beat, abort whatever is left). `cancel_write_operation`
/// no-ops on an already-`Stopped` op, so an abort that rode entirely on it would
/// silently do nothing exactly when it is needed.
#[test]
fn abort_reaches_an_operation_that_was_already_cancelled() {
    let op = install_state("abort-after-cancel", OperationIntent::Running);
    cancel_write_operation(op.id(), false);

    abort_write_operation(op.id());

    assert!(
        op.state().backend_abort.is_cancelled(),
        "the quit path cancels first and aborts second; the second step has to land"
    );
}

#[test]
fn abort_unknown_operation_is_a_silent_noop() {
    abort_write_operation("does-not-exist-xyzzy");
}

#[test]
fn abort_all_write_operations_walks_the_global_registry() {
    // Same shape and same reason as `cancel_all_write_operations_walks_the_global_registry`:
    // the public function's one job is to point at the process-global registry,
    // so it only runs when it has the process to itself.
    if !crate::file_system::write_operations::test_support::one_test_per_process() {
        return;
    }
    let op = install_state("abort-all-global-wiring", OperationIntent::Running);

    abort_all_write_operations();

    assert!(
        op.state().backend_abort.is_cancelled(),
        "the public quit-deadline entry point must reach ops in the global registry"
    );
}

#[test]
fn abort_all_fires_both_tiers_on_every_live_operation() {
    let registry = WriteOperationRegistry::new();
    let running = registered_in(&registry, "abort-all-running", OperationIntent::Running);
    // Already stopped: `cancel_all` skips it, but the deadline still has to stop
    // waiting for whatever it left in flight.
    let stopped = registered_in(&registry, "abort-all-stopped", OperationIntent::Stopped);

    registry.abort_all();

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
    let op = install_state("resolve-conflict", OperationIntent::Running);

    let (tx, rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let clash = op.state().conflict_slot.arm(tx, placeholder_conflict).conflict_id;

    assert_eq!(
        resolve_write_conflict(op.id(), clash, ConflictResolution::Overwrite, true),
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
    let op = install_state("resolve-conflict-twice", OperationIntent::Running);

    let (tx, rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let clash = op.state().conflict_slot.arm(tx, placeholder_conflict).conflict_id;

    assert_eq!(
        resolve_write_conflict(op.id(), clash, ConflictResolution::Overwrite, false),
        ConflictResolutionOutcome::Resolved
    );
    assert_eq!(
        resolve_write_conflict(op.id(), clash, ConflictResolution::Skip, true),
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
    let op = install_state("resolve-conflict-stale", OperationIntent::Running);

    let (first_tx, first_rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let first = op.state().conflict_slot.arm(first_tx, placeholder_conflict).conflict_id;
    assert_eq!(
        resolve_write_conflict(op.id(), first, ConflictResolution::Overwrite, false),
        ConflictResolutionOutcome::Resolved
    );
    first_rx.await.expect("the first clash gets its own answer");

    // The operation moved on and parked on the next clash.
    let (second_tx, mut second_rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    op.state().conflict_slot.arm(second_tx, placeholder_conflict);

    assert_eq!(
        resolve_write_conflict(op.id(), first, ConflictResolution::Skip, true),
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
    let op = install_state("resolve-no-conflict", OperationIntent::Running);
    // The operation is live but has never raised a conflict.
    assert_eq!(
        resolve_write_conflict(op.id(), ConflictId(1), ConflictResolution::Skip, false),
        ConflictResolutionOutcome::NoPendingConflict
    );
}

#[test]
fn resolve_write_conflict_on_an_unknown_operation_says_so() {
    // Nothing registered under this id: it settled, or it never existed. Not the
    // same thing as a live operation that isn't asking anything.
    assert_eq!(
        resolve_write_conflict(
            &unique_id("resolve-gone"),
            ConflictId(1),
            ConflictResolution::Skip,
            false
        ),
        ConflictResolutionOutcome::UnknownOperation
    );
}

#[test]
fn cancelling_takes_the_pending_conflict_away() {
    let op = install_state("resolve-after-cancel", OperationIntent::Running);
    let (tx, _rx) = tokio::sync::oneshot::channel::<ConflictResolutionResponse>();
    let clash = op.state().conflict_slot.arm(tx, placeholder_conflict).conflict_id;

    cancel_write_operation(op.id(), false);

    assert!(!op.state().conflict_slot.is_awaiting());
    assert_eq!(
        resolve_write_conflict(op.id(), clash, ConflictResolution::Skip, false),
        ConflictResolutionOutcome::NoPendingConflict
    );
}

// ---- CopyTransaction ----

#[test]
fn copy_transaction_rollback_deletes_files_and_dirs_in_reverse() {
    // Build a real on-disk transaction: nested dirs + a file, then roll
    // back. Both removals must happen. The rollback must walk dirs in
    // reverse-creation order so the leaf is removed before its parent.
    let tmp = tempfile::tempdir().unwrap();
    let outer = tmp.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir(&outer).unwrap();
    std::fs::create_dir(&inner).unwrap();
    let file = inner.join("data.bin");
    std::fs::write(&file, b"hello").unwrap();

    let mut tx = CopyTransaction::new();
    tx.record_dir(outer.clone());
    tx.record_dir(inner.clone());
    tx.record_file(file.clone());

    tx.rollback();

    assert!(!file.exists(), "file must be removed on rollback");
    assert!(!inner.exists(), "inner dir must be removed (leaf-first)");
    assert!(!outer.exists(), "outer dir must be removed");
}

#[test]
fn copy_transaction_commit_prevents_drop_rollback() {
    // Kills: replace CopyTransaction::commit with (), and the `!self.committed`
    // guard in Drop. After commit(), files must survive Drop.
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("kept.txt");
    std::fs::write(&file, b"persist").unwrap();

    {
        let mut tx = CopyTransaction::new();
        tx.record_file(file.clone());
        tx.commit();
    } // Drop runs here.

    assert!(file.exists(), "commit() must prevent the Drop-based rollback");
}

#[test]
fn copy_transaction_drop_rolls_back_when_not_committed() {
    // Kills: replace <impl Drop>::drop with (), and `delete !` in Drop.
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("ephemeral.txt");
    std::fs::write(&file, b"will be gone").unwrap();

    {
        let mut tx = CopyTransaction::new();
        tx.record_file(file.clone());
        // No commit; Drop should roll back.
    }

    assert!(!file.exists(), "Drop-on-uncommitted must remove recorded files");
}

#[test]
fn copy_transaction_record_methods_push_in_order() {
    // Kills: replace record_file/record_dir with ().
    let mut tx = CopyTransaction::new();
    tx.record_file(PathBuf::from("/a"));
    tx.record_file(PathBuf::from("/b"));
    tx.record_dir(PathBuf::from("/d1"));
    assert_eq!(tx.created_files, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    assert_eq!(tx.created_dirs, vec![PathBuf::from("/d1")]);
    tx.commit(); // suppress Drop rollback (paths don't exist anyway, but be tidy)
}

// ── Busy-volumes set (drives "disable Eject while an op touches a device") ──
// These assert membership of the test's own unique volume IDs (not the whole
// set) and clean up, so they stay correct under nextest's in-process
// parallelism where the global `OPERATION_STATUS_CACHE` is shared.

#[test]
fn busy_volume_ids_reflects_registered_volumes() {
    let op = unique_id("busy-op");
    let usb = unique_id("usb");
    let mtp = unique_id("mtp");

    register_operation_status(&op, WriteOperationType::Copy, vec![usb.clone(), mtp.clone()]);
    let busy = busy_volume_ids();
    assert!(busy.contains(&usb), "source volume should be busy");
    assert!(busy.contains(&mtp), "dest volume should be busy");

    unregister_operation_status(&op);
    let after = busy_volume_ids();
    assert!(!after.contains(&usb), "volume should clear once the op finishes");
    assert!(!after.contains(&mtp), "volume should clear once the op finishes");
}

#[test]
fn busy_volume_ids_excludes_root() {
    let op = unique_id("busy-root-op");
    let usb = unique_id("usb");

    let root = crate::file_system::volume::DEFAULT_VOLUME_ID.to_string();
    register_operation_status(&op, WriteOperationType::Move, vec![root.clone(), usb.clone()]);
    let busy = busy_volume_ids();
    assert!(busy.contains(&usb), "the ejectable volume should be busy");
    assert!(
        !busy.contains(&root),
        "root is never ejectable, so it must not appear in the busy set"
    );

    unregister_operation_status(&op);
}

#[test]
fn busy_volume_ids_stays_busy_until_all_overlapping_ops_finish() {
    // Two concurrent transfers touch the same device; it must stay busy
    // until BOTH finish (refcount-by-membership, no manual counter).
    let op_a = unique_id("overlap-a");
    let op_b = unique_id("overlap-b");
    let dev = unique_id("device");

    register_operation_status(&op_a, WriteOperationType::Copy, vec![dev.clone()]);
    register_operation_status(&op_b, WriteOperationType::Copy, vec![dev.clone()]);
    assert!(busy_volume_ids().contains(&dev));

    unregister_operation_status(&op_a);
    assert!(busy_volume_ids().contains(&dev), "still busy while the second op runs");

    unregister_operation_status(&op_b);
    assert!(
        !busy_volume_ids().contains(&dev),
        "clears only after the last op finishes"
    );
}

#[test]
fn busy_volume_ids_clears_on_panic_unwind_via_unregister() {
    // A panicking op must not leave its volume stuck "busy" forever. In
    // production `manager::ManagedTaskGuard` calls `unregister_operation_status`
    // on unwind; this pins that `unregister_operation_status` itself clears
    // the busy mark when invoked from a `Drop` during a panic.
    struct UnregisterOnDrop(String);
    impl Drop for UnregisterOnDrop {
        fn drop(&mut self) {
            unregister_operation_status(&self.0);
        }
    }

    let op = unique_id("panic-op");
    let dev = unique_id("panic-device");
    let op_for_thread = op.clone();
    let dev_for_thread = dev.clone();

    let handle = std::thread::spawn(move || {
        register_operation_status(&op_for_thread, WriteOperationType::Delete, vec![dev_for_thread]);
        let _guard = UnregisterOnDrop(op_for_thread.clone());
        panic!("simulated op panic while the device is busy");
    });
    assert!(handle.join().is_err(), "thread should have panicked");

    assert!(
        !busy_volume_ids().contains(&dev),
        "unregister on unwind must clear the busy mark"
    );
}

// ---- pause / resume on the live state ------------------------------------
// The pure `PauseGate` mechanics (sync/async parking, cancel-wins) are
// tested in `operation_intent.rs`; these pin the `WRITE_OPERATION_STATE`
// lookup wiring of `pause_write_operation` / `resume_write_operation`.

#[test]
fn pause_resume_write_operation_flip_the_live_gate() {
    let op = install_state("pause-live", OperationIntent::Running);
    assert!(!op.state().pause_gate.is_paused());

    assert!(pause_write_operation(op.id()), "should find the live state");
    assert!(op.state().pause_gate.is_paused(), "pause must set the gate flag");

    assert!(resume_write_operation(op.id()), "should find the live state");
    assert!(!op.state().pause_gate.is_paused(), "resume must clear the gate flag");
}

#[test]
fn pause_resume_unknown_operation_returns_false() {
    assert!(!pause_write_operation("does-not-exist-pause"));
    assert!(!resume_write_operation("does-not-exist-pause"));
}

// ---- TestOperationGuard's own contract -----------------------------------

#[test]
fn guard_unregisters_its_state_even_when_the_test_body_panics() {
    // Pins the panic-safety the guard exists for: a hand-rolled `remove` placed
    // after the assertions leaked the entry whenever an assertion failed first,
    // and the corpse then showed up in the next test's
    // `cancel_all_write_operations` / `list_active_operations`.
    let payload = std::panic::catch_unwind(|| {
        let op = TestOperationGuard::register("guard-panic-safety");
        let id = op.id().to_string();
        assert!(WRITE_OPERATION_STATE.contains(&id));
        panic!("simulated assertion failure while the state is registered: {id}");
    })
    .expect_err("the closure should have panicked");
    let id = payload
        .downcast_ref::<String>()
        .expect("panic payload is the formatted message")
        .rsplit(": ")
        .next()
        .expect("message ends with the operation id")
        .to_string();

    assert!(
        !WRITE_OPERATION_STATE.contains(&id),
        "Drop must unregister on unwind, not only on the happy path"
    );
}
