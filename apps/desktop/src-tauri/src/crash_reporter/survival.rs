//! After the panic: the two things only the LIVE process can record on its own crash file.
//!
//! Both are post-hoc amendments to a report the hook already wrote, both are idempotent, and
//! both are driven from outside this module ([`super::note_app_still_running`] from
//! `app_lifecycle.rs`, [`super::note_in_session_report_delivered`] from the error reporter).
//! The first is proof the app outlived the panic, and is what the rest of this doc is about;
//! the second is proof the user already heard about it, at § Already delivered in-session.
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

use super::{AppFate, CrashReport, read_crash_report, write_crash_report};
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
    amend(crash_path, "the app survived the panic", |report| {
        if report.app_fate != AppFate::Unconfirmed {
            return false;
        }
        report.app_fate = AppFate::KeptRunning;
        true
    });
}

// --- Already delivered in-session ---

/// Records that the panic in the crash file at `crash_path` has already gone out through the
/// error reporter's Flow B, so the next launch deletes it instead of offering it again.
///
/// Driven from `error_reporter::auto_dispatcher` at the ONE point that means "delivered": a
/// Flow B bundle came back from `upload` with an `Ok`. Everything that can go wrong before then
/// (the `updates.errorReports` gate returning early, the bundle failing to build, the upload
/// being refused) leaves the report unstamped and so still offered, which is the direction that
/// costs the user nothing.
pub(super) fn record_in_session_delivery(crash_path: &Path) {
    amend(crash_path, "the panic was reported in-session", |report| {
        if report.reported_in_session {
            return false;
        }
        report.reported_in_session = true;
        true
    });
}

/// Reads the pending report, lets `change` amend it, and writes it back only if `change` says it
/// changed something. `what` names the fact for the log line.
///
/// Tolerant of every failure: an amendment only ever makes an existing report MORE accurate, so a
/// missing file, a corrupt one, or a write that doesn't land costs some precision at the next
/// launch and never costs anyone a report.
fn amend(crash_path: &Path, what: &str, change: impl FnOnce(&mut CrashReport) -> bool) {
    let Some(mut report) = read_crash_report(crash_path) else {
        return;
    };
    if !change(&mut report) {
        return;
    }
    if let Err(e) = write_crash_report(crash_path, &report) {
        log::warn!("Crash reporter: couldn't record that {what}: {e}");
    }
}

#[cfg(test)]
pub(super) fn arm_after_for_test(crash_path: &Path, delay: Duration) -> Option<JoinHandle<()>> {
    arm_after(crash_path, delay)
}
