//! When one task of a concurrent copy fails, its siblings stop the way a
//! Cancel stops them, and the operation still ends as that failure.
//!
//! The concurrent driver used to drop its whole window on the first failure.
//! A dropped write never runs its backend's own abort and delete, so every
//! sibling's handle stayed open and its `.cmdr-tmp-*` was left to a sweep that
//! the open handle could refuse (#490). Each test here holds the failure until
//! the siblings are mid-stream, so "a sibling was mid-write when its neighbour
//! failed" is a fact rather than a timing bet. Doubles: `wedge_test_support`.

use super::tests::make_state;
use super::wedge_test_support::*;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;

use crate::types::CancelRollbackOutcome;
use cmdr_fs::staging::STAGING_TEMP_MARKER;
use cmdr_fs::testing::wait_until_async;
use std::sync::atomic::Ordering;

/// How long a draining driver may take to return before the test calls it
/// wedged.
const RETURN_WITHIN: Duration = Duration::from_secs(5);

/// How long a test holds still after the failure before it lets the siblings
/// move, so a driver that stopped waiting for them at the failure has long
/// returned by then.
const HOLD_AFTER_FAILURE: Duration = Duration::from_millis(300);

const FAILING: &str = "/fail.bin";
const SIBLINGS: [&str; 3] = ["/a.bin", "/b.bin", "/c.bin"];

/// A source that fails on cue, three that stream gated chunks, and an empty
/// one that lands at once.
async fn drain_fixture() -> (Fixture, Arc<tokio::sync::Semaphore>, Vec<PathBuf>) {
    let (fx, trigger) = fixture_with_failing_read(CHUNK as u64 * 4, Path::new(FAILING));
    fx.source_inner
        .create_file(Path::new("/landed.txt"), b"")
        .await
        .unwrap();
    let mut sources = vec![PathBuf::from("/landed.txt"), PathBuf::from(FAILING)];
    for name in std::iter::once(FAILING).chain(SIBLINGS) {
        fx.source_inner
            .create_file(Path::new(name), &vec![0xAB; CHUNK * 4])
            .await
            .unwrap();
    }
    sources.extend(SIBLINGS.iter().map(PathBuf::from));
    (fx, trigger, sources)
}

fn staged_temps(names: &[String]) -> Vec<&String> {
    names.iter().filter(|n| n.contains(STAGING_TEMP_MARKER)).collect()
}

/// Waits until `/landed.txt` is in and all four streamed sources are writing
/// their temps: the window is full, and every sibling is mid-write.
async fn wait_until_mid_stream(fx: &Fixture) {
    wait_until_async(WAIT, "every streamed source to be mid-write", || {
        fx.landed.load(Ordering::SeqCst) >= 1 && fx.started.load(Ordering::SeqCst) >= 5
    })
    .await;
}

fn is_the_injected_failure(result: &Result<(), WriteFailure>) -> bool {
    matches!(
        result,
        Err(WriteFailure {
            error: WriteOperationError::IoError { .. },
            ..
        })
    )
}

/// The heart of it: once one source fails, every sibling mid-stream stops at its
/// next chunk and its BACKEND removes its own partial, exactly as on a Cancel,
/// while the operation still ends as the failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_lets_every_sibling_close_its_own_write() {
    let engine = TestOperations::new();
    let _drain = CancelDrainGuard::set(RETURN_WITHIN);
    let (fx, trigger, sources) = drain_fixture().await;
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig::default();

    let copy = copy_volumes_with_progress(
        events.clone(),
        "failure-drain-siblings",
        &state,
        Arc::clone(&fx.source),
        &sources,
        Arc::clone(&fx.dest),
        Path::new("/"),
        &config,
    );
    tokio::pin!(copy);

    tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_mid_stream(&fx) => {}
    }
    trigger.add_permits(1);
    let mut early = None;
    tokio::select! {
        r = &mut copy => early = Some(r),
        // allowed-test-sleep: a window in which a driver that stops waiting at the failure returns.
        () = tokio::time::sleep(HOLD_AFTER_FAILURE) => {}
    }
    // Every sibling gets its next chunk, and meets the drain at that chunk's
    // `on_progress`.
    fx.gate.add_permits(100);
    let result = match early {
        Some(result) => result,
        None => tokio::time::timeout(RETURN_WITHIN, copy)
            .await
            .expect("a draining driver returns once its siblings stop"),
    };

    assert!(
        is_the_injected_failure(&result),
        "❗ the FIRST failure stays the operation's error, never a sibling's stop: {result:?}"
    );
    assert!(
        events.cancelled.lock().unwrap().is_empty(),
        "❗ a failure never reads as a cancel"
    );
    assert_eq!(
        fx.aborted.load(Ordering::SeqCst),
        SIBLINGS.len() as u64,
        "each sibling's own write stopped and removed its partial (tier 1's cleanup), \
         rather than being dropped for a sweep to find"
    );
    let names = dest_names(&fx.dest_inner).await;
    assert!(staged_temps(&names).is_empty(), "no temp left behind: {names:?}");
    assert!(
        names.iter().any(|n| n == "landed.txt"),
        "a file that landed stays: {names:?}"
    );
    assert!(
        state.in_flight_temps.lock().unwrap().is_empty(),
        "every temp left the in-flight ledger with its write"
    );
}

/// A sibling that never answers is still abandoned at the drain deadline, so a
/// wedged task can't hold the failure's report open.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sibling_that_never_stops_is_abandoned_at_the_drain_deadline() {
    let engine = TestOperations::new();
    let _drain = CancelDrainGuard::set(Duration::from_millis(150));
    let (fx, trigger, sources) = drain_fixture().await;
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig::default();

    let copy = copy_volumes_with_progress(
        events.clone(),
        "failure-drain-wedged-sibling",
        &state,
        Arc::clone(&fx.source),
        &sources,
        Arc::clone(&fx.dest),
        Path::new("/"),
        &config,
    );
    tokio::pin!(copy);

    tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_mid_stream(&fx) => {}
    }
    // The failure, and no permit ever again: every sibling stays wedged inside
    // its source read, where nothing reaches it.
    trigger.add_permits(1);
    let result = tokio::time::timeout(RETURN_WITHIN, copy)
        .await
        .expect("the drain deadline ends the wait on siblings that never stop");

    assert!(is_the_injected_failure(&result), "{result:?}");
    assert!(events.cancelled.lock().unwrap().is_empty());
    let names = dest_names(&fx.dest_inner).await;
    assert!(
        staged_temps(&names).is_empty(),
        "the abandoned siblings' temps are swept: {names:?}"
    );
}

/// A paused copy whose in-flight call fails: its parked siblings wake to stop
/// and clean up after themselves, instead of sitting on the pause until the
/// drain deadline drops them. Nothing resumes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failure_while_paused_wakes_the_parked_siblings_to_stop() {
    let engine = TestOperations::new();
    // Longer than the test waits: a sibling the drain dropped would count no
    // abort, so the assertion below can't pass by way of the deadline.
    let _drain = CancelDrainGuard::set(Duration::from_secs(30));
    let (fx, trigger, sources) = drain_fixture().await;
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig::default();

    let copy = copy_volumes_with_progress(
        events.clone(),
        "failure-drain-paused",
        &state,
        Arc::clone(&fx.source),
        &sources,
        Arc::clone(&fx.dest),
        Path::new("/"),
        &config,
    );
    tokio::pin!(copy);

    tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_mid_stream(&fx) => {}
    }
    // Pause, then let each sibling finish the chunk it's waiting for: its next
    // checkpoint parks it on the pause. The failing source is past its
    // checkpoint, waiting on its own read.
    state.pause_gate.pause();
    fx.gate.add_permits(SIBLINGS.len());
    tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_async(WAIT, "every sibling to land a chunk and park", || {
            fx.gate.available_permits() == 0
        }) => {}
    }
    // Enough for every woken sibling's next read.
    fx.gate.add_permits(100);
    trigger.add_permits(1);
    let result = tokio::time::timeout(RETURN_WITHIN, copy)
        .await
        .expect("a paused copy's failure ends without a Resume");

    assert!(is_the_injected_failure(&result), "{result:?}");
    assert_eq!(
        fx.aborted.load(Ordering::SeqCst),
        SIBLINGS.len() as u64,
        "every parked sibling woke, stopped, and removed its own partial"
    );
    assert!(state.pause_gate.is_paused(), "the failure doesn't resume anything");
    let names = dest_names(&fx.dest_inner).await;
    assert!(staged_temps(&names).is_empty(), "{names:?}");
}

/// A person's Rollback during the drain still rolls back, and the operation ends
/// as ONE rollback: one `write-cancelled`, and a cancelled result, so no
/// `write-error` goes out after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_during_the_drain_ends_as_one_rollback() {
    let engine = TestOperations::new();
    let _drain = CancelDrainGuard::set(RETURN_WITHIN);
    let (fx, trigger, sources) = drain_fixture().await;
    let events = Arc::new(CollectorEventSink::new());
    let op = engine.register_state("failure-drain-rollback", make_state(&engine));
    let config = VolumeCopyConfig::default();

    let copy = copy_volumes_with_progress(
        events.clone(),
        op.id().as_str(),
        op.state(),
        Arc::clone(&fx.source),
        &sources,
        Arc::clone(&fx.dest),
        Path::new("/"),
        &config,
    );
    tokio::pin!(copy);

    tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_mid_stream(&fx) => {}
    }
    trigger.add_permits(1);
    tokio::select! {
        r = &mut copy => panic!("the copy must still be draining its siblings: {r:?}"),
        // allowed-test-sleep: lets the failure land before the click, with the siblings still gated.
        () = tokio::time::sleep(HOLD_AFTER_FAILURE) => {}
    }
    engine.cancel_write_operation(op.id(), true);
    fx.gate.add_permits(100);
    let result = tokio::time::timeout(RETURN_WITHIN, copy)
        .await
        .expect("a rolled-back drain returns");

    assert!(
        matches!(
            result.as_ref().err().map(|f| &f.error),
            Some(WriteOperationError::Cancelled { .. })
        ),
        "the rollback is the operation's ending, so the caller emits no `write-error` after it: {result:?}"
    );
    {
        let cancelled = events.cancelled.lock().unwrap();
        assert_eq!(cancelled.len(), 1, "exactly one terminal event");
        assert_eq!(cancelled[0].rollback.outcome, CancelRollbackOutcome::RolledBack);
    }
    assert!(
        !fx.dest_inner.exists(Path::new("/landed.txt")).await,
        "the rollback undid the file that landed: {:?}",
        dest_names(&fx.dest_inner).await
    );
}
