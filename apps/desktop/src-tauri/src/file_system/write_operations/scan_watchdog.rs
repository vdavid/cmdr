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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::event_sinks::ScanPreviewEventSink;
use super::scan_cache::{ScanOutcome, ScanPreviewState, settle_preview};
use super::types::ScanPreviewErrorEvent;

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

/// One preview's clock: what it's walking, how much it has counted, and when it
/// last counted anything.
pub(super) struct ScanWatchdog {
    preview_id: String,
    /// Human-readable "what is being scanned", for the log lines.
    target: String,
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
    inactivity_limit: Duration,
}

/// What one heartbeat concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Beat {
    /// Still counting (or still within its inactivity budget): keep waiting.
    Walking,
    /// Someone already published this preview's outcome: the watchdog is done.
    Settled,
    /// Nothing counted for the whole limit: the volume isn't answering.
    Unresponsive,
}

impl ScanWatchdog {
    /// Starts the clock and spawns the heartbeat, logging the one line that says
    /// this scan began at all.
    pub(super) fn start(
        preview_id: String,
        target: String,
        inactivity_limit: Duration,
        state: Arc<ScanPreviewState>,
        events: Arc<dyn ScanPreviewEventSink>,
    ) -> Arc<Self> {
        let watchdog = Arc::new(Self {
            preview_id,
            target,
            started: Instant::now(),
            last_progress_ms: AtomicU64::new(0),
            files: AtomicUsize::new(0),
            dirs: AtomicUsize::new(0),
            bytes: AtomicU64::new(0),
            outcome_claimed: AtomicBool::new(false),
            inactivity_limit,
        });
        log::info!(
            target: LOG_TARGET,
            "scan preview {} started: {}",
            watchdog.preview_id,
            watchdog.target
        );
        let ticking = Arc::clone(&watchdog);
        tokio::spawn(async move { ticking.run(state, events).await });
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

    /// Takes the right to publish this preview's outcome, once ever. `true` for
    /// the winner, `false` for everyone after it.
    pub(super) fn claim_outcome(&self) -> bool {
        self.outcome_claimed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Logs how a claimed preview ended. `how` is a fixed word from the call site
    /// (`complete` / `cancelled` / `stopped`), never a message.
    pub(super) fn note_settled(&self, how: &str) {
        log::info!(
            target: LOG_TARGET,
            "scan preview {} {}: {} files, {} dirs, {} bytes in {:.1}s ({})",
            self.preview_id,
            how,
            self.files.load(Ordering::Relaxed),
            self.dirs.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            self.started.elapsed().as_secs_f64(),
            self.target
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
        let idle = self.idle();
        log::debug!(
            target: LOG_TARGET,
            "scan preview {} walking: {} files, {} dirs, {} bytes, last count {:.0}s ago ({})",
            self.preview_id,
            self.files.load(Ordering::Relaxed),
            self.dirs.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            idle.as_secs_f64(),
            self.target
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
        let tick = HEARTBEAT
            .min(self.inactivity_limit / 2)
            .max(Duration::from_millis(10));
        loop {
            tokio::time::sleep(tick).await;
            match self.beat() {
                Beat::Walking => {}
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
            self.target
        );
        state.cancelled.store(true, Ordering::Relaxed);
        settle_preview(&self.preview_id, ScanOutcome::Error(message.clone()), None);
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

/// The "what is being scanned" phrase the log lines carry: how many sources, the
/// first one, and the volume they live on. Enough to tell two concurrent previews
/// apart and to know which share went quiet, without printing a whole selection.
pub(super) fn scan_target_label(sources: &[PathBuf], volume_id: &str) -> String {
    let first = sources
        .first()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| String::from("nothing"));
    match sources.len() {
        0 | 1 => format!("{first} on volume {volume_id}"),
        n => format!("{first} and {} more on volume {volume_id}", n - 1),
    }
}
