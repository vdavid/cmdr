//! What every in-flight transfer task is doing right now, and a watchdog that
//! says so when a transfer stops moving.
//!
//! A wedged transfer parks its driver and its tasks on `.await`, so no thread
//! carries a transfer frame and a stack sample sees nothing (that is exactly how
//! the 2026-07-31 incident resisted diagnosis; see
//! `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). The only way to
//! learn where a parked async task is stuck is for it to say so on the way in, so
//! every phase transition records itself here and the watchdog prints the table.
//!
//! **Cost.** A phase transition is one relaxed atomic store, and per-chunk byte
//! progress is one more; nothing on the hot path takes a lock. The per-operation
//! registry lock is touched only when a task starts or finishes, and by the
//! watchdog tick.
//!
//! **Reaching the probe.** A copy task's body runs inside
//! [`CURRENT_TASK_PROBE`]`.scope(...)`, so code arbitrarily deep inside it
//! (`copy_single_path` → `stream_pipe_file` → `CheckpointStream`) reaches its own
//! probe with no signature threading. Outside a copy task the lookup simply
//! misses and every call is a no-op, which is what the unit tests and the
//! local-FS path rely on.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;

use super::super::event_sinks::OperationEventSink;
use super::super::state::{OperationIntent, WriteOperationState, load_intent};
use super::super::types::{TransferActivity, TransferWaitReason, WriteOperationPhase, WriteProgressEvent};

/// How often the watchdog samples an operation, and how long a transfer may
/// show zero byte movement before it is called out IN THE LOG.
///
/// The tick also sets the granularity of `TransferActivity::still_for_seconds`,
/// which the UI reads to decide when to stop showing a confident ETA — hence
/// 1 s rather than something coarser. It's one wakeup per second per running
/// transfer, comparing one atomic.
///
/// 20 s for the log sits well clear of a slow-but-alive SMB write window. The
/// UI speaks sooner (see `STALL_NOTICE_SECONDS` in
/// `transfer/transfer-stall.ts`); a log line wants to stay rare, while a frozen
/// bar with a confident ETA is a lie the moment it stops being true.
pub(super) const STALL_TICK: Duration = Duration::from_secs(1);
pub(super) const STALL_AFTER: Duration = Duration::from_secs(20);

/// How long the byte counter must be still before the watchdog starts
/// re-emitting the last progress event on the operation's behalf.
///
/// This is what makes a stall visible AT ALL. Progress events are driven by
/// chunk callbacks, so a wedged transfer emits nothing: without a heartbeat the
/// UI keeps rendering the last event it received, complete with a confident ETA,
/// for as long as the wedge lasts. That is precisely what the dialog did through
/// the 2026-07-31 incident.
///
/// 3 s is comfortably longer than any gap between chunk callbacks on a live
/// transfer (the progress throttle itself is sub-second) and comfortably shorter
/// than the point where a person starts wondering whether the app has died.
pub(super) const HEARTBEAT_AFTER_SECS: u64 = 3;

/// What a single copy task is doing. Ordinals are stable only within a build;
/// nothing persists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum TaskPhase {
    /// Spawned into the window, not yet doing I/O.
    Spawned = 0,
    /// Opening the source stream (a device round-trip on MTP / SMB).
    OpeningSource = 1,
    /// Actively piping chunks.
    Streaming = 2,
    /// Parked between windows because the user paused.
    ParkedPause = 3,
    /// Parked between windows for foreground work on the SOURCE device
    /// (unbounded by design).
    ParkedSourceYield = 4,
    /// Parked between windows for foreground work on the DESTINATION share
    /// (hard-capped; it holds an open write handle).
    ParkedDestYield = 5,
    /// Past the last byte: safe-replace finalize, journal, cleanup.
    Finalizing = 6,
    /// Resolving a nested conflict inside a directory source (may be waiting on
    /// the human).
    ResolvingConflict = 7,
}

impl TaskPhase {
    const fn label(self) -> &'static str {
        match self {
            Self::Spawned => "spawned",
            Self::OpeningSource => "opening-source",
            Self::Streaming => "streaming",
            Self::ParkedPause => "parked(pause)",
            Self::ParkedSourceYield => "parked(source-yield)",
            Self::ParkedDestYield => "parked(dest-yield)",
            Self::Finalizing => "finalizing",
            Self::ResolvingConflict => "resolving-conflict",
        }
    }

    /// What a task in this phase is waiting on, or `None` when the phase means
    /// "working" and so explains nothing about a stall.
    ///
    /// `ParkedPause` maps to `None` on purpose: the pause is reported from the
    /// operation's pause gate, which is authoritative, and a task can still be
    /// mid-chunk when the gate flips.
    const fn wait_reason(self) -> Option<TransferWaitReason> {
        match self {
            Self::ParkedDestYield => Some(TransferWaitReason::Destination),
            Self::ParkedSourceYield => Some(TransferWaitReason::Source),
            Self::ResolvingConflict => Some(TransferWaitReason::You),
            Self::Spawned | Self::OpeningSource | Self::Streaming | Self::ParkedPause | Self::Finalizing => None,
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::OpeningSource,
            2 => Self::Streaming,
            3 => Self::ParkedPause,
            4 => Self::ParkedSourceYield,
            5 => Self::ParkedDestYield,
            6 => Self::Finalizing,
            7 => Self::ResolvingConflict,
            _ => Self::Spawned,
        }
    }
}

/// What the DRIVER (the loop that fills and drains the concurrency window) is
/// doing. Distinguishing this from the tasks is the point: in the incident the
/// driver stopped after a destination `get_metadata` pre-check with six of eight
/// slots free, and nothing recorded that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum DriverPhase {
    Starting = 0,
    /// Running the destination pre-check / conflict resolution for the next
    /// source, before it can be spawned.
    PreparingNext = 1,
    /// Window full or sources exhausted: awaiting the next task to finish.
    AwaitingTasks = 2,
    /// Loop finished; running cleanup, rollback, or finalize.
    PostLoop = 3,
}

impl DriverPhase {
    const fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::PreparingNext => "preparing-next",
            Self::AwaitingTasks => "awaiting-tasks",
            Self::PostLoop => "post-loop",
        }
    }

    const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::PreparingNext,
            2 => Self::AwaitingTasks,
            3 => Self::PostLoop,
            _ => Self::Starting,
        }
    }
}

/// One in-flight copy task's live state.
pub(super) struct TaskProbe {
    /// Position of this source in the operation's source list, so a dump can be
    /// read against the spawn log.
    index: usize,
    source: String,
    dest: String,
    phase: AtomicU8,
    /// Millis since the operation started, at the last phase transition.
    phase_since_ms: AtomicU64,
    bytes_done: AtomicU64,
    total_bytes: AtomicU64,
    started: Instant,
}

impl TaskProbe {
    /// Record a phase transition. One relaxed store plus a timestamp; safe to
    /// call on any path, including per-chunk.
    pub(super) fn set_phase(&self, phase: TaskPhase) {
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.phase.store(phase as u8, Ordering::Relaxed);
        self.phase_since_ms.store(now_ms, Ordering::Relaxed);
    }

    /// Per-chunk byte progress. Deliberately a plain store, not an add: the
    /// caller owns the running total for its own file.
    pub(super) fn set_bytes(&self, bytes_done: u64, total_bytes: u64) {
        self.bytes_done.store(bytes_done, Ordering::Relaxed);
        self.total_bytes.store(total_bytes, Ordering::Relaxed);
    }

    fn render(&self) -> String {
        let phase = TaskPhase::from_u8(self.phase.load(Ordering::Relaxed));
        let since_ms = self.phase_since_ms.load(Ordering::Relaxed);
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let total = self.total_bytes.load(Ordering::Relaxed);
        format!(
            // allowed-pluralize-noun: a byte count in a diagnostic dump; the compact form is the point.
            "#{idx} {phase} for {held}ms, {done}/{total} bytes, {source} -> {dest}",
            idx = self.index,
            phase = phase.label(),
            held = now_ms.saturating_sub(since_ms),
            done = self.bytes_done.load(Ordering::Relaxed),
            total = total,
            source = self.source,
            dest = self.dest,
        )
    }
}

/// One operation's live state: the driver plus every task currently in the
/// concurrency window.
pub(super) struct OperationProbe {
    operation_id: String,
    concurrency: usize,
    total_files: usize,
    driver_phase: AtomicU8,
    /// Free-form detail for the driver's current step (the path it is
    /// pre-checking, typically). Written only at phase transitions.
    driver_detail: Mutex<String>,
    tasks: Mutex<Vec<Arc<TaskProbe>>>,
    /// The operation's aggregate byte counter, shared with the driver, so the
    /// watchdog measures the same number the user sees.
    bytes_done: Arc<AtomicU64>,
    /// Where to send a heartbeat. Set once at registration; `None` in the unit
    /// tests that don't exercise emission.
    sink: Mutex<Option<Arc<dyn OperationEventSink>>>,
    /// The last progress event this operation emitted, kept so the watchdog can
    /// re-send it with fresh activity when nothing is moving. Cloning one event
    /// per emit is cheap next to the IPC hop it's already making.
    last_event: Mutex<Option<WriteProgressEvent>>,
    /// Whole seconds the aggregate byte counter has been still, maintained by
    /// the watchdog at `STALL_TICK` granularity and reset on every movement and
    /// on pause. Read by [`OperationProbe::activity`] on the progress path, so
    /// the UI and the log agree by construction rather than by review.
    still_for_seconds: AtomicU64,
    state: Arc<WriteOperationState>,
    started: Instant,
}

impl OperationProbe {
    pub(super) fn set_driver_phase(&self, phase: DriverPhase, detail: &str) {
        self.driver_phase.store(phase as u8, Ordering::Relaxed);
        detail.clone_into(&mut self.driver_detail.lock_ignore_poison());
    }

    /// Register a task entering the window. The returned handle removes it on
    /// drop, so a task that panics or is aborted still leaves the table clean.
    pub(super) fn begin_task(self: &Arc<Self>, index: usize, source: &str, dest: &str) -> TaskProbeHandle {
        let probe = Arc::new(TaskProbe {
            index,
            source: source.to_owned(),
            dest: dest.to_owned(),
            phase: AtomicU8::new(TaskPhase::Spawned as u8),
            phase_since_ms: AtomicU64::new(u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)),
            bytes_done: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            started: self.started,
        });
        self.tasks.lock_ignore_poison().push(Arc::clone(&probe));
        TaskProbeHandle {
            operation: Arc::clone(self),
            probe,
        }
    }

    /// Point the heartbeat at a different sink. Production wires the sink at
    /// registration; this exists so tests can capture the heartbeat.
    #[cfg(test)]
    fn set_sink(&self, sink: Arc<dyn OperationEventSink>) {
        *self.sink.lock_ignore_poison() = Some(sink);
    }

    /// Remember the last progress event, so the watchdog can re-send it with
    /// fresh activity while nothing moves. Called from `enrich_progress`, which
    /// every emit site already routes through.
    pub(super) fn record_progress(&self, event: &WriteProgressEvent) {
        // Only the phases where a stall is meaningful. A scan emits its own
        // steady stream and finishing phases are brief.
        if matches!(
            event.phase,
            WriteOperationPhase::Copying | WriteOperationPhase::Flushing
        ) {
            *self.last_event.lock_ignore_poison() = Some(event.clone());
        }
    }

    /// One watchdog tick, split out from the timer loop so it can be tested
    /// without waiting on wall-clock seconds. `still_for` is how long the byte
    /// counter has been unchanged.
    fn watchdog_step(&self, watchdog: &mut WatchdogState, now: Duration) {
        if self.state.pause_gate.is_paused() {
            watchdog.still_since = now;
            self.still_for_seconds.store(0, Ordering::Relaxed);
            return;
        }
        let bytes = self.bytes_done.load(Ordering::Relaxed);
        if bytes != watchdog.last_bytes {
            watchdog.last_bytes = bytes;
            watchdog.still_since = now;
            self.still_for_seconds.store(0, Ordering::Relaxed);
            return;
        }
        // Publish the stillness on every tick, not just at the log threshold:
        // the UI reads this to decide when to stop showing a confident ETA, and
        // it speaks sooner than the log does.
        let still_for = now.saturating_sub(watchdog.still_since);
        self.still_for_seconds.store(still_for.as_secs(), Ordering::Relaxed);

        // Speak for the operation while it can't speak for itself.
        if still_for.as_secs() >= HEARTBEAT_AFTER_SECS {
            self.emit_heartbeat();
        }

        if still_for < STALL_AFTER || now.saturating_sub(watchdog.last_reported) < STALL_AFTER {
            return;
        }
        watchdog.last_reported = now;
        log::warn!(
            "{}",
            self.render_dump(&format!("no byte movement for {}s", still_for.as_secs()))
        );
    }

    /// Re-emit the last progress event with a fresh activity snapshot. The
    /// counters are unchanged (nothing moved, and saying otherwise would be a
    /// lie); only `activity` and the decaying rate/ETA are new.
    fn emit_heartbeat(&self) {
        let Some(sink) = self.sink.lock_ignore_poison().clone() else {
            return;
        };
        let Some(mut event) = self.last_event.lock_ignore_poison().clone() else {
            return;
        };
        // Set from the probe we're already holding rather than leaving it to the
        // registry round-trip in `enrich_progress`: this is the one caller that
        // already knows the answer.
        event.activity = Some(self.activity());
        // Goes through the normal enrich-and-emit path, so the ETA estimator
        // also sees that nothing has moved and lets its own estimate decay to
        // `None` rather than the FE having to special-case a stalled ETA.
        self.state.emit_progress_via_sink(&*sink, event);
    }

    /// The live snapshot the UI renders: how many files are open, how long
    /// nothing has moved, and what the transfer is waiting on.
    ///
    /// This is deliberately the SAME state the watchdog logs from. A dialog
    /// that says "stalled" while the log says otherwise is worse than neither.
    pub(super) fn activity(&self) -> TransferActivity {
        // A pause reads as zero stillness here as well as in the watchdog: the
        // watchdog only resets on its next tick, so without this a transfer
        // paused a moment ago would report the stall time it had accumulated
        // before the user paused it.
        let still_for_seconds = if self.state.pause_gate.is_paused() {
            0
        } else {
            self.still_for_seconds.load(Ordering::Relaxed)
        };
        let in_flight = u32::try_from(self.tasks.lock_ignore_poison().len()).unwrap_or(u32::MAX);
        TransferActivity {
            in_flight,
            still_for_seconds: u32::try_from(still_for_seconds).unwrap_or(u32::MAX),
            waiting_on: self.wait_reason(still_for_seconds),
        }
    }

    /// Classify the wait. Order matters: a pause and a conflict prompt are
    /// deliberate and outrank any device wait, and while bytes move nothing is
    /// waiting on anything (some task is always between chunks).
    fn wait_reason(&self, still_for_seconds: u64) -> TransferWaitReason {
        if self.state.pause_gate.is_paused() {
            return TransferWaitReason::Paused;
        }
        let tasks = self.tasks.lock_ignore_poison();
        let reasons: Vec<TransferWaitReason> = tasks
            .iter()
            .filter_map(|t| TaskPhase::from_u8(t.phase.load(Ordering::Relaxed)).wait_reason())
            .collect();
        // A person being asked a question beats any device wait: the transfer
        // isn't stuck, it's waiting for an answer, and the UI says so even
        // while other tasks keep streaming.
        if reasons.contains(&TransferWaitReason::You) {
            return TransferWaitReason::You;
        }
        if still_for_seconds == 0 {
            return TransferWaitReason::Moving;
        }
        // Only claim a device wait when EVERY in-flight task agrees. One task
        // still streaming means something else is holding the operation up.
        let all_waiting_on = |reason: TransferWaitReason| {
            !tasks.is_empty() && reasons.len() == tasks.len() && reasons.iter().all(|r| *r == reason)
        };
        if all_waiting_on(TransferWaitReason::Destination) {
            return TransferWaitReason::Destination;
        }
        if all_waiting_on(TransferWaitReason::Source) {
            return TransferWaitReason::Source;
        }
        TransferWaitReason::Unknown
    }

    /// The whole in-flight table as log lines. This is the record the incident
    /// needed and did not have. The watchdog prints it on a stall; the driver
    /// prints it when it abandons tasks that wouldn't wind down after a cancel.
    pub(super) fn render_dump(&self, reason: &str) -> String {
        let tasks = self.tasks.lock_ignore_poison();
        let driver = DriverPhase::from_u8(self.driver_phase.load(Ordering::Relaxed));
        let intent = match load_intent(&self.state.intent) {
            OperationIntent::Running => "running",
            OperationIntent::RollingBack => "rolling-back",
            OperationIntent::Stopped => "stopped",
        };
        let mut out = format!(
            "transfer probe ({reason}): op={op} elapsed={elapsed}s bytes_done={bytes} files_total={files} \
             driver={driver}({detail}) intent={intent} paused={paused} in_flight={in_flight}/{concurrency}",
            op = self.operation_id,
            elapsed = self.started.elapsed().as_secs(),
            bytes = self.bytes_done.load(Ordering::Relaxed),
            files = self.total_files,
            driver = driver.label(),
            detail = self.driver_detail.lock_ignore_poison(),
            paused = self.state.pause_gate.is_paused(),
            in_flight = tasks.len(),
            concurrency = self.concurrency,
        );
        if tasks.is_empty() {
            out.push_str("\n  (no tasks in flight)");
        }
        for task in tasks.iter() {
            out.push_str("\n  ");
            out.push_str(&task.render());
        }
        out
    }
}

/// RAII registration for one task. Dropping it removes the task from the table,
/// including on panic or abort.
pub(super) struct TaskProbeHandle {
    operation: Arc<OperationProbe>,
    probe: Arc<TaskProbe>,
}

impl TaskProbeHandle {
    pub(super) fn probe(&self) -> Arc<TaskProbe> {
        Arc::clone(&self.probe)
    }
}

impl Drop for TaskProbeHandle {
    fn drop(&mut self) {
        self.operation
            .tasks
            .lock_ignore_poison()
            .retain(|t| !Arc::ptr_eq(t, &self.probe));
    }
}

tokio::task_local! {
    /// The probe for the copy task currently being polled. Set by
    /// `volume_copy`'s task body; read by anything nested inside it.
    pub(super) static CURRENT_TASK_PROBE: Arc<TaskProbe>;
}

/// Set the current copy task's phase, if there is one.
///
/// A no-op outside a copy task (unit tests, the local-FS path), so callers never
/// need to know whether they are being driven by the volume copy driver.
pub(super) fn set_task_phase(phase: TaskPhase) {
    let _ = CURRENT_TASK_PROBE.try_with(|probe| probe.set_phase(phase));
}

/// Report per-chunk byte progress for the current copy task, if there is one.
pub(super) fn set_task_bytes(bytes_done: u64, total_bytes: u64) {
    let _ = CURRENT_TASK_PROBE.try_with(|probe| probe.set_bytes(bytes_done, total_bytes));
}

/// Live operations, so a watchdog tick (and any future debug command) can see
/// every transfer at once.
static REGISTRY: LazyLock<Mutex<HashMap<String, Arc<OperationProbe>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register an operation and start its stall watchdog.
///
/// The returned guard deregisters on drop, which also stops the watchdog on its
/// next tick.
pub(super) fn register_operation(
    operation_id: &str,
    concurrency: usize,
    total_files: usize,
    bytes_done: Arc<AtomicU64>,
    state: Arc<WriteOperationState>,
    sink: Arc<dyn OperationEventSink>,
) -> OperationProbeGuard {
    let probe = Arc::new(OperationProbe {
        operation_id: operation_id.to_owned(),
        concurrency,
        total_files,
        driver_phase: AtomicU8::new(DriverPhase::Starting as u8),
        driver_detail: Mutex::new(String::new()),
        tasks: Mutex::new(Vec::new()),
        bytes_done,
        sink: Mutex::new(Some(sink)),
        last_event: Mutex::new(None),
        still_for_seconds: AtomicU64::new(0),
        state,
        started: Instant::now(),
    });
    REGISTRY
        .lock_ignore_poison()
        .insert(operation_id.to_owned(), Arc::clone(&probe));
    spawn_watchdog(operation_id.to_owned());
    OperationProbeGuard {
        operation_id: operation_id.to_owned(),
        probe,
    }
}

/// The live activity for an operation, if it keeps an in-flight table.
///
/// `None` for operations with no probe (local copy, delete, trash, and the
/// pre-registration window), where the UI simply shows nothing extra. Called
/// from `WriteOperationState::enrich_progress`, so every progress event from
/// every emit site carries it without a single caller having to remember.
pub(in crate::file_system::write_operations) fn activity_for(operation_id: &str) -> Option<TransferActivity> {
    REGISTRY
        .lock_ignore_poison()
        .get(operation_id)
        .map(|probe| probe.activity())
}

/// Stash a progress event so the watchdog can re-send it while nothing moves.
/// Paired with [`activity_for`] on the `enrich_progress` path; a no-op for
/// operations with no probe.
pub(in crate::file_system::write_operations) fn record_progress(event: &WriteProgressEvent) {
    if let Some(probe) = REGISTRY.lock_ignore_poison().get(&event.operation_id) {
        probe.record_progress(event);
    }
}

/// Deregisters its operation on drop.
pub(super) struct OperationProbeGuard {
    operation_id: String,
    probe: Arc<OperationProbe>,
}

impl OperationProbeGuard {
    pub(super) fn probe(&self) -> Arc<OperationProbe> {
        Arc::clone(&self.probe)
    }
}

impl Drop for OperationProbeGuard {
    fn drop(&mut self) {
        REGISTRY.lock_ignore_poison().remove(&self.operation_id);
    }
}

/// Watches one operation's aggregate byte counter and logs the in-flight table
/// when it stops moving.
///
/// Deliberately quiet while paused: a paused transfer moves no bytes on purpose.
/// The dump repeats every `STALL_AFTER` for as long as the stall lasts, because
/// a user who force-quits after 20 minutes should leave behind more than one
/// record of it.
/// The watchdog's own carry-over between ticks. Split from `OperationProbe` so
/// the step is a pure function of (probe, this, now) and can be tested without
/// sleeping.
struct WatchdogState {
    last_bytes: u64,
    still_since: Duration,
    last_reported: Duration,
}

impl WatchdogState {
    fn new() -> Self {
        Self {
            last_bytes: u64::MAX,
            still_since: Duration::ZERO,
            last_reported: Duration::ZERO,
        }
    }
}

fn spawn_watchdog(operation_id: String) {
    tauri::async_runtime::spawn(async move {
        let mut watchdog = WatchdogState::new();
        let started = Instant::now();
        loop {
            tokio::time::sleep(STALL_TICK).await;
            let Some(probe) = REGISTRY.lock_ignore_poison().get(&operation_id).cloned() else {
                return; // operation finished; guard removed it
            };
            probe.watchdog_step(&mut watchdog, started.elapsed());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;
    use crate::file_system::write_operations::test_support::TestOperationGuard;
    use crate::file_system::write_operations::types::WriteOperationType;

    fn probe_for(id: &str, state: &Arc<WriteOperationState>) -> Arc<OperationProbe> {
        Arc::new(OperationProbe {
            operation_id: id.to_owned(),
            concurrency: 8,
            total_files: 764,
            driver_phase: AtomicU8::new(DriverPhase::AwaitingTasks as u8),
            driver_detail: Mutex::new("sms-20260724020237.xml".to_owned()),
            tasks: Mutex::new(Vec::new()),
            bytes_done: Arc::new(AtomicU64::new(83_650_000)),
            sink: Mutex::new(None),
            last_event: Mutex::new(None),
            still_for_seconds: AtomicU64::new(0),
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
        probe.record_progress(&event);
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

    /// The heartbeat must stay quiet while bytes flow: a moving transfer already
    /// emits plenty, and duplicating those would double the FE's event rate.
    #[test]
    fn a_moving_transfer_gets_no_heartbeat() {
        let guard = TestOperationGuard::register("probe-no-heartbeat");
        let state = guard.state();
        let sink = Arc::new(CollectorEventSink::new());
        let probe = probe_for(guard.id(), state);
        probe.set_sink(Arc::clone(&sink) as Arc<dyn OperationEventSink>);
        let mut event = WriteProgressEvent::new(
            guard.id().to_owned(),
            WriteOperationType::Copy,
            WriteOperationPhase::Copying,
            None,
            5,
            764,
            83_650_000,
            900_000_000,
        );
        state.enrich_progress(&mut event);
        probe.record_progress(&event);

        let mut watchdog = WatchdogState::new();
        // Bytes keep moving on every tick.
        for tick in 1..=(HEARTBEAT_AFTER_SECS + 5) {
            probe.bytes_done.fetch_add(1_000, Ordering::Relaxed);
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
}
