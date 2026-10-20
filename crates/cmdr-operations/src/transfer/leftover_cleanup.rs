//! What a stopped transfer's or archive edit's cleanup can't remove at once,
//! removed in the background, and what even that can't remove, handed to the
//! ledger's reconnect sweep.
//!
//! **A Cancel never waits on a dropped share, and neither does a failure.**
//! `volume/cleanup.rs::clean_abandoned_staged_writes` deletes inline only what a
//! LIVE destination takes, within [`CleanupPolicy::inline_budget`]; the terminal
//! event goes out the moment that's done. A temp the destination couldn't take
//! (it was out, a delete met a transport fault, or it refused) comes here: ONE
//! pass per stopped operation, spawned on the engine's runtime, holding no lane,
//! so the next queued job on that share starts as usual. A remote zip edit's
//! or a compress's upload temp that its `StagedWrite::abandon` couldn't delete
//! comes here the same way, from its terminal
//! (`archive_edit/engine.rs::EditLeftovers`), on the parent's drive.
//!
//! The pass, in order:
//!
//! 1. **A destination that's out gets one reconnect attempt**, the operation's
//!    own and the only dial anywhere in this cleanup, then a short wait
//!    ([`CleanupPolicy::wait`]) for someone else's reconnect (a pane's backoff).
//!    An outage the operation already gave up on (the lane's fifteen-minute wait
//!    ran out, `volume_outage.rs`) gets the attempt as its one look, and no wait.
//! 2. **A temp the destination refused is asked again**, up to three times
//!    from [`CleanupPolicy::retry_after`] on, each wait doubling, whether it was
//!    refused at the stop or the moment the destination came back: a write the
//!    drain's deadline dropped closes its handle a moment after the drop when no
//!    credit was on hand, and the session an outage dropped can hold its temp
//!    open until the server notices that session is gone (on Samba, a moment
//!    after the new one is up, seconds when it's busy, `#578`). An SMB
//!    reconnect names the dropped session so the server ends it first (`#581`),
//!    but Samba ignores that for a guest login, so the retries stay for guest
//!    shares and the in-flight write's late close. One refused for good (smb2#24's handle, held until the session ends) gets its
//!    answer on the next reconnect, when that session is gone.
//! 3. **What's still there goes to the ledger's pending set, ONCE, after the pass
//!    is done** (`in_flight_temps::keep_temp_for_arrival`), so this pass and the
//!    arrival sweep never act on one record at the same time. The sweep runs on
//!    the volume's next arrival or in-place reconnect, and never dials.
//!
//! ❗ **Tier 2 and the quit end it where it is** (`state.backend_abort`, and
//! [`LeftoverCleanups::stop`], which the quit deadline fires for passes whose
//! operation already unregistered). The records stay registered and on disk for
//! the startup sweep: nothing goes back through the connection after tier 2.
//!
//! ❌ Only removable temps reach this pass: what `state.in_flight_temps` still
//! listed. A landing (a complete file whose way may have been cleared) is
//! recorded as one and handed to the arrival sweep under its own kind, whose rule
//! finishes the landing; it's ❌ never deleted from here.
//!
//! [`CleanupPolicy::inline_budget`]: crate::outage_slot::CleanupPolicy::inline_budget
//! [`CleanupPolicy::wait`]: crate::outage_slot::CleanupPolicy::wait
//! [`CleanupPolicy::retry_after`]: crate::outage_slot::CleanupPolicy::retry_after

use std::path::PathBuf;
use std::sync::Arc;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use super::super::in_flight_temps::{self, RecordHome};
use super::super::state::WriteOperationState;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{ConnectionState, Volume, VolumeError};

/// Every background cleanup pass one engine runs, and the switch the quit
/// deadline throws on all of them.
pub(crate) struct LeftoverCleanups {
    /// Fired by the quit deadline (`Operations::abort_all_write_operations`):
    /// a pass whose operation already left the registry can't hear its own
    /// `backend_abort`.
    quit: CancellationToken,
    /// The passes still running, so a test can wait for them to settle.
    passes: TaskTracker,
}

impl LeftoverCleanups {
    pub(crate) fn new() -> Self {
        Self {
            quit: CancellationToken::new(),
            passes: TaskTracker::new(),
        }
    }

    /// The app is quitting: every pass stops where it is and leaves its records
    /// for the startup sweep, and none starts from here on.
    pub(crate) fn stop(&self) {
        self.quit.cancel();
    }

    /// Waits until every pass spawned so far has finished.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) async fn settled(&self) {
        self.passes.close();
        self.passes.wait().await;
        self.passes.reopen();
    }
}

/// The temps the inline cleanup couldn't take, by why.
// DEFAULT-OK: an empty hand-off is exactly a cleanup that took everything.
#[derive(Debug, Default)]
pub(crate) struct LeftBehind {
    /// The destination was out, or a delete met a transport fault.
    pub(crate) out: Vec<PathBuf>,
    /// The destination answered and refused.
    pub(crate) refused: Vec<PathBuf>,
}

impl LeftBehind {
    fn is_empty(&self) -> bool {
        self.out.is_empty() && self.refused.is_empty()
    }

    fn len(&self) -> usize {
        self.out.len() + self.refused.len()
    }
}

/// Hands `left` to a background pass on `volume` and returns at once. `home` is
/// the path space their records live in: a transfer's destination
/// (`in_flight_temps::dest_home`), or the parent's drive for an archive edit,
/// whose state names no destination (`archive_edit/engine.rs::EditLeftovers`).
/// See the module docs.
pub(crate) fn finish_in_background(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    home: Option<RecordHome>,
    left: LeftBehind,
) {
    if left.is_empty() {
        return;
    }
    let cleanups = state.scope().stores().leftover_cleanups();
    let quit = cleanups.quit.clone();
    let pass = Pass {
        volume: Arc::clone(volume),
        state: Arc::clone(state),
        home,
    };
    cleanups
        .passes
        .spawn_on(run(pass, left, quit), &state.scope().host().runtime());
}

/// One background pass: the volume it deletes on, the stopped operation, and
/// where that operation recorded its temps.
struct Pass {
    volume: Arc<dyn Volume>,
    state: Arc<WriteOperationState>,
    home: Option<RecordHome>,
}

async fn run(pass: Pass, left: LeftBehind, quit: CancellationToken) {
    let count = left.len();
    let (volume, state) = (&pass.volume, &pass.state);
    let remaining = tokio::select! {
        biased;
        () = state.backend_abort.cancelled() => None,
        () = quit.cancelled() => None,
        remaining = take_away(&pass, left) => Some(remaining),
    };
    let Some(remaining) = remaining else {
        log::info!(
            target: "copy",
            "leftover cleanup: the app is shutting down, leaving {count} staged partial(s) on {} to the startup sweep",
            LogPrivate::volume(volume.name()),
        );
        return;
    };
    if remaining.is_empty() {
        log::info!(
            target: "copy",
            "leftover cleanup: removed the {count} staged partial(s) a stop left on {}",
            LogPrivate::volume(volume.name()),
        );
        return;
    }
    log::info!(
        target: "copy",
        "leftover cleanup: {} of {count} staged partial(s) stay on {} until it next arrives or reconnects",
        remaining.len(),
        LogPrivate::volume(volume.name()),
    );
    for temp in &remaining {
        in_flight_temps::keep_temp_for_arrival(state, pass.home.clone(), temp);
    }
}

/// How many more times the pass asks for a refused temp, each wait twice the
/// one before it, from [`CleanupPolicy::retry_after`]: 3 s, 6 s, and 12 s by
/// default, about the pass's own wait for a share that's out.
///
/// [`CleanupPolicy::retry_after`]: crate::outage_slot::CleanupPolicy::retry_after
const REFUSAL_RETRIES: u32 = 3;

/// Deletes what it can of `left`, and answers what's still there.
///
/// A temp the share refuses the moment it's back gets the same retries as one
/// refused at the stop: the session the outage dropped can hold it open until
/// the server notices that session is gone, which Samba does a moment after
/// the new one is up, and seconds later when it's busy (`#578`). A named SMB
/// login's reconnect ends that session up front (`#581`); a guest's can't.
async fn take_away(pass: &Pass, left: LeftBehind) -> Vec<PathBuf> {
    let (volume, state) = (&pass.volume, &pass.state);
    if !left.out.is_empty() || !reads_live(volume) {
        bring_back(volume, state).await;
    }
    let Deletes {
        mut remaining,
        mut refused,
    } = delete_each(pass, left.out).await;
    refused.extend(left.refused);
    let mut pause = state.cleanup_policy.retry_after;
    for _ in 0..REFUSAL_RETRIES {
        if refused.is_empty() {
            break;
        }
        tokio::time::sleep(pause).await;
        pause = pause.saturating_mul(2);
        let round = delete_each(pass, std::mem::take(&mut refused)).await;
        remaining.extend(round.remaining);
        refused = round.refused;
    }
    remaining.extend(refused);
    remaining
}

/// What one round of deletes left.
// DEFAULT-OK: an empty round is exactly one that took everything.
#[derive(Default)]
struct Deletes {
    /// Not tried (the destination was out) or not answered in time.
    remaining: Vec<PathBuf>,
    /// The destination answered and refused.
    refused: Vec<PathBuf>,
}

/// Deletes each of `temps` the destination takes, retiring its record.
async fn delete_each(pass: &Pass, temps: Vec<PathBuf>) -> Deletes {
    let (volume, state) = (&pass.volume, &pass.state);
    let mut round = Deletes::default();
    for temp in temps {
        if !reads_live(volume) {
            round.remaining.push(temp);
            continue;
        }
        match tokio::time::timeout(state.outage_policy.attempt_timeout, volume.delete(&temp)).await {
            Ok(Ok(()) | Err(VolumeError::NotFound(_))) => {
                in_flight_temps::retire_temp(state, pass.home.clone(), &temp);
            }
            Ok(Err(error)) => {
                log::debug!(
                    target: "copy",
                    "leftover cleanup: {} is refused ({error})",
                    LogPrivate::file_path(&temp), error = LogError(&error),
                );
                round.refused.push(temp);
            }
            Err(_elapsed) => round.remaining.push(temp),
        }
    }
    round
}

/// Whether `volume` takes calls right now: a live session, or no session to
/// lose at all.
fn reads_live(volume: &Arc<dyn Volume>) -> bool {
    volume.connection_state().is_none_or(ConnectionState::is_live)
}

/// One reconnect attempt for a destination that's out, then a short wait for
/// someone else's, unless the operation already gave this outage up.
async fn bring_back(volume: &Arc<dyn Volume>, state: &Arc<WriteOperationState>) {
    let policy = state.cleanup_policy;
    let deadline = Instant::now() + policy.wait;
    if volume.connection_state() != Some(ConnectionState::Disconnected) {
        // Live, no session at all, or waiting on a person (a sign-in, a host
        // key): nothing a dial or the clock fixes.
        return;
    }
    let attempt_timeout = state.outage_policy.attempt_timeout.min(policy.wait);
    match tokio::time::timeout(attempt_timeout, volume.attempt_reconnect()).await {
        Ok(Ok(())) => return,
        Ok(Err(error)) => {
            log::debug!(target: "copy", "leftover cleanup: the reconnect attempt failed: {error}", error = LogError(&error))
        }
        Err(_elapsed) => log::debug!(target: "copy", "leftover cleanup: the reconnect attempt didn't return"),
    }
    let outages = state.scope().stores().outages();
    if outages.gave_up_on(&volume.lane_key(), state.born, state.outage_policy.budget) {
        // The operation already waited this outage out: one look, ❌ no
        // second wait.
        return;
    }
    while !reads_live(volume) && volume.connection_state() == Some(ConnectionState::Disconnected) {
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        tokio::time::sleep_until(deadline.min(now + state.outage_policy.poll_every)).await;
    }
}

#[cfg(test)]
#[path = "leftover_cleanup_tests.rs"]
mod tests;
