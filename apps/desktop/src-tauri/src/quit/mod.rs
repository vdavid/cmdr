//! The quit gate: the backend owns the decision to exit.
//!
//! Quitting with work in flight asks the user, counts down on a timer this
//! module owns, then stops everything and exits inside a hard budget. Both ways
//! out of the app funnel through [`request_quit`] — `RunEvent::ExitRequested`
//! (⌘Q, the menu, `AppHandle::exit`) and the main window's `CloseRequested`.
//!
//! **The countdown is Rust's, and the dialog only displays it.** A frontend
//! `setInterval` would never fire in a wedged webview, and a wedged UI is a
//! likely reason someone is quitting in the first place. The frontend renders,
//! counts down for show, and answers with `quit_confirm` / `quit_cancel`; the
//! gate quits on whichever lands first, its own deadline or the answer.
//!
//! Architecture, the budget's arithmetic, and the teardown's ordering:
//! `DETAILS.md`.

pub(crate) mod commands;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::file_system::write_operations::{LifecycleStatus, OperationSnapshot, WriteOperationType};
use crate::ignore_poison::IgnorePoison;

/// How long the dialog counts down before the gate quits on its own.
///
/// macOS gives an app a limited window to answer a logout or restart before the
/// system complains or cancels the restart, and Tauri surfaces no signal that
/// separates that case from a plain ⌘Q (`RunEvent::ExitRequested` carries only
/// an exit code, and a user-driven quit carries `None` either way). So the one
/// countdown has to fit the strictest case: 15 s plus [`DRAIN`] plus the rest of
/// the teardown leaves margin where 20 s did not.
pub(crate) const COUNTDOWN: Duration = Duration::from_secs(15);

/// How long the teardown waits for the cooperative cancel to be obeyed before
/// firing the hard abort. The whole decision-to-process-gone budget is 2 s.
pub(crate) const DRAIN: Duration = Duration::from_millis(1_500);

/// How often the drain re-checks whether the operations answered. Small enough
/// that a prompt cancel isn't waited out, large enough to cost nothing.
const DRAIN_POLL: Duration = Duration::from_millis(20);

/// Emitted when a quit is held: what's still running, and how long the user has.
///
/// Kebab-cases to `quit-requested`. The `countdownMs` is what the dialog counts
/// down for display; it is NOT the authority, [`QuitGate`]'s thread is.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct QuitRequested {
    /// The non-instant operations holding the quit, in registration order.
    pub operations: Vec<OperationSnapshot>,
    /// Milliseconds from this event to the automatic quit.
    pub countdown_ms: u32,
}

/// What the caller of [`QuitGate::request_quit`] should do about the quit it was
/// handed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuitOutcome {
    /// Nothing to lose, or the decision is already made: let the exit through.
    Proceed,
    /// The gate is asking the user. Prevent the exit (and the window close);
    /// the gate ends the process itself once it has an answer or its deadline.
    Held,
}

/// The two durations, injectable so a test doesn't sit through the real ones.
pub(crate) struct QuitTimings {
    pub countdown: Duration,
    pub drain: Duration,
}

impl Default for QuitTimings {
    fn default() -> Self {
        Self {
            countdown: COUNTDOWN,
            drain: DRAIN,
        }
    }
}

/// Everything the gate needs from the outside world. [`TauriQuitHost`] is the
/// real one; tests substitute a recorder, which is how the deadline can be
/// exercised without ending the test process.
pub(crate) trait QuitHost: Send + Sync + 'static {
    /// Every operation the manager currently knows about. The gate applies
    /// [`blocks_quit`] itself, so the policy stays in one testable place.
    fn operations(&self) -> Vec<OperationSnapshot>;
    /// Tell the windows a quit is pending.
    fn announce(&self, event: QuitRequested);
    /// Tier 1: cooperative cancel of every live operation, keep-partials.
    fn cancel_all(&self);
    /// Tier 2: stop waiting on whatever didn't answer.
    fn abort_all(&self);
    /// Fence the in-flight temp ledger so the next launch sweeps the leftovers.
    fn flush_temp_ledger(&self);
    /// End the process.
    fn exit(&self);
}

/// Whether this operation is worth interrupting a quit for.
///
/// Both matches are exhaustive on purpose: a new operation type or lifecycle
/// status has to come here and say which side it's on, rather than defaulting
/// into "silently killable at quit".
pub(crate) fn blocks_quit(snapshot: &OperationSnapshot) -> bool {
    let still_going = match snapshot.status {
        // A conflict prompt leaves the operation Running, so "waiting on an
        // answer" needs no arm of its own.
        LifecycleStatus::Queued | LifecycleStatus::Running | LifecycleStatus::Paused => true,
        LifecycleStatus::Done | LifecycleStatus::Cancelled | LifecycleStatus::Failed => false,
    };
    let moves_bytes = match snapshot.operation_type {
        WriteOperationType::Copy
        | WriteOperationType::Move
        | WriteOperationType::Delete
        | WriteOperationType::Trash
        | WriteOperationType::ArchiveEdit => true,
        // Instant metadata ops (`manager::run_instant`) finish faster than a
        // human could read a dialog about them.
        WriteOperationType::Rename | WriteOperationType::CreateFolder | WriteOperationType::CreateFile => false,
    };
    still_going && moves_bytes
}

/// The decision, on its way from an IPC command to the deadline thread.
enum Decision {
    Quit,
    Cancel,
}

/// Where the gate is. Exactly one deadline thread exists per [`Phase::Waiting`].
enum Phase {
    /// No quit pending.
    Idle,
    /// The dialog is up and the deadline thread is counting. The sender is how
    /// an answer reaches it; dropping it stands the thread down.
    Waiting(mpsc::Sender<Decision>),
    /// The decision is made and the teardown is running (or done). Every
    /// further quit request sails through: the teardown ends in
    /// `AppHandle::exit(0)`, which comes straight back as `ExitRequested`, and
    /// a second prompt there would trap the app forever.
    Quitting,
}

pub(crate) struct QuitGate {
    phase: Mutex<Phase>,
    timings: QuitTimings,
    /// Deadline threads started, ever. Nothing in production reads it; it names
    /// each thread so a log or a sample tells two apart.
    countdowns: AtomicU64,
}

impl QuitGate {
    pub(crate) fn with_timings(timings: QuitTimings) -> Self {
        Self {
            phase: Mutex::new(Phase::Idle),
            timings,
            countdowns: AtomicU64::new(0),
        }
    }

    /// Answers a quit request: [`QuitOutcome::Proceed`] to let it through,
    /// [`QuitOutcome::Held`] once the gate has taken the decision over.
    ///
    /// Cheap and synchronous — it runs on the event loop thread.
    pub(crate) fn request_quit(self: &Arc<Self>, host: Arc<impl QuitHost>) -> QuitOutcome {
        let host: Arc<dyn QuitHost> = host;
        let mut phase = self.phase.lock_ignore_poison();
        match &*phase {
            Phase::Quitting => return QuitOutcome::Proceed,
            // Pressing ⌘Q again must not restart the clock the user is watching.
            Phase::Waiting(_) => return QuitOutcome::Held,
            Phase::Idle => {}
        }

        let operations: Vec<OperationSnapshot> = host.operations().into_iter().filter(blocks_quit).collect();
        if operations.is_empty() {
            *phase = Phase::Quitting;
            return QuitOutcome::Proceed;
        }

        let (decisions, answers) = mpsc::channel();
        *phase = Phase::Waiting(decisions);
        drop(phase);

        let countdown = self.timings.countdown;
        let drain = self.timings.drain;
        log::info!(
            target: "quit",
            "holding the quit: {} operation(s) still running, {}s on the clock",
            operations.len(),
            countdown.as_secs()
        );
        host.announce(QuitRequested {
            operations,
            countdown_ms: u32::try_from(countdown.as_millis()).unwrap_or(u32::MAX),
        });

        // A dedicated OS thread, not a tokio task: the deadline must not be
        // schedulable behind whatever the runtime is already busy with, and the
        // point of the whole design is that it fires when other things are stuck.
        let name = format!("cmdr-quit-deadline-{}", self.countdowns.fetch_add(1, Ordering::SeqCst));
        let gate = Arc::clone(self);
        let spawned = std::thread::Builder::new().name(name).spawn(move || {
            let answer = answers.recv_timeout(countdown);
            match answer {
                Ok(Decision::Cancel) | Err(RecvTimeoutError::Disconnected) => {
                    log::info!(target: "quit", "the quit was called off; the countdown is gone");
                    return;
                }
                Ok(Decision::Quit) => log::info!(target: "quit", "quit confirmed; stopping everything"),
                Err(RecvTimeoutError::Timeout) => {
                    // The frontend never answered — a wedged webview, or a user
                    // who walked away. Claim the decision, unless a cancel beat
                    // us to the lock by a hair.
                    if !gate.claim_deadline() {
                        return;
                    }
                    log::info!(target: "quit", "the countdown ran out; stopping everything");
                }
            }
            tear_down_and_exit(&*host, drain);
        });
        if let Err(e) = spawned {
            // No timer means no way back out of a held quit, so don't hold it.
            crate::log_error!(target: "quit", "couldn't start the quit countdown ({e}); quitting straight away");
            *self.phase.lock_ignore_poison() = Phase::Quitting;
            return QuitOutcome::Proceed;
        }
        QuitOutcome::Held
    }

    /// The user pressed Quit. Idempotent, and a no-op once the deadline already
    /// claimed the decision.
    pub(crate) fn confirm(&self) {
        let mut phase = self.phase.lock_ignore_poison();
        match std::mem::replace(&mut *phase, Phase::Quitting) {
            Phase::Waiting(decisions) => {
                let _ = decisions.send(Decision::Quit);
            }
            other => {
                *phase = other;
                log::debug!(target: "quit", "quit_confirm with no quit pending; ignoring");
            }
        }
    }

    /// The user pressed "Keep working". The countdown is **gone**, not deferred:
    /// a snooze would still kill the transfer seconds later, which is worse than
    /// not having asked.
    pub(crate) fn cancel(&self) {
        let mut phase = self.phase.lock_ignore_poison();
        match std::mem::replace(&mut *phase, Phase::Idle) {
            Phase::Waiting(decisions) => {
                let _ = decisions.send(Decision::Cancel);
            }
            other => {
                *phase = other;
                log::debug!(target: "quit", "quit_cancel with no quit pending; ignoring");
            }
        }
    }

    /// Moves `Waiting` → `Quitting` for the deadline thread. `false` means an
    /// answer landed first and the thread should stand down.
    fn claim_deadline(&self) -> bool {
        let mut phase = self.phase.lock_ignore_poison();
        match std::mem::replace(&mut *phase, Phase::Quitting) {
            Phase::Waiting(_) => true,
            other => {
                *phase = other;
                false
            }
        }
    }
}

/// Stop everything, leave the disk safe, and end the process — inside 2 s from
/// here, whatever the operations are doing.
///
/// The order is the contract:
///
/// 1. **Cooperative cancel, no rollback.** Every fully-copied file is kept; only
///    the file in flight loses its partial. This is `OperationIntent::Stopped`.
/// 2. **Wait, but only up to `drain`.** Operations that answer let us skip the
///    rest of it.
/// 3. **Hard abort** whatever didn't answer: stop *waiting*, and let the staging
///    layer own the leftovers rather than a backend that stopped talking.
/// 4. **Fence the in-flight temp ledger**, so the next launch's sweep finds
///    every partial an abandoned worker is still writing.
/// 5. **Exit.**
fn tear_down_and_exit(host: &dyn QuitHost, drain: Duration) {
    let started = Instant::now();
    host.cancel_all();

    let deadline = started + drain;
    while Instant::now() < deadline {
        if !host.operations().iter().any(blocks_quit) {
            break;
        }
        std::thread::sleep(DRAIN_POLL);
    }

    host.abort_all();
    host.flush_temp_ledger();
    log::info!(
        target: "quit",
        "everything stopped in {:?}; exiting",
        started.elapsed()
    );
    host.exit();
}

/// The process-wide gate. One per app, so ⌘Q and a window close share one
/// countdown instead of racing two.
static GATE: LazyLock<Arc<QuitGate>> = LazyLock::new(|| Arc::new(QuitGate::with_timings(QuitTimings::default())));

pub(crate) fn gate() -> &'static Arc<QuitGate> {
    &GATE
}

/// The real outside world: the operation manager, the main window, and the app.
pub(crate) struct TauriQuitHost {
    app: tauri::AppHandle,
}

impl TauriQuitHost {
    pub(crate) fn new(app: tauri::AppHandle) -> Arc<Self> {
        Arc::new(Self { app })
    }
}

impl QuitHost for TauriQuitHost {
    fn operations(&self) -> Vec<OperationSnapshot> {
        crate::file_system::write_operations::list_operations()
    }

    fn announce(&self, event: QuitRequested) {
        use tauri_specta::Event as _;
        if let Err(e) = event.emit(&self.app) {
            // The dialog is the only thing lost; the deadline still fires, so the
            // app still quits. That's the whole point of owning the timer here.
            log::warn!(target: "quit", "couldn't tell the windows about the pending quit: {e}");
        }
    }

    fn cancel_all(&self) {
        crate::file_system::write_operations::cancel_all_write_operations();
    }

    fn abort_all(&self) {
        crate::file_system::write_operations::abort_all_write_operations();
    }

    fn flush_temp_ledger(&self) {
        crate::file_system::write_operations::flush_in_flight_temps();
    }

    fn exit(&self) {
        // Comes back around as `RunEvent::ExitRequested`, where the gate is now
        // `Quitting` and waves it through to `RunEvent::Exit`.
        self.app.exit(0);
    }
}

/// Answers a quit request from an app-level entry point. See
/// [`QuitGate::request_quit`].
pub(crate) fn request_quit(app: &tauri::AppHandle) -> QuitOutcome {
    gate().request_quit(TauriQuitHost::new(app.clone()))
}

#[cfg(test)]
mod tests;
