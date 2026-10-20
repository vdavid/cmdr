//! A copy to a share that drops mid-transfer waits for it to come back and
//! finishes, through the real `copy_volumes_with_progress` pipeline (`#398`).
//!
//! The wait itself is pinned in `transfer/volume_outage_tests.rs`. These cells
//! pin where the pipeline asks for it: the byte path (both drivers), and the
//! start of a transfer whose destination is already gone (a Retry while the NAS
//! is still down, or a job admitted mid-outage).

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::outage_slot::OutagePolicy;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::types::{ConflictResolution, WriteOperationError};
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, Volume};

const WAIT: Duration = Duration::from_secs(10);
const CADENCE: &[Duration] = &[Duration::from_millis(10), Duration::from_millis(20)];

fn outage_state(engine: &TestOperations, budget: Duration) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget,
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    )
}

/// The modification date every source file wears (2017-07-14, unix seconds):
/// long before any test runs, so a copy that lost it shows.
const SOURCE_MODIFIED: u64 = 1_500_000_000;

/// A source with `count` files, both as a folder (`/batch/…`) and at the top
/// level (`/f<n>.txt`), so a cell can pick the serial walk or the concurrent
/// window. Every file is dated [`SOURCE_MODIFIED`].
async fn source_with(count: usize) -> Arc<dyn Volume> {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_directory(Path::new("/batch")).await.unwrap();
    for n in 0..count {
        let body = format!("file {n}");
        for path in [format!("/batch/f{n}.txt"), format!("/f{n}.txt")] {
            source.create_file(Path::new(&path), body.as_bytes()).await.unwrap();
            source.set_modified_at(Path::new(&path), Some(SOURCE_MODIFIED));
        }
    }
    source
}

fn nas(lane: &str) -> (Arc<InMemoryVolume>, Arc<OutageVolume>, Arc<dyn Volume>) {
    let inner = Arc::new(InMemoryVolume::new("Naspolya").with_space_info(10_000_000, 10_000_000));
    let nas = OutageVolume::wrap(Arc::clone(&inner), lane);
    let dest: Arc<dyn Volume> = Arc::clone(&nas) as Arc<dyn Volume>;
    (inner, nas, dest)
}

fn config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

/// Brings the share back once the operation has started waiting for it.
fn come_back_when_waited_for(state: &Arc<WriteOperationState>, nas: &Arc<OutageVolume>) {
    let state = Arc::clone(state);
    let nas = Arc::clone(nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            state.outage.waiting_for().is_some()
        })
        .await;
        nas.become_reachable();
    });
}

async fn landed(inner: &Arc<InMemoryVolume>, path: &str) -> bool {
    inner.exists(Path::new(path)).await
}

/// The commonest copy there is: one folder, so the SERIAL driver and the merge
/// walk. The share drops as the second file starts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_copy_waits_out_a_dropped_share_and_finishes() {
    let engine = TestOperations::new();
    let source = source_with(4).await;
    let (inner, nas, dest) = nas("outage-copy-folder");
    nas.drop_at_write(1);
    let state = outage_state(&engine, Duration::from_secs(30));
    let op = engine.register_state("outage-copy-folder", Arc::clone(&state));
    come_back_when_waited_for(&state, &nas);

    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a share that comes back within the budget must not fail the copy");

    for n in 0..4 {
        assert!(landed(&inner, &format!("/batch/f{n}.txt")).await, "f{n} must arrive");
    }
    assert!(nas.reconnect_attempts() >= 1, "the copy reconnected the share itself");
}

/// Several top-level files: the CONCURRENT window, where every in-flight task
/// meets the outage at once and each waits on the same share.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_file_copy_waits_out_a_dropped_share_and_finishes() {
    let engine = TestOperations::new();
    let source = source_with(5).await;
    let (inner, nas, dest) = nas("outage-copy-window");
    nas.drop_at_write(2);
    let state = outage_state(&engine, Duration::from_secs(30));
    let op = engine.register_state("outage-copy-window", Arc::clone(&state));
    come_back_when_waited_for(&state, &nas);

    let sources: Vec<PathBuf> = (0..5).map(|n| PathBuf::from(format!("/f{n}.txt"))).collect();
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &sources,
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a share that comes back within the budget must not fail the copy");

    for n in 0..5 {
        assert!(landed(&inner, &format!("/f{n}.txt")).await, "f{n} must arrive");
    }
}

/// A download FROM a share that drops as a file's source opens: nothing is
/// staged yet, so the copy waits for the share and opens the file again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_download_waits_out_a_source_share_that_drops_at_the_open() {
    let engine = TestOperations::new();
    let (source_inner, nas, source) = nas("outage-source-open");
    for n in 0..3 {
        let path = format!("/f{n}.txt");
        source_inner
            .create_file(Path::new(&path), format!("file {n}").as_bytes())
            .await
            .unwrap();
    }
    nas.drop_at_open(1);
    let dest_inner = Arc::new(InMemoryVolume::new("Local").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::clone(&dest_inner) as Arc<dyn Volume>;
    let state = outage_state(&engine, Duration::from_secs(30));
    let op = engine.register_state("outage-source-open", Arc::clone(&state));
    come_back_when_waited_for(&state, &nas);

    let sources: Vec<PathBuf> = (0..3).map(|n| PathBuf::from(format!("/f{n}.txt"))).collect();
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &sources,
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a source share that comes back within the budget must not fail the copy");

    for n in 0..3 {
        assert_eq!(
            content_of(&dest_inner, &format!("/f{n}.txt")).await,
            format!("file {n}").into_bytes(),
            "f{n} must land byte-exact"
        );
    }
    assert!(
        nas.reconnect_attempts() >= 1,
        "the source share really dropped at an open"
    );
}

/// The concurrent window, where one task's reconnect brings the share back
/// while another task's call is still in flight on the OLD session: that call's
/// error arrives late, with the share already live on the new one. It's waited
/// past at once and the file runs again, so every file lands. Before, the live
/// share made the late error final and it ended the copy (`ERR-VEZUB`). The
/// error hits a landing rename the server never saw, where no per-file retry
/// would have ridden it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_late_error_from_a_replaced_session_is_run_again_and_every_file_lands() {
    let engine = TestOperations::new();
    let source = source_with(5).await;
    let (inner, nas, dest) = nas("outage-late-error");
    nas.stale_error_at_rename(2, RenameDrop::BeforeTheServerSeesIt);
    let state = outage_state(&engine, Duration::from_secs(30));
    let op = engine.register_state("outage-late-error", Arc::clone(&state));

    let sources: Vec<PathBuf> = (0..5).map(|n| PathBuf::from(format!("/f{n}.txt"))).collect();
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &sources,
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a late error from a session someone already replaced must not fail the copy");

    for n in 0..5 {
        assert_eq!(
            content_of(&inner, &format!("/f{n}.txt")).await,
            format!("file {n}").into_bytes(),
            "f{n} must land byte-exact"
        );
    }
    assert_eq!(nas.generation(), 1, "the session really was replaced under a landing");
    assert_eq!(
        nas.reconnect_attempts(),
        0,
        "the copy dialed nothing: the share was already back"
    );
    let leftovers: Vec<String> = inner
        .list_directory(Path::new("/"), None)
        .await
        .expect("the destination")
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "no staged partial survives: {leftovers:?}");
}

/// Copies a folder to a share that drops as its second file starts and never
/// comes back (after `arm` set it up), and hands back what the `write-error`
/// event carries, built the way the operation builds it.
async fn copy_that_the_share_stops(
    lane: &str,
    arm: impl FnOnce(&OutageVolume),
) -> (Arc<InMemoryVolume>, Arc<OutageVolume>, WriteOperationError) {
    let engine = TestOperations::new();
    let source = source_with(4).await;
    let (inner, nas, dest) = nas(lane);
    arm(&nas);
    nas.drop_at_write(1);
    let state = outage_state(&engine, Duration::from_millis(300));
    let op = engine.register_state(lane, Arc::clone(&state));

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect_err("a share that never comes back still fails the copy");
    let event = write_error_event_from(
        &state,
        op.id().to_string().to_owned(),
        WriteOperationType::Copy,
        failure,
    );
    (inner, nas, event.error)
}

/// The share never comes back: after the budget, the copy stops with what
/// landed still there, and the person reads the real cause: "Lost the
/// connection to Naspolya", with how long Cmdr waited, the file only as context.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_that_outlasts_its_outage_says_it_lost_the_connection() {
    let (inner, nas, error) = copy_that_the_share_stops("outage-copy-gone", |_| {}).await;

    assert!(
        matches!(
            &error,
            WriteOperationError::ServerConnectionLost { path, volume_name, gave_up_after_secs: Some(_) }
                if path.starts_with("/batch/f") && volume_name == "Naspolya"
        ),
        "the connection is the cause, named with the wait: {error:?}"
    );
    assert!(nas.reconnect_attempts() > 1, "after really trying");
    // Nothing of the user's is touched by giving up: the destination folder
    // holds only whole files under real names, never a half-written one.
    nas.reconnected_elsewhere();
    let listing = inner
        .list_directory(Path::new("/batch"), None)
        .await
        .unwrap_or_default();
    assert!(
        listing
            .iter()
            .filter(|entry| !entry.name.contains(".cmdr-tmp-"))
            .all(|entry| entry.size == Some("file 0".len() as u64)),
        "every file under a real name is whole: {listing:?}"
    );
}

/// A share that wants a sign-in can't be waited for: the copy says the
/// connection dropped, with no wait to tell.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_needs_a_sign_in_says_the_connection_dropped() {
    let (_inner, nas, error) = copy_that_the_share_stops("outage-copy-sign-in", |nas| nas.require_sign_in()).await;

    assert!(
        matches!(
            &error,
            WriteOperationError::ServerConnectionLost { volume_name, gave_up_after_secs: None, .. }
                if volume_name == "Naspolya"
        ),
        "the connection is the cause, with no wait: {error:?}"
    );
    assert_eq!(
        nas.connection_state(),
        Some(cmdr_fs::volume::ConnectionState::NeedsSignIn)
    );
}

/// A phone over ADB that stays gone is a device that left, ❌ never a server
/// to check on: it keeps the device wording.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_that_stays_gone_keeps_its_device_wording() {
    let (_inner, nas, error) = copy_that_the_share_stops("outage-copy-phone", |nas| {
        nas.serve_as(cmdr_fs::volume::BackendKind::Adb);
    })
    .await;

    assert!(
        matches!(error, WriteOperationError::DeviceDisconnected { .. }),
        "a device keeps its own words: {error:?}"
    );
    assert!(nas.reconnect_attempts() > 1, "it waited for the phone like for a share");
}

/// A clash deep inside a merged folder that gives up on the share says the
/// connection is the cause, the same as a clash at the top: the resolver's
/// typed failure rides the merge walk out whole.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deep_clash_that_outlasts_its_outage_says_it_lost_the_connection() {
    let engine = TestOperations::new();
    let source = source_with(2).await;
    let (inner, nas, dest) = nas("outage-deep-clash");
    inner.create_directory(Path::new("/batch")).await.unwrap();
    for n in 0..2 {
        inner
            .create_file(Path::new(&format!("/batch/f{n}.txt")), b"old")
            .await
            .unwrap();
    }
    // The share drops for good as the resolver asks what the clashing file is.
    nas.drop_at_stat_of(Path::new("/batch/f1.txt"));
    let state = outage_state(&engine, Duration::from_millis(300));
    let op = engine.register_state("outage-deep-clash", Arc::clone(&state));

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect_err("a share that never comes back fails the copy");
    let error = write_error_event_from(
        &state,
        op.id().to_string().to_owned(),
        WriteOperationType::Copy,
        failure,
    )
    .error;

    assert!(
        matches!(
            &error,
            WriteOperationError::ServerConnectionLost { path, volume_name, gave_up_after_secs: Some(_) }
                if path == "/batch/f1.txt" && volume_name == "Naspolya"
        ),
        "the connection is the cause, named on the clashing file: {error:?}"
    );
}

/// Cancel during the wait ends the copy at once, as a cancel.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_the_wait_cancels_the_copy() {
    let engine = TestOperations::new();
    let source = source_with(4).await;
    let (_inner, nas, dest) = nas("outage-copy-cancel");
    nas.drop_at_write(1);
    let state = outage_state(&engine, Duration::from_secs(60));
    let op = engine.register_state("outage-copy-cancel", Arc::clone(&state));
    let watched = Arc::clone(&state);
    let op_id = op.id().to_owned();
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            watched.outage.waiting_for().is_some()
        })
        .await;
        engine.cancel_write_operation(&op_id, false);
    });

    let started = Instant::now();
    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect_err("a cancelled copy doesn't finish");

    assert!(
        matches!(failure.error, WriteOperationError::Cancelled { .. }),
        "a cancel is reported as one, never as the outage: {:?}",
        failure.error
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "and doesn't wait out the budget"
    );
}

/// ❗ A Cancel never waits on a dropped share (the lead's decision 1). The share
/// drops around a landing rename, so a whole temp of ours sits there while the
/// copy waits; the person cancels, and `write-cancelled` goes out at once, with
/// the share still out and the cleanup's one dial hanging. It says the temp
/// goes once the share is back, and once it is, it does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_while_the_share_is_out_reports_at_once_and_cleans_up_once_it_is_back() {
    use crate::types::StagedLeftoverFate;

    let engine = TestOperations::new();
    let source = source_with(4).await;
    let (inner, nas, dest) = nas("outage-copy-cancel-cleanup");
    nas.drop_at_rename(1, RenameDrop::BeforeTheServerSeesIt);
    nas.hang_reconnects();
    let state = outage_state(&engine, Duration::from_secs(60));
    let op = engine.register_state("outage-copy-cancel-cleanup", Arc::clone(&state));
    let watched = Arc::clone(&state);
    let op_id = op.id().to_owned();
    let canceller = (*engine).clone();
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            watched.outage.waiting_for().is_some()
        })
        .await;
        canceller.cancel_write_operation(&op_id, false);
    });
    let events = Arc::new(CollectorEventSink::new());

    let failure = tokio::time::timeout(
        Duration::from_secs(5),
        copy_volumes_with_progress(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            op.id().as_str(),
            &state,
            source,
            &[PathBuf::from("/batch")],
            dest,
            Path::new("/"),
            &config(),
        ),
    )
    .await
    .expect("the cancel is reported while the share is still out")
    .expect_err("a cancelled copy doesn't finish");

    assert!(matches!(failure.error, WriteOperationError::Cancelled { .. }));
    let cancelled = events.cancelled.lock_ignore_poison().clone();
    let leftovers = &cancelled
        .first()
        .expect("one write-cancelled")
        .rollback
        .staged_leftovers;
    assert_eq!(
        leftovers.iter().map(|group| group.fate).collect::<Vec<_>>(),
        vec![StagedLeftoverFate::RemovedWhenBack],
        "the temp goes once the share is back: {leftovers:?}"
    );
    assert!(
        names_in_batch(&inner)
            .await
            .iter()
            .any(|name| name.contains(".cmdr-tmp-")),
        "the fixture leaves a temp while the share is out"
    );

    nas.answer_reconnects();
    nas.reconnected_elsewhere();
    engine.leftover_cleanups_settled().await;

    let names = names_in_batch(&inner).await;
    assert!(
        !names.iter().any(|name| name.contains(".cmdr-tmp-")),
        "no temp is left once the share is back: {names:?}"
    );
}

/// A copy that STARTS with its destination gone (a Retry while the NAS is still
/// down, a job admitted mid-outage) waits for it before its first step, rather
/// than failing on it at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_that_starts_while_the_share_is_gone_waits_for_it() {
    let engine = TestOperations::new();
    let source = source_with(2).await;
    let (inner, nas, dest) = nas("outage-copy-start");
    nas.go_away();
    let state = outage_state(&engine, Duration::from_secs(30));
    let op = engine.register_state("outage-copy-start", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    come_back_when_waited_for(&state, &nas);

    copy_volumes_with_progress(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        op.id().as_str(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &config(),
    )
    .await
    .expect("a share that comes back must let the copy start");

    assert!(landed(&inner, "/batch/f1.txt").await);
    assert!(
        events.progress.lock_ignore_poison().iter().any(|e| e
            .activity
            .as_ref()
            .and_then(|a| a.reconnecting_volume.as_deref())
            == Some("Naspolya")),
        "the windows were told what the copy was waiting for"
    );
}

/// Copies `/batch` (four files) onto a share armed to drop around the landing
/// rename of the second file, and hands back the outcome. With `theirs`, someone
/// else saves that file's name with those bytes while the share is gone.
async fn copy_through_a_drop_at_landing(
    engine: &TestOperations,
    lane: &str,
    side: RenameDrop,
    theirs: Option<&'static [u8]>,
) -> (Arc<InMemoryVolume>, Result<(), WriteOperationError>) {
    let source = source_with(4).await;
    let (inner, nas, dest) = nas(lane);
    nas.drop_at_rename(1, side);
    let state = outage_state(engine, Duration::from_secs(30));
    let op = engine.register_state(lane, Arc::clone(&state));
    let watched = Arc::clone(&state);
    let behind_the_share = Arc::clone(&inner);
    let share = Arc::clone(&nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            watched.outage.waiting_for().is_some()
        })
        .await;
        if let Some(theirs) = theirs {
            // Ours landed before the drop; they save over it.
            behind_the_share
                .delete(Path::new("/batch/f1.txt"))
                .await
                .expect("ours is there to save over");
            behind_the_share
                .create_file(Path::new("/batch/f1.txt"), theirs)
                .await
                .expect("their save");
        }
        share.become_reachable();
    });

    let outcome = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &VolumeCopyConfig {
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error);
    (inner, outcome)
}

async fn names_in_batch(inner: &Arc<InMemoryVolume>) -> Vec<String> {
    let mut names: Vec<String> = inner
        .list_directory(Path::new("/batch"), None)
        .await
        .expect("the destination folder")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    names
}

async fn content_of(inner: &Arc<InMemoryVolume>, path: &str) -> Vec<u8> {
    let mut stream = inner.open_read_stream(Path::new(path)).await.expect("the file opens");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("a chunk"));
    }
    bytes
}

/// The server performed the landing rename and the drop swallowed its reply:
/// the file is whole at its name and the temp is gone. Once the share is back,
/// the copy recognizes the file as the one IT just landed and counts it done,
/// rather than re-copying it into its own file and failing with
/// `DestinationExists` (the live-SMB cell
/// `smb_outage_test::smb_integration_a_copy_waits_out_a_dropped_network_and_finishes`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_the_drop_swallowed_counts_as_landed() {
    let engine = TestOperations::new();
    let (inner, outcome) = copy_through_a_drop_at_landing(
        &engine,
        "outage-landed-unanswered",
        RenameDrop::AfterTheServerPerformsIt,
        None,
    )
    .await;

    outcome.expect("a landing the server performed is the copy's own file, not a conflict");
    for n in 0..4 {
        assert_eq!(
            content_of(&inner, &format!("/batch/f{n}.txt")).await,
            format!("file {n}").into_bytes(),
            "f{n} must land byte-exact"
        );
    }
    assert_eq!(
        names_in_batch(&inner).await,
        ["f0.txt", "f1.txt", "f2.txt", "f3.txt"],
        "no staged partial survives"
    );
}

/// The landing rename never reached the server: the temp is still there and
/// whole, so once the share is back it lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_the_drop_stopped_lands_once_the_share_is_back() {
    let engine = TestOperations::new();
    let (inner, outcome) = copy_through_a_drop_at_landing(
        &engine,
        "outage-landing-unsent",
        RenameDrop::BeforeTheServerSeesIt,
        None,
    )
    .await;

    outcome.expect("a landing that never happened runs again once the share is back");
    assert_eq!(content_of(&inner, "/batch/f1.txt").await, b"file 1".to_vec());
    assert_eq!(
        names_in_batch(&inner).await,
        ["f0.txt", "f1.txt", "f2.txt", "f3.txt"],
        "no staged partial survives"
    );
}

/// The landing's reply was lost AND, while the share was gone, someone else
/// replaced the file at that name with theirs. The copy can't call that file its
/// own, so it takes the normal late-clash path: it fails naming the conflict and
/// leaves their file alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_someone_else_took_during_the_outage_stays_a_conflict() {
    let engine = TestOperations::new();
    const THEIRS: &[u8] = b"someone else's file, saved while the share was away";
    let (inner, outcome) = copy_through_a_drop_at_landing(
        &engine,
        "outage-landed-then-taken",
        RenameDrop::AfterTheServerPerformsIt,
        Some(THEIRS),
    )
    .await;

    let error = outcome.expect_err("a file the copy can't prove its own is never claimed");
    assert!(
        matches!(error, WriteOperationError::DestinationExists { .. }),
        "the clash is reported as one: {error:?}"
    );
    assert_eq!(
        content_of(&inner, "/batch/f1.txt").await,
        THEIRS.to_vec(),
        "theirs survives"
    );
}

/// How much of the swallowed write the drop cut off besides its reply.
#[derive(Clone, Copy)]
enum SwallowedWrite {
    /// The bytes and the source's date both landed.
    Whole,
    /// The bytes landed, the date stamp after them didn't (SMB stamps a
    /// compound write by path, in a frame of its own).
    WithoutItsStamp,
}

/// Copies `/batch` (four files) onto a share that lands every write in one shot
/// (no temp, `CreateNew` at the final name) and drops right after the server
/// completed the second file's write, before its reply arrived. With `theirs`,
/// someone else saves that name while the share is gone.
async fn copy_through_a_swallowed_single_shot_write(
    engine: &TestOperations,
    lane: &str,
    theirs: Option<&'static [u8]>,
    swallowed: SwallowedWrite,
) -> (Arc<InMemoryVolume>, Result<(), WriteOperationError>) {
    let source = source_with(4).await;
    let (inner, nas, dest) = nas(lane);
    nas.write_single_shot();
    match swallowed {
        SwallowedWrite::Whole => nas.drop_after_write(1),
        SwallowedWrite::WithoutItsStamp => nas.drop_after_write_before_its_stamp(1),
    }
    let state = outage_state(engine, Duration::from_secs(30));
    let op = engine.register_state(lane, Arc::clone(&state));
    let watched = Arc::clone(&state);
    let behind_the_share = Arc::clone(&inner);
    let share = Arc::clone(&nas);
    tokio::spawn(async move {
        wait_until_async(WAIT, "the copy to wait for the share", || {
            watched.outage.waiting_for().is_some()
        })
        .await;
        if let Some(theirs) = theirs {
            behind_the_share
                .delete(Path::new("/batch/f1.txt"))
                .await
                .expect("ours is there to save over");
            behind_the_share
                .create_file(Path::new("/batch/f1.txt"), theirs)
                .await
                .expect("their save");
        }
        share.become_reachable();
    });

    let outcome = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op.id().as_str(),
        &state,
        source,
        &[PathBuf::from("/batch")],
        dest,
        Path::new("/"),
        &VolumeCopyConfig {
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error);
    (inner, outcome)
}

/// A single-shot write the server completed and the drop swallowed the reply
/// of: once the share is back, the file at the name is byte-for-byte the source,
/// so the copy counts it done rather than re-creating it into its own file and
/// failing with `DestinationExists` (SMB's compound write for a small file; the
/// live-SMB cell's actual failure).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_shot_write_the_drop_swallowed_counts_as_landed() {
    let engine = TestOperations::new();
    let (inner, outcome) = copy_through_a_swallowed_single_shot_write(
        &engine,
        "outage-single-shot-unanswered",
        None,
        SwallowedWrite::Whole,
    )
    .await;

    outcome.expect("a write the server completed is the copy's own file, not a conflict");
    for n in 0..4 {
        assert_eq!(
            content_of(&inner, &format!("/batch/f{n}.txt")).await,
            format!("file {n}").into_bytes(),
            "f{n} must land byte-exact"
        );
    }
}

/// The swallowed write's bytes landed but its date stamp didn't (the stamp is
/// what met the drop): the copy counts the file done and puts the source's date
/// on by path once the share is back, ❌ never leaving it silently on the
/// server's date.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_settled_single_shot_write_gets_the_date_its_stamp_lost() {
    let engine = TestOperations::new();
    let (inner, outcome) = copy_through_a_swallowed_single_shot_write(
        &engine,
        "outage-single-shot-unstamped",
        None,
        SwallowedWrite::WithoutItsStamp,
    )
    .await;

    outcome.expect("a write the server completed is the copy's own file");
    for n in 0..4 {
        let path = format!("/batch/f{n}.txt");
        assert_eq!(
            content_of(&inner, &path).await,
            format!("file {n}").into_bytes(),
            "f{n} must land byte-exact"
        );
        let entry = inner.get_metadata(Path::new(&path)).await.expect("it landed");
        assert_eq!(entry.modified_at, Some(SOURCE_MODIFIED), "the source's date is on f{n}");
    }
}

/// The same swallowed single-shot write, but someone else saved the name while
/// the share was gone: their file isn't the source's bytes, so it stays a
/// conflict and stays theirs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_shot_name_someone_else_took_during_the_outage_stays_a_conflict() {
    let engine = TestOperations::new();
    const THEIRS: &[u8] = b"theirs!"; // one byte longer than "file 1"
    let (inner, outcome) = copy_through_a_swallowed_single_shot_write(
        &engine,
        "outage-single-shot-taken",
        Some(THEIRS),
        SwallowedWrite::Whole,
    )
    .await;

    let error = outcome.expect_err("a file the copy can't prove its own is never claimed");
    assert!(
        matches!(error, WriteOperationError::DestinationExists { .. }),
        "the clash is reported as one: {error:?}"
    );
    assert_eq!(
        content_of(&inner, "/batch/f1.txt").await,
        THEIRS.to_vec(),
        "theirs survives"
    );
}

/// Same size, different bytes: size alone never proves a temp-less write ours.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_shot_name_taken_with_the_same_size_stays_a_conflict() {
    let engine = TestOperations::new();
    const THEIRS: &[u8] = b"theirs"; // exactly as long as "file 1"
    let (inner, outcome) = copy_through_a_swallowed_single_shot_write(
        &engine,
        "outage-single-shot-same-size",
        Some(THEIRS),
        SwallowedWrite::Whole,
    )
    .await;

    let error = outcome.expect_err("a same-size file with other bytes is never claimed");
    assert!(
        matches!(error, WriteOperationError::DestinationExists { .. }),
        "the clash is reported as one: {error:?}"
    );
    assert_eq!(
        content_of(&inner, "/batch/f1.txt").await,
        THEIRS.to_vec(),
        "theirs survives"
    );
}
