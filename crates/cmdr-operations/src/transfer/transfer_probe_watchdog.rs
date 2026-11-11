//! The stall watchdog: the once-a-second tick that reads the probe's live
//! table, heartbeats the UI while nothing moves, dumps the table to the log on
//! a stall, and, gated on a connection proven dead, ends a wedged task's wait.
//!
//! A `#[path]` child of `transfer_probe.rs`, apart from the recorder it reads:
//! the parent is what every phase transition writes (atomics, no locks on the
//! hot path) and what the dump renders; this is the one reader that acts on
//! it. As a child it still reaches the parent's private fields, the same way
//! `vocabulary` and `ends` do. Time the watchdog couldn't see (the late-tick
//! rule and its CPU evidence) is its own child, `transfer_probe_unseen.rs`.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::super::super::host::OperationErrorReport;
use super::super::super::types::WriteOperationPhase;
use super::{InboundWindow, NO_INBOUND_RATE, OperationProbe, TaskPhase, TaskProbe};

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
pub(super) fn stall_abort_after() -> Duration {
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
pub(in crate::transfer) struct StallAbortGuard {
    prev: Option<Duration>,
}

#[cfg(test)]
impl StallAbortGuard {
    pub(in crate::transfer) fn set(window: Duration) -> Self {
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

/// Should this still tick re-send the last progress event?
///
/// From [`HEARTBEAT_AFTER_SECS`] once anything has landed, and from the first
/// still second while nothing has. The 3 s wait exists so a heartbeat never
/// lands between two healthy chunk callbacks and nudges the ETA; before the first
/// byte there is no ETA to nudge, and "opening the file" is worth saying the
/// moment it's true (ERR-CNK7M sat 20 s on a silent 0% bar).
fn heartbeat_due(still_for_secs: u64, nothing_landed: bool) -> bool {
    still_for_secs >= HEARTBEAT_AFTER_SECS || (nothing_landed && still_for_secs >= 1)
}

impl OperationProbe {
    /// One watchdog tick, split out from the timer loop so it can be tested
    /// without waiting on wall-clock seconds. `still_for` is how long BOTH
    /// progress counters have been unchanged.
    ///
    /// Returns how long the watchdog went unscheduled before this tick, when
    /// that was long enough to forgive ([`unseen::LATE_TICK_AFTER`]), so the loop can
    /// log it with the evidence.
    pub(super) fn watchdog_step(&self, watchdog: &mut WatchdogState, now: Duration) -> Option<Duration> {
        // First, before any branch reads a clock: time the watchdog never got
        // to observe is not stillness it may act on.
        let unseen = watchdog.unseen_since_last_tick(now);
        if let Some(unseen) = unseen {
            self.forgive_unseen_time(unseen, now);
        }
        self.observe_tick(watchdog, now);
        unseen
    }

    /// The tick proper, once any unseen time is forgiven.
    fn observe_tick(&self, watchdog: &mut WatchdogState, now: Duration) {
        // A transfer waiting for a volume to come back isn't stalled either: it
        // knows exactly what it's waiting on, and says so. It still heartbeats,
        // which is how the UI learns the wait started at all (a waiting
        // operation emits nothing of its own), but it accrues no stall time, no
        // stall dump, and no abort budget (`volume_outage.rs`). A pause or a
        // prompt during the wait outranks it, below.
        if self.state.outage.waiting_for().is_some() && !self.awaiting_human() && !self.state.pause_gate.is_paused() {
            watchdog.still_since = now;
            self.still_for_seconds.store(0, Ordering::Relaxed);
            self.restart_task_stillness(now);
            watchdog.inbound.clear();
            self.publish_inbound_rate(None);
            self.emit_heartbeat();
            return;
        }
        // A transfer waiting on a person is not stalled, and must not accrue
        // stall time or heartbeat at the UI while the conflict dialog is open.
        // Same for a pause: it moves no bytes on purpose. Both restart the
        // per-task clocks too, so the time a task spent parked can't be counted
        // toward an abort the moment the operation resumes.
        if self.awaiting_human() || self.state.pause_gate.is_paused() {
            watchdog.still_since = now;
            self.still_for_seconds.store(0, Ordering::Relaxed);
            self.restart_task_stillness(now);
            watchdog.inbound.clear();
            self.publish_inbound_rate(None);
            return;
        }
        self.track_and_abort_wedged_tasks(now);
        watchdog.inbound.record(now, self.ends.source_bytes_received());
        // Movement is EITHER axis. A stream of skipped children advances the
        // file count with the byte total flat, and it is unambiguously a
        // transfer doing its job — a user watched one work for a minute and
        // cancelled it because this said it was stuck (`ERR-AYVM4`).
        let bytes = self.bytes_done();
        let files = self.files_done();
        if bytes != watchdog.last_bytes || files != watchdog.last_files {
            watchdog.last_bytes = bytes;
            watchdog.last_files = files;
            watchdog.still_since = now;
            self.still_for_seconds.store(0, Ordering::Relaxed);
            // Bytes landing say everything the receive rate would, and say it
            // with verified numbers.
            self.publish_inbound_rate(None);
            return;
        }
        // Publish the stillness on every tick, not just at the log threshold:
        // the UI reads this to decide when to stop showing a confident ETA, and
        // it speaks sooner than the log does.
        let still_for = now.saturating_sub(watchdog.still_since);
        self.still_for_seconds.store(still_for.as_secs(), Ordering::Relaxed);
        self.publish_inbound_rate(watchdog.inbound.bytes_per_second());

        // Speak for the operation while it can't speak for itself.
        if heartbeat_due(still_for.as_secs(), bytes == 0) {
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
    /// never enough. Only `SmbVolume` can say `Dead`, off `smb2`'s own pollable
    /// reading: keepalive armed, a request outstanding, and nothing at all on
    /// the wire for a whole liveness window, probe replies and the bytes of a
    /// response still arriving included (mapping: `cmdr-smb`'s `liveness.rs`).
    /// Every other backend answers `None`, so for them the watchdog reports
    /// without acting. ❌ Don't substitute elapsed silence, a missed probe, or a
    /// slow response for this answer; that is the failure mode a keepalive
    /// exists to prevent, and it kills healthy slow transfers. Full reasoning:
    /// `DETAILS.md` § "The watchdog ACTS".
    fn connection_proven_dead(&self) -> bool {
        self.ends.any_proven_dead()
    }

    pub(super) fn publish_inbound_rate(&self, rate: Option<u64>) {
        self.source_inbound_rate
            .store(rate.unwrap_or(NO_INBOUND_RATE), Ordering::Relaxed);
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

    /// Move every in-flight task's stillness clock forward by `unseen`, a stretch
    /// the watchdog itself spent unscheduled.
    ///
    /// The abort gate's two halves both assume someone was watching. When the
    /// whole runtime starves, `smb2`'s liveness clocks starve with it and can read
    /// `Dead` on the first beat back, while the stillness half passes on minutes
    /// that piled up unobserved: ERR-P4GWB abandoned two healthy reads and filed
    /// an error report exactly that way, on its first tick after ~10 minutes.
    ///
    /// Shifted, never reset (the same rule as `smb2`'s
    /// `forgive_scheduling_stall`): stillness seen BEFORE the gap still counts,
    /// so this forgives without exonerating, and a task that is still wedged once
    /// the watchdog runs again is ended one window later.
    ///
    /// The operation-level stillness (`still_since`, which the UI and the dump
    /// read) is deliberately NOT shifted: the bar really sat still that long,
    /// and saying so is the truth. Only the clock that decides to ACT forgives.
    fn forgive_unseen_time(&self, unseen: Duration, now: Duration) {
        let unseen_ms = u64::try_from(unseen.as_millis()).unwrap_or(u64::MAX);
        let now_ms = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
        for task in self.tasks.lock_ignore_poison().iter() {
            let since = task.still_since_ms.load(Ordering::Relaxed);
            task.still_since_ms
                .store(since.saturating_add(unseen_ms).min(now_ms), Ordering::Relaxed);
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
    ///   killing it trades a rare wedge for frequent spurious failures. Only
    ///   an SMB source or destination can supply the verdict; everything else
    ///   is reported on and never acted on.
    /// - **Per-task, not per-operation.** A batch where any task is still moving
    ///   bytes leaves every other task's clock running on its own merits; only a
    ///   task that has itself gone quiet is a candidate.
    /// - **Only the two phases that mean "inside a backend call"**
    ///   ([`TaskPhase::is_abortable_on_stall`]). Every park is deliberate and ends
    ///   on its own.
    /// - **Never once the work stopped.** Cancel, rollback, and the failure drain
    ///   own their own teardown (the driver's drain deadline); a second abort path
    ///   racing them would just make the stop harder to reason about.
    fn track_and_abort_wedged_tasks(&self, now: Duration) {
        let now_ms = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
        let stopping = self.state.work_stopped();
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
        // Without a `Dead` verdict the loop below only maintains each task's
        // clock, which is what keeps the dump and the UI honest while nothing
        // escalates.
        let proven_dead = self.connection_proven_dead();
        let abort_after_ms = u64::try_from(self.stall_abort_after.as_millis()).unwrap_or(u64::MAX);
        // The table is copied out first, so the report below reaches the host
        // with no lock held (a slow host would otherwise stall every task
        // registering or leaving the window).
        let tasks: Vec<Arc<TaskProbe>> = self.tasks.lock_ignore_poison().clone();
        for task in &tasks {
            let bytes = task.bytes_done.load(Ordering::Relaxed);
            if task.watchdog_last_bytes.swap(bytes, Ordering::Relaxed) != bytes {
                task.still_since_ms.store(now_ms, Ordering::Relaxed);
                continue;
            }
            if stopping || task.is_stall_aborted() {
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
            self.state
                .scope()
                .host()
                .events()
                .error_report(OperationErrorReport::StalledTaskAbandoned {
                    operation_id: self.operation_id.as_str().into(),
                    still_for_secs: still_for_ms / 1_000,
                    task: task.render(),
                });
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
}

/// The watchdog's own carry-over between ticks. Split from `OperationProbe` so
/// the step is a pure function of (probe, this, now) and can be tested without
/// sleeping.
pub(super) struct WatchdogState {
    last_bytes: u64,
    last_files: usize,
    pub(super) still_since: Duration,
    last_reported: Duration,
    /// The source connection's recent received-byte readings, one per tick.
    inbound: InboundWindow,
    /// When the previous tick ran: the cadence witness that tells a late tick
    /// from a still transfer. `None` before the first.
    last_tick: Option<Duration>,
}

impl WatchdogState {
    pub(super) fn new() -> Self {
        Self {
            last_bytes: u64::MAX,
            last_files: usize::MAX,
            still_since: Duration::ZERO,
            last_reported: Duration::ZERO,
            inbound: InboundWindow::default(),
            last_tick: None,
        }
    }

    /// Record this tick, and return how long the watchdog went unscheduled
    /// before it, when that's long enough to forgive ([`unseen::gap`]).
    fn unseen_since_last_tick(&mut self, now: Duration) -> Option<Duration> {
        unseen::gap(self.last_tick.replace(now)?, now)
    }
}

/// Watches one operation's aggregate byte counter and logs the in-flight table
/// when it stops moving.
///
/// Deliberately quiet while paused: a paused transfer moves no bytes on purpose.
/// The dump repeats every `STALL_AFTER` for as long as the stall lasts, because
/// a user who force-quits after 20 minutes should leave behind more than one
/// record of it.
///
/// Spawns the watchdog on the operation's runtime. Each tick it asks `live`
/// for the probe, and ends at the first tick that gets `None`: the operation's
/// guard removed it, or its engine is gone. The registry supplies `live`
/// (holding its table weakly, so this background loop can't keep a dropped
/// engine alive); taking a lookup rather than the table keeps the dependency
/// one-way, registry → watchdog.
pub(super) fn spawn_watchdog(
    runtime: &cmdr_fs::rt::Runtime,
    live: impl Fn() -> Option<Arc<OperationProbe>> + Send + 'static,
    operation_id: String,
) {
    runtime.spawn(async move {
        let mut watchdog = WatchdogState::new();
        let mut cpu = CpuWitness::default();
        let started = crate::rt::now();
        loop {
            crate::rt::time::sleep(STALL_TICK).await;
            let Some(probe) = live() else {
                return; // operation finished (its guard removed it), or its engine is gone
            };
            let now = crate::rt::since(started);
            if let Some(unseen) = probe.watchdog_step(&mut watchdog, now) {
                unseen::log(&operation_id, unseen, cpu.describe_since_baseline(now));
            }
            cpu.refresh_if_due(now);
        }
    });
}

#[path = "transfer_probe_unseen.rs"]
mod unseen;
use unseen::CpuWitness;
