//! What a transfer says before its first byte lands, and while a response is on
//! its way but hasn't finished arriving (ERR-CNK7M: a 377 kB file sat 20 s on a
//! silent 0% bar, read in one SMB compound request).
//!
//! Split from `transfer_probe_tests.rs`, which covers the table, the stall, and
//! the abort; these cover the two readings the UI shows instead of a stall while
//! bytes are genuinely on their way.

use super::super::liveness_test_support::ScriptedConnectionVolume;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::WriteOperationType;
use cmdr_fs::volume::Volume;

fn probe_over(id: &str, state: &Arc<WriteOperationState>, ends: TransferEnds) -> Arc<OperationProbe> {
    Arc::new(OperationProbe {
        operation_id: id.to_owned(),
        concurrency: 10,
        total_files: 1,
        driver_phase: AtomicU8::new(DriverPhase::AwaitingTasks as u8),
        driver_detail: Mutex::new(DriverDetail::Label("")),
        tasks: Mutex::new(Vec::new()),
        sink: Mutex::new(None),
        still_for_seconds: AtomicU64::new(0),
        stall_abort_after: STALL_ABORT_AFTER,
        ends,
        source_inbound_rate: AtomicU64::new(NO_INBOUND_RATE),
        state: Arc::clone(state),
        started: Instant::now(),
    })
}

/// The operation publishes a `Copying` tick with these totals, the way
/// `copy_volumes_with_progress` does before and during the copy.
fn publish(state: &WriteOperationState, id: &str, files_done: usize, bytes_done: u64) -> WriteProgressEvent {
    let mut event = WriteProgressEvent::new(
        id.to_owned(),
        WriteOperationType::Copy,
        WriteOperationPhase::Copying,
        Some("Wedding - Ex libris.png".to_owned()),
        files_done,
        1,
        bytes_done,
        377_436,
    );
    state.enrich_progress(&mut event);
    event
}

fn last_heartbeat(sink: &CollectorEventSink) -> TransferActivity {
    sink.progress
        .lock_ignore_poison()
        .last()
        .and_then(|e| e.activity.clone())
        .expect("the watchdog must have spoken for the operation")
}

/// B.1: a task opening its source is waiting on the source, so a stall past the
/// notice threshold names it rather than falling through to "nothing explains
/// this".
#[test]
fn an_opening_task_is_waiting_on_the_source() {
    let engine = TestOperations::new();
    let guard = engine.register("opening-waits-on-source");
    let probe = probe_over(guard.id().as_str(), guard.state(), TransferEnds::none());
    probe.still_for_seconds.store(12, Ordering::Relaxed);

    let a = probe.begin_task(TaskRow::source(0), TaskRole::File, "/naspi/a.png", "/tmp/a.png");
    a.probe().set_phase(TaskPhase::OpeningSource);
    assert_eq!(probe.activity().waiting_on, TransferWaitReason::Source);

    // The all-tasks-agree rule still holds: one task streaming means something
    // other than the source is holding the operation up.
    let b = probe.begin_task(TaskRow::source(1), TaskRole::File, "/naspi/b.png", "/tmp/b.png");
    b.probe().set_phase(TaskPhase::Streaming);
    assert_eq!(probe.activity().waiting_on, TransferWaitReason::Unknown);
}

/// B.2: before the first byte there's no ETA to protect, so the operation says
/// it's opening the file from the first still second, long before the stall
/// notice's 10 s. The heartbeat carries it; nothing in the UI times anything.
#[test]
fn a_first_file_still_opening_says_so_from_the_first_still_second() {
    let engine = TestOperations::new();
    let guard = engine.register("opening-says-so");
    let state = guard.state();
    let sink = Arc::new(CollectorEventSink::new());
    let probe = probe_over(guard.id().as_str(), state, TransferEnds::none());
    probe.set_sink(Arc::clone(&sink) as Arc<dyn OperationEventSink>);
    publish(state, guard.id().as_str(), 0, 0);

    let task = probe.begin_task(TaskRow::source(0), TaskRole::File, "/naspi/a.png", "/tmp/a.png");
    task.probe().set_phase(TaskPhase::OpeningSource);

    let mut watchdog = WatchdogState::new();
    // Tick 1 sets the baseline; tick 2 is the first still second.
    probe.watchdog_step(&mut watchdog, Duration::from_secs(1));
    assert!(sink.progress.lock_ignore_poison().is_empty(), "nothing to say yet");
    probe.watchdog_step(&mut watchdog, Duration::from_secs(2));

    let activity = last_heartbeat(&sink);
    assert!(activity.opening_source, "{activity:?}");
    assert_eq!(activity.still_for_seconds, 1, "the elapsed counter starts here");
    assert_eq!(activity.waiting_on, TransferWaitReason::Source);
}

/// A directory copy's walker is listing, not opening, so it must not stop the
/// file rows under it from saying they're opening.
#[test]
fn a_walker_does_not_hide_the_files_it_is_opening() {
    let engine = TestOperations::new();
    let guard = engine.register("opening-with-walker");
    let state = guard.state();
    let probe = probe_over(guard.id().as_str(), state, TransferEnds::none());
    publish(state, guard.id().as_str(), 0, 0);

    let walker = probe.begin_task(TaskRow::source(0), TaskRole::Walker, "/naspi/album", "/tmp/album");
    walker.probe().set_phase(TaskPhase::Walking);
    let leaf = probe.begin_task(
        TaskRow::source(0).leaf(0),
        TaskRole::File,
        "/naspi/album/a.png",
        "/tmp/album/a.png",
    );
    leaf.probe().set_phase(TaskPhase::OpeningSource);
    assert!(probe.activity().opening_source);

    // With no file row at all there's nothing being opened.
    drop(leaf);
    assert!(!probe.activity().opening_source, "a walk alone isn't an opening");
}

/// Once anything has landed, the operation is past its first byte: a later file
/// sitting in its open is an ordinary stall, with an ETA worth protecting.
#[test]
fn opening_ends_once_anything_has_landed() {
    let engine = TestOperations::new();
    let guard = engine.register("opening-ends");
    let state = guard.state();
    let probe = probe_over(guard.id().as_str(), state, TransferEnds::none());
    publish(state, guard.id().as_str(), 1, 377_436);

    let task = probe.begin_task(TaskRow::source(1), TaskRole::File, "/naspi/b.png", "/tmp/b.png");
    task.probe().set_phase(TaskPhase::OpeningSource);
    assert!(!probe.activity().opening_source);
}

/// Part 2: while nothing lands, the source connection's receive rate rides the
/// heartbeat, so the UI can show bytes arriving on a bar that can't move yet.
/// ~19 KB/s is what the ERR-CNK7M read averaged.
#[test]
fn the_source_s_receive_rate_rides_the_heartbeat_while_nothing_lands() {
    let engine = TestOperations::new();
    let guard = engine.register("inbound-rate");
    let state = guard.state();
    let sink = Arc::new(CollectorEventSink::new());
    let source = ScriptedConnectionVolume::new();
    source.receive(0);
    let probe = probe_over(
        guard.id().as_str(),
        state,
        TransferEnds::source_only(Arc::clone(&source) as Arc<dyn Volume>),
    );
    probe.set_sink(Arc::clone(&sink) as Arc<dyn OperationEventSink>);
    publish(state, guard.id().as_str(), 0, 0);
    let task = probe.begin_task(TaskRow::source(0), TaskRole::File, "/naspi/a.png", "/tmp/a.png");
    task.probe().set_phase(TaskPhase::OpeningSource);

    let mut watchdog = WatchdogState::new();
    for tick in 1..=6 {
        source.receive(19_000);
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    let activity = last_heartbeat(&sink);
    assert_eq!(activity.source_inbound_bytes_per_second, Some(19_000), "{activity:?}");
    assert!(
        probe.render_dump("test").contains("source_inbound=19000B/s"),
        "the log says so too: {}",
        probe.render_dump("test")
    );
    // ❌ Never progress: the byte counter the bar reads hasn't moved.
    assert_eq!(state.last_progress_bytes(), Some(0));
}

/// While bytes land, the ordinary rate says everything with verified numbers, so
/// there's no receive rate to show.
#[test]
fn a_transfer_landing_bytes_reports_no_receive_rate() {
    let engine = TestOperations::new();
    let guard = engine.register("inbound-rate-moving");
    let state = guard.state();
    let source = ScriptedConnectionVolume::new();
    source.receive(0);
    let probe = probe_over(
        guard.id().as_str(),
        state,
        TransferEnds::source_only(Arc::clone(&source) as Arc<dyn Volume>),
    );
    let task = probe.begin_task(TaskRow::source(0), TaskRole::File, "/naspi/a.bin", "/tmp/a.bin");
    task.probe().set_phase(TaskPhase::Streaming);

    let mut watchdog = WatchdogState::new();
    for tick in 1..=6 {
        source.receive(1_000_000);
        publish(state, guard.id().as_str(), 0, tick * 1_000_000);
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }

    assert_eq!(probe.activity().source_inbound_bytes_per_second, None);
}

/// A pause stands still on purpose; whatever is still arriving from a request
/// already in flight isn't the transfer's news.
#[test]
fn a_paused_transfer_reports_no_receive_rate() {
    let engine = TestOperations::new();
    let guard = engine.register("inbound-rate-paused");
    let state = guard.state();
    let source = ScriptedConnectionVolume::new();
    source.receive(0);
    let probe = probe_over(
        guard.id().as_str(),
        state,
        TransferEnds::source_only(Arc::clone(&source) as Arc<dyn Volume>),
    );
    publish(state, guard.id().as_str(), 0, 0);

    let mut watchdog = WatchdogState::new();
    for tick in 1..=4 {
        source.receive(19_000);
        probe.watchdog_step(&mut watchdog, Duration::from_secs(tick));
    }
    assert!(probe.activity().source_inbound_bytes_per_second.is_some());

    state.pause_gate.pause();
    probe.watchdog_step(&mut watchdog, Duration::from_secs(5));
    assert_eq!(probe.activity().source_inbound_bytes_per_second, None);
}

/// The stillness and the rate are the watchdog's readings from its last tick.
/// An event that moved the counters is movement by definition, so it goes out
/// saying so, rather than carrying "no progress for 12s" from before it landed
/// and holding that on screen until the next event.
#[test]
fn an_event_that_moved_the_counters_goes_out_as_moving() {
    let engine = TestOperations::new();
    let guard = engine.register("event-moved");
    let state = guard.state();
    let probe = probe_over(guard.id().as_str(), state, TransferEnds::none());
    state
        .scope()
        .stores()
        .probes()
        .insert(guard.id().as_str(), Arc::clone(&probe));
    publish(state, guard.id().as_str(), 0, 0);
    let task = probe.begin_task(TaskRow::source(0), TaskRole::File, "/naspi/a.png", "/tmp/a.png");
    task.probe().set_phase(TaskPhase::OpeningSource);
    probe.still_for_seconds.store(12, Ordering::Relaxed);
    probe.publish_inbound_rate(Some(19_000));

    let landed = publish(state, guard.id().as_str(), 1, 377_436);
    state.scope().stores().probes().remove(guard.id().as_str());

    let activity = landed.activity.expect("a probed operation always classifies");
    assert_eq!(activity.still_for_seconds, 0);
    assert_eq!(activity.waiting_on, TransferWaitReason::Moving);
    assert_eq!(activity.source_inbound_bytes_per_second, None);
    assert!(!activity.opening_source);
}
