//! What a cross-volume move does when a write, a finalize rename, or a source
//! delete refuses: the user's bytes survive, and the report names the item that
//! actually failed.
//!
//! Two invariants live here. A finalize rename that fails after the original is
//! gone leaves the temp holding the only complete copy of the new data, so
//! nothing may clean it up. And a folder move that trips on one file 3,000
//! items down reports THAT file, not the folder the user selected, on both the
//! copy phase and the source-delete phase.
//!
//! Shared fixtures and the `MoveRenameFailsDestVolume` double live in
//! `volume/move_test_support.rs` (`super::test_support`).

use super::super::strategy::test_support::{FlakyDest, UndeletableSource};
use super::test_support::{MoveRenameFailsDestVolume, config_default, make_state};
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::{ConflictResolution, PermissionRefusal, PermissionSide};
use cmdr_fs::volume::{InMemoryVolume, VolumeError};

/// Cross-volume MOVE, file→file Overwrite, streaming write SUCCEEDS but every
/// rename of the landing FAILS. The swap moves the original aside before the
/// temp takes its name, so a refused rename moved nothing: the destination
/// keeps the user's old file, and the new data survives where it came from,
/// since the move never completed. Regression guard so a future "clean up on
/// move error" refactor can't lose either side.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_preserves_new_data_on_finalize_failure() {
    let engine = TestOperations::new();
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/f.txt"), b"NEW").await.unwrap();
    let source: Arc<dyn Volume> = source;

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_file(Path::new("/f.txt"), b"OLD").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(MoveRenameFailsDestVolume {
        inner: Arc::clone(&dest_inner),
    });

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-finalize-fail",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/f.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "a finalize-rename failure must surface as an error");

    async fn read(volume: &dyn Volume, path: &str) -> Option<Vec<u8>> {
        let mut stream = volume.open_read_stream(Path::new(path)).await.ok()?;
        let mut buf = Vec::new();
        while let Some(Ok(chunk)) = stream.next_chunk().await {
            buf.extend_from_slice(&chunk);
        }
        Some(buf)
    }
    // Nothing was replaced: the user's old file is whole at its name.
    assert_eq!(read(dest_inner.as_ref(), "/f.txt").await.as_deref(), Some(&b"OLD"[..]));
    // And the new data is whole at the source, since the move did not complete.
    assert_eq!(
        read(source.as_ref(), "/f.txt").await.as_deref(),
        Some(&b"NEW"[..]),
        "source must stay when the move's landing fails"
    );
}

/// A cross-volume move of a FOLDER that fails on a file deep inside it must
/// report THAT file's path, not the top-level folder the user selected.
///
/// Pre-fix, `move_volumes_with_progress` mapped every copy-phase failure with
/// the top-level `source_path`, so a 24 GB folder move that tripped on one
/// unwritable file 3,000 items down told the user only "this folder failed".
/// That is undiagnosable: the folder is fine, one leaf is not, and the name of
/// the leaf is the entire content of the report.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_error_names_the_child_that_failed_not_the_selected_folder() {
    let engine = TestOperations::new();
    let src = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    src.create_directory(Path::new("/tree")).await.unwrap();
    src.create_directory(Path::new("/tree/nested")).await.unwrap();
    src.create_file(Path::new("/tree/fine.txt"), b"fine").await.unwrap();
    src.create_file(Path::new("/tree/nested/doomed.txt"), b"doomed")
        .await
        .unwrap();
    let source: Arc<dyn Volume> = src as Arc<dyn Volume>;

    // Only the deep child fails, and it never recovers.
    let flaky = FlakyDest::new(
        usize::MAX,
        VolumeError::IoError {
            message: "Protocol error: STATUS_OBJECT_NAME_INVALID during Create".into(),
            raw_os_error: None,
        },
    )
    .only_for("doomed.txt");
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-deep-child-error",
        &make_state(&engine),
        Arc::clone(&source),
        &[PathBuf::from("/tree")],
        Arc::clone(&dest),
        Path::new("/"),
        &config_default(),
    )
    .await;

    let failure = result.expect_err("the deep child's write never succeeds, so the move must fail");
    let WriteOperationError::IoError { path, .. } = &failure.error else {
        panic!("expected an IoError, got {:?}", failure.error);
    };
    assert_eq!(
        path, "/tree/nested/doomed.txt",
        "the error must name the file that actually failed, not the selected folder"
    );

    // The move must not have deleted anything: the copy phase never completed.
    assert!(source.exists(Path::new("/tree/fine.txt")).await);
    assert!(source.exists(Path::new("/tree/nested/doomed.txt")).await);
}

/// Same bug, second phase: a cross-volume move whose SOURCE DELETE trips on one
/// file deep inside the folder must report that file, not the folder.
///
/// The delete phase walks the source subtree via `remove_tree`,
/// and the only thing that reaches the caller from a real backend is the parent
/// directory's `ENOTEMPTY` — the symptom of a child that survived, named after
/// the folder the user selected. The child's path is the diagnosis and it exists
/// only inside the walker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_delete_error_names_the_child_that_failed_not_the_selected_folder() {
    let engine = TestOperations::new();
    let src = UndeletableSource::new(
        "doomed.txt",
        VolumeError::IoError {
            message: "Resource busy".into(),
            raw_os_error: None,
        },
    );
    let source: Arc<dyn Volume> = Arc::clone(&src) as Arc<dyn Volume>;
    source.create_directory(Path::new("/tree")).await.unwrap();
    source.create_directory(Path::new("/tree/nested")).await.unwrap();
    source.create_file(Path::new("/tree/fine.txt"), b"fine").await.unwrap();
    source
        .create_file(Path::new("/tree/nested/doomed.txt"), b"doomed")
        .await
        .unwrap();

    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let events = Arc::new(CollectorEventSink::new());
    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-deep-child-delete-error",
        &make_state(&engine),
        Arc::clone(&source),
        &[PathBuf::from("/tree")],
        Arc::clone(&dest),
        Path::new("/"),
        &config_default(),
    )
    .await;

    let failure = result.expect_err("the source delete can't finish, so the move must fail");
    // The copy landed, so it's the "original stayed" outcome, and the leaf that
    // refused is what it names.
    let WriteOperationError::SourceNotRemoved { path, cause, .. } = &failure.error else {
        panic!("expected SourceNotRemoved, got {:?}", failure.error);
    };
    assert!(matches!(**cause, WriteOperationError::IoError { .. }), "{cause:?}");
    assert_eq!(
        path, "/tree/nested/doomed.txt",
        "the error must name the file that wouldn't delete, not the selected folder: {:?}",
        failure.error
    );

    // The copy phase completed, so the data is safe at the destination, and the
    // file that refused to go is still at the source.
    assert!(dest.exists(Path::new("/tree/nested/doomed.txt")).await);
    assert!(source.exists(Path::new("/tree/nested/doomed.txt")).await);
}

/// A cross-volume move of a FOLDER onto the user's FILE, answered Overwrite on
/// the prompt, whose copy phase fails halfway. The folder already holds the
/// name, so the file is kept beside it under a ` (recovered)` name, and the
/// failure says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_type_move_that_fails_halfway_keeps_the_file_it_was_replacing() {
    let engine = TestOperations::new();
    let src = Arc::new(InMemoryVolume::new("source").with_space_info(10_000_000, 10_000_000));
    src.create_directory(Path::new("/clash")).await.unwrap();
    src.create_file(Path::new("/clash/a.txt"), b"incoming").await.unwrap();
    let source = super::super::faulty_volume::FaultyVolume::wrapping(src)
        .failing_call(
            super::super::faulty_volume::FaultyOp::OpenReadStream,
            1,
            VolumeError::IoError {
                message: "simulated source read failure".into(),
                raw_os_error: None,
            },
        )
        .arc();
    let dest_inner = Arc::new(InMemoryVolume::new("dest").with_space_info(10_000_000, 10_000_000));
    dest_inner
        .create_file(Path::new("/clash"), b"the user's only copy")
        .await
        .unwrap();
    let dest: Arc<dyn Volume> = dest_inner.clone();

    let state = make_state(&engine);
    let events = Arc::new(
        super::super::super::conflict_responder_test_support::ConflictResponderSink::new(
            &state,
            ConflictResolution::Overwrite,
            false,
        ),
    );
    let result = move_volumes_with_progress(
        events,
        "op-move-cross-type-fails",
        &state,
        Arc::clone(&source) as Arc<dyn Volume>,
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &VolumeCopyConfig {
            // Only an answered prompt may cross types.
            conflict_resolution: ConflictResolution::Stop,
            ..VolumeCopyConfig::default()
        },
    )
    .await;

    assert!(source.fault_fired(super::super::faulty_volume::FaultyOp::OpenReadStream));
    let failure = result.expect_err("the unreadable child fails the move");
    assert!(
        matches!(failure.error, WriteOperationError::OriginalsKeptAside { ref recovered, .. } if recovered.len() == 1),
        "the failure names where the user's file went: {:?}",
        failure.error
    );
    let mut stream = dest.open_read_stream(Path::new("/clash (recovered)")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"the user's only copy");
    assert!(
        source.inner().exists(Path::new("/clash/a.txt")).await,
        "and the source never left"
    );
}

/// cmdr-reports#17: a cross-volume move whose copy LANDED and whose source delete
/// was refused (`EPERM`, a Finder-locked file) reported a flat "you don't have
/// permission to move files here". The file was complete at the destination, so
/// the user was told nothing happened while it now existed in both places.
///
/// It must say exactly that: the copy landed (and where), the original stayed,
/// and why, with the refusal's own advice kept in `cause`.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_that_cannot_remove_the_original_says_the_copy_landed() {
    let engine = TestOperations::new();
    let src = UndeletableSource::new(
        "locked.jpg",
        VolumeError::from_io_at(&std::io::Error::from_raw_os_error(libc::EPERM), "/locked.jpg"),
    );
    let source: Arc<dyn Volume> = Arc::clone(&src) as Arc<dyn Volume>;
    source.create_file(Path::new("/locked.jpg"), b"photo").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let result = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-move-source-not-removed",
        &make_state(&engine),
        Arc::clone(&source),
        &[PathBuf::from("/locked.jpg")],
        Arc::clone(&dest),
        Path::new("/photos"),
        &config_default(),
    )
    .await;

    let failure = result.expect_err("the original couldn't be removed, so the move didn't finish");
    let WriteOperationError::SourceNotRemoved { path, landed_at, cause } = &failure.error else {
        panic!("expected SourceNotRemoved, got {:?}", failure.error);
    };
    assert_eq!(path, "/locked.jpg");
    assert_eq!(landed_at, "/photos/locked.jpg");
    assert!(
        matches!(
            **cause,
            WriteOperationError::PermissionDenied {
                side: Some(PermissionSide::Source),
                refusal: PermissionRefusal::SystemProtected,
                ..
            }
        ),
        "the cause keeps its own advice: {cause:?}"
    );

    // Both copies are real: nothing was lost, and nothing was rolled back.
    assert!(dest.exists(Path::new("/photos/locked.jpg")).await);
    assert!(source.exists(Path::new("/locked.jpg")).await);
}
