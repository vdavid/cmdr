//! Destination-volume behavior: auto-creating a missing nested destination
//! folder, and the free-space pre-flight when the destination can't answer.

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

// ========================================
// The free-space pre-flight, when the destination can't answer
// ========================================
//
// ⚠️ Two correct-looking decisions collided here once and made every copy INTO an
// SFTP server fail after ~500 ms with `IoError { message: "Operation not supported
// by this volume type" }`, naming the destination path and nothing else. The
// backend was right to answer `NotSupported` (its protocol really can't ask), and
// the check was right to exist; what was missing is that "can't tell" and "no
// room" are different answers. These cells hold the seam open from both sides.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_cant_report_free_space_is_still_copyable_into() {
    // ❗ The cross-backend contract, not an SFTP quirk: ANY backend may answer
    // `NotSupported` to `get_space_info`, and the trait explicitly allows it. A
    // pre-flight that read the refusal as "no room" would make such a volume a
    // destination nothing can ever be written to, with an error message that
    // names neither the check nor the reason.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    // ❗ No `with_space_info`, so `get_space_info` answers `NotSupported` — the
    // same answer `SftpVolume` gives.
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest"));
    source.create_file(Path::new("/a.txt"), b"alpha").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"bravo").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-space-unknown",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await;

    let errors: Vec<String> = events
        .errors
        .lock()
        .unwrap()
        .iter()
        .map(|e| format!("{:?}", e.error))
        .collect();
    assert!(
        result.is_ok(),
        "a destination that can't report free space must still be copyable into; errors: {errors:?}",
    );

    // The bytes, not just the absence of an error.
    for (name, content) in [("/a.txt", &b"alpha"[..]), ("/b.txt", &b"bravo"[..])] {
        let landed = dest.read_range(Path::new(name), 0, 64).await.expect("the copy landed");
        assert_eq!(landed, content, "{name} must arrive byte for byte");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_does_report_free_space_still_refuses_what_it_cant_hold() {
    // ❗ The other half, and the one an over-eager fix breaks: tolerating "can't
    // tell" must not turn into ignoring a real "no room". A volume that ANSWERS
    // keeps the check exactly as it was.
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(1_000, 4));
    source
        .create_file(Path::new("/big.bin"), b"more than four bytes")
        .await
        .unwrap();

    let failure = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-space-too-small",
        &make_state(),
        Arc::clone(&source),
        &[PathBuf::from("/big.bin")],
        Arc::clone(&dest),
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect_err("a destination that says it has 4 bytes free must refuse 20 bytes");

    assert!(
        matches!(&failure.error, WriteOperationError::InsufficientSpace { .. }),
        "the refusal must stay the typed InsufficientSpace the dialog renders, got {:?}",
        failure.error,
    );
}
