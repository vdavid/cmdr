//! A download whose share goes away mid-file for longer than its backend
//! bridges carries on from the byte it got to once the share is back, keeping
//! what it already wrote: `transfer/DETAILS.md` § "Carrying a file on after a
//! long outage". Through the real `copy_volumes_with_progress` pipeline.

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::outage_slot::OutagePolicy;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::OutageVolume;
use crate::types::{ConflictResolution, WriteOperationError};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume};

const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

/// Big enough for several of the double's 64 KiB chunks on each side of the drop.
const FILE_BYTES: usize = 1024 * 1024;

/// Where in the file the share goes away.
const DROP_AFTER: u64 = 320 * 1024;

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
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

fn patterned() -> Vec<u8> {
    (0..FILE_BYTES).map(|i| (i % 251) as u8).collect()
}

/// A NAS holding `/big.bin` (the wrapper, its store, and the wrapper as a
/// volume), and a local disk (its store, and itself as a volume).
struct Download {
    nas: Arc<OutageVolume>,
    nas_store: Arc<InMemoryVolume>,
    source: Arc<dyn Volume>,
    local: Arc<InMemoryVolume>,
    dest: Arc<dyn Volume>,
}

async fn download(lane: &str, local: InMemoryVolume) -> Download {
    let nas_store = Arc::new(InMemoryVolume::new("Testnas").with_space_info(10_000_000, 10_000_000));
    nas_store
        .create_file(Path::new("/big.bin"), &patterned())
        .await
        .unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&nas_store), lane);
    let source: Arc<dyn Volume> = Arc::clone(&nas) as Arc<dyn Volume>;
    let local = Arc::new(local.with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::clone(&local) as Arc<dyn Volume>;
    Download {
        nas,
        nas_store,
        source,
        local,
        dest,
    }
}

/// A local disk that keeps a failed write's bytes and carries the write on.
fn continuing_disk() -> InMemoryVolume {
    InMemoryVolume::new("Local").with_continuable_writes()
}

/// Runs `between` once the copy is waiting for the share, then brings it back.
fn while_waiting(
    state: &Arc<WriteOperationState>,
    nas: &Arc<OutageVolume>,
    between: impl Future<Output = ()> + Send + 'static,
) {
    let state = Arc::clone(state);
    let nas = Arc::clone(nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            state.outage.waiting_for().is_some()
        })
        .await;
        between.await;
        nas.become_reachable();
    });
}

async fn run(
    engine: &TestOperations,
    state: &Arc<WriteOperationState>,
    source: Arc<dyn Volume>,
    dest: Arc<dyn Volume>,
) -> Result<(), WriteFailure> {
    let op = engine.register_state("copy-resume", Arc::clone(state));
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        state,
        source,
        &[PathBuf::from("/big.bin")],
        dest,
        Path::new("/"),
        &VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Stop,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await
}

async fn content_of(volume: &Arc<InMemoryVolume>, path: &str) -> Vec<u8> {
    let mut stream = volume.open_read_stream(Path::new(path)).await.expect("the file opens");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("a chunk"));
    }
    bytes
}

/// The share drops mid-file and stays away past the per-file retries, so the
/// operation's outage wait carries it. Once back, the file carries on from the
/// byte the destination had: every byte lands once, and the source is read
/// barely more than once (the chunk the drop interrupted, at most).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_carries_on_from_where_it_was_after_a_long_outage() {
    let engine = TestOperations::new();
    let d = download("resume-carries-on", continuing_disk()).await;
    d.nas.drop_mid_read(DROP_AFTER);
    let state = state(&engine);
    while_waiting(&state, &d.nas, async {});

    run(&engine, &state, d.source, d.dest)
        .await
        .expect("a share that comes back within the budget must not fail the copy");

    assert!(
        content_of(&d.local, "/big.bin").await == patterned(),
        "every byte lands once, in order"
    );
    let resumed = d.nas.resumed_at();
    assert_eq!(
        resumed.len(),
        1,
        "the file carried on once, never opened afresh: {resumed:?}"
    );
    assert!(
        resumed[0] >= DROP_AFTER - 64 * 1024,
        "from where it was, not from the top: {resumed:?}"
    );
    assert!(
        d.nas.bytes_read() <= FILE_BYTES as u64 + 64 * 1024,
        "the source is read once over: {} bytes for a {FILE_BYTES}-byte file",
        d.nas.bytes_read()
    );
}

/// A file rewritten while the share was away is refused as the source having
/// changed, never spliced and never quietly copied again: the person hears
/// what happened, and nothing lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_refuses_a_file_rewritten_while_the_share_was_away() {
    let engine = TestOperations::new();
    let d = download("resume-refuses-changed", continuing_disk()).await;
    d.nas.drop_mid_read(DROP_AFTER);
    let state = state(&engine);
    // Another client saves over the file meanwhile.
    let store = Arc::clone(&d.nas_store);
    while_waiting(&state, &d.nas, async move {
        store.delete(Path::new("/big.bin")).await.unwrap();
        store
            .create_file(Path::new("/big.bin"), b"a different file")
            .await
            .unwrap();
    });

    let failure = run(&engine, &state, d.source, d.dest)
        .await
        .expect_err("a file that changed meanwhile must not be copied");
    assert!(
        matches!(failure.error, WriteOperationError::SourceChanged { .. }),
        "the source changed, and says so: {:?}",
        failure.error
    );
    assert!(!d.local.exists(Path::new("/big.bin")).await, "nothing lands");
    let leftovers: Vec<String> = d
        .local
        .list_directory(Path::new("/"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(leftovers.is_empty(), "the partial goes too: {leftovers:?}");
}

/// A destination that can't carry a write on runs the file again from its first
/// byte, as before: the resume is an improvement, never a requirement.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_cant_continue_runs_the_file_again() {
    let engine = TestOperations::new();
    let d = download("resume-dest-cant", InMemoryVolume::new("Local")).await;
    d.nas.drop_mid_read(DROP_AFTER);
    let state = state(&engine);
    while_waiting(&state, &d.nas, async {});

    run(&engine, &state, d.source, d.dest)
        .await
        .expect("the copy finishes the old way");

    assert!(content_of(&d.local, "/big.bin").await == patterned());
    assert!(d.nas.resumed_at().is_empty(), "nothing was carried on");
}
