//! Destination-volume behavior: auto-creating a missing nested destination
//! folder, and a destination the copy can't address naming itself rather than
//! the source.

use super::*;

// ========================================================================
// Volume-aware destination auto-create (recursive `create_directory_all`).
//
// A cross-volume copy into a not-yet-existing nested destination folder
// creates the folder (and any missing ancestors) on the dest volume, then
// lands the files. Parity with the local-FS `ensure_destination_dir`.
// ========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_copy_creates_missing_nested_dest() {
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();

    // `/incoming/2026/trip` does not exist on the dest volume yet.
    assert!(!dest.exists(Path::new("/incoming")).await);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-mkdir-copy",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/incoming/2026/trip"),
        &config,
    )
    .await;

    assert!(
        result.is_ok(),
        "copy into a missing nested dest should succeed: {:?}",
        result
    );

    // Every ancestor was created as a directory.
    for dir in ["/incoming", "/incoming/2026", "/incoming/2026/trip"] {
        assert!(
            dest.is_directory(Path::new(dir)).await.expect("ancestor statable"),
            "{dir} should be a directory"
        );
    }

    // Both files landed in the freshly-created dest.
    let mut stream_a = dest
        .open_read_stream(Path::new("/incoming/2026/trip/a.txt"))
        .await
        .unwrap();
    assert_eq!(stream_a.next_chunk().await.unwrap().unwrap(), b"alpha");
    let mut stream_b = dest
        .open_read_stream(Path::new("/incoming/2026/trip/b.txt"))
        .await
        .unwrap();
    assert_eq!(stream_b.next_chunk().await.unwrap().unwrap(), b"bravo");

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_copy_into_existing_dest_is_a_no_op_create() {
    // Re-running into an already-existing dest must not fail the create gate
    // (a merge into an existing folder is a no-op create).
    let (source, dest) = make_volumes();
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    dest.create_directory(Path::new("/existing")).await.unwrap();
    dest.create_file(Path::new("/existing/keep.txt"), b"keep")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-mkdir-copy-existing",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/a.txt")],
        Arc::clone(&dest),
        Path::new("/existing"),
        &config,
    )
    .await;

    assert!(
        result.is_ok(),
        "copy into an existing dest should succeed: {:?}",
        result
    );
    // The pre-existing dest file survived (no wholesale recreate).
    assert!(dest.exists(Path::new("/existing/keep.txt")).await);
    assert!(dest.exists(Path::new("/existing/a.txt")).await);
}

// ========================================================================
// A destination fault names ITSELF, never the source.
// ========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_cannot_be_addressed_is_never_reported_as_a_missing_source() {
    // The destination volume answers `NotFound` for the folder the copy is
    // asked to create — the shape a share produces for a path it can't address.
    // The user's source file is sitting right there, so telling them it "no
    // longer exists" sends them hunting for data loss that never happened,
    // while the real fault (the destination) goes unnamed. This is the report
    // that reached us from a NAS user: a destination problem wearing the
    // source's name.
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)) as Arc<dyn Volume>;
    let dest = Arc::new(
        InMemoryVolume::new("Dest")
            .with_space_info(10_000_000, 10_000_000)
            .with_create_directory_not_found(),
    ) as Arc<dyn Volume>;

    source.create_file(Path::new("/report.pdf"), b"payload").await.unwrap();

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-dest-not-found",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/report.pdf")],
        Arc::clone(&dest),
        Path::new("/photos/2026"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect_err("a destination that can't be created must fail the copy");

    // `/photos` rather than `/photos/2026`: `create_directory_all` walks
    // shallowest-first, so the ancestor it stopped on IS the honest answer to
    // "which folder couldn't be made".
    assert!(
        matches!(&failure.error, WriteOperationError::DestinationNotFound { path } if path == "/photos"),
        "expected a typed DestinationNotFound naming the destination folder, got: {:?}",
        failure.error
    );
}

#[tokio::test]
async fn an_explicit_volume_copy_name_uses_the_requested_leaf_and_conflict_policy() {
    let (source, dest) = make_volumes();
    source
        .create_file(Path::new("/original.txt"), b"new bytes")
        .await
        .unwrap();
    dest.create_file(Path::new("/renamed.txt"), b"old bytes").await.unwrap();
    for policy in [ConflictResolution::Skip, ConflictResolution::Overwrite] {
        let config = VolumeCopyConfig {
            destination_name: Some("renamed.txt".into()),
            conflict_resolution: policy,
            ..Default::default()
        };
        let result = copy_volumes_with_progress(
            Arc::new(CollectorEventSink::new()),
            "named-volume-copy",
            &make_state(),
            Arc::clone(&source),
            &[PathBuf::from("/original.txt")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;
        assert!(result.is_ok(), "{result:?}");
        assert!(!dest.exists(Path::new("/original.txt")).await);
        let mut reader = dest.open_read_stream(Path::new("/renamed.txt")).await.unwrap();
        let expected = if policy == ConflictResolution::Skip {
            b"old bytes"
        } else {
            b"new bytes"
        };
        assert_eq!(reader.next_chunk().await.unwrap().unwrap(), expected);
        assert!(source.exists(Path::new("/original.txt")).await);
    }
}

#[tokio::test]
async fn an_explicit_volume_copy_folder_name_preserves_children_and_empty_folders() {
    let (source, dest) = make_volumes();
    source.create_directory_all(Path::new("/original/empty")).await.unwrap();
    source
        .create_file(Path::new("/original/file.txt"), b"payload")
        .await
        .unwrap();
    let config = VolumeCopyConfig {
        destination_name: Some("renamed".into()),
        ..Default::default()
    };
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "named-volume-folder-copy",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/original")],
        Arc::clone(&dest),
        Path::new("/nested"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert!(dest.exists(Path::new("/nested/renamed/file.txt")).await);
    assert!(dest.is_directory(Path::new("/nested/renamed/empty")).await.unwrap());
    assert!(!dest.exists(Path::new("/nested/original")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_where_the_destination_folder_should_be_is_named_and_nothing_is_written() {
    // `/photos/2026` is a FILE, and the copy is told to land in it and then in
    // a folder below it. Both used to go wrong quietly: the first read the file
    // as "the folder is already there" and failed per item afterwards, the
    // second reported the folder it was asked to CREATE as missing.
    for destination in ["/photos/2026", "/photos/2026/trip"] {
        let (source, dest) = make_volumes();
        source.create_file(Path::new("/report.pdf"), b"payload").await.unwrap();
        dest.create_directory(Path::new("/photos")).await.unwrap();
        dest.create_file(Path::new("/photos/2026"), b"the user's file")
            .await
            .unwrap();

        let failure = copy_volumes_with_progress(
            Arc::new(CollectorEventSink::new()),
            "test-op-dest-file-in-the-way",
            &make_state(),
            Arc::clone(&source),
            &[PathBuf::from("/report.pdf")],
            Arc::clone(&dest),
            Path::new(destination),
            &VolumeCopyConfig::default(),
        )
        .await
        .expect_err("a file where the destination folder should be must fail the copy");

        assert!(
            matches!(&failure.error, WriteOperationError::DestinationNotAFolder { path } if path == "/photos/2026"),
            "copy into {destination}: expected DestinationNotAFolder naming the file, got: {:?}",
            failure.error
        );
        let in_the_way = dest.get_metadata(Path::new("/photos/2026")).await.unwrap();
        assert!(
            !in_the_way.is_directory && in_the_way.size == Some(15),
            "the file in the way must be left exactly as it was"
        );
    }
}
