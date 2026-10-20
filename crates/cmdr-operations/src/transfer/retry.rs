//! When one FILE inside a transfer is worth running again, and how long to wait
//! before doing it.
//!
//! **Why the file, not the operation.** Before this, a single failed write ended
//! the whole transfer: 12 files into a 764-file copy, one write that never came
//! back took the other 752 with it
//! (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Now that a dead
//! SMB session surfaces as a typed error instead of hanging (`smb2`'s send and
//! response deadlines, plus `Error::ServerUnresponsive`), the file that hit the
//! blip can simply be run again and the batch carries on.
//!
//! **Why it lives at `stream_pipe_file` and nowhere higher.** That function is the
//! one place a file's bytes are streamed, for a top-level file source and for a
//! deep-merge child alike, and everything a retry must not redo has already
//! happened above it:
//!
//! - Conflict resolution ran on the driver, so a retry re-prompts nobody and
//!   re-decides nothing; it re-runs the write the user already approved.
//! - The rollback ledger (`CreatedPaths`), the journal, and the per-file progress
//!   milestone are all recorded from the CALLER's `Ok` arm, so a retried file is
//!   recorded exactly once no matter how many attempts it took.
//! - Staging is re-derived per attempt, so each attempt writes a fresh
//!   `.cmdr-tmp-*` and the previous one is dropped before the next begins.
//!
//! **Bounded, always.** The bug this whole effort exists to kill is an infinite
//! hang, so a retry loop that can spin forever would reintroduce it in a new
//! costume. Attempts are capped ([`MAX_ATTEMPTS`]), every wait between them is
//! short and cancel-aware, and no error class is retried that isn't a transport
//! blip.

use std::sync::Arc;
use std::time::Duration;

use super::super::state::WriteOperationState;
use super::transfer_probe::{TaskPhase, note_task_retry, set_task_phase};
use super::volume_outage::OutageEnd;
use cmdr_fs::log_error::LogError;
use cmdr_fs::volume::VolumeError;

/// How many times one file's write may run in total, the first try included.
///
/// Three is the smallest number that survives the shape we actually saw: a blip
/// takes out the attempt in flight, the next one runs on a session the backend
/// has since torn down and rebuilt, and the third is the one that finds a healthy
/// connection. A fourth adds latency to a genuinely broken destination without
/// adding a case it rescues — past two failures the problem isn't a blip.
pub(super) const MAX_ATTEMPTS: u32 = 3;

/// How long to wait after the first failed attempt.
///
/// Long enough that an SMB reconnect or an MTP session reset has somewhere to
/// happen, short enough to be invisible on a healthy transfer that hits one
/// stale MTP folder handle.
const FIRST_BACKOFF: Duration = Duration::from_millis(250);

/// Multiplier per further attempt, so the waits are 250 ms then 1 s.
const BACKOFF_FACTOR: u32 = 4;

/// Ceiling on a single wait. With [`MAX_ATTEMPTS`] at 3 nothing reaches it today;
/// it exists so raising the cap can never turn the backoff into a long park.
const MAX_BACKOFF: Duration = Duration::from_secs(2);

/// How long to wait after `failed_attempt` (1-based) before running the file
/// again. 250 ms, then 1 s.
pub(super) fn backoff_after(failed_attempt: u32) -> Duration {
    let factor = BACKOFF_FACTOR.saturating_pow(failed_attempt.saturating_sub(1));
    FIRST_BACKOFF.saturating_mul(factor).min(MAX_BACKOFF)
}

/// Should the file run again after `failed_attempt` (1-based) hit `err`?
///
/// Three independent gates, and a stop beats all of them: an operation whose
/// work stopped (a cancel, or a sibling's failure draining the window) never
/// starts another attempt, however retryable the error looked. Only a
/// [`VolumeError::transport_fault`] is worth another try: a refusal or a fact
/// about the data fails the same way again and only delays the report.
pub(super) fn should_retry(err: &VolumeError, failed_attempt: u32, state: &Arc<WriteOperationState>) -> bool {
    !state.work_stopped() && failed_attempt < MAX_ATTEMPTS && err.transport_fault().is_some()
}

/// Waits out the backoff for `failed_attempt`, returning `false` the moment the
/// operation's work stops.
///
/// ❌ Not a plain `sleep`: a cancel must win instantly, and a retry that outlives
/// a cancel is the hang this whole effort exists to remove, wearing a new hat.
pub(super) async fn wait_before_retry(state: &Arc<WriteOperationState>, failed_attempt: u32) -> bool {
    if state.work_stopped() {
        return false;
    }
    tokio::select! {
        biased;
        () = state.stop_work.cancelled() => false,
        () = tokio::time::sleep(backoff_after(failed_attempt)) => !state.work_stopped(),
    }
}

/// How the wait after a landing step the transport cut off (a rename, a swap's
/// clearing delete, a publish's replace) ended, once a blip with nothing to
/// wait for has had the per-file retry: `ended` is
/// `volume_outage::wait_after_failure`'s answer for `error`, and `attempt` is
/// the file's count (1-based).
///
/// - [`OutageEnd::Back`] gives the file a fresh set of attempts, as a write's
///   outage does.
/// - [`OutageEnd::NotWaitable`] (a stale MTP handle, an OS-mounted share's
///   timeout, a live session's timeout) takes a write's gates and backoff
///   ([`should_retry`], [`wait_before_retry`]): `Back` once the backoff ran,
///   `Cancelled` when the work stopped during it, and `NotWaitable` still when
///   there's no retry for it (a refusal, or the attempts are spent).
///
/// ❗ On `Back` the caller RESUMES (`CutOffLanding::resume`), ❌ never gives up
/// on a blip: the step may have been performed, and only the resume's stat
/// says so. Giving up parked the new bytes under ` (recovered)` with the
/// original already gone, or named a temp a performed rename had moved.
pub(crate) async fn after_a_cut_off_landing_wait(
    ended: OutageEnd,
    error: &VolumeError,
    attempt: &mut u32,
    state: &Arc<WriteOperationState>,
) -> OutageEnd {
    match ended {
        OutageEnd::Back => {
            *attempt = 1;
            OutageEnd::Back
        }
        OutageEnd::NotWaitable if should_retry(error, *attempt, state) => {
            log::warn!(
                target: "copy",
                "landing: attempt {attempt} of {MAX_ATTEMPTS} was cut off ({error}) with nothing to wait for; \
                 settling it again in {:?}",
                backoff_after(*attempt), error = LogError(error),
            );
            note_task_retry();
            set_task_phase(TaskPhase::WaitingToRetry);
            if !wait_before_retry(state, *attempt).await {
                return OutageEnd::Cancelled;
            }
            *attempt += 1;
            OutageEnd::Back
        }
        ended => ended,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestOperations;

    /// Only a transport fault earns another attempt (which errors are one:
    /// `cmdr-fs` `transport_fault_test.rs`). A refusal fails the same way again.
    #[test]
    fn a_refusal_schedules_no_further_attempt() {
        let engine = TestOperations::new();
        let guard = engine.register("retry-refusal");
        let refused = VolumeError::PermissionDenied {
            path: "/x".into(),
            raw_os_error: None,
        };
        assert!(!should_retry(&refused, 1, guard.state()));
        assert!(!should_retry(
            &VolumeError::Cancelled("Operation cancelled by user".into()),
            1,
            guard.state()
        ));
    }

    /// The whole point of the cap: a retry loop must terminate. Attempt 3 of 3 is
    /// the last one, however retryable its error was.
    #[test]
    fn the_attempt_cap_terminates_the_loop() {
        let engine = TestOperations::new();
        let guard = engine.register("retry-cap");
        let err = VolumeError::ConnectionTimeout("send timed out".into());
        assert!(should_retry(&err, 1, guard.state()));
        assert!(should_retry(&err, 2, guard.state()));
        assert!(
            !should_retry(&err, MAX_ATTEMPTS, guard.state()),
            "the last attempt must not schedule another one"
        );
    }

    /// Cancel outranks a retryable error: the user clicked Cancel and the file is
    /// not being run again.
    #[test]
    fn a_cancelled_operation_schedules_no_further_attempt() {
        let engine = TestOperations::new();
        let guard = engine.register("retry-cancelled");
        let err = VolumeError::ConnectionTimeout("send timed out".into());
        assert!(should_retry(&err, 1, guard.state()));
        engine.cancel_write_operation(guard.id(), false);
        assert!(!should_retry(&err, 1, guard.state()));
    }

    /// A sibling draining after another task's failure never STARTS another
    /// attempt either: the drain exists to stop work, and a fresh write would
    /// open a handle the drain then has to wait out.
    #[test]
    fn a_draining_sibling_schedules_no_further_attempt() {
        let engine = TestOperations::new();
        let guard = engine.register("retry-draining");
        let err = VolumeError::ConnectionTimeout("send timed out".into());
        guard.state().stop_work_after_failure();
        assert!(!should_retry(&err, 1, guard.state()));
    }

    /// The drain ends a backoff already under way, like a cancel does.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failure_drain_ends_the_backoff_immediately() {
        let engine = TestOperations::new();
        let guard = engine.register("retry-backoff-drain");
        let state = Arc::clone(guard.state());
        let draining = Arc::clone(&state);
        tokio::spawn(async move {
            // allowed-test-sleep: the drain has to land INSIDE the running backoff,
            // which publishes nothing to wait on.
            tokio::time::sleep(Duration::from_millis(20)).await;
            draining.stop_work_after_failure();
        });
        let started = tokio::time::Instant::now();
        assert!(!wait_before_retry(&state, 1).await, "a draining sibling doesn't go on");
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "the backoff ends on the drain, not on its timer (took {:?})",
            started.elapsed()
        );
    }

    /// Every wait is short and rises, and the total a file can spend backing off
    /// is a number we can state: 1.25 s.
    #[test]
    fn the_backoff_is_bounded_and_rising() {
        assert_eq!(backoff_after(1), Duration::from_millis(250));
        assert_eq!(backoff_after(2), Duration::from_millis(1000));
        let total: Duration = (1..MAX_ATTEMPTS).map(backoff_after).sum();
        assert_eq!(
            total,
            Duration::from_millis(1250),
            "the whole retry budget a file can add has to stay statable"
        );
        // However far the cap is ever raised, one wait stays short.
        assert_eq!(backoff_after(99), MAX_BACKOFF);
    }

    /// The wait must end the instant a cancel lands, not when the timer would
    /// have expired. Otherwise a cancelled transfer sits in a backoff nobody
    /// asked for.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_cancel_ends_the_backoff_immediately() {
        let engine = TestOperations::new();
        let guard = engine.register("retry-backoff-cancel");
        let state = Arc::clone(guard.state());
        let op_id = guard.id().to_owned();
        tokio::spawn(async move {
            // allowed-test-sleep: the head start IS the subject — the cancel has to land
            // INSIDE the running 250 ms backoff, and there is no condition to wait on
            // (a backoff in progress publishes nothing).
            tokio::time::sleep(Duration::from_millis(20)).await;
            engine.cancel_write_operation(&op_id, false);
        });
        let started = tokio::time::Instant::now();
        let go_on = wait_before_retry(&state, 1).await;
        assert!(!go_on, "a cancelled operation must not go on to another attempt");
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "the backoff has to end on the cancel, not on its timer (took {:?})",
            started.elapsed()
        );
    }

    /// An uncancelled wait runs its course and says "go on".
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_uncancelled_backoff_lets_the_next_attempt_run() {
        let engine = TestOperations::new();
        let guard = engine.register("retry-backoff-ok");
        assert!(wait_before_retry(guard.state(), 1).await);
    }
}
