//! Cancel and Rollback have to reach a driver that is parked on its in-flight
//! tasks, and have to finish even when a task never winds down.
//!
//! On 2026-07-31 the concurrent copy driver went quiet with six of its eight
//! slots free and two tasks parked forever. Rollback was clicked, nothing
//! happened, the window wouldn't close, and the app had to be force-quit —
//! which is what turned a recoverable stall into data loss
//! (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). The driver only
//! looked at `OperationIntent` while pushing new tasks, a line a driver parked on
//! `in_flight.next().await` never reaches again.
//!
//! Each test here parks the driver for real: the gated source hands out no
//! permits, so every spawned task sits in `next_chunk()` and the driver sits in
//! `in_flight.next()`. Doubles: `wedge_test_support`.

use super::tests::make_state;
use super::wedge_test_support::*;
use super::*;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::CollectorEventSink;
use cmdr_fs::testing::wait_until_async;
use std::sync::atomic::Ordering;

/// How long a cancelled driver may take to return before the test calls it
/// wedged. Comfortably past the drain deadline the tests install.
const RETURN_WITHIN: Duration = Duration::from_secs(5);

/// A short drain window, so the abandon path is observable without waiting out
/// the production deadline.
const TEST_DRAIN: Duration = Duration::from_millis(150);

/// Three wedged sources plus (optionally) one that lands immediately, so the
/// driver fills its window, parks on `in_flight.next()`, and stays there.
async fn wedged_fixture(with_a_landed_file: bool) -> (Fixture, Vec<PathBuf>) {
    let fx = fixture(CHUNK as u64 * 4);
    let mut sources = Vec::new();
    if with_a_landed_file {
        // Empty ⇒ the gated stream yields nothing, so this one copy completes
        // without a permit and gives Rollback something to undo.
        fx.source_inner
            .create_file(Path::new("/landed.txt"), b"")
            .await
            .unwrap();
        sources.push(PathBuf::from("/landed.txt"));
    }
    for name in ["/wedge-a.bin", "/wedge-b.bin", "/wedge-c.bin"] {
        fx.source_inner
            .create_file(Path::new(name), &vec![0xAB; CHUNK * 4])
            .await
            .unwrap();
        sources.push(PathBuf::from(name));
    }
    (fx, sources)
}

/// Waits until every source has an open read stream: at that point the driver has
/// spawned everything it can and is parked awaiting its tasks.
async fn wait_until_parked(fx: &Fixture, expected: u64) {
    wait_until_async(WAIT, "the driver to park with its window full", || {
        fx.opened.load(Ordering::SeqCst) >= expected
    })
    .await;
}

/// Cancel must reach a driver parked on `in_flight.next()`.
///
/// Pre-fix this hangs forever: `is_cancelled` was only consulted in the spawn
/// loop, and the wedged tasks never complete, so the driver never returns to it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_reaches_a_driver_parked_on_its_in_flight_tasks() {
    let _drain = CancelDrainGuard::set(TEST_DRAIN);
    let (fx, sources) = wedged_fixture(false).await;
    let events = Arc::new(CollectorEventSink::new());
    let op = TestOperationGuard::register_state("cancel-parked-driver", make_state());
    let config = VolumeCopyConfig::default();

    let copy = copy_volumes_with_progress(
        events.clone(),
        op.id(),
        op.state(),
        Arc::clone(&fx.source),
        &sources,
        Arc::clone(&fx.dest),
        Path::new("/"),
        &config,
    );
    tokio::pin!(copy);

    let result = tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_parked(&fx, 3) => {
            cancel_write_operation(op.id(), false);
            tokio::time::timeout(RETURN_WITHIN, copy)
                .await
                .expect("a cancelled driver must return even while parked on wedged tasks")
        }
    };

    assert!(
        matches!(
            result.as_ref().err().map(|f| &f.error),
            Some(WriteOperationError::Cancelled { .. })
        ),
        "the operation must end as cancelled, got {result:?}"
    );
    let cancelled = events.cancelled.lock().unwrap();
    assert_eq!(
        cancelled.len(),
        1,
        "the FE needs exactly one write-cancelled to close on"
    );
    assert!(!cancelled[0].rolled_back);
}

/// Rollback must reach the same parked driver, and must actually undo the files
/// that already landed. The incident's Rollback click never produced a single
/// `rolling back op=` line.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_reaches_a_parked_driver_and_undoes_what_landed() {
    let _drain = CancelDrainGuard::set(TEST_DRAIN);
    let (fx, sources) = wedged_fixture(true).await;
    let events = Arc::new(CollectorEventSink::new());
    let op = TestOperationGuard::register_state("rollback-parked-driver", make_state());
    let config = VolumeCopyConfig::default();

    let copy = copy_volumes_with_progress(
        events.clone(),
        op.id(),
        op.state(),
        Arc::clone(&fx.source),
        &sources,
        Arc::clone(&fx.dest),
        Path::new("/"),
        &config,
    );
    tokio::pin!(copy);

    let result = tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_async(WAIT, "the landed file to arrive at the destination", || {
            fx.written.load(Ordering::SeqCst) == 0 && fx.opened.load(Ordering::SeqCst) >= 4
        }) => {
            cancel_write_operation(op.id(), true);
            tokio::time::timeout(RETURN_WITHIN, copy)
                .await
                .expect("a rolled-back driver must return even while parked on wedged tasks")
        }
    };

    assert!(
        matches!(
            result.as_ref().err().map(|f| &f.error),
            Some(WriteOperationError::Cancelled { .. })
        ),
        "a rollback ends the operation as cancelled, got {result:?}"
    );
    {
        let cancelled = events.cancelled.lock().unwrap();
        assert_eq!(cancelled.len(), 1);
        assert!(cancelled[0].rolled_back, "the FE must be told the rollback ran");
    }
    assert!(
        !fx.dest_inner.exists(Path::new("/landed.txt")).await,
        "rollback must delete the file that already landed; dest holds {:?}",
        dest_names(&fx.dest_inner).await
    );
}

/// A task that will not wind down must not hold the operation open forever.
/// After the drain deadline the driver abandons it, and the partial it was
/// writing is left under a recognizable `.cmdr-tmp-*` name rather than at the
/// file's real name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_task_that_never_winds_down_is_abandoned_at_the_deadline() {
    let _drain = CancelDrainGuard::set(TEST_DRAIN);
    let (fx, sources) = wedged_fixture(false).await;
    let events = Arc::new(CollectorEventSink::new());
    let op = TestOperationGuard::register_state("abandon-wedged-task", make_state());
    let config = VolumeCopyConfig::default();

    let copy = copy_volumes_with_progress(
        events.clone(),
        op.id(),
        op.state(),
        Arc::clone(&fx.source),
        &sources,
        Arc::clone(&fx.dest),
        Path::new("/"),
        &config,
    );
    tokio::pin!(copy);

    // Each wait rides its own `select!` arm alongside the copy: the copy only
    // advances while it is being polled, and a `select!` branch body doesn't poll
    // the other branch.
    tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_parked(&fx, 3) => {}
    }
    // Let one chunk through, so there really is a partial on the destination when
    // its task is abandoned.
    fx.gate.add_permits(1);
    tokio::select! {
        r = &mut copy => panic!("the copy must still be running: {r:?}"),
        () = wait_until_async(WAIT, "a chunk to land at the destination", || {
            fx.written.load(Ordering::SeqCst) > 0
        }) => {}
    }

    cancel_write_operation(op.id(), false);
    tokio::time::timeout(RETURN_WITHIN, &mut copy)
        .await
        .expect("the driver must abandon a task that will not wind down")
        .expect_err("an abandoned cancel still ends the operation as cancelled");

    let names = dest_names(&fx.dest_inner).await;
    assert!(
        !names.iter().any(|n| n.ends_with(".bin")),
        "no partial may sit at a source file's real name; dest holds {names:?}"
    );
    assert!(
        names.is_empty(),
        "the abandoned tasks' staged partials must be swept too; dest holds {names:?}"
    );
}
