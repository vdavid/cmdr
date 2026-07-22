//! Operation intent and pause gate: the two per-operation state machines.
//!
//! [`OperationIntent`] is the cancellation/rollback machine (`Running →
//! RollingBack/Stopped`, `Stopped` terminal); [`PauseGate`] is the orthogonal
//! pause/resume machine. Both are owned by `WriteOperationState` (in `lifecycle/state.rs`)
//! and re-exported from there, so existing `state::OperationIntent` /
//! `state::PauseGate` / `state::is_cancelled` / `state::load_intent` paths keep
//! resolving.

use crate::ignore_poison::IgnorePoison;
use std::sync::Condvar;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

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
pub(crate) enum OperationIntent {
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
}

/// Loads the current intent from an `AtomicU8`.
pub(crate) fn load_intent(intent: &AtomicU8) -> OperationIntent {
    OperationIntent::from_u8(intent.load(Ordering::Relaxed))
}

/// Returns true if the operation should stop (intent is not `Running`).
/// Use this for the common cancellation check in copy/delete/move loops.
pub(crate) fn is_cancelled(intent: &AtomicU8) -> bool {
    intent.load(Ordering::Relaxed) != OperationIntent::Running as u8
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
/// wrapper in `transfer/volume_strategy.rs`, so a paused single large file (e.g.
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
    condvar_mutex: std::sync::Mutex<()>,
    condvar: Condvar,
    notify: tokio::sync::Notify,
}

impl PauseGate {
    pub(super) fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            condvar_mutex: std::sync::Mutex::new(()),
            condvar: Condvar::new(),
            notify: tokio::sync::Notify::new(),
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
    }

    /// Clears the paused flag and wakes any waiter (sync condvar + async
    /// notify). Safe to call when not paused (just wakes spurious waiters,
    /// which re-check the flag and continue).
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Release);
        self.wake();
    }

    /// Wakes any parked waiter WITHOUT clearing the paused flag. The cancel path
    /// calls this: a cancel must unblock a paused, parked op so it can observe
    /// the non-`Running` intent and bail (cancellation wins over pause). The
    /// waiters re-check `is_cancelled` on wake and return even though `paused`
    /// is still set. Without this, a paused op parked on the condvar would never
    /// see a cancel (the cancel path doesn't touch `paused`).
    pub fn wake(&self) {
        // Wake the sync waiter under the condvar mutex so a wake that races a
        // just-about-to-`wait` thread isn't missed (the waiter re-checks under
        // the lock).
        {
            let _guard = self.condvar_mutex.lock_ignore_poison();
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
    pub fn wait_while_paused_sync(&self, intent: &AtomicU8) {
        let mut guard = self.condvar_mutex.lock_ignore_poison();
        while self.is_paused() && !is_cancelled(intent) {
            // Recover from poison the same way `lock_ignore_poison` does: a
            // panic elsewhere shouldn't abort the app, and this guards no real
            // data (the flag is the `AtomicBool`).
            guard = self.condvar.wait(guard).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Async sibling of [`wait_while_paused_sync`]: parks the calling task
    /// (without blocking an executor thread) while paused, returning as soon as
    /// the op resumes OR cancellation is observed. Call from the async volume
    /// drivers, AFTER their `is_cancelled` loop-top check.
    pub async fn wait_while_paused_async(&self, intent: &AtomicU8) {
        loop {
            // Register interest BEFORE the flag check so a `resume()` /
            // `notify_waiters()` racing between the check and the await can't be
            // lost (tokio's documented `Notify` pattern).
            let notified = self.notify.notified();
            if !self.is_paused() || is_cancelled(intent) {
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
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
    use std::time::Duration;

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

    // ---- load_intent / is_cancelled ----

    #[test]
    fn load_intent_reflects_atomic_value() {
        let atom = AtomicU8::new(OperationIntent::RollingBack as u8);
        assert_eq!(load_intent(&atom), OperationIntent::RollingBack);
        atom.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
        assert_eq!(load_intent(&atom), OperationIntent::Stopped);
        atom.store(OperationIntent::Running as u8, Ordering::Relaxed);
        assert_eq!(load_intent(&atom), OperationIntent::Running);
    }

    #[test]
    fn is_cancelled_is_true_for_any_non_running_value() {
        // Kills: replace is_cancelled → true / → false, replace != with ==.
        let running = AtomicU8::new(OperationIntent::Running as u8);
        assert!(!is_cancelled(&running), "Running must not be reported as cancelled");

        let rolling = AtomicU8::new(OperationIntent::RollingBack as u8);
        assert!(is_cancelled(&rolling), "RollingBack must be reported as cancelled");

        let stopped = AtomicU8::new(OperationIntent::Stopped as u8);
        assert!(is_cancelled(&stopped), "Stopped must be reported as cancelled");
    }

    // ---- PauseGate -----------------------------------------------------------

    #[test]
    fn pause_gate_starts_unpaused() {
        let gate = PauseGate::new();
        assert!(!gate.is_paused());
    }

    #[test]
    fn pause_gate_pause_then_resume_toggles_flag() {
        let gate = PauseGate::new();
        gate.pause();
        assert!(gate.is_paused());
        gate.resume();
        assert!(!gate.is_paused());
    }

    #[test]
    fn wait_while_paused_sync_returns_immediately_when_not_paused() {
        // Not paused → the wait must be a no-op (no deadlock, no condvar park).
        let gate = PauseGate::new();
        let intent = AtomicU8::new(OperationIntent::Running as u8);
        gate.wait_while_paused_sync(&intent); // returns instantly or the test hangs
    }

    #[test]
    fn wait_while_paused_sync_unblocks_on_resume() {
        // Pause, park a worker thread on the gate, then resume from the main
        // thread. The worker must wake. Without a working condvar notify it
        // would hang and the test would time out.
        let gate = Arc::new(PauseGate::new());
        let intent = Arc::new(AtomicU8::new(OperationIntent::Running as u8));
        gate.pause();

        let woke = Arc::new(AtomicBool::new(false));
        let gate_t = Arc::clone(&gate);
        let intent_t = Arc::clone(&intent);
        let woke_t = Arc::clone(&woke);
        let handle = std::thread::spawn(move || {
            gate_t.wait_while_paused_sync(&intent_t);
            woke_t.store(true, Ordering::SeqCst);
        });

        // Give the worker time to park, then resume.
        std::thread::sleep(Duration::from_millis(50));
        assert!(!woke.load(Ordering::SeqCst), "worker must still be parked while paused");
        gate.resume();
        handle.join().expect("worker thread joins");
        assert!(woke.load(Ordering::SeqCst), "resume must wake the parked worker");
    }

    #[test]
    fn wait_while_paused_sync_unblocks_on_cancel() {
        // Cancellation must win over pause: a paused, parked thread wakes when
        // the intent flips to a non-Running state, WITHOUT a resume.
        let gate = Arc::new(PauseGate::new());
        let intent = Arc::new(AtomicU8::new(OperationIntent::Running as u8));
        gate.pause();

        let woke = Arc::new(AtomicBool::new(false));
        let gate_t = Arc::clone(&gate);
        let intent_t = Arc::clone(&intent);
        let woke_t = Arc::clone(&woke);
        let handle = std::thread::spawn(move || {
            gate_t.wait_while_paused_sync(&intent_t);
            woke_t.store(true, Ordering::SeqCst);
        });

        std::thread::sleep(Duration::from_millis(50));
        assert!(!woke.load(Ordering::SeqCst), "still parked before cancel");
        // Mirror the production cancel path: flip intent to a non-Running state,
        // then `wake()` (NOT `resume()`) so the paused flag stays set. The
        // parked thread must still wake — cancellation wins over pause.
        intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
        gate.wake();
        handle.join().expect("worker joins");
        assert!(woke.load(Ordering::SeqCst), "cancel must unblock a paused wait");
        assert!(
            gate.is_paused(),
            "cancel path leaves paused set; the waiter returned because cancel won, not because of resume"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wait_while_paused_async_returns_immediately_when_not_paused() {
        let gate = PauseGate::new();
        let intent = AtomicU8::new(OperationIntent::Running as u8);
        // Must not hang.
        tokio::time::timeout(Duration::from_secs(1), gate.wait_while_paused_async(&intent))
            .await
            .expect("not-paused async wait must return immediately");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wait_while_paused_async_unblocks_on_resume() {
        let gate = Arc::new(PauseGate::new());
        let intent = Arc::new(AtomicU8::new(OperationIntent::Running as u8));
        gate.pause();

        let gate_t = Arc::clone(&gate);
        let intent_t = Arc::clone(&intent);
        let task = tokio::spawn(async move {
            gate_t.wait_while_paused_async(&intent_t).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished(), "async waiter must still be parked while paused");
        gate.resume();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("resume must wake the async waiter")
            .expect("task joins");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn wait_while_paused_async_unblocks_on_cancel() {
        // Cancellation wins over pause for the async path too.
        let gate = Arc::new(PauseGate::new());
        let intent = Arc::new(AtomicU8::new(OperationIntent::Running as u8));
        gate.pause();

        let gate_t = Arc::clone(&gate);
        let intent_t = Arc::clone(&intent);
        let task = tokio::spawn(async move {
            gate_t.wait_while_paused_async(&intent_t).await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!task.is_finished(), "still parked before cancel");
        // Mirror the production cancel path: flip intent, then `wake()` (NOT
        // `resume()`) — paused stays set. The waiter must observe cancel and
        // return anyway.
        intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
        gate.wake();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("cancel must unblock the async waiter")
            .expect("task joins");
        assert!(gate.is_paused(), "cancel path leaves paused set");
    }
}
