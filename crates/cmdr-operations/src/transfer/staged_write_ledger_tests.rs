//! What a staged write's landing leaves in the in-flight ledger's log, cells
//! that record into a real one: a claimed name's landing record on disk before
//! the clearing delete (and the next launch finishing a landing cut off after
//! it), and a swap that refuses with the original in place when its aside
//! record can't reach the disk.

use super::*;
use crate::types::WriteOperationError;

/// A destination that notes what the ledger's log held when the landing's
/// clearing `delete` went out, and can hold the landing rename that follows it.
struct WatchedLanding {
    inner: Arc<InMemoryVolume>,
    data_dir: PathBuf,
    on_disk_at_delete: Mutex<Option<Vec<Record>>>,
    /// Whether the rename after a delete never answers: the landing a crash cut
    /// off once the way was clear.
    hold_rename_after_delete: bool,
    deleted: std::sync::atomic::AtomicBool,
}

impl Volume for WatchedLanding {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn delete<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        *self.on_disk_at_delete.lock_ignore_poison() = Some(Ledger::replay_log(&self.data_dir));
        self.deleted.store(true, std::sync::atomic::Ordering::SeqCst);
        self.inner.delete(path)
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        if self.hold_rename_after_delete && self.deleted.load(std::sync::atomic::Ordering::SeqCst) {
            return Box::pin(std::future::pending());
        }
        self.inner.rename(from, to, force)
    }
}

/// A claimed name (`/notes.txt`, holding the caller's placeholder) and a
/// complete staged write on its way there, recording into a ledger of its own.
struct ClaimedLanding {
    dir: cmdr_fs::testing::TestDir,
    engine: TestOperations,
    ledger: Ledger,
    state: Arc<WriteOperationState>,
    inner: Arc<InMemoryVolume>,
    dest: Arc<WatchedLanding>,
    staged: StagedWrite,
    temp: PathBuf,
}

async fn claimed_landing(name: &str, hold_rename_after_delete: bool) -> ClaimedLanding {
    let dir = cmdr_fs::testing::TestDir::new(name);
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);
    let engine = TestOperations::new();
    let state = state_recording_into(&engine, &ledger);
    let inner = Arc::new(InMemoryVolume::new("dest"));
    inner.create_file(Path::new("/notes.txt"), b"").await.unwrap();
    let dest = Arc::new(WatchedLanding {
        inner: Arc::clone(&inner),
        data_dir,
        on_disk_at_delete: Mutex::new(None),
        hold_rename_after_delete,
        deleted: std::sync::atomic::AtomicBool::new(false),
    });
    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::StageOntoClaimedName);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"THE NEW BYTES").await.unwrap();
    ClaimedLanding {
        dir,
        engine,
        ledger,
        state,
        inner,
        dest,
        staged,
        temp,
    }
}

/// ❗ From the moment a landing may clear a name, its temp can be the
/// destination's only copy, so the ledger has to know that BEFORE the clearing
/// delete goes out: a crash right after it would otherwise leave the only copy
/// with nothing recording it, or recorded as a temp, the one kind the sweep
/// deletes on sight.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claimed_name_landing_is_on_disk_as_a_landing_before_it_clears_the_way() {
    let fixture = claimed_landing("staged-write-landing-recorded", false).await;
    let dest: Arc<dyn Volume> = Arc::clone(&fixture.dest) as Arc<dyn Volume>;

    fixture
        .staged
        .commit(&dest, 13)
        .await
        .expect("the claimed name is cleared and landed");

    let on_disk = fixture
        .dest
        .on_disk_at_delete
        .lock_ignore_poison()
        .clone()
        .expect("the landing cleared the way");
    assert!(
        records_landing(&on_disk, &fixture.temp, 13),
        "the landing record was on disk when the delete went out: {on_disk:?}"
    );
    assert!(
        fixture.state.in_flight_temps.lock_ignore_poison().is_empty(),
        "and the temp is no copy of ours to remove any more"
    );
    assert_eq!(
        size_of(&fixture.inner, "/notes.txt").await,
        Some(13),
        "the new bytes landed"
    );
    fixture.ledger.clone().die();
    assert!(
        Ledger::replay_log(&fixture.dir.join("data")).is_empty(),
        "and once landed, nothing is left recorded"
    );
}

/// The crash that record is for: the way was cleared and the landing rename
/// never answered. The temp is out of every in-operation sweep's reach, and
/// the next launch finishes the landing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_cut_off_after_clearing_the_way_is_finished_by_the_next_launch() {
    let fixture = claimed_landing("staged-write-landing-cut-off", true).await;
    let dest: Arc<dyn Volume> = Arc::clone(&fixture.dest) as Arc<dyn Volume>;

    let cut_off = tokio::time::timeout(Duration::from_millis(100), fixture.staged.commit(&dest, 13)).await;
    assert!(
        cut_off.is_err(),
        "the rename after the delete is rigged never to answer"
    );
    assert!(
        !fixture.inner.exists(Path::new("/notes.txt")).await,
        "the way was cleared"
    );
    assert!(
        fixture.state.in_flight_temps.lock_ignore_poison().is_empty(),
        "❗ the only copy is no temp the abandoned-write sweep may delete"
    );

    let data_dir = fixture.dir.join("data");
    fixture.ledger.die();
    let next_app = TestOperations::new();
    next_app
        .volumes()
        .register(LEDGER_VOLUME_ID, Arc::clone(&fixture.inner) as Arc<dyn Volume>);
    let tally = Ledger::for_test().launch_in(&data_dir, &next_app).wait();

    assert_eq!(
        size_of(&fixture.inner, "/notes.txt").await,
        Some(13),
        "the next launch lands the new bytes at their name: {tally:?}"
    );
    assert!(!fixture.inner.exists(&fixture.temp).await);
    drop(fixture.engine);
}

/// The user's original at `/notes.txt` and a complete write over it, staged on
/// `state`, landing on a plain in-memory destination.
async fn swap_over(state: &Arc<WriteOperationState>) -> (Arc<InMemoryVolume>, Arc<dyn Volume>, StagedWrite) {
    let inner = Arc::new(InMemoryVolume::new("dest"));
    inner.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
    let staged = StagedWrite::begin_unannounced(state, Path::new("/notes.txt"), WriteStaging::StageOverOriginal);
    inner.create_file(staged.target(), b"THE NEW BYTES").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    (inner, dest, staged)
}

/// Every aside (`.cmdr-temp-*`) in `folder`.
async fn asides_in(volume: &InMemoryVolume, folder: &str) -> Vec<PathBuf> {
    volume
        .list_directory(Path::new(folder), None)
        .await
        .unwrap()
        .into_iter()
        .filter(|entry| entry.name.contains(cmdr_fs::staging::STAGING_ASIDE_MARKER))
        .map(|entry| PathBuf::from(entry.path))
        .collect()
}

/// Asserts a swap refused before it moved anything: the original is whole at
/// its name, nothing went aside, and nothing is left recorded.
async fn assert_refused_with_the_original_in_place(
    inner: &InMemoryVolume,
    ledger: &Ledger,
    landed: Result<(), FinalizeFailure>,
) {
    let failure = landed.expect_err("❗ an aside record that never reached the disk must not move the original");
    assert_eq!(size_of(inner, "/notes.txt").await, Some(3), "the original survives");
    assert_eq!(
        asides_in(inner, "/").await,
        Vec::<PathBuf>::new(),
        "and never went aside"
    );
    assert_eq!(
        (ledger.recorded_landings(), ledger.recorded_asides()),
        (Vec::<PathBuf>::new(), Vec::<PathBuf>::new()),
        "and nothing is left recorded"
    );
    // Typed, so the dialog says the original stayed: nothing was cleared, so
    // nothing was rescued, and ❌ no sentence comes from the backend.
    let reported = crate::transfer::volume::map_finalize_failure(Path::new("/notes.txt"), failure);
    assert!(
        matches!(&reported, WriteOperationError::LandingNotRecorded { path } if path == "/notes.txt"),
        "the refusal is its own variant, named on the file it left alone: {reported:?}"
    );
}

/// ❗ The ledger's writer is gone (it panicked, or the log was closed), so the
/// aside record can't reach the disk. A crash after the move aside would leave
/// the user's original on a scratch name no record points back from, so the
/// swap refuses with nothing moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swap_whose_aside_record_has_no_writer_leaves_the_original_in_place() {
    let dir = cmdr_fs::testing::TestDir::new("staged-write-landing-no-writer");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);
    let engine = TestOperations::new();
    let state = state_recording_into(&engine, &ledger);
    let (inner, dest, staged) = swap_over(&state).await;
    ledger.clone().die();

    let landed = staged.commit(&dest, 13).await.map_err(LandingError::ended);

    assert_refused_with_the_original_in_place(&inner, &ledger, landed).await;
}

/// The same refusal when the log is there but won't take the line (a full or
/// read-only data disk): only a record the log wrote counts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swap_whose_aside_record_the_log_refused_leaves_the_original_in_place() {
    let dir = cmdr_fs::testing::TestDir::new("staged-write-landing-log-refused");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_into_a_log_that_refuses_writes(&data_dir);
    let engine = TestOperations::new();
    let state = state_recording_into(&engine, &ledger);
    let (inner, dest, staged) = swap_over(&state).await;

    let landed = staged.commit(&dest, 13).await.map_err(LandingError::ended);

    assert_refused_with_the_original_in_place(&inner, &ledger, landed).await;
}

/// An operation that never named its destination volume has no path space to
/// record the aside in. On a ledger that keeps a log, that's a record that
/// can't reach the disk, so the swap refuses with nothing moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swap_with_no_path_space_for_its_aside_record_leaves_the_original_in_place() {
    let dir = cmdr_fs::testing::TestDir::new("staged-write-landing-no-home");
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let ledger = Ledger::recording_in(&data_dir);
    let engine = TestOperations::new();
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_in_flight_ledger(ledger.clone()),
    );
    let (inner, dest, staged) = swap_over(&state).await;

    let landed = staged.commit(&dest, 13).await.map_err(LandingError::ended);

    assert_refused_with_the_original_in_place(&inner, &ledger, landed).await;
}
