//! Operation intent and pause gate: the two per-operation state machines.
//!
//! [`OperationIntent`] is the cancellation/rollback machine (`Running →
//! RollingBack/Stopped`, `Stopped` terminal); [`PauseGate`] is the orthogonal
//! pause/resume machine. Both are owned by `WriteOperationState` (in `lifecycle/state.rs`)
//! and re-exported from there, so existing `state::OperationIntent` /
//! `state::PauseGate` / `state::IntentCell` paths keep resolving.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::human_wait::{HumanWaitClock, HumanWaitSource};
use super::sync_primitives::{AtomicBool, AtomicU8, Condvar, Mutex, lock_ignore_poison};

// ============================================================================
// Operation intent (state machine for cancellation)
// ============================================================================

/// What the operation should do next.
///
/// State machine: `Running` → `RollingBack` or `Stopped`, `RollingBack` → `Stopped`.
/// No reverse transitions. Encoded as `AtomicU8` for lock-free sharing with native
/// copy callbacks (macOS `copyfile`, chunked copy, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OperationIntent {
    /// Continue the operation normally.
    Running = 0,
    /// Stop the forward operation and delete created files.
    RollingBack = 1,
    /// Stop immediately, keep partial files.
    Stopped = 2,
}

impl OperationIntent {
    pub(super) fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::RollingBack,
            2 => Self::Stopped,
            _ => Self::Running,
        }
    }

    /// The whole transition table: `Running` → `RollingBack` or `Stopped`,
    /// `RollingBack` → `Stopped`. Everything else, `Stopped` → anything included,
    /// is refused.
    pub(super) fn may_become(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Running, Self::RollingBack | Self::Stopped) | (Self::RollingBack, Self::Stopped)
        )
    }
}

/// One operation's [`OperationIntent`], shared with every reader (the drivers,
/// the native copy callbacks, the reversal) as one atomic.
///
/// The cell is the type that holds the intent rule: there is no raw `store`.
/// Its only writers are [`Self::request_stop`] (a person's Stop or Rollback)
/// and [`Self::stop_for_teardown`] (quit's walk, which can only produce
/// `Stopped`), and both go through [`transition_intent`]'s one atomic,
/// table-checked read-modify-write. Cloning shares the same atomic.
#[derive(Clone)]
pub struct IntentCell(Arc<AtomicU8>);

/// What a person asked a running operation to do when they stopped it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopRequest {
    /// Stop and keep what was copied: `Stopped`.
    KeepPartials,
    /// Stop and remove what it created: `RollingBack`.
    RollBack,
}

impl IntentCell {
    /// A cell reading `Running`.
    pub(crate) fn running() -> Self {
        Self(Arc::new(AtomicU8::new(OperationIntent::Running as u8)))
    }

    /// The current intent.
    pub(crate) fn load(&self) -> OperationIntent {
        OperationIntent::from_u8(self.0.load(Ordering::Relaxed))
    }

    /// `true` once the operation should stop (the intent isn't `Running`). The
    /// common check in copy, delete, and move loops; a reversal reads
    /// [`StopMeans`] instead.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.load() != OperationIntent::Running
    }

    /// Moves `Running` to `Stopped` or `RollingBack`, or a rollback to
    /// `Stopped`. Returns the intent it moved from, or the one that refused it.
    /// `in_gap` is [`transition_intent`]'s test seam.
    pub(super) fn request_stop(
        &self,
        request: StopRequest,
        in_gap: impl FnMut(),
    ) -> Result<OperationIntent, OperationIntent> {
        let next = match request {
            StopRequest::KeepPartials => OperationIntent::Stopped,
            StopRequest::RollBack => OperationIntent::RollingBack,
        };
        transition_intent(&self.0, next, in_gap)
    }

    /// Stops for a teardown: `Stopped`, never `RollingBack`, because a teardown
    /// must never delete files with no visual feedback. An already-`Stopped`
    /// operation refuses.
    pub(super) fn stop_for_teardown(&self, in_gap: impl FnMut()) -> Result<OperationIntent, OperationIntent> {
        transition_intent(&self.0, OperationIntent::Stopped, in_gap)
    }

    /// A cell already reading `intent`, for a test.
    #[cfg(test)]
    pub(crate) fn reading(intent: OperationIntent) -> Self {
        Self(Arc::new(AtomicU8::new(intent as u8)))
    }

    /// Writes `intent` with no table check, for a test that simulates a stop
    /// without the stop path.
    #[cfg(any(test, feature = "testing"))]
    pub fn set_for_test(&self, intent: OperationIntent) {
        self.0.store(intent as u8, Ordering::Relaxed);
    }
}

/// Moves `intent` to `next` if the table allows it from the value that's there
/// AT THE WRITE, and returns the intent it moved from; a refusal returns the
/// intent that refused it.
///
/// One read-modify-write, ❌ never a load then a `store`: two stops can race (a
/// Rollback click and quit's teardown walk), and a writer that validated against
/// a stale `Running` would overwrite a terminal `Stopped` with `RollingBack`,
/// turning "keep the partial files" into "delete them". A lost race re-checks
/// the table against what the winner wrote.
///
/// `in_gap` runs between each read and its write. Production passes a no-op; it's
/// the seam the race tests park a writer on.
fn transition_intent(
    intent: &AtomicU8,
    next: OperationIntent,
    mut in_gap: impl FnMut(),
) -> Result<OperationIntent, OperationIntent> {
    intent
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            if !OperationIntent::from_u8(current).may_become(next) {
                return None;
            }
            in_gap();
            Some(next as u8)
        })
        .map(OperationIntent::from_u8)
        .map_err(OperationIntent::from_u8)
}

/// Whose stop a REVERSAL watches — and therefore what a non-`Running` intent
/// means to it.
///
/// [`IntentCell::is_cancelled`] answers "the intent isn't `Running`", and `RollingBack` is one
/// of the values it calls cancelled. That's right for a forward transfer, which
/// must stop and undo itself. It is WRONG for a reversal running under an
/// operation that is itself rolling back: there the same value is the instruction
/// to reverse, so a reversal inheriting the transfer's reading would bail on its
/// first item, park on nothing, and report a clean stop having reversed nothing.
/// A reversal therefore names which reading it means, and both of Cmdr's
/// reversals do (the in-flight volume rollback, and the operation-log engine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopMeans {
    /// The reversal is its own managed operation, opened `Running`: any move off
    /// `Running` is somebody stopping the REVERSAL. The operation-log rollback,
    /// dispatched from history or over MCP, is this one.
    IntentLeavesRunning,
    /// The reversal is the wind-down of the operation it reverses, whose intent
    /// already reads `RollingBack`. Only `Stopped` stops it. A cancelled
    /// transfer's own cleanup is this one.
    IntentReachesStopped,
}

impl StopMeans {
    pub(crate) fn requested(self, intent: &IntentCell) -> bool {
        match self {
            StopMeans::IntentLeavesRunning => intent.is_cancelled(),
            StopMeans::IntentReachesStopped => intent.load() == OperationIntent::Stopped,
        }
    }
}

// ============================================================================
// Pause gate
// ============================================================================

/// Cooperative pause gate for a single write operation.
///
/// A paused op parks at a between-files boundary (the loop tops in the transfer
/// drivers and the delete-phase walker loops, gated immediately AFTER the
/// existing `is_cancelled` check so the data-safety ordering — cancel/skip
/// before any destructive call — is preserved). The cross-volume streaming copy
/// path parks at a finer grain too: BETWEEN CHUNKS, via the `CheckpointStream`
/// wrapper in `transfer/volume/stream_pipe.rs`, so a paused single large file (e.g.
/// MTP→local) stops mid-stream instead of streaming to completion. It is
/// **orthogonal to** [`OperationIntent`]: pausing never perturbs the validated
/// `Running → RollingBack/Stopped` transitions. Cancellation ALWAYS wins over
/// pause: the wait helpers return the instant cancel is observed, so the
/// existing cancel path takes over.
///
/// Two waiters for two execution shapes:
/// - `condvar` (+ its `Mutex<()>`) parks the **sync** driver, which runs inside
///   `tokio::task::spawn_blocking` (a real OS thread — `std::sync::Condvar` is
///   correct there, and the parked thread is the accepted resource asymmetry
///   documented in the plan: a paused Running op legitimately holds its lane).
/// - `notify` parks the **async** volume drivers without blocking an executor
///   thread (`Notify::notified().await`).
///
/// `resume()` wakes both: `notify_all()` on the condvar and `notify_waiters()`
/// on the `Notify`, so whichever shape is parked unblocks.
///
/// Mid-file pause is honored on the cross-volume streaming path (the
/// `CheckpointStream` parks between chunks before reading the next one). A paused
/// op therefore holds only its invisible `.cmdr-tmp-<uuid>` (the previous chunk
/// is fully written, the next isn't yet read), never a torn target. The
/// sync `on_progress` callbacks stay cancel-only — they can't `.await` to park,
/// so the async wrapper owns mid-file parking. The local-FS sync chunk loop
/// (`chunked_copy.rs`) is the one path that pauses only between files (it gets
/// the cancel atom, not this gate); see transfer/DETAILS.md § "Pause reaches
/// between chunks".
pub struct PauseGate {
    paused: AtomicBool,
    /// Guards nothing real — `Condvar::wait` needs a held `MutexGuard`. The flag
    /// itself is the `AtomicBool` so non-waiting readers (`is_paused`) and the
    /// async waiter don't need the lock.
    condvar_mutex: Mutex<()>,
    condvar: Condvar,
    notify: tokio::sync::Notify,
    /// The operation's human-wait accounting, driven from the same two calls
    /// that flip the flag so the two can't disagree. A pause is somebody
    /// thinking, and the ETA must not charge the transfer for it. See
    /// [`HumanWaitClock`].
    human_wait: Arc<HumanWaitClock>,
}

impl PauseGate {
    pub(super) fn new(human_wait: Arc<HumanWaitClock>) -> Self {
        Self {
            paused: AtomicBool::new(false),
            condvar_mutex: Mutex::new(()),
            condvar: Condvar::new(),
            notify: tokio::sync::Notify::new(),
            human_wait,
        }
    }

    /// Returns whether the op is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// Sets the paused flag. The op parks at its next loop-boundary gate. A
    /// no-op-on-double-pause: setting an already-set flag changes nothing.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Release);
        self.human_wait.set(HumanWaitSource::Pause, true);
    }

    /// Clears the paused flag and wakes any waiter (sync condvar + async
    /// notify). Safe to call when not paused (just wakes spurious waiters,
    /// which re-check the flag and continue).
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        self.human_wait.set(HumanWaitSource::Pause, false);
        self.wake();
    }

    /// Wakes any parked waiter WITHOUT clearing the paused flag. The cancel path
    /// calls this: a cancel must unblock a paused, parked op so it can observe
    /// the non-`Running` intent and bail (cancellation wins over pause). The
    /// waiters re-check `is_cancelled` on wake and return even though `paused`
    /// is still set. Without this, a paused op parked on the condvar would never
    /// see a cancel (the cancel path doesn't touch `paused`).
    pub fn wake(&self) {
        // The park is being released without a resume, which today means a
        // cancel: the operation is winding down and nobody is being waited on
        // any more, so stop charging the person even though the flag stays set.
        // Without this the clock would run for the rest of the operation, and a
        // rollback after a cancel-while-paused would never measure a rate.
        self.human_wait.set(HumanWaitSource::Pause, false);
        // Wake the sync waiter under the condvar mutex so a wake that races a
        // just-about-to-`wait` thread isn't missed (the waiter re-checks under
        // the lock).
        {
            let _guard = lock_ignore_poison(&self.condvar_mutex);
            self.condvar.notify_all();
        }
        // Wake the async waiter. `notify_waiters` only wakes tasks already
        // parked in `notified().await`; the async helper's loop re-checks the
        // flag on wake and after the await is registered, so no wake is lost.
        self.notify.notify_waiters();
    }

    /// Parks the calling (blocking) thread while paused, returning as soon as
    /// either the op resumes OR cancellation is observed. Cancellation wins:
    /// `is_cancelled` is re-checked under the condvar lock before every wait, so
    /// a cancel that lands during a pause unblocks the thread and the existing
    /// cancel path takes over. Call from the sync driver, AFTER its
    /// `is_cancelled` loop-top check.
    pub(crate) fn wait_while_paused_sync(&self, intent: &IntentCell) {
        self.wait_while_paused_sync_until(&|| intent.is_cancelled());
    }

    /// [`wait_while_paused_sync`](Self::wait_while_paused_sync) with the
    /// caller's own reading of "stop", the sync twin of
    /// [`wait_while_paused_until`](Self::wait_while_paused_until).
    ///
    /// A scan preview walks on its own thread for an operation it learns about
    /// through a claim, so what stops it is its OWN cancel flag as much as the
    /// operation's intent. Whoever parks names what stops them; ❌ a predicate
    /// that misses one of the walk's cancel routes leaves the thread on the
    /// gate with nobody left to open it.
    pub fn wait_while_paused_sync_until(&self, should_stop: &dyn Fn() -> bool) {
        let mut guard = lock_ignore_poison(&self.condvar_mutex);
        while self.is_paused() && !should_stop() {
            // Recover from poison the same way `lock_ignore_poison` does: a
            // panic elsewhere shouldn't abort the app, and this guards no real
            // data (the flag is the `AtomicBool`).
            guard = self.condvar.wait(guard).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Async sibling of [`wait_while_paused_sync_until`](Self::wait_while_paused_sync_until):
    /// parks the calling task (without blocking an executor thread) while
    /// paused, returning as soon as the op resumes OR the caller's reading of
    /// "stop" holds.
    ///
    /// A forward transfer's reading is `WriteOperationState::work_stopped` (a
    /// cancel, or a concurrent copy's failure drain), which it parks on through
    /// `WriteOperationState::park_while_paused`. An operation-log REVERSAL running
    /// under an op whose intent already says `RollingBack` needs a different one,
    /// or its park would return instantly and a pause would hold nothing. Whoever
    /// parks names what stops them
    /// (`StopMeans`, in `cmdr-operations`' `operation_intent.rs`).
    pub async fn wait_while_paused_until(&self, should_stop: &(dyn Fn() -> bool + Sync)) {
        loop {
            // Register interest BEFORE the flag check so a `resume()` /
            // `notify_waiters()` racing between the check and the await can't be
            // lost (tokio's documented `Notify` pattern).
            let notified = self.notify.notified();
            if !self.is_paused() || should_stop() {
                return;
            }
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    // ---- OperationIntent::from_u8 ----

    #[test]
    fn from_u8_maps_each_known_variant() {
        // Kills: replace from_u8 → Default::default(), delete match arm 1, delete match arm 2.
        assert_eq!(OperationIntent::from_u8(0), OperationIntent::Running);
        assert_eq!(OperationIntent::from_u8(1), OperationIntent::RollingBack);
        assert_eq!(OperationIntent::from_u8(2), OperationIntent::Stopped);
    }

    #[test]
    fn from_u8_unknown_values_fall_back_to_running() {
        // Pins the catch-all arm. If a future variant is added, this should fail.
        assert_eq!(OperationIntent::from_u8(3), OperationIntent::Running);
        assert_eq!(OperationIntent::from_u8(255), OperationIntent::Running);
    }

    fn cell(intent: OperationIntent) -> IntentCell {
        IntentCell::reading(intent)
    }

    /// The intent a raw atom holds, for the `transition_intent` cells.
    fn load_intent(atom: &AtomicU8) -> OperationIntent {
        OperationIntent::from_u8(atom.load(Ordering::Relaxed))
    }

    // ---- IntentCell::load / is_cancelled ----

    #[test]
    fn load_intent_reflects_atomic_value() {
        let intent = cell(OperationIntent::RollingBack);
        assert_eq!(intent.load(), OperationIntent::RollingBack);
        intent.set_for_test(OperationIntent::Stopped);
        assert_eq!(intent.load(), OperationIntent::Stopped);
        intent.set_for_test(OperationIntent::Running);
        assert_eq!(intent.load(), OperationIntent::Running);
    }

    #[test]
    fn is_cancelled_is_true_for_any_non_running_value() {
        // Kills: replace is_cancelled → true / → false, replace != with ==.
        let running = cell(OperationIntent::Running);
        assert!(!running.is_cancelled(), "Running must not be reported as cancelled");

        let rolling = cell(OperationIntent::RollingBack);
        assert!(rolling.is_cancelled(), "RollingBack must be reported as cancelled");

        let stopped = cell(OperationIntent::Stopped);
        assert!(stopped.is_cancelled(), "Stopped must be reported as cancelled");
    }

    #[test]
    fn teardown_can_only_stop() {
        let running = IntentCell::running();
        assert_eq!(running.stop_for_teardown(|| {}), Ok(OperationIntent::Running));
        assert_eq!(running.load(), OperationIntent::Stopped);

        let rolling = cell(OperationIntent::RollingBack);
        assert_eq!(rolling.stop_for_teardown(|| {}), Ok(OperationIntent::RollingBack));
        assert_eq!(
            rolling.load(),
            OperationIntent::Stopped,
            "a teardown ends a rollback too"
        );

        let stopped = cell(OperationIntent::Stopped);
        assert_eq!(stopped.stop_for_teardown(|| {}), Err(OperationIntent::Stopped));
    }

    #[test]
    fn a_stop_request_names_the_intent_it_moves_to() {
        let keep = IntentCell::running();
        assert_eq!(
            keep.request_stop(StopRequest::KeepPartials, || {}),
            Ok(OperationIntent::Running)
        );
        assert_eq!(keep.load(), OperationIntent::Stopped);

        let roll_back = IntentCell::running();
        assert_eq!(
            roll_back.request_stop(StopRequest::RollBack, || {}),
            Ok(OperationIntent::Running)
        );
        assert_eq!(roll_back.load(), OperationIntent::RollingBack);
        assert_eq!(
            keep.request_stop(StopRequest::RollBack, || {}),
            Err(OperationIntent::Stopped),
            "a stopped operation never starts deleting"
        );
    }

    // ---- transition_intent ----

    #[test]
    fn transition_intent_follows_the_whole_table() {
        use OperationIntent::{RollingBack, Running, Stopped};
        let allowed = [(Running, RollingBack), (Running, Stopped), (RollingBack, Stopped)];
        for from in [Running, RollingBack, Stopped] {
            for to in [Running, RollingBack, Stopped] {
                let atom = AtomicU8::new(from as u8);
                let result = transition_intent(&atom, to, || {});
                if allowed.contains(&(from, to)) {
                    assert_eq!(result, Ok(from), "{from:?} -> {to:?} is allowed");
                    assert_eq!(load_intent(&atom), to);
                } else {
                    assert_eq!(result, Err(from), "{from:?} -> {to:?} is refused");
                    assert_eq!(load_intent(&atom), from, "a refusal writes nothing");
                }
            }
        }
    }

    #[test]
    fn transition_intent_rechecks_the_table_against_a_write_that_landed_in_its_gap() {
        // The writer read `Running`; `Stopped` lands before it writes. Its retry
        // sees `Stopped`, which `RollingBack` can't follow.
        let atom = AtomicU8::new(OperationIntent::Running as u8);
        let result = transition_intent(&atom, OperationIntent::RollingBack, || {
            atom.fetch_max(OperationIntent::Stopped as u8, Ordering::Relaxed);
        });
        assert_eq!(result, Err(OperationIntent::Stopped));
        assert_eq!(load_intent(&atom), OperationIntent::Stopped);
    }

    /// The whole reason [`StopMeans`] exists: `RollingBack` reads as "stop" to a
    /// forward transfer and as "reverse" to the reversal running under it.
    ///
    /// A reversal that inherited `is_cancelled` and ran against an operation whose
    /// intent already says `RollingBack` would stop on its first item, park on
    /// nothing, and report a clean stop having reversed nothing. Pinning both
    /// readings means a caller gets a named choice rather than a silent one.
    #[test]
    fn the_two_readings_of_a_rolling_back_intent_disagree_on_purpose() {
        let rolling_back = cell(OperationIntent::RollingBack);
        assert!(
            StopMeans::IntentLeavesRunning.requested(&rolling_back),
            "a reversal that is its own operation never sees RollingBack; anything but Running stops it"
        );
        assert!(
            !StopMeans::IntentReachesStopped.requested(&rolling_back),
            "a reversal running UNDER a rolling-back operation reads that value as its instruction, not as a stop"
        );

        let stopped = cell(OperationIntent::Stopped);
        assert!(StopMeans::IntentLeavesRunning.requested(&stopped));
        assert!(
            StopMeans::IntentReachesStopped.requested(&stopped),
            "Stopped stops either way"
        );

        let running = cell(OperationIntent::Running);
        assert!(!StopMeans::IntentLeavesRunning.requested(&running));
        assert!(!StopMeans::IntentReachesStopped.requested(&running));
    }

    // ---- PauseGate -----------------------------------------------------------

    #[test]
    fn pause_gate_starts_unpaused() {
        let gate = PauseGate::new(HumanWaitClock::shared());
        assert!(!gate.is_paused());
    }

    #[test]
    fn pause_gate_pause_then_resume_toggles_flag() {
        let gate = PauseGate::new(HumanWaitClock::shared());
        gate.pause();
        assert!(gate.is_paused());
        gate.resume();
        assert!(!gate.is_paused());
    }

    #[test]
    fn a_pause_is_charged_to_the_person_who_made_it() {
        // The gate drives the operation's human-wait clock, which is what keeps
        // the paused seconds out of the ETA's rate window. Both halves matter:
        // a pause that never opened the clock would collapse the rate on
        // resume, and one that never closed it would freeze the rate forever.
        let clock = HumanWaitClock::shared();
        let gate = PauseGate::new(Arc::clone(&clock));
        let start = Instant::now();
        assert_eq!(clock.total_at(start), Duration::ZERO);

        gate.pause();
        let paused_at = Instant::now();
        assert!(
            clock.total_at(paused_at + Duration::from_secs(3)) >= Duration::from_secs(3),
            "a pause opens the clock",
        );

        gate.resume();
        let banked = clock.total_at(Instant::now());
        assert_eq!(
            banked,
            clock.total_at(Instant::now() + Duration::from_secs(5)),
            "a resume closes the clock, so later seconds are the transfer's own",
        );
    }

    #[test]
    fn a_cancel_during_a_pause_stops_charging_the_person() {
        // Cancel wakes the gate WITHOUT clearing the flag (cancellation wins
        // over pause), so the paused bit alone would leave the clock running
        // for the rest of the operation — and a rollback that follows would
        // measure every interval as somebody's thinking time and never report a
        // rate.
        let clock = HumanWaitClock::shared();
        let gate = PauseGate::new(Arc::clone(&clock));
        gate.pause();
        gate.wake();

        assert!(gate.is_paused(), "the cancel path leaves the flag set");
        let banked = clock.total_at(Instant::now());
        assert_eq!(
            banked,
            clock.total_at(Instant::now() + Duration::from_secs(5)),
            "the wake closed the clock, so the wind-down counts as the operation's own time",
        );
    }

    #[test]
    fn wait_while_paused_sync_returns_immediately_when_not_paused() {
        // Not paused → the wait must be a no-op (no deadlock, no condvar park).
        let gate = PauseGate::new(HumanWaitClock::shared());
        let intent = IntentCell::running();
        gate.wait_while_paused_sync(&intent); // returns instantly or the test hangs
    }

    /// Parks a worker thread on `wait_while_paused_sync` and proves it stays parked while
    /// the gate is paused. Returns the worker's handle and its "woke" flag, so the test
    /// can resume or cancel and watch the wake.
    fn park_worker_on_gate(
        gate: &Arc<PauseGate>,
        intent: &Arc<IntentCell>,
    ) -> (std::thread::JoinHandle<()>, Arc<AtomicBool>) {
        let woke = Arc::new(AtomicBool::new(false));
        let gate_t = Arc::clone(gate);
        let intent_t = Arc::clone(intent);
        let woke_t = Arc::clone(&woke);
        let handle = std::thread::spawn(move || {
            gate_t.wait_while_paused_sync(&intent_t);
            woke_t.store(true, Ordering::SeqCst);
        });

        // The condvar park exposes no "I am parked now" signal, so a window is the only way to
        // establish the worker really stays blocked while paused.
        // allowed-test-sleep: negative assertion over a window; the condvar park has nothing to await.
        std::thread::sleep(Duration::from_millis(50));
        assert!(!woke.load(Ordering::SeqCst), "worker must still be parked while paused");
        (handle, woke)
    }

    #[test]
    fn wait_while_paused_sync_unblocks_on_resume() {
        // Pause, park a worker thread on the gate, then resume from the main
        // thread. The worker must wake. Without a working condvar notify it
        // would hang and the test would time out.
        let gate = Arc::new(PauseGate::new(HumanWaitClock::shared()));
        let intent = Arc::new(IntentCell::running());
        gate.pause();
        let (handle, woke) = park_worker_on_gate(&gate, &intent);
        gate.resume();
        handle.join().expect("worker thread joins");
        assert!(woke.load(Ordering::SeqCst), "resume must wake the parked worker");
    }

    #[test]
    fn wait_while_paused_sync_unblocks_on_cancel() {
        // Cancellation must win over pause: a paused, parked thread wakes when
        // the intent flips to a non-Running state, WITHOUT a resume.
        let gate = Arc::new(PauseGate::new(HumanWaitClock::shared()));
        let intent = Arc::new(IntentCell::running());
        gate.pause();
        let (handle, woke) = park_worker_on_gate(&gate, &intent);
        // Mirror the production cancel path: flip intent to a non-Running state,
        // then `wake()` (NOT `resume()`) so the paused flag stays set. The
        // parked thread must still wake — cancellation wins over pause.
        intent.set_for_test(OperationIntent::Stopped);
        gate.wake();
        handle.join().expect("worker joins");
        assert!(woke.load(Ordering::SeqCst), "cancel must unblock a paused wait");
        assert!(
            gate.is_paused(),
            "cancel path leaves paused set; the waiter returned because cancel won, not because of resume"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wait_while_paused_until_returns_immediately_when_not_paused() {
        let gate = PauseGate::new(HumanWaitClock::shared());
        let intent = IntentCell::running();
        // Must not hang.
        tokio::time::timeout(
            Duration::from_secs(1),
            gate.wait_while_paused_until(&|| intent.is_cancelled()),
        )
        .await
        .expect("not-paused async wait must return immediately");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wait_while_paused_until_unblocks_on_resume() {
        let gate = Arc::new(PauseGate::new(HumanWaitClock::shared()));
        let intent = Arc::new(IntentCell::running());
        gate.pause();

        let gate_t = Arc::clone(&gate);
        let intent_t = Arc::clone(&intent);
        let task = tokio::spawn(async move {
            gate_t.wait_while_paused_until(&|| intent_t.is_cancelled()).await;
        });

        // `notified().await` exposes no "registered" signal, so we hold a window to prove the
        // waiter stays parked before resume.
        // allowed-test-sleep: negative assertion over a window; `notified().await` has nothing to await on.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished(), "async waiter must still be parked while paused");
        gate.resume();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("resume must wake the async waiter")
            .expect("task joins");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wait_while_paused_until_unblocks_on_cancel() {
        // Cancellation wins over pause for the async path too.
        let gate = Arc::new(PauseGate::new(HumanWaitClock::shared()));
        let intent = Arc::new(IntentCell::running());
        gate.pause();

        let gate_t = Arc::clone(&gate);
        let intent_t = Arc::clone(&intent);
        let task = tokio::spawn(async move {
            gate_t.wait_while_paused_until(&|| intent_t.is_cancelled()).await;
        });

        // `notified().await` has no "registered now" signal, so we hold a window to prove the
        // waiter stays parked before the cancel lands.
        // allowed-test-sleep: negative assertion over a window; `notified().await` has nothing to await on.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished(), "still parked before cancel");
        // Mirror the production cancel path: flip intent, then `wake()` (NOT
        // `resume()`) — paused stays set. The waiter must observe cancel and
        // return anyway.
        intent.set_for_test(OperationIntent::Stopped);
        gate.wake();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("cancel must unblock the async waiter")
            .expect("task joins");
        assert!(gate.is_paused(), "cancel path leaves paused set");
    }
}

#[cfg(all(test, feature = "shuttle"))]
#[path = "operation_intent_shuttle_tests.rs"]
mod shuttle_tests;
