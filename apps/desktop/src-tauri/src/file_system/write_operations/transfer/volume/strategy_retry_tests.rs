//! Retrying one FILE after a transport blip (`retry.rs`, wired into
//! `stream_pipe_file`).
//!
//! What these prove, in order of how much they'd cost to get wrong:
//!
//! 1. A blip no longer kills the transfer: the file runs again and lands.
//! 2. The loop TERMINATES. A retry that can spin forever is the 2026-07-31 hang
//!    in a new costume, so the attempt cap is pinned by count, not by hope.
//! 3. A cancel outranks every retry, during the write and during the backoff.
//! 4. A refusal (permission, disk full, a missing source) is reported at once —
//!    retrying it only delays the answer the user needs.
//! 5. Data safety across an attempt boundary: nothing byte-incomplete ever wears
//!    the final name, the abandoned attempt's partial is gone, the operation's
//!    in-flight temp set is clean, and a safe-replace keeps the ORIGINAL intact
//!    the whole way through — including when every attempt fails.
//!
//! What they do NOT prove: anything about a REAL backend's behavior under a real
//! blip. `FlakyDest` fails where a test tells it to; whether `smb2`'s typed errors
//! arrive at the moments assumed here is `smb_full_concurrency_test.rs`'s and the
//! smb2 crate's business.

use super::test_support::{FlakyDest, make_state};
use super::*;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::transfer::{retry, transfer_probe};
use crate::ignore_poison::IgnorePoison;

const PAYLOAD: &[u8] = b"the-bytes-that-must-arrive-whole";

/// An in-memory source holding one file at `/a.txt`.
async fn source_with_payload() -> Arc<dyn Volume> {
    let inner = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    inner.create_file(Path::new("/a.txt"), PAYLOAD).await.unwrap();
    inner as Arc<dyn Volume>
}

/// Runs one file through the copy engine against `dest`.
async fn copy_one(
    source: &Arc<dyn Volume>,
    dest: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    staging: WriteStaging,
    dest_path: &str,
) -> Result<u64, VolumeError> {
    // These tests assert on the VolumeError variant, not on which path failed
    // (they copy one known file), so drop the path the engine attaches.
    copy_single_path(
        source,
        Path::new("/a.txt"),
        Some(false),
        None,
        dest,
        Path::new(dest_path),
        state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|_| {},
        None,
        staging,
    )
    .await
    .map_err(|e| e.error)
}

/// The headline: a transport blip takes out one attempt, the file runs again, and
/// the transfer carries on with every byte intact. Before this, that single
/// failed write ended the whole operation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transport_blip_runs_the_file_again_and_it_lands_whole() {
    let guard = TestOperationGuard::register_state("retry-blip", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(1, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let bytes = copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt")
        .await
        .expect("a transport blip must not end the transfer");

    assert_eq!(bytes, PAYLOAD.len() as u64);
    assert_eq!(flaky.write_calls(), 2, "the blip, then the attempt that landed");
    assert_eq!(
        flaky.read("/a.txt").await.as_deref(),
        Some(PAYLOAD),
        "the retried file must arrive whole, not doubled or truncated"
    );
}

/// The single most important property of a retry loop: it stops. Three attempts,
/// then the failure is reported — never a fourth, never a spin.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_never_recovers_gives_up_at_the_attempt_cap() {
    let guard = TestOperationGuard::register_state("retry-cap", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(usize::MAX, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let err = copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt")
        .await
        .expect_err("a destination that never recovers must surface the failure");

    assert!(matches!(err, VolumeError::ConnectionTimeout(_)), "got {err:?}");
    assert_eq!(
        flaky.write_calls(),
        retry::MAX_ATTEMPTS as usize,
        "the loop must terminate at the cap"
    );
}

/// A refusal is an answer, not a blip. Retrying it would fail identically twice
/// more and delay the report the user is waiting for.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_is_reported_at_once_and_never_retried() {
    let guard = TestOperationGuard::register_state("retry-refusal", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(usize::MAX, VolumeError::PermissionDenied("/a.txt".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let err = copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt")
        .await
        .expect_err("a refusal must surface");

    assert!(matches!(err, VolumeError::PermissionDenied(_)), "got {err:?}");
    assert_eq!(flaky.write_calls(), 1, "a refusal must be reported on the first try");
}

/// Cancel wins. A cancelled operation schedules no further attempt, and the error
/// it reports is the cancel — so the post-loop emits `write-cancelled` rather than
/// logging the user's own click as a transport failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_backoff_ends_the_retries() {
    let guard = TestOperationGuard::register_state("retry-cancel", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(usize::MAX, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let op_id = guard.id().to_owned();
    let flaky_for_watcher = Arc::clone(&flaky);
    tokio::spawn(async move {
        // Cancel as soon as the first attempt has failed and the backoff started.
        crate::test_support::wait_until_async(Duration::from_secs(5), "the first write attempt to fail", || {
            flaky_for_watcher.write_calls() >= 1
        })
        .await;
        crate::file_system::write_operations::state::cancel_write_operation(&op_id, false);
    });

    let err = copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt")
        .await
        .expect_err("a cancelled copy must not report success");

    assert!(
        matches!(err, VolumeError::Cancelled(_)),
        "a cancel must be reported as a cancel, not as the transport error that triggered the retry; got {err:?}"
    );
    assert!(
        flaky.write_calls() < retry::MAX_ATTEMPTS as usize,
        "the cancel must cut the attempts short, not let them run out (ran {})",
        flaky.write_calls()
    );
}

/// The data-safety core. Across an attempt boundary:
/// - the abandoned attempt's partial is gone (it was not the backend's to clean),
/// - nothing byte-incomplete ever wore the final name,
/// - the operation's in-flight temp set is empty, so nothing is left to be swept
///   or to hide from the pane forever.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_leaves_no_partial_behind_and_never_lands_one_at_the_final_name() {
    let guard = TestOperationGuard::register_state("retry-no-litter", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(1, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt")
        .await
        .expect("the retried copy must succeed");

    let names = flaky.names().await;
    assert_eq!(
        names,
        vec!["a.txt".to_string()],
        "exactly the finished file must remain: no `.cmdr-tmp-*` from the abandoned attempt"
    );
    assert!(
        guard.state().in_flight_temps.lock_ignore_poison().is_empty(),
        "a retried file must not leave an in-flight temp registered"
    );
}

/// Every failed attempt's partial is dropped, not just the last one. A file that
/// exhausts its attempts must leave the destination as it found it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_exhausts_its_attempts_leaves_nothing_behind() {
    let guard = TestOperationGuard::register_state("retry-exhausted-clean", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(usize::MAX, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let _ = copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt").await;

    assert!(
        flaky.names().await.is_empty(),
        "a copy that failed every attempt must leave the destination empty, got {:?}",
        flaky.names().await
    );
    assert!(guard.state().in_flight_temps.lock_ignore_poison().is_empty());
}

/// Overwrite is not reversible, so a retry must never make it worse. Under a
/// caller-staged safe-replace the ORIGINAL stays untouched through every attempt:
/// the retry rewrites the caller's temp, and only the caller's `finalize` (which
/// runs after this returns) ever touches the original.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retried_safe_replace_keeps_the_original_intact_the_whole_way() {
    let guard = TestOperationGuard::register_state("retry-safe-replace", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(1, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;
    flaky.inner.create_file(Path::new("/a.txt"), b"ORIGINAL").await.unwrap();

    // The conflict layer's shape: the write targets a temp sibling it minted, and
    // the original is swapped in later by `finalize_safe_replace`.
    let temp = "/a.txt.cmdr-tmp-retrytest";
    let bytes = copy_one(&source, &dest, guard.state(), WriteStaging::AlreadyStaged, temp)
        .await
        .expect("the retried safe-replace write must land in the temp");

    assert_eq!(bytes, PAYLOAD.len() as u64);
    assert_eq!(
        flaky.read("/a.txt").await.as_deref(),
        Some(&b"ORIGINAL"[..]),
        "the original must be untouched until the caller finalizes"
    );
    assert_eq!(
        flaky.read(temp).await.as_deref(),
        Some(PAYLOAD),
        "the caller's temp must hold the complete new bytes after the retry"
    );
}

/// The same, when the blip never clears: an Overwrite that fails every attempt
/// must leave the user's existing file exactly where it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_safe_replace_that_fails_every_attempt_still_leaves_the_original() {
    let guard = TestOperationGuard::register_state("retry-safe-replace-fail", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(usize::MAX, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;
    flaky.inner.create_file(Path::new("/a.txt"), b"ORIGINAL").await.unwrap();

    let _ = copy_one(
        &source,
        &dest,
        guard.state(),
        WriteStaging::AlreadyStaged,
        "/a.txt.cmdr-tmp-retrytest",
    )
    .await;

    assert_eq!(
        flaky.read("/a.txt").await.as_deref(),
        Some(&b"ORIGINAL"[..]),
        "a failed Overwrite must never cost the user the file it was replacing"
    );
}

/// The stale MTP folder handle keeps working through the general policy, and now
/// gets the full budget rather than a single extra try.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_destination_handle_still_retries() {
    let guard = TestOperationGuard::register_state("retry-stale", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(2, VolumeError::StaleDestinationHandle("/Documents".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt")
        .await
        .expect("a re-keyed destination folder handle must be retried");
    assert_eq!(flaky.write_calls(), 3);
}

/// The rollback ledger records each destination file ONCE, whatever the attempt
/// count. A double entry would make Rollback try to delete the same path twice
/// (noisy, and it masks a real failure) and would make the journal's per-leaf
/// rows disagree with what is on disk.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retried_child_is_recorded_in_the_rollback_ledger_exactly_once() {
    let guard = TestOperationGuard::register_state("retry-ledger", make_state());
    let src = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    src.create_directory(Path::new("/tree")).await.unwrap();
    src.create_file(Path::new("/tree/one.txt"), b"one").await.unwrap();
    src.create_file(Path::new("/tree/two.txt"), b"two").await.unwrap();
    let source: Arc<dyn Volume> = src as Arc<dyn Volume>;

    // Only `two.txt` blips, and only once.
    let flaky = FlakyDest::new(1, VolumeError::ConnectionTimeout("send timed out".into())).only_for("two.txt");
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let created = CreatedPaths::default();
    copy_single_path(
        &source,
        Path::new("/tree"),
        Some(true),
        None,
        &dest,
        Path::new("/tree"),
        guard.state(),
        &created,
        &|_, _| ControlFlow::Continue(()),
        &|_| {},
        None,
        WriteStaging::Stage,
    )
    .await
    .expect("the retried child must not fail the directory copy");

    let files = created.files.lock_ignore_poison().clone();
    let mut sorted = files.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        files.len(),
        sorted.len(),
        "a retried child must appear once in the ledger, got {files:?}"
    );
    assert_eq!(files.len(), 2, "both children must be recorded: {files:?}");
}

/// M4.2 meeting M4.1: a write that will never return, never error, and never
/// report a byte is ended by the watchdog, and the retry then lands the file.
///
/// This is the whole point of the effort in one test. The 2026-07-31 wedge had
/// exactly this shape — a write parked forever with nothing to bound it — and the
/// only way out was force-quitting the app, which is what cost the user two
/// files. Now the watchdog ends the wait, the wait becomes a typed error, and the
/// file runs again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_watchdog_ends_a_wedged_write_and_the_file_runs_again() {
    let guard = TestOperationGuard::register_state("retry-wedge", make_state());
    let source = source_with_payload().await;
    let wedged = test_support::WedgedThenWorkingDest::new();
    let dest: Arc<dyn Volume> = Arc::clone(&wedged) as Arc<dyn Volume>;

    // Short enough to observe, long enough to need several watchdog ticks. Read
    // on THIS thread when the operation registers, which is why the override is
    // thread-local while the watchdog itself runs elsewhere.
    let _window = transfer_probe::StallAbortGuard::set(Duration::from_secs(1));
    let op_probe = transfer_probe::register_operation(
        guard.id(),
        1,
        1,
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
        // The keepalive verdict the pinned smb2 can't give. Without it the
        // watchdog reports and never acts, which is production today — so this
        // test would hang on the wedged write instead of proving anything.
        vec![crate::file_system::write_operations::transfer::liveness_test_support::dead_connection_volume()],
        Arc::clone(guard.state()),
        Arc::new(crate::file_system::write_operations::event_sinks::CollectorEventSink::new()),
    );
    let task = op_probe.probe().begin_task(0, "/a.txt", "/a.txt");
    let handle = task.probe();

    let bytes = transfer_probe::CURRENT_TASK_PROBE
        .scope(
            handle,
            copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt"),
        )
        .await
        .expect("the file must land on the attempt after the watchdog ended the wedged one");

    assert_eq!(bytes, PAYLOAD.len() as u64);
    assert_eq!(wedged.write_calls(), 2, "the wedged write, then the one that landed");
    assert!(
        op_probe.probe().render_dump("test").contains("stall-aborts=1"),
        "the dump must say the watchdog ended a wait: {}",
        op_probe.probe().render_dump("test")
    );
}

/// The probe has to say a file was retried. A silent retry turns "this file took
/// three tries" and "this file never happened" into the same log.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_retry_shows_up_in_the_in_flight_table() {
    let guard = TestOperationGuard::register_state("retry-visible", make_state());
    let source = source_with_payload().await;
    let flaky = FlakyDest::new(1, VolumeError::ConnectionTimeout("send timed out".into()));
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let op_probe = transfer_probe::register_operation(
        guard.id(),
        1,
        1,
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
        // No liveness verdict: this test is about the retry being VISIBLE, not
        // about the watchdog acting.
        Vec::new(),
        Arc::clone(guard.state()),
        Arc::new(crate::file_system::write_operations::event_sinks::CollectorEventSink::new()),
    );
    let task = op_probe.probe().begin_task(0, "/a.txt", "/a.txt");
    let handle = task.probe();

    transfer_probe::CURRENT_TASK_PROBE
        .scope(
            handle,
            copy_one(&source, &dest, guard.state(), WriteStaging::Stage, "/a.txt"),
        )
        .await
        .expect("the retried copy must succeed");

    let dump = op_probe.probe().render_dump("test");
    assert!(
        dump.contains("retries=1"),
        "the dump must record that the file was run again; got {dump}"
    );
}
