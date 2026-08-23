//! Proof that the app outlived the panic its crash file describes.
//!
//! The panic hook writes `crash-report.json` at panic *initiation*, before unwinding, when
//! nothing yet knows whether the process will live. Since the lock-poison policy in
//! [`cmdr_fs::ignore_poison`] a panic on a background thread routinely leaves the app
//! running, so "the app quit unexpectedly" stopped being a safe thing for the next-launch
//! dialog to say. This module is where the missing fact gets recorded.
//!
//! ## Two seams, one fact, one direction
//!
//! Survival is recorded by whichever of these comes first, both calling
//! [`confirm_survival`]:
//!
//! - **The watchdog's timer.** [`arm`] parks a thread for [`SURVIVAL_PROOF_DELAY`]. The
//!   thread reaching its second line IS the proof: a panic that took the app down took
//!   this thread with it.
//! - **The app's own quit path.** `app_lifecycle.rs` calls
//!   [`super::note_app_still_running`] when a quit is requested. An app alive enough to be
//!   asked to quit outlived the panic, and this covers the user who quits sooner than the
//!   timer fires.
//!
//! Both can only ever move a report from [`AppFate::Unconfirmed`] to
//! [`AppFate::KeptRunning`], never the other way, so the fact can be recorded twice (it
//! routinely is) without the two disagreeing.
//!
//! ## Why the absence of a mark means "the app ended"
//!
//! Nothing writes [`AppFate::Ended`] for a panic. `process_pending_crash` infers it at the
//! next launch from a report still sitting at `Unconfirmed`: a living process had
//! `SURVIVAL_PROOF_DELAY` and its entire quit path to say otherwise. That's why the timer
//! only has to be unambiguous rather than long, and why it must never fire early: a
//! premature mark would call a fatal panic a survival, which is the one direction of
//! wrongness the next-launch copy can't absorb.

use super::{AppFate, read_crash_report, write_crash_report};
use std::path::Path;
use std::thread::JoinHandle;
use std::time::Duration;

/// How long the app has to stay up after a panic before the watchdog calls it survived.
///
/// Orders of magnitude above a fatal panic's teardown (a process on its way out goes in
/// milliseconds) and far below a session, so the timer can't mistake one for the other. It
/// buys nothing to make it longer: the quit path records the same fact for anyone who
/// quits sooner.
const SURVIVAL_PROOF_DELAY: Duration = Duration::from_secs(10);

/// Thread name for the watchdog, so it's identifiable in a later crash report's
/// `thread_name` and in a debugger.
const WATCHDOG_THREAD_NAME: &str = "cmdr-panic-survival";

/// Starts the survival watchdog for the crash file the panic hook just wrote.
///
/// ❌ **Called from the panic hook, so every line here must be incapable of panicking**:
/// one `PathBuf` allocation (which aborts rather than panics if it fails) and
/// `Builder::spawn`, which reports failure as an `Err` where `thread::spawn` would panic.
/// Everything that could panic runs on the spawned thread, which has its own panic count
/// and so is an ordinary thread again. The full mechanism, and why no guard inline in the
/// hook could make this safe, is in `panic_courier.rs` § "Why a separate thread and not a
/// call from the hook".
///
/// Keep-first means at most one panic per session writes the crash file, so at most one
/// watchdog is ever armed. Out of threads is a silent no-op: the report then reads as
/// `Ended` at the next launch, the same answer it gave before this module existed.
pub(super) fn arm(crash_path: &Path) {
    // Detached on purpose. Nothing waits for it: a fatal panic must not be slowed down by
    // a timer it won't live to see.
    let _detached = arm_after(crash_path, SURVIVAL_PROOF_DELAY);
}

fn arm_after(crash_path: &Path, delay: Duration) -> Option<JoinHandle<()>> {
    let crash_path = crash_path.to_path_buf();
    std::thread::Builder::new()
        .name(WATCHDOG_THREAD_NAME.to_string())
        .spawn(move || {
            std::thread::sleep(delay);
            confirm_survival(&crash_path);
        })
        .ok()
}

/// Records that the app outlived the panic described by the crash file at `crash_path`.
///
/// Idempotent, and tolerant of every failure: this only ever makes an existing report more
/// truthful, so a missing file, a corrupt one, or a write that doesn't land costs the
/// dialog some precision and never costs anyone a report. Only [`AppFate::Unconfirmed`] is
/// upgraded, so a fate a previous launch already settled can't be rewritten.
pub(super) fn confirm_survival(crash_path: &Path) {
    let Some(mut report) = read_crash_report(crash_path) else {
        return;
    };
    if report.app_fate != AppFate::Unconfirmed {
        return;
    }
    report.app_fate = AppFate::KeptRunning;
    if let Err(e) = write_crash_report(crash_path, &report) {
        log::warn!("Crash reporter: couldn't record that the app survived the panic: {e}");
    }
}

#[cfg(test)]
pub(super) fn arm_after_for_test(crash_path: &Path, delay: Duration) -> Option<JoinHandle<()>> {
    arm_after(crash_path, delay)
}
