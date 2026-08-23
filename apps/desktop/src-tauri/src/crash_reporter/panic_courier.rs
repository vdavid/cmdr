//! In-session delivery of a panic the app survived.
//!
//! The panic hook writes `crash-report.json` and the next launch reports it. That covers
//! a panic that kills the app. It does NOT cover a panic on a background thread, which
//! (since the lock-poison policy in [`cmdr_fs::ignore_poison`]) leaves the app running:
//! the news then waits for however long it takes the user to relaunch.
//!
//! This module closes that gap. The hook hands a small notice to a **courier thread**,
//! which logs the panic and opens a Flow B window
//! ([`crate::error_reporter::auto_dispatcher`]). Flow B's own 60 s debounce is what sorts
//! fatal from non-fatal, with no heuristic: a fatal panic kills the process long before
//! the timer fires, so only a survived panic actually ships in-session.
//!
//! ## Why a separate thread and not a call from the hook
//!
//! **A panic raised inside a panic hook aborts the process outright**, and `catch_unwind`
//! can't stop it: `std::panicking` flips a thread-local `in_panic_hook` bit around the
//! hook call, and the next `panic!` on that thread hits
//! `panic_count::MustAbort::PanicInHook` and calls `abort()` before any unwinding starts
//! (`library/std/src/panicking.rs`, verified against Rust 1.97.1, 2026-08-23). So nothing
//! that can panic may run inline in the hook, and no guard inline in the hook can make it
//! safe.
//!
//! A second thread has its own panic count and its own `in_panic_hook` bit, both zero, so
//! there `catch_unwind` works normally. Same reason for the second hazard: the hook must
//! not take a lock the panicking thread might already hold (`log`'s, the dispatcher's),
//! because a `std::sync::Mutex` is not reentrant and re-locking it self-deadlocks the app
//! forever. Off-thread, that same contention is a wait of microseconds until the panicking
//! thread unwinds and drops the guard.
//!
//! Rate limiting is one courier at a time ([`COURIER_RUNNING`]), which is also the
//! reentrancy guard: if the courier itself panics, the hook re-enters `notify` on the
//! courier thread and finds the flag set. A panic storm therefore costs one short-lived
//! thread at a time, and the log line each one writes goes through the log coalescer.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

/// Log target for a survived panic. Becomes the Flow B window's category, so it reads as
/// `crash_reporter::panic` in the log and in the report's user note.
pub(super) const PANIC_LOG_TARGET: &str = "cmdr_lib::crash_reporter::panic";

/// Thread name for the courier, so it's identifiable in a later crash report's
/// `thread_name` and in a debugger.
const COURIER_THREAD_NAME: &str = "cmdr-panic-courier";

/// Placeholder for a panic whose payload wasn't a string (a custom `panic_any` type).
const NO_MESSAGE: &str = "(no panic message)";

/// Placeholder for a panic on a thread nobody named.
const UNNAMED_THREAD: &str = "<unnamed>";

/// True while a courier thread is alive. Doubles as the reentrancy guard: a panic raised
/// BY the courier re-enters the hook on the courier's own thread, where this is set.
static COURIER_RUNNING: AtomicBool = AtomicBool::new(false);

/// What the hook hands over. Every field is already in its final, sendable form: the hook
/// builds the crash report first and clones out of it, so the courier does no redaction of
/// its own and can't disagree with what went to disk.
pub(super) struct PanicNotice {
    /// The panic message, already through `sanitize_panic_message` (redacted, then capped).
    pub message: Option<String>,
    /// Name of the thread that panicked, as it goes into the crash report.
    pub thread_name: Option<String>,
    /// Symbol names from the panicking thread's backtrace, as they go into the crash report.
    pub backtrace_frames: Vec<String>,
    /// `CRASH-XXXXX` of the crash file THIS panic wrote, so a next-launch crash report and
    /// this in-session error report can be matched up during triage. `None` when no file
    /// was written for it (no data dir, or keep-first kept an earlier panic's report).
    pub crash_file_short_id: Option<String>,
}

impl PanicNotice {
    /// The single error-level line. Carries the short id so triage can pair this with the
    /// crash report the same panic wrote to disk.
    fn headline(&self) -> String {
        let thread = self.thread_name.as_deref().unwrap_or(UNNAMED_THREAD);
        let message = self.message.as_deref().unwrap_or(NO_MESSAGE);
        match self.crash_file_short_id.as_deref() {
            Some(id) => format!("Panic on thread `{thread}` ({id}): {message}"),
            None => format!("Panic on thread `{thread}`: {message}"),
        }
    }
}

/// Hands a survived-panic notice to a courier thread. **Called from the panic hook, so
/// every line here must be incapable of panicking**: an atomic swap, allocation (which
/// aborts rather than panics if it fails), and `Builder::spawn`, which reports failure as
/// an `Err` instead of panicking the way `thread::spawn` does.
///
/// No-op when a courier is already running.
pub(super) fn notify(notice: PanicNotice) {
    // The handle is dropped, which detaches the thread. Nothing waits for a courier: a
    // fatal panic must not be slowed down by in-session delivery it won't live to see.
    let _detached = spawn_courier(move || deliver(&notice));
}

/// Runs `work` on a fresh thread with its unwind caught, unless a courier is already
/// running. Returns the handle so tests can join; production drops it.
///
/// The `catch_unwind` is the load-bearing part: `work` reaches `log` and the auto-dispatcher,
/// either of which could panic if it's what broke. On this thread that's an ordinary panic
/// that unwinds into the catch, so the process survives and the flag is still released.
fn spawn_courier(work: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    if COURIER_RUNNING.swap(true, Ordering::SeqCst) {
        return None;
    }
    let spawned = std::thread::Builder::new()
        .name(COURIER_THREAD_NAME.to_string())
        .spawn(move || {
            let _unwound = std::panic::catch_unwind(AssertUnwindSafe(work));
            COURIER_RUNNING.store(false, Ordering::SeqCst);
        });
    match spawned {
        Ok(handle) => Some(handle),
        Err(_) => {
            // Out of threads. Release the flag so the next panic can try again.
            COURIER_RUNNING.store(false, Ordering::SeqCst);
            None
        }
    }
}

/// Logs the panic and opens a Flow B window for it.
///
/// Goes through [`crate::error_reporter::report_error`] rather than `log_error!` so the
/// debug-level backtrace record carries the PANICKING thread's frames; a `force_capture()`
/// here would describe the courier's stack. Whether anything leaves the machine is still
/// the auto-dispatcher's call: it returns on the `updates.errorReports` opt-in check, and
/// the log records are local either way.
fn deliver(notice: &PanicNotice) {
    crate::error_reporter::report_error(
        PANIC_LOG_TARGET,
        &notice.headline(),
        &notice.backtrace_frames.join("\n"),
    );
}

#[cfg(test)]
pub(super) fn courier_running_for_test() -> bool {
    COURIER_RUNNING.load(Ordering::SeqCst)
}

#[cfg(test)]
pub(super) fn spawn_courier_for_test(work: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    spawn_courier(work)
}

#[cfg(test)]
pub(super) fn deliver_for_test(notice: &PanicNotice) {
    deliver(notice);
}

#[cfg(test)]
pub(super) fn headline_for_test(notice: &PanicNotice) -> String {
    notice.headline()
}
