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

use super::super::state::{WriteOperationState, is_cancelled};
use crate::file_system::volume::VolumeError;

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

/// Is this failure a transport blip worth another try, or a decision to report?
///
/// Matched exhaustively on purpose: a new [`VolumeError`] variant should not
/// inherit a retry policy by falling into a wildcard. Classification is by TYPE
/// (and, for the one `IoError` case, by errno), never by message text.
pub(super) fn is_retryable(err: &VolumeError) -> bool {
    match err {
        // The M2 typed errors. `smb2`'s send deadline (`SendTimeout`), its
        // response deadline (`Timeout`), and credit starvation all classify as
        // `ErrorKind::TimedOut` and land here. Every one of them means "this
        // connection is dead but the socket doesn't know it yet", which is
        // exactly the case a fresh attempt on a rebuilt session rescues.
        VolumeError::ConnectionTimeout(_) => true,
        // SMB `ConnectionLost` / `SessionExpired`: the backend has its own
        // reconnect path, so the next attempt runs on a new session. On MTP the
        // device really is gone and every attempt fails immediately — a bounded
        // 1.25 s of extra waiting, and no data at risk.
        VolumeError::DeviceDisconnected(_) => true,
        // The variant's own doc says retrying in a few seconds works: the MTP
        // session died and a reopen is already running in the background.
        VolumeError::DeviceSessionReset(_) => true,
        // The destination folder's cached handle was re-keyed and the backend has
        // already refreshed it, so the next attempt uses the fresh one.
        VolumeError::StaleDestinationHandle(_) => true,
        // A write onto an OS-mounted network share (`/Volumes/naspi`) surfaces the
        // transport failure as an errno rather than a typed backend error. The
        // allowlist is the same one `error_classification.rs` maps to
        // `ConnectionInterrupted`, so the two agree on what "the link blipped"
        // means.
        VolumeError::IoError {
            raw_os_error: Some(code),
            ..
        } => is_transient_errno(*code),

        // Everything below is a decision or a fact about the data, not a blip.
        // Re-running the write would fail the same way and only delay the report.
        VolumeError::Cancelled(_) // the user asked us to stop; ❌ never retry
        | VolumeError::NotFound(_)
        | VolumeError::PermissionDenied(_)
        | VolumeError::AlreadyExists(_)
        | VolumeError::NotSupported
        | VolumeError::ReadOnly(_)
        | VolumeError::StorageFull { .. }
        | VolumeError::IsADirectory(_)
        | VolumeError::InvalidName(_) // the destination can't hold this name; a rename is the only fix
        | VolumeError::DeletePending(_)
        | VolumeError::IoError { raw_os_error: None, .. }
        | VolumeError::NeedsPassword { .. }
        | VolumeError::FriendlyGit(_) => false,
    }
}

/// The errnos that mean "the link went away", not "the write was refused".
#[cfg(unix)]
fn is_transient_errno(code: i32) -> bool {
    matches!(
        code,
        libc::ENOTCONN | libc::ENETDOWN | libc::ENETUNREACH | libc::EHOSTUNREACH | libc::ETIMEDOUT | libc::ECONNRESET
    )
}

#[cfg(not(unix))]
fn is_transient_errno(_code: i32) -> bool {
    false
}

/// How long to wait after `failed_attempt` (1-based) before running the file
/// again. 250 ms, then 1 s.
pub(super) fn backoff_after(failed_attempt: u32) -> Duration {
    let factor = BACKOFF_FACTOR.saturating_pow(failed_attempt.saturating_sub(1));
    FIRST_BACKOFF.saturating_mul(factor).min(MAX_BACKOFF)
}

/// Should the file run again after `failed_attempt` (1-based) hit `err`?
///
/// Three independent gates, and a cancel beats all of them: a cancelled operation
/// never starts another attempt, however retryable the error looked.
pub(super) fn should_retry(err: &VolumeError, failed_attempt: u32, state: &Arc<WriteOperationState>) -> bool {
    !is_cancelled(&state.intent) && failed_attempt < MAX_ATTEMPTS && is_retryable(err)
}

/// Waits out the backoff for `failed_attempt`, returning `false` the moment the
/// operation is cancelled.
///
/// ❌ Not a plain `sleep`: a cancel must win instantly, and a retry that outlives
/// a cancel is the hang this whole effort exists to remove, wearing a new hat.
pub(super) async fn wait_before_retry(state: &Arc<WriteOperationState>, failed_attempt: u32) -> bool {
    if is_cancelled(&state.intent) {
        return false;
    }
    tokio::select! {
        biased;
        () = state.backend_cancel.cancelled() => false,
        () = tokio::time::sleep(backoff_after(failed_attempt)) => !is_cancelled(&state.intent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::test_support::TestOperationGuard;

    /// The transport blips a retry exists for. Each one means "this connection is
    /// dead but the write itself was never refused".
    #[test]
    fn transport_blips_are_retryable() {
        assert!(is_retryable(&VolumeError::ConnectionTimeout("send timed out".into())));
        assert!(is_retryable(&VolumeError::DeviceDisconnected("session lost".into())));
        assert!(is_retryable(&VolumeError::DeviceSessionReset("ptp reset".into())));
        assert!(is_retryable(&VolumeError::StaleDestinationHandle("/Documents".into())));
        assert!(is_retryable(&VolumeError::IoError {
            message: "network is down".into(),
            raw_os_error: Some(libc::ENETDOWN),
        }));
    }

    /// A cancel is the user's decision, and re-running the write would fight it.
    /// This is the single most important negative in the whole policy.
    #[test]
    fn a_cancel_is_never_retryable() {
        assert!(!is_retryable(&VolumeError::Cancelled(
            "Operation cancelled by user".into()
        )));
    }

    /// Refusals and facts about the data: running the write again fails the same
    /// way and only delays the report the user needs.
    #[test]
    fn refusals_and_data_facts_are_not_retryable() {
        assert!(!is_retryable(&VolumeError::PermissionDenied("/x".into())));
        assert!(!is_retryable(&VolumeError::NotFound("/x".into())));
        assert!(!is_retryable(&VolumeError::AlreadyExists("/x".into())));
        assert!(!is_retryable(&VolumeError::ReadOnly("/x".into())));
        assert!(!is_retryable(&VolumeError::StorageFull {
            message: "disk full".into()
        }));
        assert!(!is_retryable(&VolumeError::NotSupported));
        assert!(!is_retryable(&VolumeError::IsADirectory("/x".into())));
        assert!(!is_retryable(&VolumeError::DeletePending("/x".into())));
        assert!(!is_retryable(&VolumeError::NeedsPassword { wrong_attempt: true }));
        // An IoError with no errno carries nothing to classify on, so it is a
        // report, not a blip.
        assert!(!is_retryable(&VolumeError::IoError {
            message: "something".into(),
            raw_os_error: None,
        }));
        // A refusal that DOES carry an errno still isn't transport.
        assert!(!is_retryable(&VolumeError::IoError {
            message: "permission".into(),
            raw_os_error: Some(libc::EACCES),
        }));
    }

    /// The whole point of the cap: a retry loop must terminate. Attempt 3 of 3 is
    /// the last one, however retryable its error was.
    #[test]
    fn the_attempt_cap_terminates_the_loop() {
        let guard = TestOperationGuard::register("retry-cap");
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
        let guard = TestOperationGuard::register("retry-cancelled");
        let err = VolumeError::ConnectionTimeout("send timed out".into());
        assert!(should_retry(&err, 1, guard.state()));
        super::super::super::state::cancel_write_operation(guard.id(), false);
        assert!(!should_retry(&err, 1, guard.state()));
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
        let guard = TestOperationGuard::register("retry-backoff-cancel");
        let state = Arc::clone(guard.state());
        let op_id = guard.id().to_owned();
        tokio::spawn(async move {
            // allowed-test-sleep: the head start IS the subject — the cancel has to land
            // INSIDE the running 250 ms backoff, and there is no condition to wait on
            // (a backoff in progress publishes nothing).
            tokio::time::sleep(Duration::from_millis(20)).await;
            super::super::super::state::cancel_write_operation(&op_id, false);
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
        let guard = TestOperationGuard::register("retry-backoff-ok");
        assert!(wait_before_retry(guard.state(), 1).await);
    }
}
