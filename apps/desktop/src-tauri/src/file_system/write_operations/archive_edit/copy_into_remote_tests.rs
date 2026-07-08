//! Copy/move INTO a zip from a REMOTE source (MTP / SMB, modeled by a non-local
//! `InMemoryVolume`). The source has no local path to walk, so the driver streams
//! the source subtree into a scratch dir first, then runs the ordinary local
//! ingest against the pulled bytes. These pin: the bytes land, nested trees
//! survive, the metadata size is never trusted, a MOVE deletes the REMOTE
//! originals only after a durable commit, a pull fault surfaces typed and leaves
//! the zip intact, and a cancel before the pull leaves the zip untouched.

use super::test_support::*;

#[tokio::test]
async fn copy_into_from_a_remote_source_lands_the_file() {
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep")]);

    let (source_id, _source) = register_remote_source(&[("new.txt", b"fresh")]).await;
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        false,
    )
    .await
    .expect("start remote-source copy-into");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "remote-source copy-into should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );
    assert_eq!(read_entry(&archive, "keep.txt").as_deref(), Some(b"keep".as_slice()));
    assert_eq!(
        read_entry(&archive, "new.txt").as_deref(),
        Some(b"fresh".as_slice()),
        "the file pulled from the remote source must land in the zip"
    );

    get_volume_manager().unregister(&source_id);
}

#[tokio::test]
async fn copy_into_from_a_remote_source_lands_a_nested_tree() {
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    // A remote tree: d/{top.txt, sub/deep.txt}.
    let (source_id, _source) = register_remote_source(&[("d/top.txt", b"top"), ("d/sub/deep.txt", b"deep")]).await;
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("d")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        false,
    )
    .await
    .expect("start remote-source tree copy-into");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "remote-source tree copy-into should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );
    assert_eq!(read_entry(&archive, "d/top.txt").as_deref(), Some(b"top".as_slice()));
    assert_eq!(
        read_entry(&archive, "d/sub/deep.txt").as_deref(),
        Some(b"deep".as_slice()),
        "a nested remote subtree must land with its structure intact"
    );

    get_volume_manager().unregister(&source_id);
}

#[tokio::test]
async fn copy_into_from_a_remote_source_uses_the_real_bytes_not_the_lying_size() {
    // The remote source's listed size disagrees with its real byte count. The
    // pull streams the REAL bytes and the changeset plans against them, so the
    // zip entry carries the true content — the metadata size is never trusted.
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    let real_bytes: &[u8] = b"the real, honest, full-length payload";
    let (source_id, source) = register_remote_source(&[("liar.txt", real_bytes)]).await;
    // Make the directory entry claim a laughably wrong size (5 bytes).
    source.set_reported_size(Path::new("/liar.txt"), 5);
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("liar.txt")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        false,
    )
    .await
    .expect("start lying-size copy-into");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "lying-size copy-into should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );
    assert_eq!(
        read_entry(&archive, "liar.txt").as_deref(),
        Some(real_bytes),
        "the zip entry must carry the REAL bytes, not a truncation to the lying size"
    );

    get_volume_manager().unregister(&source_id);
}

#[tokio::test]
async fn move_into_from_a_remote_source_deletes_the_remote_originals_after_commit() {
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    let (source_id, source) = register_remote_source(&[("d/top.txt", b"top"), ("d/sub/deep.txt", b"deep")]).await;
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("d")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        true, // is_move
    )
    .await
    .expect("start remote-source move-into");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "remote-source move-into should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );
    // The bytes landed in the zip...
    assert_eq!(read_entry(&archive, "d/top.txt").as_deref(), Some(b"top".as_slice()));
    assert_eq!(
        read_entry(&archive, "d/sub/deep.txt").as_deref(),
        Some(b"deep".as_slice())
    );
    // ...and the REMOTE originals were deleted (a clean move), not the scratch copies.
    assert!(
        !source.exists(Path::new("/d")).await,
        "a clean move-into must delete the remote source tree after a durable commit"
    );

    get_volume_manager().unregister(&source_id);
}

#[tokio::test]
async fn move_into_from_a_remote_source_keeps_originals_when_a_collision_is_skipped() {
    // The move invariant carries to remote sources: a Skipped collision must NOT
    // delete the remote source (its bytes didn't fully land).
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/a.txt", b"OLD")]);

    let (source_id, source) = register_remote_source(&[
        ("d/a.txt", b"NEW"), // collides → Skip
        ("d/b.txt", b"bbb"), // lands
    ])
    .await;
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("d")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Skip,
        0,
        true, // is_move
    )
    .await
    .expect("start remote-source move-into with a skip");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(read_entry(&archive, "d/b.txt").as_deref(), Some(b"bbb".as_slice()));
    assert_eq!(read_entry(&archive, "d/a.txt").as_deref(), Some(b"OLD".as_slice()));
    assert!(
        source.exists(Path::new("/d/a.txt")).await && source.exists(Path::new("/d/b.txt")).await,
        "a partial (skipped) remote move must NOT delete the remote source"
    );

    get_volume_manager().unregister(&source_id);
}

#[tokio::test]
async fn copy_into_from_a_remote_source_surfaces_a_pull_failure_and_leaves_the_zip_untouched() {
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep")]);
    let before = std::fs::read(&archive).expect("read archive before");

    // The remote source has NO such file, so the pull's stat/stream faults.
    let (source_id, _source) = register_remote_source(&[("present.txt", b"here")]).await;
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("missing.txt")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        false,
    )
    .await
    .expect("start (the fault surfaces on the terminal event, not the start)");

    assert!(
        wait_until(|| !events.errors.lock_ignore_poison().is_empty()).await,
        "a remote pull fault must surface as a write-error"
    );
    // No entry was added and the archive bytes are unchanged: the pull faulted
    // before the rewrite ever opened the zip.
    assert!(read_entry(&archive, "missing.txt").is_none());
    assert_eq!(
        std::fs::read(&archive).expect("read archive after"),
        before,
        "a pull fault must leave the zip byte-for-byte intact"
    );

    get_volume_manager().unregister(&source_id);
}

#[tokio::test]
async fn copy_into_from_a_remote_source_cancelled_before_the_pull_leaves_the_zip_untouched() {
    // On the current-thread test runtime the spawned op doesn't run until we
    // await, so cancelling right after `route` returns lands the cancel at the
    // pull's first checkpoint — before the zip is ever opened.
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("keep.txt", b"keep")]);
    let before = std::fs::read(&archive).expect("read archive before");

    let (source_id, _source) = register_remote_source(&[("new.txt", b"fresh")]).await;
    let source_volume: Arc<dyn Volume> = get_volume_manager().get(&source_id).expect("source volume");

    let events = Arc::new(CollectorEventSink::new());
    let start = route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        false,
    )
    .await
    .expect("start remote-source copy-into");

    // Cancel before yielding to the spawned op.
    super::super::state::cancel_write_operation(&start.operation_id, false);

    assert!(
        wait_until(|| !events.cancelled.lock_ignore_poison().is_empty()).await,
        "a cancel before the pull must emit write-cancelled, complete: {:?}, errors: {:?}",
        events.complete.lock_ignore_poison(),
        events.errors.lock_ignore_poison()
    );
    assert!(read_entry(&archive, "new.txt").is_none());
    assert_eq!(
        std::fs::read(&archive).expect("read archive after"),
        before,
        "a cancel during the pull must leave the zip byte-for-byte intact"
    );

    get_volume_manager().unregister(&source_id);
}
