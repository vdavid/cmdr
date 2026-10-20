//! A cross-volume move whose SOURCE share drops during its source sweep waits
//! for it, then sweeps again from what the share shows.
//!
//! The copy has landed by then, so the sweep is the last step. The wait is the
//! one every transfer step takes (`../volume_outage.rs`); what these cells pin
//! is the sweep's half: a run after a wait re-lists and re-stamps every entry,
//! so a file someone saved over while the share was out stays, and a delete
//! the drop swallowed (the server performed it) reads as done.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::super::faulty_volume::{FaultyOp, FaultyVolume};
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::outage_slot::OutagePolicy;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::types::WriteOperationError;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};

const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

fn outage_state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(30),
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    )
}

/// A source share holding `/batch/f0.txt`…`/batch/f3.txt` and `/single.txt`.
async fn source_share(lane: &str) -> (Arc<InMemoryVolume>, Arc<OutageVolume>) {
    let inner = Arc::new(InMemoryVolume::new("Naspolya").with_space_info(10_000_000, 10_000_000));
    inner.create_directory(Path::new("/batch")).await.unwrap();
    for n in 0..4 {
        inner
            .create_file(Path::new(&format!("/batch/f{n}.txt")), format!("file {n}").as_bytes())
            .await
            .unwrap();
    }
    inner.create_file(Path::new("/single.txt"), b"single").await.unwrap();
    let share = OutageVolume::wrap(Arc::clone(&inner), lane);
    (inner, share)
}

fn local() -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new("Local").with_space_info(10_000_000, 10_000_000))
}

/// Once the move waits for the share: runs `meanwhile` (someone else touching
/// the share while Cmdr can't reach it), then makes the share reachable.
fn come_back_when_waited_for<F, Fut>(state: &Arc<WriteOperationState>, share: &Arc<OutageVolume>, meanwhile: F)
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let state = Arc::clone(state);
    let share = Arc::clone(share);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the move to wait for the share", || {
            state.outage.waiting_for().is_some()
        })
        .await;
        meanwhile().await;
        share.become_reachable();
    });
}

/// Someone saves over `path` on the share: new bytes, a new size.
async fn save_over(inner: &InMemoryVolume, path: &str) {
    inner.delete(Path::new(path)).await.unwrap();
    inner
        .create_file(Path::new(path), b"saved while the share was out")
        .await
        .unwrap();
}

async fn move_from(
    engine: &TestOperations,
    op: &str,
    state: &Arc<WriteOperationState>,
    source: Arc<dyn Volume>,
    path: &str,
    dest: &Arc<InMemoryVolume>,
) -> Result<(), WriteFailure> {
    let op = engine.register_state(op, Arc::clone(state));
    move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        state,
        source,
        &[PathBuf::from(path)],
        Arc::clone(dest) as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
}

/// The share drops as the sweep deletes a folder's second file, and someone
/// saves over the third while it's out. Once back, the sweep runs again from a
/// fresh listing: what it carried goes, and the file saved over stays, since
/// the source now holds its only copy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_sweep_waits_out_a_dropped_share_and_keeps_what_changed_meanwhile() {
    let engine = TestOperations::new();
    let (inner, share) = source_share("outage-sweep-folder").await;
    share.drop_at_delete_of(Path::new("/batch/f1.txt"), RenameDrop::BeforeTheServerSeesIt);
    let dest = local();
    let state = outage_state(&engine);
    let saved_over = Arc::clone(&inner);
    come_back_when_waited_for(&state, &share, move || async move {
        save_over(&saved_over, "/batch/f2.txt").await;
    });

    move_from(
        &engine,
        "outage-sweep-folder",
        &state,
        Arc::clone(&share) as Arc<dyn Volume>,
        "/batch",
        &dest,
    )
    .await
    .expect("a source share that comes back within the budget must not fail the move");

    assert!(
        share.reconnect_attempts() >= 1,
        "the share really dropped under the sweep"
    );
    for n in [0, 1, 3] {
        let path = format!("/batch/f{n}.txt");
        assert!(!inner.exists(Path::new(&path)).await, "{path} was carried, so it goes");
        assert!(dest.exists(Path::new(&path)).await, "{path} arrived");
    }
    let mut kept = Vec::new();
    let mut stream = inner
        .open_read_stream(Path::new("/batch/f2.txt"))
        .await
        .expect("f2 stays");
    while let Some(chunk) = stream.next_chunk().await {
        kept.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(
        kept, b"saved while the share was out",
        "the save made during the outage survives"
    );
}

/// The server deleted the moved file, and the drop swallowed its answer. After
/// the wait the file is gone, which is the delete having happened: the move is
/// done, not "couldn't remove the original".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_the_drop_swallowed_counts_as_done() {
    let engine = TestOperations::new();
    let (inner, share) = source_share("outage-sweep-swallowed").await;
    share.drop_at_delete_of(Path::new("/single.txt"), RenameDrop::AfterTheServerPerformsIt);
    let dest = local();
    let state = outage_state(&engine);
    come_back_when_waited_for(&state, &share, || async {});

    move_from(
        &engine,
        "outage-sweep-swallowed",
        &state,
        Arc::clone(&share) as Arc<dyn Volume>,
        "/single.txt",
        &dest,
    )
    .await
    .expect("a delete the server performed before the drop is a finished move");

    assert!(
        share.reconnect_attempts() >= 1,
        "the share really dropped at the delete"
    );
    assert!(!inner.exists(Path::new("/single.txt")).await);
    assert!(dest.exists(Path::new("/single.txt")).await);
}

/// The drop comes before the server sees the delete, and someone saves over
/// the file while the share is out. After the wait the file no longer matches
/// what the move carried, so it stays: the destination has the OLD bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_saved_over_during_the_outage_stays_in_the_source() {
    let engine = TestOperations::new();
    let (inner, share) = source_share("outage-sweep-saved-over").await;
    share.drop_at_delete_of(Path::new("/single.txt"), RenameDrop::BeforeTheServerSeesIt);
    let dest = local();
    let state = outage_state(&engine);
    let saved_over = Arc::clone(&inner);
    come_back_when_waited_for(&state, &share, move || async move {
        save_over(&saved_over, "/single.txt").await;
    });

    move_from(
        &engine,
        "outage-sweep-saved-over",
        &state,
        Arc::clone(&share) as Arc<dyn Volume>,
        "/single.txt",
        &dest,
    )
    .await
    .expect("a file that changed is kept and counted, not a failure");

    assert!(
        share.reconnect_attempts() >= 1,
        "the share really dropped at the delete"
    );
    assert!(
        inner.exists(Path::new("/single.txt")).await,
        "the saved-over file is the only copy of its new bytes"
    );
    assert!(dest.exists(Path::new("/single.txt")).await);
}

/// A share that's up and refuses the delete is a refusal, not an outage: the
/// move reports the original it couldn't remove at once, with no reconnect.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_delete_on_a_live_share_fails_at_once() {
    let engine = TestOperations::new();
    let (inner, share) = source_share("outage-sweep-refused").await;
    let refusing = FaultyVolume::wrapping(Arc::clone(&share))
        .failing_call_at(
            FaultyOp::Delete,
            "/single.txt",
            1,
            VolumeError::PermissionDenied {
                path: "/single.txt".to_owned(),
                raw_os_error: None,
            },
        )
        .arc();
    let dest = local();
    let state = outage_state(&engine);

    let failure = move_from(
        &engine,
        "outage-sweep-refused",
        &state,
        Arc::clone(&refusing) as Arc<dyn Volume>,
        "/single.txt",
        &dest,
    )
    .await
    .expect_err("a refused delete leaves the original, and the move says so");

    assert!(refusing.fault_fired(FaultyOp::Delete), "the refusal reached the sweep");
    let WriteOperationError::SourceNotRemoved { path, .. } = &failure.error else {
        panic!("expected SourceNotRemoved, got {:?}", failure.error);
    };
    assert_eq!(path, "/single.txt");
    assert_eq!(
        share.reconnect_attempts(),
        0,
        "a live share's refusal is never waited for"
    );
    assert!(inner.exists(Path::new("/single.txt")).await);
}
