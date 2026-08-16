//! Picking a first index back up when it stopped with ground still to walk.
//!
//! A phase gets two passes (`phases::MAX_PASSES_PER_PHASE`) and a folder created
//! on already-covered ground becomes frontier, because the live reconciler writes
//! its row without reading it. So a drive somebody is writing to fast enough can
//! leave the machine stopping with a non-empty frontier, and nothing marks it
//! complete. Measured, that takes ~200 new folders a second sustained
//! (`docs/notes/churn-against-completion-2026-08-15.md`); the next launch settles
//! the same drive in about two seconds. This is what stops the wait being "the
//! next launch" for somebody who leaves Cmdr open for a week.
//!
//! ## What a retry is, and what it is not
//!
//! Each attempt is the ORDINARY resume: the phases restart, recompute their queue,
//! and walk what the frontier still names — the same ~2 s of work a relaunch does.
//! ❌ Nothing here changes when a volume counts as complete. While the frontier is
//! non-empty the drive genuinely isn't covered and every surface saying so is
//! right; this makes the drive converge sooner, ❌ never claim completion earlier.
//!
//! ## Why a backoff, and why it repeats forever
//!
//! **1 min → 5 min → 15 min, per volume, the last step repeating.** The first step
//! is for the ordinary case: the writing was a burst, it is over, and one cheap
//! resume finishes the drive. The tail is for a drive being written to for hours,
//! where each attempt is work that will lose the same race again — 15 minutes
//! bounds that to four attempts an hour, each a couple of seconds.
//!
//! ❌ No give-up after N attempts: an attempt costs ~2 s, and stopping would
//! recreate the stuck-until-relaunch state this exists to remove. The ladder resets
//! when the drive completes, so the next unfinished session starts at a minute
//! again.
//!
//! **In memory, ❌ never persisted**, which is the one place this parts company
//! with the abandoned-ground backoff it copies (`../../writer/abandoned_retry.rs`).
//! A relaunch already resumes the volume and settles it, so a stored window would
//! buy no behavior — it would only describe a wait nobody is still waiting on.
//!
//! ## The rule that makes it safe
//!
//! **Two machines walking one volume is the failure the whole subsystem is built
//! to prevent**, so a retry that finds work in flight runs NOTHING and comes back
//! later: `state::resume_the_phases` asks `phases_have_work` with the manager held
//! out of the registry, which is the same mutual exclusion every scan entry uses.
//! One window per volume and a claim that moves it before the attempt runs mean
//! retries can't stack either.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;

use super::manager::PhaseResume;
use super::{master, state};

/// How long to wait before each successive attempt. The last entry repeats
/// forever; see the module docs for why each step is what it is.
const BACKOFF: [Duration; 3] = [
    Duration::from_secs(60),
    Duration::from_secs(5 * 60),
    Duration::from_secs(15 * 60),
];

/// One volume's wait: when it started, and which step of [`BACKOFF`] it is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RetryWindow {
    /// Unix seconds when this window opened.
    opened_at: u64,
    /// Index into [`BACKOFF`], saturated at its last entry.
    step: usize,
}

impl RetryWindow {
    /// This window's length.
    fn length(self) -> Duration {
        BACKOFF[self.step.min(BACKOFF.len() - 1)]
    }

    /// Whether the window has elapsed at `now`.
    ///
    /// A window that opened in the FUTURE counts as elapsed: a backwards clock
    /// jump must not park a drive's retries for years, and being wrong costs one
    /// early attempt.
    fn is_due(self, now: u64) -> bool {
        now < self.opened_at || now - self.opened_at >= self.length().as_secs()
    }
}

/// The volumes waiting for another go, one window each. A volume that finished,
/// or never stopped short, holds no entry.
static WAITING: LazyLock<Mutex<HashMap<String, RetryWindow>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Note that this volume's machine stopped with ground still on its frontier, so
/// something should offer it another pass.
///
/// ⚠️ **Deliberately a no-op while a window is open**, exactly as the
/// abandoned-ground backoff is: the machine an attempt starts usually stops short
/// again (the writing is still going), and re-arming there would pin the ladder at
/// one minute forever and retry a busy drive every minute for as long as somebody
/// is writing to it.
pub(in crate::indexing) fn arm(volume_id: &str, now: u64) {
    WAITING
        .lock_ignore_poison()
        .entry(volume_id.to_string())
        .or_insert(RetryWindow {
            opened_at: now,
            step: 0,
        });
}

/// Take this volume's attempt if its window has elapsed.
///
/// **Taking MOVES the window**, before the attempt runs and under the same lock
/// that reads it, so nothing can take a second one out from under the first: a
/// retry that then declines to run (a machine already working) has rescheduled
/// itself for another window of the same length by having been claimed at all.
fn claim_if_due(volume_id: &str, now: u64) -> bool {
    let mut waiting = WAITING.lock_ignore_poison();
    let Some(window) = waiting.get_mut(volume_id) else {
        return false;
    };
    if !window.is_due(now) {
        return false;
    }
    window.opened_at = now;
    true
}

/// An attempt started, so the next wait is the longer one.
fn note_it_ran(volume_id: &str, now: u64) {
    if let Some(window) = WAITING.lock_ignore_poison().get_mut(volume_id) {
        window.opened_at = now;
        window.step = (window.step + 1).min(BACKOFF.len() - 1);
    }
}

/// Stop waiting for this volume: it completed, or it stopped indexing.
///
/// The ladder resets here rather than growing forever, so a drive that finishes
/// and is later written to hard again starts at a minute rather than a quarter of
/// an hour.
pub(in crate::indexing) fn forget(volume_id: &str) {
    WAITING.lock_ignore_poison().remove(volume_id);
}

/// Whether this volume is waiting for another go, asked without spending it.
#[cfg(test)]
pub(in crate::indexing) fn is_waiting(volume_id: &str) -> bool {
    WAITING.lock_ignore_poison().contains_key(volume_id)
}

/// Offer this volume its next attempt if one is due. The production door, called
/// from the volume's own maintenance tick.
///
/// A 30 s tick against windows of minutes: the granularity is the tick's, so an
/// attempt lands up to half a minute late, which is a rounding error against the
/// wait it ends. A volume with nothing waiting costs one map lookup.
pub(in crate::indexing) fn nudge(volume_id: &str) {
    nudge_at(volume_id, crate::indexing::store::now_unix());
}

/// The attempt itself, at a given moment. [`nudge`] is the production door; this
/// is what a test drives when it doesn't want to wait a minute for one.
pub(in crate::indexing::lifecycle) fn nudge_at(volume_id: &str, now: u64) {
    if !claim_if_due(volume_id, now) {
        return;
    }
    // The master switch outranks a wait that started before it went off. Master-off
    // stops every volume, so nothing is coming back for this one until the user
    // turns it on again — which routes through the normal per-drive enable.
    if !master::master_enabled() {
        forget(volume_id);
        return;
    }
    match state::resume_the_phases(volume_id) {
        PhaseResume::Started => {
            note_it_ran(volume_id, now);
            log::info!("Completion retry: '{volume_id}' stopped short, so its phases pick the rest up now");
        }
        // The claim above already pushed the window out by its own length, so this
        // comes back after another wait of the same length rather than at the next
        // tick — and ❌ nothing here runs alongside the machine that is working.
        PhaseResume::AlreadyWorking => {
            log::debug!("Completion retry: '{volume_id}' is already covering ground, so its retry waits");
        }
        // Completed, torn down, or no longer the machine's to cover. Nothing left
        // to retry, and the next stop-short arms a fresh ladder.
        PhaseResume::NothingToCover => {
            forget(volume_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60;

    /// The window from the module docs, and the property the whole thing rests on:
    /// a machine that stopped short is offered another pass a minute later without
    /// anybody relaunching the app.
    #[test]
    fn a_volume_that_stopped_short_waits_a_minute() {
        forget("retry-first-step");
        arm("retry-first-step", 1_000);

        assert!(!claim_if_due("retry-first-step", 1_000 + MINUTE - 1), "not due yet");
        assert!(claim_if_due("retry-first-step", 1_000 + MINUTE), "and then it is");
    }

    /// A volume nothing armed is never retried: the ladder exists only while a
    /// machine has actually stopped with ground left.
    #[test]
    fn a_volume_that_finished_is_never_retried() {
        forget("retry-unarmed");

        assert!(!claim_if_due("retry-unarmed", 1_000_000));
    }

    /// 1 → 5 → 15 → 15…, and ❌ never a give-up. Each step is only ever reached by
    /// an attempt that RAN, which is what makes the tail mean "this drive keeps
    /// losing the race" rather than "this drive was busy once".
    #[test]
    fn the_ladder_grows_to_a_quarter_of_an_hour_and_stays_there() {
        forget("retry-ladder");
        arm("retry-ladder", 0);

        let mut now = 0;
        for expected in [1, 5, 15, 15, 15] {
            let due = now + expected * MINUTE;
            assert!(
                !claim_if_due("retry-ladder", due - 1),
                "a {expected}-minute wait can't be due a second early"
            );
            assert!(claim_if_due("retry-ladder", due), "a {expected}-minute wait ends");
            note_it_ran("retry-ladder", due);
            now = due;
        }
    }

    /// The rule the abandoned-ground backoff learned first: a machine that stops
    /// short AGAIN, right after an attempt, must not restart the wait at a minute.
    /// Without this a drive somebody writes to all afternoon is retried every
    /// minute all afternoon.
    #[test]
    fn stopping_short_again_doesnt_pin_the_ladder_at_a_minute() {
        forget("retry-rearm");
        arm("retry-rearm", 0);
        assert!(claim_if_due("retry-rearm", MINUTE));
        note_it_ran("retry-rearm", MINUTE);

        arm("retry-rearm", MINUTE); // the attempt's machine stopped short too

        assert!(
            !claim_if_due("retry-rearm", MINUTE + MINUTE),
            "a minute after the attempt is the FIRST step's wait, and this volume is past it"
        );
        assert!(claim_if_due("retry-rearm", MINUTE + 5 * MINUTE), "five minutes is");
    }

    /// Completing resets the ladder, so a drive that finishes and is written to
    /// hard again next week waits a minute rather than a quarter of an hour.
    #[test]
    fn completing_resets_the_ladder() {
        forget("retry-reset");
        arm("retry-reset", 0);
        assert!(claim_if_due("retry-reset", MINUTE));
        note_it_ran("retry-reset", MINUTE);

        forget("retry-reset"); // the drive completed
        arm("retry-reset", 10_000);

        assert!(
            claim_if_due("retry-reset", 10_000 + MINUTE),
            "the next ladder starts at the first step"
        );
    }

    /// Claiming moves the window under the lock that read it, so two callers in the
    /// same moment produce ONE attempt. Retries that stacked would put two machines
    /// on one volume, which is the failure this subsystem is built to prevent.
    #[test]
    fn two_callers_in_the_same_moment_get_one_attempt() {
        forget("retry-no-stacking");
        arm("retry-no-stacking", 0);

        assert!(claim_if_due("retry-no-stacking", MINUTE), "the first caller takes it");
        assert!(
            !claim_if_due("retry-no-stacking", MINUTE),
            "and the second finds nothing due"
        );
    }

    /// Per volume, like every other invariant in the subsystem: one drive's wait
    /// says nothing about another's.
    #[test]
    fn two_volumes_back_off_independently() {
        forget("retry-vol-one");
        forget("retry-vol-two");
        arm("retry-vol-one", 0);

        assert!(!claim_if_due("retry-vol-two", 10 * MINUTE), "nothing armed this one");
        assert!(claim_if_due("retry-vol-one", MINUTE));
    }

    /// A clock that jumps backwards (a timezone-confused sleep, a corrected NTP
    /// step) must not park a drive's retries for years.
    #[test]
    fn a_backwards_clock_costs_one_early_attempt_rather_than_years_of_none() {
        forget("retry-clock-jump");
        arm("retry-clock-jump", 10_000);

        assert!(
            claim_if_due("retry-clock-jump", 5_000),
            "a window that opened later is due now"
        );
    }
}
