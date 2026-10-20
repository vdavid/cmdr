//! The clock over a scan preview: a heartbeat in the log while it walks, and an
//! INACTIVITY bound that publishes an outcome when the volume stops answering.
//!
//! ## Why inactivity, not total duration
//!
//! A legitimate scan of a large tree over SMB genuinely runs for minutes, so any
//! total-duration cap either kills real work or is too generous to catch a wedge.
//! What separates "slow but working" from "dead" is whether the walk is still
//! COUNTING: every entry a backend hands back is proof the far end answered.
//! `note_progress` records that proof, and the watchdog fires only when it has
//! seen none for [`SCAN_INACTIVITY_LIMIT`].
//!
//! ## Why the watchdog publishes the outcome itself
//!
//! The wedge this exists for is a walk parked in a syscall on a dead mount, which
//! observes no cancel flag and may never return. So the watchdog can't ask the
//! worker to stop and report; it settles the preview and tells the dialog
//! directly, leaving the wedged walk detached behind it. Exactly one of the two
//! publishes: [`ScanWatchdog::claim_outcome`] is the CAS both sides pass through,
//! so a walk that finishes late stays quiet rather than contradicting a timeout
//! the user has already been shown.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use super::Operations;
use super::event_sinks::ScanPreviewEventSink;
use super::scan_cache::{ScanOutcome, ScanPreviewState, ScanPreviews};
use super::types::{ScanPreviewErrorEvent, WriteOperationError};

/// How long a preview may count NOTHING before we call the volume unresponsive.
///
/// Sized above every bound the layers below it own, so their own (better) message
/// wins whenever they have one: a direct-SMB request gives up after ~50 s (20 s to
/// the socket, then 30 s of server silence, `volume/backends/CLAUDE.md`), and the
/// IPC scan deadlines are 30 s. What's left underneath is the case with no bound
/// at all: a syscall on a wedged kernel mount, which blocks until the mount is
/// forced down, and that is what this catches.
pub(super) const SCAN_INACTIVITY_LIMIT: Duration = Duration::from_secs(60);

/// How often the watchdog wakes to log a heartbeat and re-check inactivity.
/// Matches the writer and reconciler stall probes, so a bundle reads at one
/// resolution (`docs/tooling/logging.md`).
const HEARTBEAT: Duration = Duration::from_secs(5);

/// The log target every scan-preview line carries. Grepping it answers the three
/// questions a hang leaves open: did the scan start, did it progress, how did it
/// end.
pub(super) const LOG_TARGET: &str = "scan_preview";

/// How much a walk has counted: the numbers every scan-preview log line carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScanTally {
    pub(super) files: usize,
    pub(super) dirs: usize,
    pub(super) bytes: u64,
}

/// What a preview walks: the phrase its log lines carry, and the path a timeout
/// names to the operation waiting on it.
pub struct ScanTarget {
    /// Human-readable "what is being scanned", for the timeout message the user
    /// reads. See [`ScanTarget::of`].
    pub label: String,
    /// The same, with the first source as a quoted `path=` field, for the log
    /// lines. Bare, a report's redactor can't tell a lowercase last word of the
    /// folder (`Summer trip`) from the ` on volume …` after it, and ships it.
    pub log_label: String,
    /// The first source, which is what a timed-out operation names: a failure
    /// with an empty path reads as "Path: ;" and tells the user nothing.
    pub(super) first_source: String,
}

impl ScanTarget {
    /// The label is how many sources, the first one, and the volume they live
    /// on: enough to tell two concurrent previews apart and to know which share
    /// went quiet, without printing a whole selection.
    pub fn of(sources: &[PathBuf], volume_id: &str) -> Self {
        let first_source = sources.first().map(|p| p.display().to_string()).unwrap_or_default();
        let (first, first_logged) = if first_source.is_empty() {
            (String::from("nothing"), String::from("nothing"))
        } else {
            (first_source.clone(), format!("path={first_source:?}"))
        };
        let rest = match sources.len() {
            0 | 1 => format!("on volume {volume_id}"),
            n => format!("and {} more on volume {volume_id}", n - 1),
        };
        Self {
            label: format!("{first} {rest}"),
            log_label: format!("{first_logged} {rest}"),
            first_source,
        }
    }
}

/// One preview's clock: what it's walking, how much it has counted, and when it
/// last counted anything.
pub(super) struct ScanWatchdog {
    /// The engine's previews, where a timeout settles this one. Weak: the
    /// heartbeat is a background loop, and an engine that's gone has no
    /// preview left to settle.
    previews: Weak<ScanPreviews>,
    preview_id: String,
    /// Human-readable "what is being scanned", for the timeout message.
    target: String,
    /// The same with a keyed path, for the log lines. See [`ScanTarget::log_label`].
    log_target: String,
    /// What a timeout names to the waiting operation. See [`ScanTarget`].
    first_source: String,
    started: Instant,
    /// Milliseconds since `started` at the last `note_progress`. An atomic rather
    /// than a lock: the local walk feeds it from its own OS thread, on a path
    /// that must never wait for anything.
    last_progress_ms: AtomicU64,
    files: AtomicUsize,
    dirs: AtomicUsize,
    bytes: AtomicU64,
    /// Set by whoever publishes this preview's outcome, worker or watchdog.
    outcome_claimed: AtomicBool,
    /// The walk is parked on its operation's pause gate. A paused scan counts
    /// nothing by design, which looks exactly like a volume that stopped
    /// answering, so the inactivity budget is suspended while this is set. See
    /// [`note_parked`](Self::note_parked).
    parked: AtomicBool,
    inactivity_limit: Duration,
}

/// What one heartbeat concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Beat {
    /// Still counting (or still within its inactivity budget): keep waiting.
    Walking,
    /// Parked on the operation's pause gate: counting nothing, on purpose.
    Parked,
    /// Someone already published this preview's outcome: the watchdog is done.
    Settled,
    /// Nothing counted for the whole limit: the volume isn't answering.
    Unresponsive,
}

impl ScanWatchdog {
    /// Starts the clock and spawns the heartbeat on `engine`'s runtime, logging
    /// the one line that says this scan began at all. A timeout settles the
    /// preview in `engine`'s previews.
    pub(super) fn start(
        engine: &Operations,
        preview_id: String,
        target: ScanTarget,
        inactivity_limit: Duration,
        state: Arc<ScanPreviewState>,
        events: Arc<dyn ScanPreviewEventSink>,
    ) -> Arc<Self> {
        let watchdog = Arc::new(Self {
            previews: Arc::downgrade(engine.stores().previews_arc()),
            preview_id,
            target: target.label,
            log_target: target.log_label,
            first_source: target.first_source,
            started: Instant::now(),
            last_progress_ms: AtomicU64::new(0),
            files: AtomicUsize::new(0),
            dirs: AtomicUsize::new(0),
            bytes: AtomicU64::new(0),
            outcome_claimed: AtomicBool::new(false),
            parked: AtomicBool::new(false),
            inactivity_limit,
        });
        log::info!(
            target: LOG_TARGET,
            "scan preview {} started: {}",
            watchdog.preview_id,
            watchdog.log_target
        );
        let ticking = Arc::clone(&watchdog);
        engine
            .host()
            .runtime()
            .spawn(async move { ticking.run(state, events).await });
        watchdog
    }

    /// The walk counted something: the far end is answering.
    pub(super) fn note_progress(&self, files: usize, dirs: usize, bytes: u64) {
        self.files.store(files, Ordering::Relaxed);
        self.dirs.store(dirs, Ordering::Relaxed);
        self.bytes.store(bytes, Ordering::Relaxed);
        self.last_progress_ms
            .store(self.started.elapsed().as_millis() as u64, Ordering::Relaxed);
    }

    /// The walk has parked on its operation's pause gate: stop the inactivity
    /// clock. A person thinking is not a volume that died, and without this a
    /// pause held for a minute would end the scan with "stopped responding".
    pub(super) fn note_parked(&self) {
        self.parked.store(true, Ordering::Release);
    }

    /// The walk is counting again. Restarts the inactivity clock from now, so
    /// the pause itself is never charged against the budget.
    pub(super) fn note_resumed(&self) {
        self.last_progress_ms
            .store(self.started.elapsed().as_millis() as u64, Ordering::Relaxed);
        self.parked.store(false, Ordering::Release);
    }

    /// Takes the right to publish this preview's outcome, once ever. `true` for
    /// the winner, `false` for everyone after it.
    pub(super) fn claim_outcome(&self) -> bool {
        self.outcome_claimed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// What the walk has counted so far, as the watchdog saw it.
    pub(super) fn walked(&self) -> ScanTally {
        ScanTally {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }

    /// Logs a finished walk with the totals it really produced, and adopts them
    /// as the watchdog's own counters.
    ///
    /// The counters the watchdog keeps come from the progress tick, which only
    /// fires on the `fileOperations.progressUpdateInterval` timer. A scan that
    /// finishes inside one interval never ticks, so reporting those counters
    /// here would log `0 files, 0 dirs, 0 bytes` over a tree that was fully
    /// walked — and this line is the first thing anyone reads when triaging a
    /// scan.
    pub(super) fn note_completed(&self, tally: ScanTally) {
        self.files.store(tally.files, Ordering::Relaxed);
        self.dirs.store(tally.dirs, Ordering::Relaxed);
        self.bytes.store(tally.bytes, Ordering::Relaxed);
        self.log_settled("complete", tally);
    }

    /// Logs how a claimed preview ended short, reporting how far the walk got
    /// before it stopped. `how` is a fixed word from the call site (`cancelled` /
    /// `stopped`), never a message.
    ///
    /// Deliberately the watchdog's own counters: a cancelled or abandoned walk
    /// has no totals, and "how far we got" is the number worth having.
    pub(super) fn note_settled(&self, how: &str) {
        self.log_settled(how, self.walked());
    }

    fn log_settled(&self, how: &str, tally: ScanTally) {
        log::info!(
            target: LOG_TARGET,
            "scan preview {} {}: {} files, {} dirs, {} bytes in {:.1}s ({})",
            self.preview_id,
            how,
            tally.files,
            tally.dirs,
            tally.bytes,
            self.started.elapsed().as_secs_f64(),
            self.log_target
        );
    }

    /// How long since the walk last counted anything.
    fn idle(&self) -> Duration {
        self.started
            .elapsed()
            .saturating_sub(Duration::from_millis(self.last_progress_ms.load(Ordering::Relaxed)))
    }

    /// One heartbeat: says where the walk is, and whether it has gone quiet past
    /// its budget.
    fn beat(&self) -> Beat {
        if self.outcome_claimed.load(Ordering::SeqCst) {
            return Beat::Settled;
        }
        if self.parked.load(Ordering::Acquire) {
            log::debug!(
                target: LOG_TARGET,
                "scan preview {} parked: paused at {} files, {} dirs, {} bytes ({})",
                self.preview_id,
                self.files.load(Ordering::Relaxed),
                self.dirs.load(Ordering::Relaxed),
                self.bytes.load(Ordering::Relaxed),
                self.log_target
            );
            return Beat::Parked;
        }
        let idle = self.idle();
        log::debug!(
            target: LOG_TARGET,
            "scan preview {} walking: {} files, {} dirs, {} bytes, last count {:.0}s ago ({})",
            self.preview_id,
            self.files.load(Ordering::Relaxed),
            self.dirs.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            idle.as_secs_f64(),
            self.log_target
        );
        if idle >= self.inactivity_limit {
            Beat::Unresponsive
        } else {
            Beat::Walking
        }
    }

    /// The heartbeat loop. Ends when the preview settles, whoever settles it.
    async fn run(self: Arc<Self>, state: Arc<ScanPreviewState>, events: Arc<dyn ScanPreviewEventSink>) {
        // A short budget has to be checked more often than the 5 s a production
        // one uses, or the first beat lands long after the deadline it is meant
        // to catch.
        let tick = HEARTBEAT.min(self.inactivity_limit / 2).max(Duration::from_millis(10));
        loop {
            tokio::time::sleep(tick).await;
            match self.beat() {
                Beat::Walking | Beat::Parked => {}
                Beat::Settled => return,
                Beat::Unresponsive => {
                    self.give_up(&state, events.as_ref());
                    return;
                }
            }
        }
    }

    /// Publishes the timeout: the preview settles, the dialog is told, and the
    /// walk is asked to stop.
    ///
    /// The cancel flag is a courtesy, not the mechanism. A walk parked in a
    /// syscall on a dead mount never reads it and stays detached behind us; a
    /// walk that is merely slower than we believed reads it and stops working
    /// for a dialog that has moved on. Neither can publish anything afterwards:
    /// `claim_outcome` is already spent.
    fn give_up(&self, state: &ScanPreviewState, events: &dyn ScanPreviewEventSink) {
        if !self.claim_outcome() {
            // The worker got there between the beat and here.
            return;
        }
        let message = unresponsive_message(&self.target, self.inactivity_limit);
        log::warn!(
            target: LOG_TARGET,
            "scan preview {} gave up after {:.0}s: nothing counted for {:.0}s, stopped at {} files, {} dirs, {} bytes ({})",
            self.preview_id,
            self.started.elapsed().as_secs_f64(),
            self.idle().as_secs_f64(),
            self.files.load(Ordering::Relaxed),
            self.dirs.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            self.log_target
        );
        state.cancelled.store(true, Ordering::Relaxed);
        let failure = WriteOperationError::IoError {
            path: self.first_source.clone(),
            message: message.clone().into(),
        };
        if let Some(previews) = self.previews.upgrade() {
            previews.settle_preview(&self.preview_id, ScanOutcome::Error(failure), None);
        }
        events.emit_error(ScanPreviewErrorEvent {
            preview_id: self.preview_id.clone(),
            message,
            timed_out: true,
        });
    }
}

/// The message a timed-out preview carries. It reaches the user two ways: the
/// dialog's own notice, and (for a transfer already confirmed) that operation's
/// failure. So it stays plain about what happened, and never says "error".
fn unresponsive_message(target: &str, limit: Duration) -> String {
    format!(
        "{target} stopped responding: nothing counted for {} seconds",
        limit.as_secs()
    )
}
