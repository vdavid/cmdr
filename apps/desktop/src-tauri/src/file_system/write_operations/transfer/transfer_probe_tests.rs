//! What the in-flight table and the stall watchdog must say, and when the
//! watchdog must ACT.
//!
//! Split out of `transfer_probe.rs` so the module itself stays readable; wired
//! back in with `#[path]`, the same shape every other big module in this
//! directory uses. `watchdog_step` is a pure function of (probe, carry-over,
//! now), so every case here drives synthetic ticks instead of sleeping.

use std::sync::atomic::AtomicUsize;

use super::super::transfer_driver::SerialLeafProgress;
use super::*;
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{WriteOperationType, WriteProgressEvent};

/// A probe whose stall-abort window is `window` AND whose connection is proven
/// dead, so a test can drive the watchdog past the window in a handful of
/// synthetic ticks and see it act.
///
/// The dead verdict is not decoration: the abort is gated on it, and no backend
/// in this workspace can produce one yet (`Volume::connection_liveness`). A test
/// that left the volumes at the honest `None` would be asserting nothing.
fn probe_with_abort_window(id: &str, state: &Arc<WriteOperationState>, window: Duration) -> Arc<OperationProbe> {
    let _guard = StallAbortGuard::set(window);
    probe_with(
        id,
        state,
        vec![super::super::liveness_test_support::dead_connection_volume()],
    )
}

/// A probe whose volumes give the honest default answer: no evidence either way.
/// This is what production looks like today.
fn probe_for(id: &str, state: &Arc<WriteOperationState>) -> Arc<OperationProbe> {
    probe_with(id, state, Vec::new())
}

fn probe_with(id: &str, state: &Arc<WriteOperationState>, volumes: Vec<Arc<dyn Volume>>) -> Arc<OperationProbe> {
    Arc::new(OperationProbe {
        operation_id: id.to_owned(),
        concurrency: 8,
        total_files: 764,
        driver_phase: AtomicU8::new(DriverPhase::AwaitingTasks as u8),
        driver_detail: Mutex::new("sms-20260724020237.xml".to_owned()),
        tasks: Mutex::new(Vec::new()),
        sink: Mutex::new(None),
        still_for_seconds: AtomicU64::new(0),
        stall_abort_after: stall_abort_after(),
        volumes,
        state: Arc::clone(state),
        started: Instant::now(),
    })
}

/// The dump has to answer the questions the 2026-07-31 incident could not:
/// what each task awaits, what the driver was doing, and what the intent was.
#[test]
fn dump_names_driver_phase_intent_and_every_parked_task() {
    let guard = TestOperationGuard::register("probe-dump");
    let state = guard.state();
    let probe = probe_for(guard.id(), state);

    let a = probe.begin_task(9, "/src/sms-0726.xml", "/dst/sms-0726.xml");
    a.probe().set_phase(TaskPhase::ParkedDestYield);
    a.probe().set_bytes(0, 13_421_021);
    let b = probe.begin_task(11, "/src/sms-0725.xml", "/dst/sms-0725.xml");
    b.probe().set_phase(TaskPhase::Streaming);
    b.probe().set_bytes(4_194_304, 13_421_021);

    let dump = probe.render_dump("test");

    assert!(dump.contains("driver=awaiting-tasks(sms-20260724020237.xml)"), "{dump}");
    assert!(dump.contains("intent=running"), "{dump}");
    assert!(dump.contains("in_flight=2/8"), "{dump}");
    assert!(dump.contains("#9 parked(dest-yield)"), "{dump}");
    assert!(dump.contains("0/13421021 bytes"), "{dump}");
    assert!(dump.contains("#11 streaming"), "{dump}");
    assert!(dump.contains("4194304/13421021 bytes"), "{dump}");
}

/// A task that is dropped mid-flight (abort, panic) must not linger in the
/// table and make the next dump lie about what is in flight.
#[test]
fn dropping_a_task_handle_removes_it_from_the_table() {
    let guard = TestOperationGuard::register("probe-drop");
    let state = guard.state();
    let probe = probe_for(guard.id(), state);

    let a = probe.begin_task(0, "/src/a", "/dst/a");
    {
        let _b = probe.begin_task(1, "/src/b", "/dst/b");
        assert!(probe.render_dump("test").contains("in_flight=2/8"));
    }
    assert!(probe.render_dump("test").contains("in_flight=1/8"));
    drop(a);
    let dump = probe.render_dump("test");
    assert!(dump.contains("in_flight=0/8"), "{dump}");
    assert!(dump.contains("(no tasks in flight)"), "{dump}");
}

/// Outside a copy task the task-local is unset; the helpers must be silent
/// no-ops rather than panicking.
#[test]
fn phase_helpers_are_noops_outside_a_copy_task() {
    set_task_phase(TaskPhase::Streaming);
    set_task_bytes(1, 2);
}

/// The distinction the UI hangs on: parked ON PURPOSE reads differently
/// from genuinely stuck. Calling a deliberate yield a stall would train
/// users to ignore the warning.
#[test]
fn activity_names_what_the_transfer_is_waiting_on() {
    let guard = TestOperationGuard::register("probe-activity");
    let state = guard.state();
    let probe = probe_for(guard.id(), state);
    // Stand in for the watchdog having seen 12 s with no byte movement.
    probe.still_for_seconds.store(12, Ordering::Relaxed);

    // Every task parked on the destination ⇒ that's what we're waiting on.
    let a = probe.begin_task(0, "/src/a", "/dst/a");
    a.probe().set_phase(TaskPhase::ParkedDestYield);
    let b = probe.begin_task(1, "/src/b", "/dst/b");
    b.probe().set_phase(TaskPhase::ParkedDestYield);
    let activity = probe.activity();
    assert_eq!(activity.in_flight, 2);
    assert_eq!(activity.waiting_on, TransferWaitReason::Destination);

    // One task still streaming ⇒ not a destination wait; nothing explains it.
    b.probe().set_phase(TaskPhase::Streaming);
    assert_eq!(probe.activity().waiting_on, TransferWaitReason::Unknown);

    // A conflict prompt outranks everything: the transfer waits on a person.
    a.probe().set_phase(TaskPhase::ResolvingConflict);
    assert_eq!(probe.activity().waiting_on, TransferWaitReason::You);
}

/// The hole this closes: a wedged transfer emits NO progress events, because
/// progress events are driven by chunk callbacks and no chunk ever lands. So
/// the last event the UI holds says "moving" forever, and the dialog keeps a
/// confident ETA on screen through a total stall — exactly what happened on
/// 2026-07-31. The watchdog has to speak up on the operation's behalf.
#[test]
fn a_wedged_transfer_keeps_telling_the_ui_it_is_wedged() {
    let guard = TestOperationGuard::register("probe-heartbeat");
    let state = guard.state();
    let sink = Arc::new(CollectorEventSink::new());
    let probe = probe_for(guard.id(), state);
    probe.set_sink(Arc::clone(&sink) as Arc<dyn OperationEventSink>);

    // The operation emitted one progress event while it was still moving.
    let mut event = WriteProgressEvent::new(
        guard.id().to_owned(),
        WriteOperationType::Copy,
        WriteOperationPhase::Copying,
        Some("sms-0726.xml".to_owned()),
        5,
        764,
        83_650_000,
        900_000_000,
    );
    state.enrich_progress(&mut event);
    let a = probe.begin_task(9, "/src/a", "/dst/a");
    a.probe().set_phase(TaskPhase::ParkedDestYield);

    // Nothing emits for a while: every task is parked on the destination.
    // The first tick only establishes the byte baseline, so run past the
    // threshold rather than exactly to it.
    let mut watchdog = WatchdogState::new();
    for tick in 1..=(HEARTBEAT_AFTER_SECS + 2) {
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    let emitted = sink.progress.lock_ignore_poison();
    let last = emitted.last().expect("the watchdog must speak for a wedged transfer");
    let activity = last.activity.expect("a re-emitted event carries fresh activity");
    assert_eq!(activity.waiting_on, TransferWaitReason::Destination);
    assert!(activity.still_for_seconds >= 1, "{activity:?}");
    assert_eq!(activity.in_flight, 1);
    // The counters are unchanged, because nothing moved. That's the point:
    // only the activity is new.
    assert_eq!(last.files_done, 5);
    assert_eq!(last.bytes_done, 83_650_000);
}

/// THE FALSE POSITIVE, and the reason the watchdog reads the operation's own
/// published byte total rather than a counter of its own.
///
/// A directory copy on the serial path streams leaf after leaf through ONE
/// `SerialLeafProgress`, and every leaf restarts its own byte count at zero. The
/// number the watchdog judges has to be the operation-wide one the dialog is
/// showing, so a copy whose bar is visibly climbing is never called stalled —
/// however many file boundaries it crosses.
///
/// ❌ Don't relax this to "the probe was told about some bytes". Any counter a
/// single driver has to remember to feed is one a second driver forgets, which
/// is exactly how a healthy 333 GB SMB copy came to display "the transfer has
/// stopped moving" for its whole run.
#[test]
fn a_transfer_publishing_progress_across_file_boundaries_is_never_called_still() {
    let guard = TestOperationGuard::register("probe-leaf-progress");
    let state = guard.state();
    let sink = Arc::new(CollectorEventSink::new());
    let probe = probe_for(guard.id(), state);
    probe.set_sink(Arc::clone(&sink) as Arc<dyn OperationEventSink>);

    // One operation-wide leaf counter and one throttle cell, shared across every
    // leaf, exactly as the serial copy driver wires them.
    let leaf_files_done = Arc::new(AtomicUsize::new(0));
    let last_emit = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
    let leaf_bytes = 2_800_000_u64;

    let mut watchdog = WatchdogState::new();
    let mut tick = 0_u64;
    let mut base = 0_u64;
    for _leaf in 0..3 {
        let leaf = SerialLeafProgress::new(
            Arc::clone(&sink) as Arc<dyn OperationEventSink>,
            Arc::clone(state),
            guard.id().to_owned(),
            WriteOperationType::Copy,
            None,
            base,
            Arc::clone(&leaf_files_done),
            119_204,
            333_000_000_000,
            Arc::clone(&last_emit),
            // No throttle: every chunk has to be observable.
            Duration::ZERO,
        );
        for chunk in [leaf_bytes / 2, leaf_bytes] {
            let _ = leaf.on_chunk(chunk);
            tick += 1;
            probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
            assert_eq!(
                probe.still_for_seconds.load(Ordering::Relaxed),
                0,
                "a transfer that just reported {chunk} more bytes is moving, not still"
            );
        }
        leaf.on_leaf_complete(leaf_bytes);
        base += leaf_bytes;
    }

    assert_eq!(
        probe.activity().waiting_on,
        TransferWaitReason::Moving,
        "the dialog must never be handed a stall reason for a copy that is streaming"
    );
}

/// The heartbeat must stay quiet while bytes flow: a moving transfer already
/// emits plenty, and duplicating those would double the FE's event rate.
#[test]
fn a_moving_transfer_gets_no_heartbeat() {
    let guard = TestOperationGuard::register("probe-no-heartbeat");
    let state = guard.state();
    let sink = Arc::new(CollectorEventSink::new());
    let probe = probe_for(guard.id(), state);
    probe.set_sink(Arc::clone(&sink) as Arc<dyn OperationEventSink>);
    let mut watchdog = WatchdogState::new();
    // Bytes keep moving on every tick, published the way a driver publishes
    // them: one more progress event carrying a higher total.
    for tick in 1..=(HEARTBEAT_AFTER_SECS + 5) {
        let mut event = WriteProgressEvent::new(
            guard.id().to_owned(),
            WriteOperationType::Copy,
            WriteOperationPhase::Copying,
            None,
            5,
            764,
            83_650_000 + tick * 1_000,
            900_000_000,
        );
        state.enrich_progress(&mut event);
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    assert!(
        sink.progress.lock_ignore_poison().is_empty(),
        "a moving transfer needs no help from the watchdog"
    );
}

/// A paused transfer moves no bytes on purpose, and must never be reported
/// as waiting on a device or as stuck.
#[test]
fn a_paused_transfer_reports_paused_not_stuck() {
    let guard = TestOperationGuard::register("probe-paused");
    let state = guard.state();
    let probe = probe_for(guard.id(), state);
    let a = probe.begin_task(0, "/src/a", "/dst/a");
    a.probe().set_phase(TaskPhase::ParkedPause);
    probe.still_for_seconds.store(30, Ordering::Relaxed);
    state.pause_gate.pause();

    let activity = probe.activity();
    assert_eq!(activity.waiting_on, TransferWaitReason::Paused);
    assert_eq!(activity.still_for_seconds, 0, "a pause is not time spent stalled");
}

/// A TOP-LEVEL conflict prompt is resolved on the DRIVER, so no task carries
/// `ResolvingConflict` for it. Reading only task phases meant the dialog told
/// the user their transfer had stopped moving while it was asking them which
/// file to overwrite — and heartbeat re-emits piled up behind the prompt.
#[test]
fn a_transfer_waiting_on_a_conflict_answer_is_not_stalled() {
    let guard = TestOperationGuard::register("probe-conflict");
    let state = guard.state();
    let probe = probe_for(guard.id(), state);
    // A task streaming normally: nothing here says "asking the human".
    let a = probe.begin_task(0, "/src/a", "/dst/a");
    a.probe().set_phase(TaskPhase::Streaming);
    probe.still_for_seconds.store(30, Ordering::Relaxed);

    // The driver stores the responder before emitting `write-conflict`.
    let (tx, _rx) = tokio::sync::oneshot::channel();
    state.conflict_slot.arm(tx);

    let activity = probe.activity();
    assert_eq!(activity.waiting_on, TransferWaitReason::You);
    assert_eq!(
        activity.still_for_seconds, 0,
        "time spent waiting for a person is not time spent stalled"
    );

    // Answering it hands the transfer back to the device wait.
    state.conflict_slot.abandon();
    assert_ne!(probe.activity().waiting_on, TransferWaitReason::You);
}

/// The watchdog must not accrue stall time (or heartbeat) behind a prompt.
#[test]
fn the_watchdog_does_not_accrue_stall_time_behind_a_conflict_prompt() {
    let guard = TestOperationGuard::register("probe-conflict-watchdog");
    let state = guard.state();
    let probe = probe_for(guard.id(), state);
    let (tx, _rx) = tokio::sync::oneshot::channel();
    state.conflict_slot.arm(tx);

    let mut watchdog = WatchdogState::new();
    // First step syncs the byte counter; a second one with the SAME bytes is
    // what would accrue stall time if the prompt weren't recognized.
    probe.watchdog_step(&mut watchdog, Duration::from_secs(1));
    probe.watchdog_step(&mut watchdog, Duration::from_secs(60));

    assert_eq!(probe.still_for_seconds.load(Ordering::Relaxed), 0);
    assert_eq!(watchdog.still_since, Duration::from_secs(60), "the clock restarts");
}

// ========================================================================
// The watchdog ACTING (M4.2): ending a wait nothing else will bound.
// ========================================================================

/// The point of the whole thing. A task sitting inside a backend call with
/// zero byte movement past the window gets its wait ended, so the streaming
/// write it is parked on turns into a typed error the retry can use — instead
/// of the dialog saying "stalled" until the user force-quits.
#[test]
fn the_watchdog_ends_the_wait_on_a_task_that_stopped_moving() {
    let guard = TestOperationGuard::register("probe-abort");
    let state = guard.state();
    let probe = probe_with_abort_window(guard.id(), state, Duration::from_secs(5));

    let task = probe.begin_task(0, "/src/a", "/dst/a");
    task.probe().set_phase(TaskPhase::Streaming);
    task.probe().set_bytes(4_194_304, 13_421_021);
    let signal = task.probe().arm_stall_abort();

    let mut watchdog = WatchdogState::new();
    // Well past the window, with the byte counter frozen the whole time.
    for tick in 1..=8 {
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    assert!(
        signal.is_cancelled(),
        "the watchdog has to end a wait no backend deadline is going to bound"
    );
    assert!(
        probe.render_dump("test").contains("stall-aborts=1"),
        "the dump must record that we gave up on the task: {}",
        probe.render_dump("test")
    );
}

/// THE OTHER HALF OF THE CONJUNCTION: a `Dead` verdict is not sufficient on its
/// own. This probe's volume reports its connection dead for every tick below,
/// and the task is still never aborted, because it keeps moving bytes.
///
/// That is not a hypothetical guard. The verdict is a keepalive result, and a
/// keepalive false-positives under exactly the load a transfer creates: against
/// a QNAP TS-464 (2026-08-02, smb2's live-hardware suite) an ECHO probe under
/// heavy write load reported `2 answered, 1 unanswered` — a false `Dead` — while
/// five consecutive idle runs on the same box reported `0 unanswered`. So a
/// healthy transfer to a busy NAS can genuinely be told its connection is dead,
/// and the ONLY thing standing between that and a killed transfer is this: it is
/// still moving. ❌ Don't let anyone simplify the gate to trust `Dead` directly.
#[test]
fn a_task_that_keeps_moving_is_never_aborted() {
    let guard = TestOperationGuard::register("probe-abort-moving");
    let state = guard.state();
    let probe = probe_with_abort_window(guard.id(), state, Duration::from_secs(2));

    let task = probe.begin_task(0, "/src/a", "/dst/a");
    task.probe().set_phase(TaskPhase::Streaming);
    let signal = task.probe().arm_stall_abort();

    let mut watchdog = WatchdogState::new();
    for tick in 1..=20 {
        // One chunk lands every tick: slow, but alive.
        task.probe().set_bytes(tick * 1_000, 1_000_000);
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    assert!(!signal.is_cancelled(), "a slow but moving transfer must be left alone");
}

/// Every park is deliberate and ends on its own — a pause when the user
/// resumes, a yield when foreground drains, a prompt when the human answers.
/// Aborting one would break something that is working as designed.
#[test]
fn a_deliberately_parked_task_is_never_aborted() {
    for phase in [
        TaskPhase::ParkedPause,
        TaskPhase::ParkedSourceYield,
        TaskPhase::ParkedDestYield,
        TaskPhase::ResolvingConflict,
        TaskPhase::WaitingToRetry,
        TaskPhase::Finalizing,
    ] {
        let guard = TestOperationGuard::register("probe-abort-parked");
        let state = guard.state();
        let probe = probe_with_abort_window(guard.id(), state, Duration::from_secs(2));
        let task = probe.begin_task(0, "/src/a", "/dst/a");
        task.probe().set_phase(phase);
        let signal = task.probe().arm_stall_abort();

        let mut watchdog = WatchdogState::new();
        for tick in 1..=20 {
            probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
        }

        assert!(
            !signal.is_cancelled(),
            "{phase:?} is deliberate and must not be aborted"
        );
    }
}

/// A pause must not be spent from the abort budget. Before this the seconds a
/// user spent paused would count, and the first tick after resume could end a
/// perfectly healthy transfer.
#[test]
fn time_spent_paused_does_not_count_toward_the_abort() {
    let guard = TestOperationGuard::register("probe-abort-paused");
    let state = guard.state();
    let probe = probe_with_abort_window(guard.id(), state, Duration::from_secs(5));
    let task = probe.begin_task(0, "/src/a", "/dst/a");
    task.probe().set_phase(TaskPhase::Streaming);
    let signal = task.probe().arm_stall_abort();

    let mut watchdog = WatchdogState::new();
    state.pause_gate.pause();
    for tick in 1..=20 {
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }
    state.pause_gate.resume();
    // One tick after the resume: the budget has to start over here.
    probe.watchdog_step(&mut watchdog, Duration::from_secs(21));

    assert!(!signal.is_cancelled(), "a long pause must not be charged to the task");
}

/// Cancel and rollback own their own teardown (the driver's drain deadline).
/// A second abort path racing them would only make the wind-down harder to
/// reason about, so the watchdog stands down.
#[test]
fn the_watchdog_stands_down_once_the_operation_is_cancelling() {
    let guard = TestOperationGuard::register("probe-abort-cancelling");
    let state = guard.state();
    let probe = probe_with_abort_window(guard.id(), state, Duration::from_secs(2));
    let task = probe.begin_task(0, "/src/a", "/dst/a");
    task.probe().set_phase(TaskPhase::Streaming);
    let signal = task.probe().arm_stall_abort();
    crate::file_system::write_operations::state::cancel_write_operation(guard.id(), false);

    let mut watchdog = WatchdogState::new();
    for tick in 1..=20 {
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    assert!(!signal.is_cancelled(), "the cancel path owns teardown from here");
}

/// Arming a new attempt hands it a FRESH budget. Without this, one task
/// copying a directory would abort every remaining child instantly after the
/// first wedge, and the retry budget would be three no-ops.
#[test]
fn a_new_attempt_gets_a_fresh_signal_and_a_fresh_budget() {
    let guard = TestOperationGuard::register("probe-abort-rearm");
    let state = guard.state();
    let probe = probe_with_abort_window(guard.id(), state, Duration::from_secs(5));
    let task = probe.begin_task(0, "/src/a", "/dst/a");
    task.probe().set_phase(TaskPhase::Streaming);
    let first = task.probe().arm_stall_abort();

    let mut watchdog = WatchdogState::new();
    for tick in 1..=8 {
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }
    assert!(first.is_cancelled());

    // The retry arms its own signal; the exhausted budget must not carry over.
    let second = task.probe().arm_stall_abort();
    probe.watchdog_step(&mut watchdog, Duration::from_secs(9));
    assert!(
        !second.is_cancelled(),
        "a fresh attempt must not inherit the previous attempt's spent budget"
    );
}

/// THE GATE, and the most important negative in this file. Elapsed silence is
/// not evidence that a connection is dead — a large write to a loaded
/// spinning-disk NAS is legitimately slow — so a volume that reports no verdict
/// must never have its wait ended, however long it has been still.
///
/// This is production today: no backend answers the liveness question, and
/// `smb2` 0.16.0's keepalive doesn't change that (a missed probe is not death,
/// and its sound verdict tears the connection down before anyone can read it).
/// The watchdog keeps dumping the in-flight table and feeding the UI's stall
/// signal, and acts on nothing.
/// Deleting this test would let a future change quietly re-arm the teeth on a
/// timer.
#[test]
fn a_connection_with_no_liveness_verdict_is_never_aborted() {
    let guard = TestOperationGuard::register("probe-abort-no-verdict");
    let state = guard.state();
    let _window = StallAbortGuard::set(Duration::from_secs(1));
    // No volumes ⇒ nobody answers `connection_liveness`, exactly like every
    // backend in the workspace today.
    let probe = probe_for(guard.id(), state);
    let task = probe.begin_task(0, "/src/a", "/dst/a");
    task.probe().set_phase(TaskPhase::Streaming);
    let signal = task.probe().arm_stall_abort();

    let mut watchdog = WatchdogState::new();
    for tick in 1..=30 {
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    assert!(
        !signal.is_cancelled(),
        "without proof the connection is dead, a still transfer must be reported and never killed"
    );
    // It still did its reporting job throughout.
    assert!(
        probe.still_for_seconds.load(Ordering::Relaxed) > 0,
        "the stall must still be visible to the UI and the log"
    );
}

/// While bytes flow the UI must get `Moving`, whatever the tasks are doing
/// at the instant we sample: some are always between chunks.
#[test]
fn a_moving_transfer_reports_moving() {
    let guard = TestOperationGuard::register("probe-moving");
    let state = guard.state();
    let probe = probe_for(guard.id(), state);
    let a = probe.begin_task(0, "/src/a", "/dst/a");
    a.probe().set_phase(TaskPhase::ParkedDestYield);
    // The watchdog hasn't observed a still period, so bytes are moving.
    assert_eq!(probe.activity().waiting_on, TransferWaitReason::Moving);
}
