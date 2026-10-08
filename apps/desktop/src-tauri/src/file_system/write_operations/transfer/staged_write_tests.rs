//! `staged_write.rs`'s cells: where a staged write goes, what its landing
//! does with a name that's free, claimed, or taken, and what happens to the
//! temp when the landing fails or never finishes.

use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::transfer::volume::forward_volume_methods;
use crate::ignore_poison::IgnorePoison;
use std::time::Duration;

/// What `path` reports right now, which is how these cells tell "the
/// destination is untouched" from "we replaced it".
async fn size_of(volume: &InMemoryVolume, path: &str) -> Option<u64> {
    volume.get_metadata(Path::new(path)).await.ok().and_then(|e| e.size)
}

fn state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// A staged write must never hand the writer the final name, and the temp it
/// picks must be a recognizable sibling.
#[test]
fn staging_writes_to_a_recognizable_temp_sibling() {
    let state = state();
    let staged = StagedWrite::begin(&state, Path::new("/dir/notes.txt"), WriteStaging::Stage);
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

/// A caller-staged write is passed through: no second temp, and nothing
/// registered (the caller owns that path's lifetime).
#[test]
fn a_caller_staged_write_is_not_staged_again() {
    let state = state();
    let caller_temp = Path::new("/dir/notes.txt.cmdr-tmp-abc");
    let staged = StagedWrite::begin(&state, caller_temp, WriteStaging::AlreadyStaged);
    assert_eq!(staged.target(), caller_temp);
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// A single-shot write goes to the final name with no temp and nothing
/// tracked: the destination lands it whole or not at all, so there is no
/// partial to find, sweep, or land.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_single_shot_write_targets_the_final_name_and_needs_no_landing() {
    let state = state();
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let staged = StagedWrite::begin(
        &state,
        Path::new("/notes.txt"),
        WriteStaging::SingleShot(LandingName::ExpectedFree),
    );
    assert_eq!(staged.target(), Path::new("/notes.txt"));
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());

    // The backend wrote the whole file in one shot; committing is a no-op
    // that must not touch the destination.
    inner.create_file(Path::new("/notes.txt"), b"NEW").await.unwrap();
    staged.commit(&dest).await.unwrap();
    assert!(inner.exists(Path::new("/notes.txt")).await);
}

/// A write that goes straight to a final name nobody resolved a conflict for
/// must refuse a taken name itself, because no landing will: `CreateNew`.
/// Every other write may replace what's at its target (our own temp, the
/// caller's temp, or a name the caller claimed with a placeholder).
#[test]
fn only_a_single_shot_write_onto_a_name_expected_free_must_create_new() {
    let state = state();
    let mode_for = |requested: WriteStaging, single_shot: bool| {
        StagedWrite::begin(&state, Path::new("/notes.txt"), resolve_staging(requested, single_shot)).write_mode()
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
    assert_eq!(mode_for(WriteStaging::AlreadyStaged, true), WriteMode::CreateOrReplace);
    for requested in [
        WriteStaging::Stage,
        WriteStaging::StageOntoClaimedName,
        WriteStaging::AlreadyStaged,
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
    let state = state();
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let staged = StagedWrite::begin(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    staged.commit(&dest).await.unwrap();

    assert!(inner.exists(Path::new("/notes.txt")).await);
    assert!(!inner.exists(&temp).await);
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// Landing onto a name the CALLER claimed — a `Rename` pick's placeholder,
/// a cross-type Overwrite's cleared destination — is the case the second
/// attempt exists for: clear the way, then rename again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn landing_clears_a_claimed_name_that_is_genuinely_in_the_way() {
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
    inner.create_file(Path::new("/temp"), b"NEW").await.unwrap();

    land(
        &dest,
        Path::new("/temp"),
        Path::new("/notes.txt"),
        LandingName::ClaimedByTheCaller,
        &|| {},
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

    let outcome = land(
        &dest,
        Path::new("/temp"),
        Path::new("/notes.txt"),
        LandingName::ExpectedFree,
        &|| {},
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
        !inner.exists(Path::new("/temp")).await,
        "and the temp the bytes couldn't leave goes at once: nothing was cleared, so the source still holds them"
    );
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
    let outcome = land(
        &dest,
        Path::new("/temp"),
        Path::new("/notes.txt"),
        LandingName::ClaimedByTheCaller,
        &|| {},
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
    inner.create_file(Path::new("/notes.txt"), b"CLAIMED").await.unwrap();
    inner
        .create_file(Path::new("/notes.txt.cmdr-tmp-abc"), b"THE NEW BYTES")
        .await
        .unwrap();
    // Only the final name is refused, so the rescue's own rename can land.
    inner.set_rename_to_failing(Path::new("/notes.txt"));

    let failure = land(
        &dest,
        Path::new("/notes.txt.cmdr-tmp-abc"),
        Path::new("/notes.txt"),
        LandingName::ClaimedByTheCaller,
        &|| {},
    )
    .await
    .expect_err("a landing that can't rename has to fail");

    assert!(
        !inner.exists(Path::new("/notes.txt.cmdr-tmp-abc")).await,
        "committed data may not be left under a name the stale-temp reap matches"
    );
    assert_eq!(
        failure.new_data_at,
        Some(PathBuf::from("/notes (recovered).txt")),
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
    let state = state();
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

    let staged = StagedWrite::begin(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"half").await.unwrap();

    staged.abandon(&dest).await;

    assert!(!inner.exists(&temp).await);
    assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
}

/// A landing refused over a name nobody resolved takes its temp away AND
/// stops tracking it, so nothing is left in the user's folder and nothing
/// is left for a sweep to chase.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_landing_takes_its_temp_away_and_stops_tracking_it() {
    let state = state();
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner
        .create_file(Path::new("/notes.txt"), b"THE USER'S FILE")
        .await
        .unwrap();

    let staged = StagedWrite::begin(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    let outcome = staged.commit(&dest).await;

    assert!(
        matches!(
            outcome,
            Err(FinalizeFailure {
                error: VolumeError::AlreadyExists(_),
                new_data_at: None,
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
    let state = state();
    let inner =
        Arc::new(InMemoryVolume::new("dest").with_rename_failing(VolumeError::DeviceDisconnected("blip".to_string())));
    let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    inner
        .create_file(Path::new("/notes.txt"), b"THE USER'S FILE")
        .await
        .unwrap();
    let staged = StagedWrite::begin_generated(&state, Path::new("/notes.txt"), true);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    let outcome = staged.commit_with_displaced_original(&dest).await;

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
    let state = state();
    let inner = Arc::new(InMemoryVolume::new("dest"));
    let dest: Arc<dyn Volume> = Arc::new(RenameNeverAnswers {
        inner: Arc::clone(&inner),
    });

    let staged = StagedWrite::begin(&state, Path::new("/notes.txt"), WriteStaging::Stage);
    let temp = staged.target().to_path_buf();
    inner.create_file(&temp, b"NEW").await.unwrap();

    let landed = tokio::time::timeout(Duration::from_millis(50), staged.commit(&dest)).await;

    assert!(landed.is_err(), "the rename is rigged never to answer");
    assert_eq!(
        *state.in_flight_temps.lock_ignore_poison(),
        vec![temp],
        "the abandoned landing's temp must stay findable"
    );
}
