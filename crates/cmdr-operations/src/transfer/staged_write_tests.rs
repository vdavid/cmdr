//! `staged_write.rs`'s cells: where a staged write goes, what its landing
//! does with a name that's free, claimed, or taken, and what happens to the
//! temp when the landing fails or never finishes.

use super::super::recovered_name::LandingAftermath;
use super::*;
use crate::in_flight_temps::{ItemKind, Ledger, Record, TrackedItem};
use crate::testing::TestOperations;
use crate::transfer::volume::forward_volume_methods;
use crate::types::WriteOperationError;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::InMemoryVolume;
use std::sync::Mutex;
use std::time::Duration;

/// What `path` reports right now, which is how these cells tell "the
/// destination is untouched" from "we replaced it".
async fn size_of(volume: &InMemoryVolume, path: &str) -> Option<u64> {
    volume.get_metadata(Path::new(path)).await.ok().and_then(|e| e.size)
}

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(50)))
}

/// `land`, for a cell about the landing alone: on a state of its own whose
/// ledger records nothing on disk, at the size the temp holds (what the write
/// would have confirmed), with no outage wait (a cut-off landing gives up at
/// once, as `StagedWrite::commit_without_a_wait` does).
async fn land_unrecorded(
    dest: &Arc<dyn Volume>,
    temp: &Path,
    final_path: &Path,
    landing: LandingName,
) -> Result<(), FinalizeFailure> {
    let engine = TestOperations::new();
    let state = state(&engine);
    let size = dest
        .get_metadata(temp)
        .await
        .ok()
        .and_then(|entry| entry.size)
        .unwrap_or(0);
    let temp = StagingTemp::adopt(temp.to_path_buf(), state.liveness_token());
    let home_volume = state.dest_volume_id().map(str::to_owned);
    match land(dest, Landing::new(&state, temp, final_path, landing, size, home_volume)).await {
        Ok(()) => Ok(()),
        Err(LandingError::Failed(failure)) => Err(failure),
        Err(LandingError::CutOff(cut_off)) => cut_off.give_up(dest).await,
    }
}

/// A staged write must never hand the writer the final name, and the temp it
/// picks must be a recognizable sibling.
#[test]
fn staging_writes_to_a_recognizable_temp_sibling() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let staged = StagedWrite::begin_unannounced(&state, Path::new("/dir/notes.txt"), WriteStaging::Stage);
    assert_ne!(staged.target(), Path::new("/dir/notes.txt"));
    assert_eq!(staged.target().parent(), Some(Path::new("/dir")));
    assert!(
        staged.target().to_string_lossy().contains(".cmdr-tmp-"),
        "got {}",
        staged.target().display()
    );
    assert_eq!(
        state.in_flight_temps.lock_ignore_poison().len(),
        1,
        "the partial must be findable while it is being written"
    );
}

/// A write over the user's original stages exactly like any other: its own
/// tracked temp beside the original, ❗ even where the destination would take
/// it in one shot. The original's survival through the whole write is the
/// stronger guarantee.
#[test]
fn a_write_over_an_original_stages_and_is_never_single_shot() {
    let engine = TestOperations::new();
    let state = state(&engine);
    for single_shot in [false, true] {
        let staging = resolve_staging(WriteStaging::StageOverOriginal, single_shot);
        assert_eq!(staging, WriteStaging::StageOverOriginal, "single-shot: {single_shot}");
        let staged = StagedWrite::begin_unannounced(&state, Path::new("/dir/notes.txt"), staging);
        assert_ne!(staged.target(), Path::new("/dir/notes.txt"));
        assert_eq!(staged.target().parent(), Some(Path::new("/dir")));
        assert!(staged.target().to_string_lossy().contains(".cmdr-tmp-"));
        assert_eq!(staged.write_mode(), WriteMode::CreateOrReplace);
        assert!(
            state
                .in_flight_temps
                .lock_ignore_poison()
                .iter()
                .any(|path| path == staged.target())
        );
    }
}

/// What a destination was asked to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Delete(PathBuf),
    Rename { from: PathBuf, to: PathBuf, force: bool },
}

/// A destination that logs every delete and rename it gets.
struct CallLog {
    inner: Arc<InMemoryVolume>,
    calls: Mutex<Vec<Call>>,
    /// Whether its forced rename replaces a file in one step
    /// (`Volume::supports_atomic_replace_rename`).
    replaces_atomically: bool,
}

impl CallLog {
    async fn over_an_original(replaces_atomically: bool) -> (Arc<InMemoryVolume>, Arc<Self>) {
        let inner = Arc::new(InMemoryVolume::new("dest"));
        inner.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
        let log = Arc::new(CallLog {
            inner: Arc::clone(&inner),
            calls: Mutex::new(Vec::new()),
            replaces_atomically,
        });
        (inner, log)
    }
}

impl Volume for CallLog {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_atomic_replace_rename(&self) -> bool {
        self.replaces_atomically
    }
    fn delete<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.calls.lock_ignore_poison().push(Call::Delete(path.to_path_buf()));
        self.inner.delete(path)
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.calls.lock_ignore_poison().push(Call::Rename {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            force,
        });
        self.inner.rename(from, to, force)
    }
}

/// ❗ On a backend whose forced rename isn't one atomic step, a swap moves the
/// original ASIDE first, lands the temp on the name it vacated, and deletes the
/// aside last: the original is whole at one of its names at every moment. And
/// ❌ never `rename(force)`, which on MTP can leave a duplicate beside the
/// original and on SFTP may delete it in a step of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swap_moves_the_original_aside_first_and_never_renames_with_force() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let (inner, log) = CallLog::over_an_original(false).await;
    let dest: Arc<dyn Volume> = Arc::clone(&log) as Arc<dyn Volume>;

    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::StageOverOriginal);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();
    staged.commit(&dest, 3).await.expect("the swap lands");

    let calls = log.calls.lock_ignore_poison().clone();
    let Some(Call::Rename { to: aside, .. }) = calls.first() else {
        panic!("the original moves first: {calls:?}");
    };
    assert!(
        aside.to_string_lossy().contains(".cmdr-temp-"),
        "onto an aside name no stale-temp reap takes: {aside:?}"
    );
    assert_eq!(
        calls,
        vec![
            Call::Rename {
                from: PathBuf::from("/notes.txt"),
                to: aside.clone(),
                force: false,
            },
            Call::Rename {
                from: temp.clone(),
                to: PathBuf::from("/notes.txt"),
                force: false,
            },
            Call::Delete(aside.clone()),
        ],
    );
    assert_eq!(
        size_of(&inner, "/notes.txt").await,
        Some(3),
        "the new bytes replaced the original"
    );
    assert!(!inner.exists(&temp).await);
    assert!(!inner.exists(aside).await, "and the aside is gone");
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// ❗ Where the destination's forced rename replaces a file in ONE step (a local
/// disk's `rename(2)`, SMB's `ReplaceIfExists`), a swap takes it: the name
/// never goes empty, so there's no moment where the temp is the only copy and
/// nothing to record as a landing, and it's one round trip instead of two.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_swap_on_an_atomic_replace_backend_lands_in_one_replacing_rename() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let (inner, log) = CallLog::over_an_original(true).await;
    let dest: Arc<dyn Volume> = Arc::clone(&log) as Arc<dyn Volume>;

    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::StageOverOriginal);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"THE NEW BYTES").await.unwrap();
    staged.commit(&dest, 13).await.expect("the swap lands");

    assert_eq!(
        *log.calls.lock_ignore_poison(),
        vec![Call::Rename {
            from: temp.clone(),
            to: PathBuf::from("/notes.txt"),
            force: true,
        }],
        "one replacing rename, ❌ no delete of the original",
    );
    assert_eq!(
        size_of(&inner, "/notes.txt").await,
        Some(13),
        "the new bytes replaced the original"
    );
    assert!(!inner.exists(&temp).await);
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// A replacing rename the destination refused never touched the original (the
/// replace is one step or nothing), so the temp is a spare copy and goes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_replacing_rename_leaves_the_original_and_takes_the_temp_away() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner = Arc::new(
        InMemoryVolume::new("dest").with_rename_failing(VolumeError::PermissionDenied {
            path: "/notes.txt".to_owned(),
            raw_os_error: None,
        }),
    );
    inner.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
    let log = Arc::new(CallLog {
        inner: Arc::clone(&inner),
        calls: Mutex::new(Vec::new()),
        replaces_atomically: true,
    });
    let dest: Arc<dyn Volume> = Arc::clone(&log) as Arc<dyn Volume>;
    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::StageOverOriginal);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"THE NEW BYTES").await.unwrap();

    let failure = staged
        .commit(&dest, 13)
        .await
        .map_err(LandingError::ended)
        .expect_err("refused");

    assert!(matches!(failure.error, VolumeError::PermissionDenied { .. }));
    assert!(
        failure.aftermath.is_none(),
        "nothing was cleared, so nothing was rescued"
    );
    assert_eq!(size_of(&inner, "/notes.txt").await, Some(3), "the original stays");
    assert!(!inner.exists(&temp).await, "the spare temp goes");
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// A single-shot write goes to the final name with no temp and nothing
/// tracked: the destination lands it whole or not at all, so there is no
/// partial to find, sweep, or land.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_shot_write_targets_the_final_name_and_needs_no_landing() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let staged = StagedWrite::begin_unannounced(
        &state,
        Path::new("/notes.txt"),
        WriteStaging::SingleShot(LandingName::ExpectedFree),
    );
    assert_eq!(staged.target(), Path::new("/notes.txt"));
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());

    // The backend wrote the whole file in one shot; committing is a no-op
    // that must not touch the destination.
    inner.create_file(Path::new("/notes.txt"), b"NEW").await.unwrap();
    staged.commit(&dest, 3).await.unwrap();
    assert!(inner.exists(Path::new("/notes.txt")).await);
}

/// A write that goes straight to a final name nobody resolved a conflict for
/// must refuse a taken name itself, because no landing will: `CreateNew`.
/// Every other write may replace what's at its target (our own temp, or a name
/// the caller claimed with a placeholder).
#[test]
fn only_a_single_shot_write_onto_a_name_expected_free_must_create_new() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let mode_for = |requested: WriteStaging, single_shot: bool| {
        StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), resolve_staging(requested, single_shot))
            .write_mode()
    };

    assert_eq!(mode_for(WriteStaging::Stage, true), WriteMode::CreateNew);
    assert_eq!(
        mode_for(WriteStaging::StageInFreshFolder, true),
        WriteMode::CreateNewInFreshFolder,
        "a name in a folder this operation made carries that fact to the destination"
    );
    assert_eq!(
        mode_for(WriteStaging::StageInFreshFolder, false),
        WriteMode::CreateOrReplace,
        "staged, it writes its own temp, as `Stage` does"
    );
    assert_eq!(
        mode_for(WriteStaging::StageOntoClaimedName, true),
        WriteMode::CreateOrReplace,
        "a claimed name holds the caller's own placeholder, which the write must replace"
    );
    assert_eq!(
        mode_for(WriteStaging::StageOverOriginal, true),
        WriteMode::CreateOrReplace
    );
    for requested in [
        WriteStaging::Stage,
        WriteStaging::StageOntoClaimedName,
        WriteStaging::StageOverOriginal,
    ] {
        assert_eq!(
            mode_for(requested, false),
            WriteMode::CreateOrReplace,
            "{requested:?} writes a temp, and the landing decides about the final name"
        );
    }
}

/// Committing lands the bytes at the final name and drops the temp from the
/// in-flight set, so nothing can sweep the committed data afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_lands_the_bytes_and_stops_tracking_them() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    staged.commit(&dest, 3).await.unwrap();

    assert!(inner.exists(Path::new("/notes.txt")).await);
    assert!(!inner.exists(&temp).await);
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// Landing onto a name the CALLER claimed, a `Rename` pick's empty
/// placeholder, is the case the second attempt exists for: clear the way, then
/// rename again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn landing_clears_a_claimed_name_that_is_genuinely_in_the_way() {
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner.create_file(Path::new("/notes.txt"), b"").await.unwrap();
    inner.create_file(Path::new("/temp"), b"NEW").await.unwrap();

    land_unrecorded(
        &dest,
        Path::new("/temp"),
        Path::new("/notes.txt"),
        LandingName::ClaimedByTheCaller,
    )
    .await
    .unwrap();

    assert!(!inner.exists(Path::new("/temp")).await);
    assert_eq!(size_of(&inner, "/notes.txt").await, Some(3), "the new bytes landed");
}

/// The same collision under a name the caller believed FREE is a conflict
/// nobody answered, and clearing it would replace a file under a policy
/// (Skip, an unanswered Stop) that promised not to.
///
/// The reachable case needs no race: a case-insensitive destination resolves
/// `Report.docx` onto the user's `report.docx`, so the rename says
/// `AlreadyExists` for a name the level listing reported as free.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn landing_onto_a_name_believed_free_leaves_it_alone() {
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner
        .create_file(Path::new("/notes.txt"), b"THE USER'S FILE")
        .await
        .unwrap();
    inner.create_file(Path::new("/temp"), b"NEW").await.unwrap();

    let outcome = land_unrecorded(
        &dest,
        Path::new("/temp"),
        Path::new("/notes.txt"),
        LandingName::ExpectedFree,
    )
    .await;

    assert!(
        matches!(
            outcome,
            Err(FinalizeFailure {
                error: VolumeError::AlreadyExists(_),
                ..
            })
        ),
        "the clash is what the caller has to see; got {outcome:?}"
    );
    assert_eq!(
        size_of(&inner, "/notes.txt").await,
        Some(15),
        "a name nobody resolved a conflict for must still hold the user's bytes"
    );
    assert!(
        !inner.exists(Path::new("/temp")).await,
        "and our complete-but-unplaced temp goes at once: the source still holds those bytes"
    );
}

/// A rename that failed for ANY OTHER reason must leave the destination
/// alone.
///
/// The transient case is the one that costs a file. Over SFTP or SMB a
/// rename can fail because the session blinked, and SFTP v3 collapses most
/// of errno into one catch-all code, so a landing that cleared the way on
/// every `Err` would delete the user's existing file and then report the
/// blip. `AlreadyExists` is the only answer that says something is in the
/// way; everything else says the destination is none of our business.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rename_that_failed_for_another_reason_leaves_the_destination_alone() {
    let (inner, outcome) = land_with_a_rename_that_fails(VolumeError::DeviceDisconnected("blip".to_string())).await;

    assert!(
        matches!(
            outcome,
            Err(FinalizeFailure {
                error: VolumeError::DeviceDisconnected(_),
                ..
            })
        ),
        "the rename's own failure is what the caller has to see; got {outcome:?}"
    );
    assert_eq!(
        size_of(&inner, "/notes.txt").await,
        Some(15),
        "a rename that never said the destination was in the way must not have cost the user their file"
    );
    assert!(
        inner.exists(Path::new("/temp")).await,
        "and a transport failure leaves the temp for the caller's settle: the server may have performed the rename, \
         and only a temp no discard of ours touched proves it was ours"
    );
}

/// A rename the destination REFUSED (not a dropped link) is a plain no: nothing
/// was cleared and the source still holds the bytes, so the complete-but-unplaced
/// temp goes at once rather than waiting for the hourly reap.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_rename_takes_its_temp_away_at_once() {
    let (inner, outcome) = land_with_a_rename_that_fails(VolumeError::PermissionDenied {
        path: "/notes.txt".to_owned(),
        raw_os_error: None,
    })
    .await;

    assert!(
        matches!(
            outcome,
            Err(FinalizeFailure {
                error: VolumeError::PermissionDenied { .. },
                ..
            })
        ),
        "got {outcome:?}"
    );
    assert_eq!(size_of(&inner, "/notes.txt").await, Some(15));
    assert!(!inner.exists(Path::new("/temp")).await, "the refused temp goes at once");
}

/// The same shape, one flavor further: a backend that can delete but can't
/// rename must not destroy the destination and then report `NotSupported`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_backend_without_rename_does_not_delete_what_it_cannot_replace() {
    let (inner, outcome) = land_with_a_rename_that_fails(VolumeError::NotSupported).await;

    assert!(
        matches!(
            outcome,
            Err(FinalizeFailure {
                error: VolumeError::NotSupported,
                ..
            })
        ),
        "got {outcome:?}"
    );
    assert!(inner.exists(Path::new("/notes.txt")).await);
}

/// A landing onto a destination the user already has, over a backend whose
/// rename fails with `failure`. Answers the volume so a cell can ask what
/// survived.
async fn land_with_a_rename_that_fails(failure: VolumeError) -> (Arc<InMemoryVolume>, Result<(), FinalizeFailure>) {
    let inner = Arc::new(InMemoryVolume::new("dest").with_rename_failing(failure));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner
        .create_file(Path::new("/notes.txt"), b"THE USER'S FILE")
        .await
        .unwrap();
    inner.create_file(Path::new("/temp"), b"NEW").await.unwrap();

    // The claimed reading, so the cell is about the rename's error and not
    // about who owns the name.
    let outcome = land_unrecorded(
        &dest,
        Path::new("/temp"),
        Path::new("/notes.txt"),
        LandingName::ClaimedByTheCaller,
    )
    .await;
    (inner, outcome)
}

/// The landing cleared a name the caller claimed, and then the rename onto
/// it was refused. The destination is gone, so the temp holds the ONLY
/// complete copy of the new file — and it wears a `.cmdr-tmp-*` name, which
/// `cleanup.rs::reap_stale_transfer_temps` matches on age at the start of the
/// next transfer into that folder. It has to get out of temp space, and the
/// failure has to say where it went.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_that_cleared_the_way_and_then_could_not_rename_rescues_the_new_bytes() {
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    // A `Rename` pick's placeholder: the name this write claimed.
    inner.create_file(Path::new("/notes.txt"), b"").await.unwrap();
    inner
        .create_file(Path::new("/notes.txt.cmdr-tmp-abc"), b"THE NEW BYTES")
        .await
        .unwrap();
    // Only the final name is refused, so the rescue's own rename can land.
    inner.set_rename_to_failing(Path::new("/notes.txt"));

    let failure = land_unrecorded(
        &dest,
        Path::new("/notes.txt.cmdr-tmp-abc"),
        Path::new("/notes.txt"),
        LandingName::ClaimedByTheCaller,
    )
    .await
    .expect_err("a landing that can't rename has to fail");

    assert!(
        !inner.exists(Path::new("/notes.txt.cmdr-tmp-abc")).await,
        "committed data may not be left under a name the stale-temp reap matches"
    );
    assert_eq!(
        failure.aftermath,
        Some(LandingAftermath::NewDataKeptAt(PathBuf::from("/notes (recovered).txt"))),
        "and the failure has to name where the only copy of the new file is"
    );
    assert_eq!(
        size_of(&inner, "/notes (recovered).txt").await,
        Some(13),
        "which is where the bytes actually are"
    );
}

/// Abandoning removes the partial and stops tracking it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandon_removes_the_partial() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"half").await.unwrap();

    staged.abandon(&dest).await;

    assert!(!inner.exists(&temp).await);
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// ❗ An abandon whose delete the destination turns down (a share that's out, a
/// handle still open) keeps the temp LISTED, so the stop's cleanup takes it
/// later, or the ledger does on the share's next reconnect. Retired first, it
/// would sit on the share with nothing knowing it's ours.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_abandon_the_destination_refuses_keeps_the_temp_listed() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner = Arc::new(InMemoryVolume::new("dest").with_delete_failing());
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"half").await.unwrap();

    staged.abandon(&dest).await;

    assert!(inner.exists(&temp).await, "the fixture refuses the delete");
    assert_eq!(*state.in_flight_temps.lock_ignore_poison(), vec![temp]);
}

/// A landing refused over a name nobody resolved takes its temp away AND
/// stops tracking it, so nothing is left in the user's folder and nothing
/// is left for a sweep to chase.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_landing_takes_its_temp_away_and_stops_tracking_it() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner
        .create_file(Path::new("/notes.txt"), b"THE USER'S FILE")
        .await
        .unwrap();

    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    let outcome = staged.commit(&dest, 3).await.map_err(LandingError::ended);

    assert!(
        matches!(
            outcome,
            Err(FinalizeFailure {
                error: VolumeError::AlreadyExists(_),
                aftermath: None,
            })
        ),
        "got {outcome:?}"
    );
    assert_eq!(size_of(&inner, "/notes.txt").await, Some(15), "the user's file stays");
    assert!(!inner.exists(&temp).await, "our temp goes");
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// Generated-file replacement first tries to set the old destination aside.
/// If that rename refuses, the old bytes are still authoritative and the
/// completed stage is ordinary disposable output: remove it and retire its
/// record immediately rather than leaving cleanup to a later sweep.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_displacement_preserves_the_original_and_cleans_the_generated_stage() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner =
        Arc::new(InMemoryVolume::new("dest").with_rename_failing(VolumeError::DeviceDisconnected("blip".to_string())));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner
        .create_file(Path::new("/notes.txt"), b"THE USER'S FILE")
        .await
        .unwrap();
    let staged = StagedWrite::begin_generated(&state, dest.as_ref(), Announced::assumed(Path::new("/notes.txt")), true);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    let outcome = staged.commit_with_displaced_original(&dest, 3).await;

    assert!(outcome.is_err());
    assert_eq!(size_of(&inner, "/notes.txt").await, Some(15));
    assert!(!inner.exists(&temp).await, "the completed but unpublished stage goes");
    assert!(
        state.in_flight_temps.lock_ignore_poison().is_empty(),
        "a stage deleted after displacement refusal must not remain recovery-owned"
    );
}

/// A destination whose `rename` never answers: the landing a cancel (or the
/// concurrent driver dropping its window) abandons midway.
struct RenameNeverAnswers {
    inner: Arc<InMemoryVolume>,
}

impl Volume for RenameNeverAnswers {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file, delete,
    );
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn rename<'a>(
        &'a self,
        _from: &'a Path,
        _to: &'a Path,
        _force: bool,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}

/// ❗ A landing dropped midway keeps its temp REGISTERED: the rename may not
/// have happened, so the temp can still be sitting beside the free name, and
/// the post-loop sweep of abandoned writes (and the startup sweep after a
/// crash) is the only thing that will ever find it. The source still holds
/// its bytes, so the sweep deleting it loses nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_dropped_midway_leaves_its_temp_to_the_abandoned_write_sweep() {
    let engine = TestOperations::new();
    let state = state(&engine);
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::new(RenameNeverAnswers {
        inner: Arc::clone(&inner),
    });

    let staged = StagedWrite::begin_unannounced(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    let landed = tokio::time::timeout(Duration::from_millis(50), staged.commit(&dest, 3)).await;

    assert!(landed.is_err(), "the rename is rigged never to answer");
    assert_eq!(
        *state.in_flight_temps.lock_ignore_poison(),
        vec![temp],
        "the abandoned landing's temp must stay findable"
    );
}

/// The destination volume id the ledger cells below record their temps under.
const LEDGER_VOLUME_ID: &str = "staged-write-test-nas";

/// A state writing to [`LEDGER_VOLUME_ID`] and recording into `ledger`, the
/// way a volume copy's state names its destination.
fn state_recording_into(engine: &TestOperations, ledger: &Ledger) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(Duration::from_millis(50))
            .with_journal_volumes("staged-write-test-source".to_string(), LEDGER_VOLUME_ID.to_string())
            .with_in_flight_ledger(ledger.clone()),
    )
}

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

/// Whether `records` hold `temp` as a landing on its way to `/notes.txt` at
/// `size` bytes.
fn records_landing(records: &[Record], temp: &Path, size: u64) -> bool {
    records.iter().any(|record| {
        matches!(record, Record::Tracked(TrackedItem {
            kind: ItemKind::Landing { destination, expected_size },
            path,
            ..
        }) if path == temp && destination == Path::new("/notes.txt") && *expected_size == size)
    })
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
