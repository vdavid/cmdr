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
use crate::file_system::volume::{InMemoryVolume, VolumeError};
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution};

/// Cross-volume MOVE, file→file Overwrite, streaming write SUCCEEDS but the
/// finalize rename FAILS. The move path has no dest partial-cleanup, so the
/// temp (holding the only complete copy of the new data after finalize deleted
/// the original) must survive. The source must also stay (the move never
/// completed). Regression guard so a future "clean up the temp on move error"
/// refactor can't reintroduce the data-loss hole.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_preserves_new_data_on_finalize_failure() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/f.txt"), b"NEW").await.unwrap();
    let source: Arc<dyn Volume> = source;

    let dest_inner = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest_inner.create_file(Path::new("/f.txt"), b"OLD").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(MoveRenameFailsDestVolume {
        inner: Arc::clone(&dest_inner),
    });

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
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

    // The NEW data must survive somewhere on dest (orig slot or a temp sibling).
    let entries = dest_inner.list_directory(Path::new("/"), None).await.unwrap();
    let mut found_new = false;
    for e in &entries {
        if let Ok(mut s) = dest_inner.open_read_stream(&PathBuf::from(&e.path)).await {
            let mut buf = Vec::new();
            while let Some(Ok(chunk)) = s.next_chunk().await {
                buf.extend_from_slice(&chunk);
            }
            if buf == b"NEW" {
                found_new = true;
                break;
            }
        }
    }
    assert!(
        found_new,
        "new data must survive on dest after a move finalize failure; entries: {:?}",
        entries.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    // The source must remain, since the move did not complete.
    assert!(
        source.exists(Path::new("/f.txt")).await,
        "source must stay when the move's finalize fails"
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
            message: "Protocol error: STATUS_OBJECT_NAME_INVALID during Create".to_string(),
            raw_os_error: None,
        },
    )
    .only_for("doomed.txt");
    let dest: Arc<dyn Volume> = Arc::clone(&flaky) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-deep-child-error",
        &make_state(),
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
    let src = UndeletableSource::new(
        "doomed.txt",
        VolumeError::IoError {
            message: "Resource busy".to_string(),
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
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/tree")],
        Arc::clone(&dest),
        Path::new("/"),
        &config_default(),
    )
    .await;

    let failure = result.expect_err("the source delete can't finish, so the move must fail");
    let WriteOperationError::IoError { path, .. } = &failure.error else {
        panic!("expected an IoError, got {:?}", failure.error);
    };
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
