//! What every in-flight transfer task is doing right now, and a watchdog that
//! says so when a transfer stops moving.
//!
//! A wedged transfer parks its driver and its tasks on `.await`, so no thread
//! carries a transfer frame and a stack sample sees nothing (that is exactly how
//! the 2026-07-31 incident resisted diagnosis; see
//! `docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). The only way to
//! learn where a parked async task is stuck is for it to say so on the way in, so
//! every phase transition records itself here and the watchdog prints the table.
//! The watchdog itself (its tick, heartbeat, dump cadence, and the gated abort)
//! lives in the `watchdog` child, `transfer_probe_watchdog.rs`.
//!
//! **Cost.** A phase transition is one relaxed atomic store, and per-chunk byte
//! progress is one more; nothing on the hot path takes a lock. The per-operation
//! registry lock is touched only when a task starts or finishes, and by the
//! watchdog tick, which also reads the operation's newest published byte total
//! once a second.
//!
//! **Reaching the probe.** A copy task's body runs inside
//! [`CURRENT_TASK_PROBE`]`.scope(...)`, so code arbitrarily deep inside it
//! (`copy_single_path` → `stream_pipe_file` → `CheckpointStream`) reaches its own
//! probe with no signature threading. Outside a copy task the lookup simply
//! misses and every call is a no-op, which is what the unit tests and the
//! local-FS path rely on.

use crate::rt::Instant;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;

use super::super::event_sinks::OperationEventSink;
use super::super::state::{OperationIntent, WriteOperationState};
use super::super::types::{TransferActivity, TransferWaitReason, WriteProgressEvent};

/// The stall watchdog that reads this table once a second, and the constants
/// that set its cadence and its patience. Re-exported so callers keep naming
/// them as `transfer_probe::<item>`.
#[path = "transfer_probe_watchdog.rs"]
mod watchdog;

#[cfg(test)]
pub(super) use watchdog::StallAbortGuard;
#[cfg(test)]
use watchdog::{HEARTBEAT_AFTER_SECS, STALL_ABORT_AFTER, WatchdogState};
use watchdog::{spawn_watchdog, stall_abort_after};

/// What a row and a driver can SAY: [`TaskPhase`], [`DriverPhase`], [`TaskRole`],
/// and [`TaskRow`]. Kept apart from the live table below because they are pure
/// values, and re-exported here so every caller still names them as
/// `transfer_probe::<item>`.
#[path = "transfer_probe_vocabulary.rs"]
mod vocabulary;

pub(super) use vocabulary::{DriverPhase, TaskPhase, TaskRole, TaskRow};

/// What the probe asks the transfer's two volumes about their connections
/// ([`TransferEnds`]), and the receive rate it derives from the answers.
#[path = "transfer_probe_ends.rs"]
mod ends;

use ends::InboundWindow;
pub(super) use ends::TransferEnds;

/// One in-flight copy task's live state.
pub(super) struct TaskProbe {
    /// Where this row sits in the operation, so a dump can be read against the
    /// spawn log and against the driver's phase detail.
    row: TaskRow,
    /// Whether this row holds one of the window's slots. Fixed for the row's
    /// life: a walker never becomes a file copy or the reverse.
    role: TaskRole,
    source: PathBuf,
    dest: PathBuf,
    phase: AtomicU8,
    /// Millis since the operation started, at the last phase transition.
    phase_since_ms: AtomicU64,
    bytes_done: AtomicU64,
    total_bytes: AtomicU64,
    /// How many times this task's current file has been run again after a
    /// transport blip (`retry.rs`). Rendered in every dump so a log reader can
    /// tell "this file was retried twice and then succeeded" from "this file
    /// silently vanished".
    retries: AtomicU64,
    /// The watchdog's private carry-over for this task: the byte count it saw
    /// last tick, and the tick at which this task's own counter last moved.
    /// Written only by the watchdog (one writer), read by the dump.
    watchdog_last_bytes: AtomicU64,
    still_since_ms: AtomicU64,
    /// Tripped by the watchdog when this task has been inside a backend call with
    /// zero byte movement for `OperationProbe::stall_abort_after`. The streaming
    /// write races it, so a wedge that the backend's own deadlines never bound
    /// still turns into a typed error and a retry instead of an endless park.
    ///
    /// Re-armed with a FRESH token per write attempt ([`TaskProbe::arm_stall_abort`]),
    /// because one task copies one top-level source — which for a directory is
    /// many files, each with its own attempts. A token that stayed tripped would
    /// abort every subsequent write in the subtree instantly and turn the retry
    /// budget into three no-ops.
    stall_abort: Mutex<CancellationToken>,
    /// How many times the watchdog has ended this task's wait. Sticky (unlike the
    /// token) so a dump taken later still records it.
    stall_aborts: AtomicU64,
    started: Instant,
}

impl TaskProbe {
    /// Record a phase transition. One relaxed store plus a timestamp; safe to
    /// call on any path, including per-chunk.
    pub(super) fn set_phase(&self, phase: TaskPhase) {
        let now_ms = u64::try_from(crate::rt::since(self.started).as_millis()).unwrap_or(u64::MAX);
        self.phase.store(phase as u8, Ordering::Relaxed);
        self.phase_since_ms.store(now_ms, Ordering::Relaxed);
    }

    /// Per-chunk byte progress. Deliberately a plain store, not an add: the
    /// caller owns the running total for its own file.
    pub(super) fn set_bytes(&self, bytes_done: u64, total_bytes: u64) {
        self.bytes_done.store(bytes_done, Ordering::Relaxed);
        self.total_bytes.store(total_bytes, Ordering::Relaxed);
    }

    /// This task's file is being run again after a transport blip.
    pub(super) fn note_retry(&self) {
        self.retries.fetch_add(1, Ordering::Relaxed);
    }

    /// Arms a FRESH stall-abort signal for the write attempt about to start and
    /// hands it to the caller to race.
    ///
    /// Also restarts this task's stillness clock, because the attempt starts at
    /// byte zero: without that, an attempt the watchdog just ended (zero bytes
    /// moved, so the counter doesn't change) would hand its exhausted budget
    /// straight to the next attempt and abort it on the first tick.
    pub(super) fn arm_stall_abort(&self) -> CancellationToken {
        let token = CancellationToken::new();
        *self.stall_abort.lock_ignore_poison() = token.clone();
        // `u64::MAX` is not a reachable byte count, so the next watchdog tick
        // reads this task as having just moved and re-seeds from there.
        self.watchdog_last_bytes.store(u64::MAX, Ordering::Relaxed);
        self.still_since_ms.store(
            u64::try_from(crate::rt::since(self.started).as_millis()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        token
    }

    /// End this task's wait. Trips whichever token the current attempt armed.
    fn trip_stall_abort(&self) {
        self.stall_aborts.fetch_add(1, Ordering::Relaxed);
        self.stall_abort.lock_ignore_poison().cancel();
    }

    /// Is the attempt currently in flight one the watchdog has already ended?
    fn is_stall_aborted(&self) -> bool {
        self.stall_abort.lock_ignore_poison().is_cancelled()
    }

    fn render(&self) -> String {
        let phase = TaskPhase::from_u8(self.phase.load(Ordering::Relaxed));
        let since_ms = self.phase_since_ms.load(Ordering::Relaxed);
        let now_ms = u64::try_from(crate::rt::since(self.started).as_millis()).unwrap_or(u64::MAX);
        let total = self.total_bytes.load(Ordering::Relaxed);
        let stall_aborts = self.stall_aborts.load(Ordering::Relaxed);
        // allowed-log-private: a dump row is log text, and the dump only ever reaches a log macro
        format!(
            // allowed-pluralize-noun: a byte count in a diagnostic dump; the compact form is the point.
            "{row}{role} {phase} for {held}ms, {done}/{total} bytes, retries={retries}{aborted}, {source} -> {dest}",
            row = self.row.label(),
            // Named here too, so the header's `walkers=` can be read back against the table.
            role = match self.role {
                TaskRole::File => "",
                TaskRole::Walker => " (walker)",
            },
            phase = phase.label(),
            held = now_ms.saturating_sub(since_ms),
            done = self.bytes_done.load(Ordering::Relaxed),
            total = total,
            retries = self.retries.load(Ordering::Relaxed),
            aborted = if stall_aborts > 0 {
                format!(" stall-aborts={stall_aborts}")
            } else {
                String::new()
            },
            source = LogPrivate::path(&self.source),
            dest = LogPrivate::path(&self.dest),
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
    /// What the driver's current step is about (the path it is pre-checking,
    /// typically). Written only at phase transitions.
    driver_detail: Mutex<DriverDetail>,
    tasks: Mutex<Vec<Arc<TaskProbe>>>,
    /// Where to send a heartbeat. Set once at registration; `None` in the unit
    /// tests that don't exercise emission.
    sink: Mutex<Option<Arc<dyn OperationEventSink>>>,
    /// Whole seconds the aggregate byte counter has been still, maintained by
    /// the watchdog at `STALL_TICK` granularity and reset on every movement and
    /// on pause. Read by [`OperationProbe::activity`] on the progress path, so
    /// the UI and the log agree by construction rather than by review.
    still_for_seconds: AtomicU64,
    /// How long one task may sit inside a backend call with no byte movement
    /// before the watchdog ends its wait. Per-operation rather than a bare
    /// constant read so a test can shorten it (see [`stall_abort_after`]).
    stall_abort_after: Duration,
    /// The operation's source and destination volumes, held ONLY to ask their
    /// connections whether either has been proven dead (the gate on the one
    /// aggressive thing the watchdog does; see
    /// [`OperationProbe::connection_proven_dead`]) and how fast the source is
    /// receiving while nothing lands.
    ends: TransferEnds,
    /// The source connection's receive rate the watchdog measured on its last
    /// still tick, in bytes per second, or [`NO_INBOUND_RATE`]. Published the
    /// same way `still_for_seconds` is, so [`OperationProbe::activity`] reads one
    /// number on the progress path without touching a volume.
    source_inbound_rate: AtomicU64,
    state: Arc<WriteOperationState>,
    started: Instant,
}

/// `source_inbound_rate`'s "nothing to report". No link moves 2^64 bytes a
/// second, so the value can't collide with a reading.
const NO_INBOUND_RATE: u64 = u64::MAX;

impl OperationProbe {
    /// The operation's aggregate byte total, read straight off the newest
    /// progress event it published — the number in the dialog, whichever driver
    /// produced it.
    ///
    /// ❌ Never give the probe a byte counter of its own again. It had one: an
    /// `Arc<AtomicU64>` the CONCURRENT copy path fed and the serial path did
    /// not, so every transfer of one or two top-level sources (and every MTP
    /// transfer, which is always serial) read a counter frozen at zero and was
    /// declared stalled within 20 s while its bar climbed normally. A counter
    /// each driver has to remember to feed is a counter the next driver
    /// forgets; this reads the one thing every emit site is already required to
    /// go through (`WriteOperationState::enrich_progress`).
    ///
    /// Zero before the first progress event, which no registered operation can
    /// observe: `copy_volumes_with_progress` emits its opening `Copying` tick
    /// before it registers here.
    fn bytes_done(&self) -> u64 {
        self.state.last_progress_bytes().unwrap_or(0)
    }

    /// The file count the UI is showing, from the same event `bytes_done` comes
    /// from. See [`WriteOperationState::last_progress_files`]; the probe keeps no
    /// counter of its own here either, for the reason above.
    fn files_done(&self) -> usize {
        self.state.last_progress_files().unwrap_or(0)
    }

    /// The driver's step, named by a fixed label of our own.
    pub(super) fn set_driver_phase(&self, phase: DriverPhase, label: &'static str) {
        self.driver_phase.store(phase as u8, Ordering::Relaxed);
        *self.driver_detail.lock_ignore_poison() = DriverDetail::Label(label);
    }

    /// The driver's step, at `path` (under `row`'s label when the step belongs
    /// to one source, so a reader can match it against the table).
    pub(super) fn set_driver_phase_at(&self, phase: DriverPhase, row: Option<TaskRow>, path: &Path) {
        self.driver_phase.store(phase as u8, Ordering::Relaxed);
        *self.driver_detail.lock_ignore_poison() = DriverDetail::At {
            row,
            path: path.to_path_buf(),
        };
    }

    /// Register a task entering the window. The returned handle removes it on
    /// drop, so a task that panics or is aborted still leaves the table clean.
    ///
    /// `row` says where the row sits in the operation ([`TaskRow`]) and is what
    /// the dump renders it under; `role` says whether it holds one of the
    /// window's slots ([`TaskRole`]); a DIRECTORY source is always a walker and
    /// a file copy is always a `File`, so it is `source_is_dir` at every real
    /// call site.
    pub(super) fn begin_task(
        self: &Arc<Self>,
        row: TaskRow,
        role: TaskRole,
        source: &(impl AsRef<Path> + ?Sized),
        dest: &(impl AsRef<Path> + ?Sized),
    ) -> TaskProbeHandle {
        let probe = Arc::new(TaskProbe {
            row,
            role,
            source: source.as_ref().to_path_buf(),
            dest: dest.as_ref().to_path_buf(),
            phase: AtomicU8::new(TaskPhase::Spawned as u8),
            phase_since_ms: AtomicU64::new(
                u64::try_from(crate::rt::since(self.started).as_millis()).unwrap_or(u64::MAX),
            ),
            bytes_done: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            retries: AtomicU64::new(0),
            // `u64::MAX` can't be a real byte count, so the first watchdog tick
            // that sees this task always reads as "it just moved" and starts its
            // stillness clock from there rather than from the operation's start.
            watchdog_last_bytes: AtomicU64::new(u64::MAX),
            still_since_ms: AtomicU64::new(0),
            stall_abort: Mutex::new(CancellationToken::new()),
            stall_aborts: AtomicU64::new(0),
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

    fn inbound_rate(&self) -> Option<u64> {
        match self.source_inbound_rate.load(Ordering::Relaxed) {
            NO_INBOUND_RATE => None,
            rate => Some(rate),
        }
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
        let still_for_seconds = if self.state.pause_gate.is_paused() || self.awaiting_human() {
            0
        } else {
            self.still_for_seconds.load(Ordering::Relaxed)
        };
        let in_flight = u32::try_from(self.tasks.lock_ignore_poison().len()).unwrap_or(u32::MAX);
        let waiting_on = self.wait_reason(still_for_seconds);
        // A pause and a prompt are deliberate, and they say so themselves: an
        // opening or a trickle under them isn't the operation's news.
        // A volume that's gone is the same: what the transfer is waiting on is
        // known, and an opening or a receive rate under it is noise.
        let deliberate = matches!(
            waiting_on,
            TransferWaitReason::Paused | TransferWaitReason::Conflict | TransferWaitReason::Reconnecting
        );
        TransferActivity {
            in_flight,
            still_for_seconds: u32::try_from(still_for_seconds).unwrap_or(u32::MAX),
            waiting_on,
            opening_source: !deliberate && self.opening_first_source(),
            source_inbound_bytes_per_second: if deliberate { None } else { self.inbound_rate() },
            reconnecting_volume: self.state.reconnecting_volume(waiting_on),
        }
    }

    /// Has nothing landed yet, with every file row in flight still opening its
    /// source? Walker rows don't count either way: a walk lists while its leaves
    /// open, and the leaves are what the first byte is waiting on.
    fn opening_first_source(&self) -> bool {
        if self.bytes_done() != 0 {
            return false;
        }
        let tasks = self.tasks.lock_ignore_poison();
        let mut files = tasks.iter().filter(|t| t.role == TaskRole::File).peekable();
        files.peek().is_some()
            && files.all(|t| TaskPhase::from_u8(t.phase.load(Ordering::Relaxed)) == TaskPhase::OpeningSource)
    }

    /// [`activity`](Self::activity) for a progress event about to go out.
    ///
    /// The stillness and the receive rate are the watchdog's readings from its
    /// last tick, up to a second old. An event whose counters differ from the
    /// last one published IS movement, so it must not go out carrying a
    /// stillness or a rate from before it: a chunk that lands mid-tick would
    /// otherwise reach the UI still saying "no progress for 12s", and on a
    /// transfer that emits once per chunk, keep saying it until the next one.
    fn activity_for_event(&self, event: &WriteProgressEvent) -> TransferActivity {
        let moved = Some(event.bytes_done) != self.state.last_progress_bytes()
            || Some(event.files_done) != self.state.last_progress_files();
        let activity = self.activity();
        if !moved || activity.still_for_seconds == 0 {
            return activity;
        }
        let waiting_on = self.wait_reason(0);
        TransferActivity {
            still_for_seconds: 0,
            waiting_on,
            opening_source: false,
            source_inbound_bytes_per_second: None,
            reconnecting_volume: self.state.reconnecting_volume(waiting_on),
            ..activity
        }
    }

    /// Is a conflict prompt outstanding, i.e. is the app waiting on a person?
    ///
    /// The conflict slot holds the responder while a `write-conflict` is
    /// unanswered (armed BEFORE the emit, spent when the answer lands), so it
    /// is exactly "a human is being asked" for both the top-level dispatch and
    /// deep-merge children.
    fn awaiting_human(&self) -> bool {
        self.state.conflict_slot.is_awaiting()
    }

    /// Classify the wait. Order matters: a pause and a conflict prompt are
    /// deliberate and outrank any device wait, and while bytes move nothing is
    /// waiting on anything (some task is always between chunks).
    fn wait_reason(&self, still_for_seconds: u64) -> TransferWaitReason {
        if self.state.pause_gate.is_paused() {
            return TransferWaitReason::Paused;
        }
        // A TOP-LEVEL conflict prompt is resolved on the DRIVER, not inside a
        // task, so no task carries `ResolvingConflict` for it and the task scan
        // below would miss it entirely. The outstanding oneshot sender is the
        // authoritative signal, and it covers deep-merge prompts too.
        if self.awaiting_human() {
            return TransferWaitReason::Conflict;
        }
        // A volume that went away explains every still task at once, and the
        // operation can't move until it's back, whatever any one task says.
        if self.state.outage.waiting_for().is_some() {
            return TransferWaitReason::Reconnecting;
        }
        let tasks = self.tasks.lock_ignore_poison();
        let reasons: Vec<TransferWaitReason> = tasks
            .iter()
            .filter_map(|t| TaskPhase::from_u8(t.phase.load(Ordering::Relaxed)).wait_reason())
            .collect();
        // A person being asked a question beats any device wait: the transfer
        // isn't stuck, it's waiting for an answer, and the UI says so even
        // while other tasks keep streaming.
        if reasons.contains(&TransferWaitReason::Conflict) {
            return TransferWaitReason::Conflict;
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
    /// prints it when it abandons tasks that wouldn't stop after a cancel or a
    /// failure.
    pub(super) fn render_dump(&self, reason: &str) -> String {
        let tasks = self.tasks.lock_ignore_poison();
        let driver = DriverPhase::from_u8(self.driver_phase.load(Ordering::Relaxed));
        let intent = match self.state.intent().load() {
            // The intent stays `Running` through the failure drain; only the
            // stop-work token says the work stopped.
            OperationIntent::Running if self.state.work_stopped() => "running(draining-after-failure)",
            OperationIntent::Running => "running",
            OperationIntent::RollingBack => "rolling-back",
            OperationIntent::Stopped => "stopped",
        };
        // Only the file rows take a window slot, so only they may be measured against
        // it: counting the walkers too rendered a healthy transfer as `in_flight=11/10`.
        let walkers = tasks.iter().filter(|t| t.role == TaskRole::Walker).count();
        let mut out = format!(
            "transfer probe ({reason}): op={op} elapsed={elapsed}s bytes_done={bytes} files_total={files} \
             driver={driver}({detail}) intent={intent} paused={paused} in_flight={in_flight}/{concurrency}{walkers}{inbound}",
            op = self.operation_id,
            elapsed = crate::rt::since(self.started).as_secs(),
            bytes = self.bytes_done(),
            files = self.total_files,
            driver = driver.label(),
            detail = self.driver_detail.lock_ignore_poison(),
            paused = self.state.pause_gate.is_paused(),
            in_flight = tasks.len() - walkers,
            concurrency = self.concurrency,
            walkers = if walkers > 0 {
                format!(" walkers={walkers}")
            } else {
                String::new()
            },
            // Whether the source is still sending while nothing lands: a slow
            // response on its way, not a wedge. Connection-wide.
            inbound = self
                .inbound_rate()
                .map_or_else(String::new, |rate| format!(" source_inbound={rate}B/s")),
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

/// What the driver's current step is about, as the dump renders it. A path is
/// marked there: the dump reaches error reports (the stall warning, the
/// wind-down report), and a raw path in prose is what they can't bound.
pub(super) enum DriverDetail {
    Label(&'static str),
    At { row: Option<TaskRow>, path: PathBuf },
}

impl fmt::Display for DriverDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Label(label) => f.write_str(label),
            Self::At { row: Some(row), path } => write!(f, "{} {}", row.label(), LogPrivate::path(path)),
            Self::At { row: None, path } => write!(f, "{}", LogPrivate::path(path)),
        }
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
    /// `volume::copy`'s task body; read by anything nested inside it.
    pub(super) static CURRENT_TASK_PROBE: Arc<TaskProbe>;
}

/// Set the current copy task's phase, if there is one.
///
/// A no-op outside a copy task (unit tests, the local-FS path), so callers never
/// need to know whether they are being driven by the volume copy driver.
pub(super) fn set_task_phase(phase: TaskPhase) {
    let _ = CURRENT_TASK_PROBE.try_with(|probe| probe.set_phase(phase));
}

/// The current copy task's phase, if there is one: what a park that sets its
/// own phase restores when it ends (`volume_outage.rs`).
pub(super) fn current_task_phase() -> Option<TaskPhase> {
    CURRENT_TASK_PROBE
        .try_with(|probe| TaskPhase::from_u8(probe.phase.load(Ordering::Relaxed)))
        .ok()
}

/// Report per-chunk byte progress for the current copy task, if there is one.
pub(super) fn set_task_bytes(bytes_done: u64, total_bytes: u64) {
    let _ = CURRENT_TASK_PROBE.try_with(|probe| probe.set_bytes(bytes_done, total_bytes));
}

/// Record that the current copy task is running its file again after a transport
/// blip, so the dump can say how many attempts a file took.
pub(super) fn note_task_retry() {
    let _ = CURRENT_TASK_PROBE.try_with(|probe| probe.note_retry());
}

/// Arms a fresh stall-abort signal for the write attempt about to start, if this
/// is running inside a copy task.
///
/// `None` outside one (unit tests, the local-FS path), where nothing watches and
/// the write simply awaits as before.
pub(super) fn arm_current_task_stall_abort() -> Option<CancellationToken> {
    CURRENT_TASK_PROBE.try_with(|probe| probe.arm_stall_abort()).ok()
}

#[path = "transfer_probe_registry.rs"]
mod registry;
pub(crate) use registry::Probes;
pub(super) use registry::{OperationProbeGuard, register_operation};

#[cfg(test)]
#[path = "transfer_probe_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "transfer_probe_opening_tests.rs"]
mod opening_tests;
