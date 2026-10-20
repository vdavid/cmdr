//! A copy's own pre-flight scan over a share that drops mid-walk waits for it,
//! the way the rest of the transfer does (`scan_wait.rs`).
//!
//! The scan is one backend call over every source, so these cells drop the
//! session at a listing deep in the walk (`OutageVolume::drop_at_listing_of`):
//! the walk has to wait, run that one listing again, and carry on from there
//! with every entry counted once.

use super::preflight::scan_volume_sources;
use crate::event_sinks::CollectorEventSink;
use crate::outage_slot::OutagePolicy;
use crate::state::WriteOperationState;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::OutageVolume;
use crate::types::{
    TransferWaitReason, VolumeCopyConfig, WriteOperationError, WriteOperationPhase, WriteOperationType,
};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

fn outage_state(engine: &TestOperations, budget: Duration) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(0))
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    )
}

/// A share holding `/batch` (four files) and `/batch/sub` (two more): six
/// files, 6 bytes each, two folders.
async fn nas(lane: &str) -> (Arc<OutageVolume>, Arc<dyn Volume>) {
    let inner = Arc::new(InMemoryVolume::new("Naspolya").with_space_info(10_000_000, 10_000_000));
    inner.create_directory(Path::new("/batch")).await.unwrap();
    inner.create_directory(Path::new("/batch/sub")).await.unwrap();
    for path in [
        "/batch/f0.txt",
        "/batch/f1.txt",
        "/batch/f2.txt",
        "/batch/f3.txt",
        "/batch/sub/g0.txt",
        "/batch/sub/g1.txt",
    ] {
        inner.create_file(Path::new(path), b"file 0").await.unwrap();
    }
    let nas = OutageVolume::wrap(inner, lane);
    let volume = Arc::clone(&nas) as Arc<dyn Volume>;
    (nas, volume)
}

async fn scan(
    state: &Arc<WriteOperationState>,
    volume: &Arc<dyn Volume>,
    events: &CollectorEventSink,
) -> Result<super::preflight::VolumePreflight, super::transfer_error::WriteFailure> {
    scan_volume_sources(
        volume,
        &[PathBuf::from("/batch")],
        &VolumeCopyConfig::default(),
        "op-preflight-outage",
        WriteOperationType::Copy,
        state,
        events,
    )
    .await
}

/// The share drops as the walk lists `/batch/sub`. The scan waits, saying so
/// on its progress, lists that folder again once the share is back, and
/// finishes with the totals a scan with no drop gets: no file counted twice,
/// none skipped, and `/batch` itself listed only once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_the_share_drops_under_waits_and_counts_every_file_once() {
    let engine = TestOperations::new();
    let (nas, volume) = nas("outage-preflight-waits").await;
    nas.drop_at_listing_of(Path::new("/batch/sub"));
    let state = outage_state(&engine, Duration::from_secs(30));
    {
        let state = Arc::clone(&state);
        let nas = Arc::clone(&nas);
        tokio::spawn(async move {
            wait_until_async(WAIT, "the scan to wait for the share", || {
                state.outage.waiting_for().is_some()
            })
            .await;
            nas.become_reachable();
        });
    }
    let events = CollectorEventSink::new();

    let preflight = scan(&state, &volume, &events)
        .await
        .expect("a share that comes back within the budget must not fail the scan");

    assert_eq!(preflight.total_files, 6, "every file once");
    assert_eq!(preflight.total_bytes, 6 * "file 0".len() as u64);
    assert!(nas.reconnect_attempts() >= 1, "the scan reconnected the share itself");
    assert_eq!(
        nas.listings_of(Path::new("/batch")),
        1,
        "the walk carried on, never from the top"
    );
    assert_eq!(
        nas.listings_of(Path::new("/batch/sub")),
        2,
        "the dropped listing ran again"
    );
    let progress = events.progress.lock().unwrap();
    assert!(
        progress.iter().any(|event| event.phase == WriteOperationPhase::Scanning
            && event
                .activity
                .as_ref()
                .is_some_and(|activity| activity.waiting_on == TransferWaitReason::Reconnecting
                    && activity.reconnecting_volume.as_deref() == Some("Naspolya"))),
        "the scan's progress said it was waiting for the share: {progress:?}"
    );
    let last = progress.last().expect("the scan's last tally");
    assert!(
        last.activity
            .as_ref()
            .is_none_or(|activity| activity.waiting_on != TransferWaitReason::Reconnecting),
        "and stopped saying so once it was back: {last:?}"
    );
}

/// The share never comes back: after the budget, the scan stops and the person
/// reads "Lost the connection to Naspolya", the folder it was listing as
/// context.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scan_whose_share_stays_gone_says_it_lost_the_connection() {
    let engine = TestOperations::new();
    let (nas, volume) = nas("outage-preflight-gone").await;
    nas.drop_at_listing_of(Path::new("/batch/sub"));
    let state = outage_state(&engine, Duration::from_millis(300));

    let failure = scan(&state, &volume, &CollectorEventSink::new())
        .await
        .expect_err("a share that never comes back still fails the scan");

    assert!(
        matches!(
            &failure.error,
            WriteOperationError::ServerConnectionLost { path, volume_name, gave_up_after_secs: Some(_) }
                if path == "/batch/sub" && volume_name == "Naspolya"
        ),
        "the connection is the cause, named with the wait: {:?}",
        failure.error
    );
    assert!(nas.reconnect_attempts() > 1, "after really trying");
}

/// A server that refuses a listing with its session up gave an answer, not an
/// outage: the scan fails at once, with nothing dialed and nothing listed twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_the_server_refuses_fails_the_scan_at_once() {
    let engine = TestOperations::new();
    let (nas, volume) = nas("outage-preflight-refused").await;
    nas.refuse_listing_of(Path::new("/batch/sub"));
    let state = outage_state(&engine, Duration::from_secs(30));

    let failure = scan(&state, &volume, &CollectorEventSink::new())
        .await
        .expect_err("a refusal fails the scan");

    assert!(
        matches!(&failure.error, WriteOperationError::PermissionDenied { .. }),
        "the server's own answer: {:?}",
        failure.error
    );
    assert_eq!(
        nas.listings_of(Path::new("/batch/sub")),
        1,
        "the refusal happened, once"
    );
    assert_eq!(nas.reconnect_attempts(), 0, "nothing waited");
}

/// A Cancel while the scan waits for the share ends it as the cancel it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_while_the_scan_waits_cancels_it() {
    let engine = TestOperations::new();
    let (nas, volume) = nas("outage-preflight-cancel").await;
    nas.drop_at_listing_of(Path::new("/batch/sub"));
    let state = outage_state(&engine, Duration::from_secs(60));
    let op = engine.register_state("outage-preflight-cancel", Arc::clone(&state));
    let op_id = op.id().to_owned();
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            wait_until_async(WAIT, "the scan to wait for the share", || {
                state.outage.waiting_for().is_some()
            })
            .await;
            engine.cancel_write_operation(&op_id, false);
        });
    }
    let events = CollectorEventSink::new();

    let started = std::time::Instant::now();
    let failure = scan(&state, &volume, &events)
        .await
        .expect_err("a cancel ends the scan");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "and doesn't wait out the budget"
    );

    assert!(
        matches!(&failure.error, WriteOperationError::Cancelled { .. }),
        "the cancel, never a lost connection: {:?}",
        failure.error
    );
    assert_eq!(events.cancelled.lock().unwrap().len(), 1, "one write-cancelled");
    assert!(nas.reconnect_attempts() >= 1, "the share really was out");
}
