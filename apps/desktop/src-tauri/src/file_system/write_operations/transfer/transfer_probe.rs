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
//! watchdog tick, which also reads the operation's newest published byte total
//! once a second.
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

use tokio_util::sync::CancellationToken;

use crate::file_system::volume::{ConnectionLiveness, Volume};
use crate::ignore_poison::IgnorePoison;

use super::super::event_sinks::OperationEventSink;
use super::super::state::{OperationIntent, WriteOperationState, load_intent};
use super::super::types::{TransferActivity, TransferWaitReason, WriteOperationPhase};

/// How often the watchdog samples an operation, and how long a transfer may
/// show zero byte movement before it is called out IN THE LOG.
///
/// The tick also sets the granularity of `TransferActivity::still_for_seconds`,
/// which the UI reads to decide when to stop showing a confident ETA — hence
/// 1 s rather than something coarser. It's one wakeup per second per running
/// transfer, reading one already-published number.
///
/// 20 s for the log sits well clear of a slow-but-alive SMB write window. The
/// UI speaks sooner (see `STALL_NOTICE_SECONDS` in
/// `transfer/transfer-stall.ts`); a log line wants to stay rare, while a frozen
/// bar with a confident ETA is a lie the moment it stops being true.
pub(super) const STALL_TICK: Duration = Duration::from_secs(1);
pub(super) const STALL_AFTER: Duration = Duration::from_secs(20);

/// How long a task may sit inside a backend call with ZERO byte movement before
/// the watchdog stops reporting and ends the wait itself.
///
/// This is the layer of last resort, and its length says so. Every backend that
/// can bound its own waits already does: on `smb2` a request fails after
/// `RESPONSE_TIMEOUT` (30 s) of silence, measured from the moment the request
/// REGISTERS, so a dead SMB session surfaces as a typed error on its own and the
/// file's retry picks it up without the watchdog ever being involved. That
/// deadline stretches to `ALIVE_DEADLINE_FACTOR` × 30 s = 180 s on a connection
/// an ECHO has just proven alive, which is this constant exactly: against a
/// server that answers ECHO while one operation is wedged, the two clocks tie
/// rather than SMB's arriving first.
///
/// ❗ `SEND_TIMEOUT` (20 s) is NOT a second, tighter bound, and reading it as
/// one is what a wedge investigation gets wrong. It wraps `sender.send(...)`
/// alone — the socket write AFTER `writer_loop` has dequeued the frame — so time
/// spent in the send queue behind an earlier stuck frame is measured
/// (`queued_for`) and bounded by nothing on that path. A user's bundle caught a
/// frame `registered 69.997358458s ago and NOT YET ON THE WIRE` with no
/// `SendTimeout` anywhere in the log; what bounds such a frame is the response
/// deadline above, which runs from registration. (Verified against `smb2`
/// 0.18.1, `client/connection.rs` `writer_loop` and `await_response`,
/// 2026-08-26.)
///
/// What is left for this constant is the case that has no deadline anywhere — an
/// OS-mounted share, a USB stack, a future backend that forgot — which is
/// precisely the shape that cost a user two files and a force-quit on
/// 2026-07-31.
///
/// 180 s, because the number has to clear the slowest HEALTHY thing that can
/// happen between two byte reports. That is one chunk: a 1 MiB SMB read window
/// needs a link under 6 KB/s to take this long, and an 8 MiB MTP window needs USB
/// to run at 45 KB/s. Neither is a transfer anyone is waiting on. ❌ Don't tighten
/// this toward a plausible slow link: killing a healthy transfer to catch a wedge
/// sooner is the trade this gate exists to refuse (`DETAILS.md`
/// § "The watchdog ACTS").
pub(super) const STALL_ABORT_AFTER: Duration = Duration::from_secs(180);

/// The stall-abort window in force, honoring a test override.
///
/// Read once per operation at registration, on the CALLER's thread (the copy
/// driver's), because the watchdog itself runs on the app runtime where a
/// thread-local override would not be visible.
fn stall_abort_after() -> Duration {
    #[cfg(test)]
    if let Some(d) = stall_abort_override() {
        return d;
    }
    STALL_ABORT_AFTER
}

#[cfg(test)]
thread_local! {
    /// Per-test override of [`STALL_ABORT_AFTER`], read when an operation
    /// registers. `None` ⇒ the production constant. Set through
    /// [`StallAbortGuard`].
    static STALL_ABORT_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn stall_abort_override() -> Option<Duration> {
    STALL_ABORT_OVERRIDE.with(std::cell::Cell::get)
}

/// Shortens the stall-abort window for the current thread, restoring it on drop,
/// so a suite can watch the watchdog end a wedge without waiting out three
/// minutes. Mirrors `volume::copy::wedge_test_support::CancelDrainGuard`.
#[cfg(test)]
pub(super) struct StallAbortGuard {
    prev: Option<Duration>,
}

#[cfg(test)]
impl StallAbortGuard {
    pub(super) fn set(window: Duration) -> Self {
        Self {
            prev: STALL_ABORT_OVERRIDE.with(|c| c.replace(Some(window))),
        }
    }
}

#[cfg(test)]
impl Drop for StallAbortGuard {
    fn drop(&mut self) {
        STALL_ABORT_OVERRIDE.with(|c| c.set(self.prev));
    }
}

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
    /// Between attempts at the same file: a transport blip took the last one out
    /// and the backoff is running (`retry.rs`).
    WaitingToRetry = 8,
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
            Self::WaitingToRetry => "waiting-to-retry",
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
            Self::ResolvingConflict => Some(TransferWaitReason::Conflict),
            Self::Spawned
            | Self::OpeningSource
            | Self::Streaming
            | Self::ParkedPause
            | Self::Finalizing
            // A backoff is our own doing, not a wait on a device or a person, and
            // it is over in a second or less. The dump names the phase; the UI
            // keeps whatever reason the stall itself produced.
            | Self::WaitingToRetry => None,
        }
    }

    /// May the watchdog abort a task sitting in this phase when nothing has moved
    /// for a very long time?
    ///
    /// Only the two phases that mean "inside a backend call, waiting on the wire".
    /// Every park is deliberate and self-limiting — a pause ends when the user
    /// resumes, a yield when foreground drains (and the destination yield is
    /// hard-capped), a conflict when the human answers, a retry backoff on its own
    /// timer — so aborting one would break something that was working as designed.
    const fn is_abortable_on_stall(self) -> bool {
        matches!(self, Self::OpeningSource | Self::Streaming)
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
            8 => Self::WaitingToRetry,
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
            u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
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
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let total = self.total_bytes.load(Ordering::Relaxed);
        let stall_aborts = self.stall_aborts.load(Ordering::Relaxed);
        format!(
            // allowed-pluralize-noun: a byte count in a diagnostic dump; the compact form is the point.
            "#{idx} {phase} for {held}ms, {done}/{total} bytes, retries={retries}{aborted}, {source} -> {dest}",
            idx = self.index,
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
    /// The operation's source and destination volumes, held ONLY to ask them
    /// whether their connection has been proven dead
    /// ([`Volume::connection_liveness`]). That verdict is the gate on the one
    /// aggressive thing the watchdog does; see [`OperationProbe::connection_proven_dead`].
    volumes: Vec<Arc<dyn Volume>>,
    state: Arc<WriteOperationState>,
    started: Instant,
}

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

    /// One watchdog tick, split out from the timer loop so it can be tested
    /// without waiting on wall-clock seconds. `still_for` is how long the byte
    /// counter has been unchanged.
    fn watchdog_step(&self, watchdog: &mut WatchdogState, now: Duration) {
        // A transfer waiting on a person is not stalled, and must not accrue
        // stall time or heartbeat at the UI while the conflict dialog is open.
        // Same for a pause: it moves no bytes on purpose. Both restart the
        // per-task clocks too, so the time a task spent parked can't be counted
        // toward an abort the moment the operation resumes.
        if self.awaiting_human() || self.state.pause_gate.is_paused() {
            watchdog.still_since = now;
            self.still_for_seconds.store(0, Ordering::Relaxed);
            self.restart_task_stillness(now);
            return;
        }
        self.track_and_abort_wedged_tasks(now);
        let bytes = self.bytes_done();
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

    /// Has either end of this transfer been PROVEN dead, as opposed to merely
    /// slow to answer?
    ///
    /// One of the TWO conditions on the watchdog's aggressive action; see
    /// [`OperationProbe::track_and_abort_wedged_tasks`] for why this one alone is
    /// never enough. `Volume::connection_liveness` answers `None` for every
    /// backend in this workspace, so this is `false` in production and the
    /// watchdog reports without acting. `smb2` 0.16.0's ECHO keepalive doesn't
    /// change that: a missed probe is deliberately not a death verdict (a busy
    /// NAS drops probes), and the sound verdict it does produce arrives as an
    /// error that has already torn the connection down and failed every waiter.
    /// ❌ Don't substitute elapsed silence, a missed probe, or a slow response
    /// for this answer; that is the failure mode a keepalive exists to prevent,
    /// and it kills healthy slow transfers.
    ///
    /// **To turn the teeth on**: `smb2` must first expose its own
    /// "quiet past the liveness window with work outstanding" reading as
    /// pollable state; then override `connection_liveness` on `SmbVolume`
    /// against it. Nothing here changes. Full reasoning: `DETAILS.md`
    /// § "The watchdog ACTS".
    fn connection_proven_dead(&self) -> bool {
        self.volumes
            .iter()
            .any(|v| v.connection_liveness() == Some(ConnectionLiveness::Dead))
    }

    /// Restart every in-flight task's stillness clock, for the ticks where the
    /// whole operation is standing still on purpose (paused, or waiting on an
    /// answer). Without this a task would carry the pause into its abort budget.
    fn restart_task_stillness(&self, now: Duration) {
        let now_ms = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
        for task in self.tasks.lock_ignore_poison().iter() {
            task.watchdog_last_bytes
                .store(task.bytes_done.load(Ordering::Relaxed), Ordering::Relaxed);
            task.still_since_ms.store(now_ms, Ordering::Relaxed);
        }
    }

    /// The watchdog ACTING rather than only reporting: track each in-flight task's
    /// own byte counter and, for one that has not moved a byte inside a backend
    /// call for [`OperationProbe::stall_abort_after`], trip its abort signal.
    ///
    /// The streaming write races that signal, so tripping it turns an unbounded
    /// park into a typed error, which the file's retry policy then treats as the
    /// transport blip it is. That is the whole difference between the dialog
    /// saying "stalled" forever and the transfer getting itself unstuck.
    ///
    /// Four guards keep this from touching anything healthy, and the FIRST is the
    /// one that matters:
    /// - **Proof that the connection is DEAD**
    ///   ([`OperationProbe::connection_proven_dead`]) **AND** the stillness
    ///   window, never either alone. Elapsed silence is not proof of death: a
    ///   large write to a loaded spinning-disk NAS is legitimately slow, and
    ///   killing it trades a rare wedge for frequent spurious failures.
    ///   **No backend answers the liveness question**, so in production this
    ///   method reports and never acts — the correct behavior until `smb2`
    ///   exposes a verdict a consumer can poll before the connection is already
    ///   torn down.
    /// - **Per-task, not per-operation.** A batch where any task is still moving
    ///   bytes leaves every other task's clock running on its own merits; only a
    ///   task that has itself gone quiet is a candidate.
    /// - **Only the two phases that mean "inside a backend call"**
    ///   ([`TaskPhase::is_abortable_on_stall`]). Every park is deliberate and ends
    ///   on its own.
    /// - **Never while cancelling.** Cancel and rollback own their own teardown
    ///   (the driver's drain deadline); a second abort path racing them would just
    ///   make the wind-down harder to reason about.
    fn track_and_abort_wedged_tasks(&self, now: Duration) {
        let now_ms = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
        let cancelling = !matches!(load_intent(&self.state.intent), OperationIntent::Running);
        // ❌ NEVER collapse this conjunction to "trust the liveness verdict".
        //
        // The verdict it reads is a keepalive result, and a keepalive
        // FALSE-POSITIVES under exactly the conditions a transfer creates.
        // Measured against a QNAP TS-464 (2026-08-02, smb2's live-hardware
        // suite): under heavy write load an ECHO probe reported `2 answered, 1
        // unanswered` — a `Dead` verdict on a NAS that was demonstrably fine —
        // while five consecutive runs on the same idle box reported `0
        // unanswered`. Acting on that alone would kill healthy transfers to
        // busy servers, which is the whole failure mode this gate exists to
        // prevent, one layer up.
        //
        // The stillness window is what makes the pair sound: a NAS that drops a
        // probe because it is busy writing has not ALSO moved zero bytes for
        // `stall_abort_after`. Each condition covers the other's false positive,
        // so both are load-bearing and neither is belt-and-braces.
        //
        // Without a verdict the loop below only maintains each task's clock,
        // which is what keeps the dump and the UI honest while nothing escalates.
        let proven_dead = self.connection_proven_dead();
        let abort_after_ms = u64::try_from(self.stall_abort_after.as_millis()).unwrap_or(u64::MAX);
        for task in self.tasks.lock_ignore_poison().iter() {
            let bytes = task.bytes_done.load(Ordering::Relaxed);
            if task.watchdog_last_bytes.swap(bytes, Ordering::Relaxed) != bytes {
                task.still_since_ms.store(now_ms, Ordering::Relaxed);
                continue;
            }
            if cancelling || task.is_stall_aborted() {
                continue;
            }
            let phase = TaskPhase::from_u8(task.phase.load(Ordering::Relaxed));
            if !phase.is_abortable_on_stall() {
                // A deliberate park restarts the clock, so the seconds it spent
                // parked never count toward the abort budget once it resumes.
                task.still_since_ms.store(now_ms, Ordering::Relaxed);
                continue;
            }
            let still_for_ms = now_ms.saturating_sub(task.still_since_ms.load(Ordering::Relaxed));
            if still_for_ms < abort_after_ms || !proven_dead {
                continue;
            }
            crate::log_error!(
                target: "copy",
                "transfer probe: op={op} ending the wait on a task that has moved no bytes for {secs}s. \
                 The write is abandoned and the file runs again if it has attempts left.\n  {task}",
                op = self.operation_id,
                secs = still_for_ms / 1_000,
                task = task.render(),
            );
            task.trip_stall_abort();
        }
    }

    /// Re-emit the last progress event with a fresh activity snapshot. The
    /// counters are unchanged (nothing moved, and saying otherwise would be a
    /// lie); only `activity` and the decaying rate/ETA are new.
    fn emit_heartbeat(&self) {
        let Some(sink) = self.sink.lock_ignore_poison().clone() else {
            return;
        };
        // The operation keeps the newest tick (`WriteOperationState::last_progress`);
        // the phases where a stall is meaningful are this caller's business. A
        // scan emits its own steady stream, and the finishing phases are brief.
        let Some(mut event) = self
            .state
            .last_progress()
            .filter(|e| matches!(e.phase, WriteOperationPhase::Copying | WriteOperationPhase::Flushing))
        else {
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
        let still_for_seconds = if self.state.pause_gate.is_paused() || self.awaiting_human() {
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
            bytes = self.bytes_done(),
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
    // Source and destination, held only so the watchdog can ask them whether
    // their connection is proven dead before it acts on a stall.
    volumes: Vec<Arc<dyn Volume>>,
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
        sink: Mutex::new(Some(sink)),
        still_for_seconds: AtomicU64::new(0),
        stall_abort_after: stall_abort_after(),
        volumes,
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

/// The in-flight table of a still-running operation, rendered.
///
/// The watchdog logs the same text once an operation has been still for
/// `STALL_AFTER`, which is right for a user's session and useless to a test: a
/// test that bounds its own wait needs the table AT the moment its deadline
/// expires, in its panic message, where a human will actually read it. The SMB
/// full-concurrency suite calls this before it abandons a copy that overran, so
/// a red run names the phase every task was parked in instead of only saying
/// "timed out".
///
/// `None` once the operation has settled (its guard deregistered it), so the
/// caller must keep the copy alive — awaiting a `JoinHandle`, not the copy
/// future itself, which would drop the guard before the dump could be taken.
#[cfg(test)]
pub(crate) fn render_live_dump(operation_id: &str, reason: &str) -> Option<String> {
    REGISTRY
        .lock_ignore_poison()
        .get(operation_id)
        .map(|probe| probe.render_dump(reason))
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
#[path = "transfer_probe_tests.rs"]
mod tests;
