//! The hard-abort tier (Q2): ending a wait the backend is never going to end,
//! and the tier-1 invariant that survives it.
//!
//! Tier 1 (`state.backend_cancel`) is how every user-initiated cancel travels: it
//! reaches the backend through the per-chunk `on_progress` callback, so the
//! backend drops its own handle and deletes its own partial. That is the right
//! wind-down and it is deliberately NOT a `select!` around the write — dropping
//! the write future would skip that cleanup.
//!
//! What tier 1 cannot do is bound the wait. A write that never calls
//! `on_progress` never sees the cancel, and SMB's own deadlines are 20 s to send
//! plus 30 s of server silence, so one chunk can hold a quit for ~30 s. Tier 2
//! (`state.backend_abort`) is the deadline holder's answer: the streaming write is
//! raced against it, so the wait ends on our clock. It costs the backend's own
//! cleanup, which is why nothing a user clicks can reach it.
//!
//! So this suite is two halves, and the second is the load-bearing one:
//!
//! 1. An abort ends a wedged open and a wedged write, promptly, without retrying
//!    and without going back through the wedged connection to tidy up.
//! 2. An ordinary cancel still routes through tier 1, with the backend deleting
//!    its own partial and `backend_abort` never firing.

use super::test_support::{SlowSource, TierOneWitnessDest, WedgedOpenSource, WedgedThenWorkingDest, make_state};
use super::*;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError};
use crate::file_system::write_operations::state::{abort_write_operation, cancel_write_operation, is_cancelled};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::wait_until_async;

const PAYLOAD: &[u8] = b"the-bytes-that-were-still-in-flight";

/// How long a wait-for-a-known-state may take before the test gives up.
/// Generous: these run on a loaded CI box.
const WAIT: Duration = Duration::from_secs(5);

/// How long an aborted copy may take to return. Short on purpose: the whole
/// point of tier 2 is that the answer doesn't wait on the backend, and the
/// wedged doubles here never return at all, so a miss is a hang, not a slow pass.
const ABORT_WITHIN: Duration = Duration::from_secs(2);

/// An in-memory source holding one file at `/a.txt`.
async fn source_with_payload() -> Arc<dyn Volume> {
    let inner = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    inner.create_file(Path::new("/a.txt"), PAYLOAD).await.unwrap();
    inner as Arc<dyn Volume>
}

/// Runs one file through the copy engine. These tests assert on the error
/// variant, not on which path failed, so the engine's path label is dropped.
async fn copy_one(
    source: &Arc<dyn Volume>,
    source_path: &str,
    dest: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
) -> Result<u64, VolumeError> {
    copy_single_path(
        source,
        Path::new(source_path),
        Some(false),
        None,
        dest,
        Path::new("/a.txt"),
        state,
        &CreatedPaths::default(),
        on_progress,
        &|_| {},
        None,
        WriteStaging::Stage,
    )
    .await
    .map_err(|e| e.error)
}

// ========================================================================
// Tier 2: the wait ends on our clock.
// ========================================================================

/// The headline. A destination write that never returns, never errors, and never
/// reports a byte is the shape that cost a user a force-quit on 2026-07-31, and
/// the one that would hold a quit for ~30 s on a real SMB session. The abort ends
/// it, and the file reports as cancelled.
///
/// Without the tier-2 arm in `stream_pipe_file` this test hangs: nothing else in
/// the per-file path can end that await.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hard_abort_ends_a_wedged_write_instead_of_waiting_for_the_backend() {
    let op = TestOperationGuard::register_state("abort-wedged-write", make_state());
    let source = source_with_payload().await;
    let wedged = WedgedThenWorkingDest::new();
    let dest: Arc<dyn Volume> = Arc::clone(&wedged) as Arc<dyn Volume>;
    let is_wedged = Arc::clone(&wedged.wedged);

    let copy = copy_one(&source, "/a.txt", &dest, op.state(), &|_, _| ControlFlow::Continue(()));
    tokio::pin!(copy);

    // Each wait rides its own `select!` arm alongside the copy: the copy only
    // advances while it is being polled.
    tokio::select! {
        r = &mut copy => panic!("a write that never returns can't finish on its own: {r:?}"),
        () = wait_until_async(WAIT, "the destination write to wedge", || is_wedged.load(Ordering::SeqCst)) => {}
    }

    let started = tokio::time::Instant::now();
    abort_write_operation(op.id());
    let err = tokio::time::timeout(ABORT_WITHIN, copy)
        .await
        .expect("tier 2 must end the wait; without it this write never returns")
        .expect_err("an aborted file reports as cancelled, not as bytes copied");

    assert!(
        matches!(err, VolumeError::Cancelled(_)),
        "an abort must report as a cancel so nothing retries it and the post-loop emits write-cancelled; got {err:?}"
    );
    assert!(
        started.elapsed() < ABORT_WITHIN,
        "the wait must end on the abort, not on a timer (took {:?})",
        started.elapsed()
    );
    assert_eq!(
        wedged.write_calls(),
        1,
        "❌ an abort is not a transport blip: the file must not run again (this double's SECOND write would succeed)"
    );
}

/// The other half of the wedge shape: the device round trip that opens the source
/// hangs before a byte has moved. The serial driver awaits each file directly, so
/// nothing above `stream_pipe_file` can end that wait either.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hard_abort_ends_a_wedged_source_open() {
    let op = TestOperationGuard::register_state("abort-wedged-open", make_state());
    let opening = Arc::new(AtomicBool::new(false));
    let source: Arc<dyn Volume> = Arc::new(WedgedOpenSource {
        opening: Arc::clone(&opening),
    });
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest").with_space_info(10_000_000, 10_000_000));

    let copy = copy_one(&source, "/a.txt", &dest, op.state(), &|_, _| ControlFlow::Continue(()));
    tokio::pin!(copy);

    tokio::select! {
        r = &mut copy => panic!("an open that never returns can't finish on its own: {r:?}"),
        () = wait_until_async(WAIT, "the source open to wedge", || opening.load(Ordering::SeqCst)) => {}
    }

    abort_write_operation(op.id());
    let err = tokio::time::timeout(ABORT_WITHIN, copy)
        .await
        .expect("tier 2 must end the wait on a source open too")
        .expect_err("an aborted file reports as cancelled");
    assert!(matches!(err, VolumeError::Cancelled(_)), "got {err:?}");
}

/// Cleanup after an abort is the staging layer's job, ❌ never the backend's: the
/// delete would go back through the very connection that just failed to answer,
/// which is how the quit deadline gets held a second time. So the partial stays
/// registered as in-flight and the startup sweep removes it
/// (`write_operations/in_flight_temps.rs`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hard_abort_leaves_the_partial_for_the_sweep_rather_than_deleting_it_through_the_backend() {
    let op = TestOperationGuard::register_state("abort-leaves-partial", make_state());
    let source = source_with_payload().await;
    let wedged = WedgedThenWorkingDest::new();
    let dest: Arc<dyn Volume> = Arc::clone(&wedged) as Arc<dyn Volume>;
    let is_wedged = Arc::clone(&wedged.wedged);

    let copy = copy_one(&source, "/a.txt", &dest, op.state(), &|_, _| ControlFlow::Continue(()));
    tokio::pin!(copy);
    tokio::select! {
        r = &mut copy => panic!("the write must still be wedged: {r:?}"),
        () = wait_until_async(WAIT, "the destination write to wedge", || is_wedged.load(Ordering::SeqCst)) => {}
    }

    abort_write_operation(op.id());
    tokio::time::timeout(ABORT_WITHIN, copy)
        .await
        .expect("tier 2 must end the wait")
        .expect_err("an aborted file reports as cancelled");

    let temps = op.state().in_flight_temps.lock_ignore_poison().clone();
    assert_eq!(
        temps.len(),
        1,
        "the abandoned partial must stay registered so the startup sweep finds it; got {temps:?}"
    );
    assert!(
        temps[0].to_string_lossy().contains(".cmdr-tmp-"),
        "and it must wear the recoverable scratch marker, never a real name; got {}",
        temps[0].display()
    );
}

// ========================================================================
// Tier 1 survives: the regression guard.
// ========================================================================

/// The invariant the two-tier split exists to protect. A user's Cancel must still
/// travel through `on_progress`, so the BACKEND drops its handle and removes its
/// own partial — the reason writes are not raced against `backend_cancel`
/// (`transfer/DETAILS.md` § "Two tiers of cancel").
///
/// Turning tier 2 on for an ordinary cancel turns this test red on three separate
/// assertions: the backend's cleanup never runs, `backend_abort` is set, and the
/// staged temp is left behind instead of removed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_ordinary_cancel_still_routes_through_tier_one_and_the_backend_deletes_its_own_partial() {
    let op = TestOperationGuard::register_state("cancel-stays-in-tier-one", make_state());
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let source: Arc<dyn Volume> = Arc::new(SlowSource {
        gate: Arc::clone(&gate),
    });
    let witness = TierOneWitnessDest::new();
    let dest: Arc<dyn Volume> = Arc::clone(&witness) as Arc<dyn Volume>;
    let written = Arc::clone(&witness.written);

    let bytes_seen = Arc::new(AtomicU64::new(0));
    let seen = Arc::clone(&bytes_seen);
    // Tier 1 IS this callback: production's per-chunk progress closure is what
    // carries the cancel to the backend (see `strategy::pull_path_to_local`), so a
    // test that hard-codes `Continue` would be measuring nothing.
    let cancel_state = Arc::clone(op.state());
    let on_progress = move |done: u64, _total: u64| {
        seen.store(done, Ordering::SeqCst);
        if is_cancelled(&cancel_state.intent) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let copy = copy_one(&source, "/big.bin", &dest, op.state(), &on_progress);
    tokio::pin!(copy);

    // One chunk through: the write is provably mid-file, with a partial on the
    // destination for the backend to clean up.
    gate.add_permits(1);
    tokio::select! {
        r = &mut copy => panic!("a 30-chunk copy can't be done after one permit: {r:?}"),
        () = wait_until_async(WAIT, "the first chunk to reach the destination", || written.load(Ordering::SeqCst) > 0) => {}
    }

    cancel_write_operation(op.id(), false);
    // Let the next chunk flow so the destination's loop reaches its `on_progress`
    // check — which is exactly the cooperative path under test.
    gate.add_permits(1);
    let err = tokio::time::timeout(WAIT, copy)
        .await
        .expect("a cancelled copy must return")
        .expect_err("a cancelled file reports as cancelled");

    assert!(matches!(err, VolumeError::Cancelled(_)), "got {err:?}");
    assert!(
        witness.own_cleanup_ran.load(Ordering::SeqCst),
        "❌ tier-1 regression: the backend never got to remove its own partial, so the cancel skipped the cooperative path"
    );
    assert!(
        !op.state().backend_abort.is_cancelled(),
        "❌ tier-2 leak: a user's Cancel must never fire the hard abort"
    );
    assert!(
        op.state().in_flight_temps.lock_ignore_poison().is_empty(),
        "a cooperative cancel abandons its staged temp there and then; nothing is left for the sweep"
    );
    assert!(
        witness.names().await.is_empty(),
        "and nothing may sit at the destination, under a real name or a temp one; got {:?}",
        witness.names().await
    );
    assert!(
        bytes_seen.load(Ordering::SeqCst) > 0,
        "the fixture must really have streamed"
    );
}
