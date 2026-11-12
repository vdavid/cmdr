//! A cross-volume move's held sweeps (#1148): the originals wait for one device
//! barrier per batch, ❌ never one per top-level source, and a stop keeps every
//! original no barrier has proven yet (`held_sweeps.rs`).
//!
//! Each cell sets the operation's barrier interval (`state.barriers`) to pick
//! its batches: `Duration::MAX` makes the whole move one batch closed by the
//! final barrier, `Duration::ZERO` closes a batch after every source.

use std::time::Duration;

use super::super::faulty_volume::{FaultyOp, FaultyVolume};
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use cmdr_fs::volume::{InMemoryVolume, VolumeError};

fn state_with_barrier_interval(engine: &TestOperations, interval: Duration) -> Arc<WriteOperationState> {
    let mut state = engine.new_operation_state(Duration::from_millis(50));
    state.barriers = cmdr_fs::durable::DeviceBarriers::with_interval(interval);
    Arc::new(state)
}

fn volume(name: &str) -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new(name).with_space_info(10_000_000, 10_000_000))
}

/// `count` loose files named `/f00.txt`, `/f01.txt`, … on `source`.
async fn loose_files(source: &InMemoryVolume, count: usize) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for i in 0..count {
        let path = PathBuf::from(format!("/f{i:02}.txt"));
        source.create_file(&path, b"loose").await.unwrap();
        paths.push(path);
    }
    paths
}

async fn run_move(
    state: &Arc<WriteOperationState>,
    events: &Arc<CollectorEventSink>,
    source: Arc<dyn Volume>,
    sources: &[PathBuf],
    dest: Arc<dyn Volume>,
) -> Result<(), WriteFailure> {
    move_volumes_with_progress(
        Arc::clone(events) as Arc<dyn OperationEventSink>,
        "op-held-sweeps",
        state,
        source,
        sources,
        dest,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
}

/// The LAST `source_removed` each source got, which is its verdict.
fn last_removed(events: &CollectorEventSink, source: &str) -> Option<bool> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .rev()
        .find(|event| event.source_path == source)
        .map(|event| event.source_removed)
}

fn refused(path: &str) -> VolumeError {
    VolumeError::PermissionDenied {
        path: path.to_string(),
        raw_os_error: None,
    }
}

fn refused_barrier() -> VolumeError {
    VolumeError::IoError {
        message: "simulated device barrier refusal".into(),
        raw_os_error: Some(libc::EIO),
    }
}

/// The whole point of #1148: a move of N loose files used to pay N barriers
/// (~60 ms each on a spinning disk). Within one interval they share ONE.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n_loose_files_share_one_barrier_within_an_interval() {
    const FILES: usize = 40;
    let engine = TestOperations::new();
    let source = volume("Source");
    let sources = loose_files(&source, FILES).await;
    let dest_inner = volume("Dest");
    let dest = FaultyVolume::wrapping(Arc::clone(&dest_inner)).arc();
    let events = Arc::new(CollectorEventSink::new());

    run_move(
        &state_with_barrier_interval(&engine, Duration::MAX),
        &events,
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect("the move completes");

    assert_eq!(dest.calls_to(FaultyOp::FlushWrites), 1, "one barrier, not one per file");
    for path in &sources {
        assert!(dest_inner.exists(path).await, "{} landed", path.display());
        assert!(!source.exists(path).await, "{} moved", path.display());
        assert_eq!(last_removed(&events, &path.display().to_string()), Some(true));
    }
}

/// The interval paces the batches: a barrier closes one whenever the interval
/// has gone by, so a long move keeps sweeping as it goes rather than holding
/// every original to the end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_barrier_interval_paces_the_batches() {
    let engine = TestOperations::new();
    let source = volume("Source");
    let sources = loose_files(&source, 5).await;
    let dest = FaultyVolume::wrapping(volume("Dest")).arc();

    run_move(
        &state_with_barrier_interval(&engine, Duration::ZERO),
        &Arc::new(CollectorEventSink::new()),
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect("the move completes");

    // One per source, and none at the end: nothing was still held.
    assert_eq!(dest.calls_to(FaultyOp::FlushWrites), 5);
}

/// A barrier refused mid-move: the batch it covered stays in both places with
/// today's typed error, what earlier barriers proved is gone from the source,
/// and nothing after it was touched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_barrier_refused_mid_move_keeps_its_batch() {
    let engine = TestOperations::new();
    let source = volume("Source");
    let sources = loose_files(&source, 5).await;
    let dest_inner = volume("Dest");
    let dest = FaultyVolume::wrapping(Arc::clone(&dest_inner))
        .failing_call(FaultyOp::FlushWrites, 3, refused_barrier())
        .arc();
    let events = Arc::new(CollectorEventSink::new());

    let failure = run_move(
        &state_with_barrier_interval(&engine, Duration::ZERO),
        &events,
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect_err("a refused barrier stops the move");

    assert!(dest.fault_fired(FaultyOp::FlushWrites));
    match &failure.error {
        WriteOperationError::SourceNotRemoved { path, landed_at, cause } => {
            assert_eq!(path, "/f02.txt");
            assert_eq!(landed_at, "/f02.txt");
            assert!(
                !matches!(**cause, WriteOperationError::SourceNotRemoved { .. }),
                "the cause is the destination's refusal: {cause:?}"
            );
        }
        other => panic!("expected SourceNotRemoved, got {other:?}"),
    }
    for moved in ["/f00.txt", "/f01.txt"] {
        assert!(
            !source.exists(Path::new(moved)).await,
            "{moved}: a held barrier proved it"
        );
        assert!(dest_inner.exists(Path::new(moved)).await);
    }
    assert!(
        source.exists(Path::new("/f02.txt")).await,
        "the unproven original stays"
    );
    assert!(dest_inner.exists(Path::new("/f02.txt")).await, "beside its landed copy");
    assert_eq!(last_removed(&events, "/f02.txt"), Some(false));
    for untouched in ["/f03.txt", "/f04.txt"] {
        assert!(source.exists(Path::new(untouched)).await);
        assert!(
            !dest_inner.exists(Path::new(untouched)).await,
            "{untouched}: never started"
        );
    }
}

/// The closing barrier refused: EVERY original of the batch stays, and the
/// error names the batch's first source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_closing_barrier_keeps_the_whole_batch() {
    let engine = TestOperations::new();
    let source = volume("Source");
    let sources = loose_files(&source, 4).await;
    let dest_inner = volume("Dest");
    let dest = FaultyVolume::wrapping(Arc::clone(&dest_inner))
        .failing_call(FaultyOp::FlushWrites, 1, refused_barrier())
        .arc();
    let events = Arc::new(CollectorEventSink::new());

    let failure = run_move(
        &state_with_barrier_interval(&engine, Duration::MAX),
        &events,
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect_err("a refused barrier stops the move");

    assert!(dest.fault_fired(FaultyOp::FlushWrites));
    assert!(
        matches!(&failure.error, WriteOperationError::SourceNotRemoved { path, .. } if path == "/f00.txt"),
        "{:?}",
        failure.error
    );
    for path in &sources {
        assert!(source.exists(path).await, "{}: unproven, so kept", path.display());
        assert!(dest_inner.exists(path).await, "{}: its copy landed", path.display());
        assert_eq!(last_removed(&events, &path.display().to_string()), Some(false));
    }
}

/// A sweep that refuses one original mid-batch stops there: what it already
/// swept is gone, and the rest of the batch stays in both places.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_sweep_keeps_the_rest_of_its_batch() {
    let engine = TestOperations::new();
    let source_inner = volume("Source");
    let sources = loose_files(&source_inner, 3).await;
    let source = FaultyVolume::wrapping(Arc::clone(&source_inner))
        .failing_call_at(FaultyOp::Delete, "/f01.txt", 1, refused("/f01.txt"))
        .arc();
    let dest = volume("Dest");
    let events = Arc::new(CollectorEventSink::new());

    let failure = run_move(
        &state_with_barrier_interval(&engine, Duration::MAX),
        &events,
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect_err("a refused source delete stops the move");

    assert!(source.fault_fired(FaultyOp::Delete));
    assert!(
        matches!(&failure.error, WriteOperationError::SourceNotRemoved { path, .. } if path == "/f01.txt"),
        "{:?}",
        failure.error
    );
    assert!(
        !source_inner.exists(Path::new("/f00.txt")).await,
        "swept before the refusal"
    );
    for kept in ["/f01.txt", "/f02.txt"] {
        assert!(
            source_inner.exists(Path::new(kept)).await,
            "{kept}: still at the source"
        );
        assert!(dest.exists(Path::new(kept)).await, "{kept} landed");
        assert_eq!(last_removed(&events, kept), Some(false));
    }
}

/// A copy that fails mid-move keeps the originals still held before it, in
/// both places: only a completed loop sweeps its last batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_copy_keeps_the_originals_still_held() {
    let engine = TestOperations::new();
    let source_inner = volume("Source");
    let sources = loose_files(&source_inner, 3).await;
    let source = FaultyVolume::wrapping(Arc::clone(&source_inner))
        .failing_call_at(FaultyOp::OpenReadStream, "/f02.txt", 1, refused("/f02.txt"))
        .arc();
    let dest = FaultyVolume::wrapping(volume("Dest")).arc();

    let failure = run_move(
        &state_with_barrier_interval(&engine, Duration::MAX),
        &Arc::new(CollectorEventSink::new()),
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect_err("the unreadable file fails the move");

    assert!(source.fault_fired(FaultyOp::OpenReadStream));
    assert!(
        !matches!(failure.error, WriteOperationError::SourceNotRemoved { .. }),
        "the copy's own failure: {:?}",
        failure.error
    );
    assert_eq!(
        dest.calls_to(FaultyOp::FlushWrites),
        0,
        "nothing swept, so no barrier owed"
    );
    for kept in ["/f00.txt", "/f01.txt"] {
        assert!(
            source_inner.exists(Path::new(kept)).await,
            "{kept}: still at the source, no barrier proved it"
        );
        assert!(dest.exists(Path::new(kept)).await, "{kept} landed");
    }
}
