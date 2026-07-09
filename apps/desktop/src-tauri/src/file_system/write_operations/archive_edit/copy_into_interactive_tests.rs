//! Interactive in-archive conflict prompts (the Stop policy): a file collision
//! emits a `write-conflict` and blocks on the user's answer, the `ApplyToAll`
//! latch suppresses repeats, a cancel mid-prompt leaves the archive intact, and
//! dir-vs-dir merges without prompting.

use super::test_support::*;

/// Starts an interactive (Stop-policy) copy INTO `archive` of local dir `src_rel`
/// (relative to `src_root`), landing at the archive root. Returns the collector +
/// the operation id (for `resolve_write_conflict`).
async fn start_interactive_copy_into(
    archive: &Path,
    src_root: &Path,
    src_rel: &str,
) -> (Arc<CollectorEventSink>, String) {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.to_path_buf()));
    let events = Arc::new(CollectorEventSink::new());
    let start = route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from(src_rel)],
        archive.to_path_buf(),
        unique_lane_id(),
        ConflictResolution::Stop,
        0,
        false,
        None,
    )
    .await
    .expect("start interactive copy-into");
    (events, start.operation_id)
}

#[tokio::test]
async fn interactive_copy_into_prompts_on_a_file_collision_and_overwrite_replaces() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLD")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");
    std::fs::write(src_root.join("d/fresh.txt"), b"fresh").expect("w2");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    // The collision fires a prompt; answer Overwrite.
    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "a file collision must emit a write-conflict prompt"
    );
    resolve_write_conflict(&op_id, ConflictResolution::Overwrite, false);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the edit should complete after the prompt is answered"
    );
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"NEW".as_slice()),
        "Overwrite must replace the colliding entry"
    );
    assert_eq!(
        read_entry(&archive, "d/fresh.txt").as_deref(),
        Some(b"fresh".as_slice()),
        "the non-colliding file is added"
    );
    // A NON-root parent carries its volume id in the settle event (so the FE can
    // clear that drive's eject guard); a `root` local disk settles with `None`,
    // pinned by `driver_tests` and `move_out_tests`.
    assert!(wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await);
    assert!(
        events.settled.lock_ignore_poison()[0].volume_id.is_some(),
        "a non-root parent archive edit carries its volume id in the settle event"
    );
}

#[tokio::test]
async fn interactive_copy_into_skip_keeps_the_existing_entry() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLD")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "a file collision must prompt"
    );
    resolve_write_conflict(&op_id, ConflictResolution::Skip, false);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the edit should complete"
    );
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"OLD".as_slice()),
        "Skip must keep the existing entry untouched"
    );
}

#[tokio::test]
async fn interactive_apply_to_all_latches_and_stops_prompting() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/one.txt", b"OLD1"), ("d/two.txt", b"OLD2")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/one.txt"), b"NEW1").expect("w1");
    std::fs::write(src_root.join("d/two.txt"), b"NEW2").expect("w2");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    // Answer the FIRST prompt with Skip + apply-to-all; the second collision must
    // be resolved from the latch WITHOUT a second prompt.
    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "the first collision must prompt"
    );
    resolve_write_conflict(&op_id, ConflictResolution::Skip, true);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the edit should complete"
    );
    assert_eq!(
        events.conflicts.lock_ignore_poison().len(),
        1,
        "apply-to-all must suppress the second prompt"
    );
    // Both colliding entries kept their OLD bytes (Skip-all).
    assert_eq!(read_entry(&archive, "d/one.txt").as_deref(), Some(b"OLD1".as_slice()));
    assert_eq!(read_entry(&archive, "d/two.txt").as_deref(), Some(b"OLD2".as_slice()));
}

#[tokio::test]
async fn interactive_cancel_during_a_prompt_leaves_the_archive_intact() {
    use crate::file_system::write_operations::cancel_write_operation;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLD")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");
    std::fs::write(src_root.join("d/fresh.txt"), b"fresh").expect("w2");

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "the collision must prompt"
    );
    // Cancel while the prompt is pending: the planner's recv unblocks with an
    // error, the mutator never runs, and the archive is untouched.
    cancel_write_operation(&op_id, false);

    assert!(
        wait_until(|| !events.cancelled.lock_ignore_poison().is_empty()).await,
        "cancel during a prompt should reach write-cancelled"
    );
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"OLD".as_slice()),
        "cancel must leave the existing entry untouched"
    );
    assert!(
        read_entry(&archive, "d/fresh.txt").is_none(),
        "cancel before commit must add nothing"
    );
    assert!(
        events.complete.lock_ignore_poison().is_empty(),
        "no write-complete on cancel"
    );
}

#[tokio::test]
async fn interactive_conflict_event_carries_both_sides_metadata() {
    use crate::file_system::write_operations::resolve_write_conflict;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/existing.txt", b"OLDDD")]); // 5 bytes

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NN").expect("w1"); // 2 bytes

    let (events, op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "a file collision must prompt"
    );
    {
        let conflicts = events.conflicts.lock_ignore_poison();
        let ev = &conflicts[0];
        assert_eq!(ev.source_size, Some(2), "source size = incoming file length");
        assert_eq!(ev.destination_size, Some(5), "destination size = archive entry length");
        assert_eq!(ev.size_difference, Some(3), "size_difference = dest - source (5 - 2)");
        assert!(!ev.source_is_directory, "the incoming side is a file");
        assert!(
            !ev.destination_is_directory,
            "the colliding archive entry is a file, not a folder"
        );
    }
    resolve_write_conflict(&op_id, ConflictResolution::Skip, false);
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
}

#[tokio::test]
async fn interactive_move_into_with_a_skipped_collision_keeps_the_source() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::{resolve_write_conflict, route_archive_copy_into};

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/a.txt", b"OLD")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/a.txt"), b"NEW").expect("w1"); // collides
    std::fs::write(src_root.join("d/b.txt"), b"bbb").expect("w2"); // lands

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.to_path_buf()));
    let events = Arc::new(CollectorEventSink::new());
    let start = route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("d")],
        archive.clone(),
        unique_lane_id(),
        ConflictResolution::Stop,
        0,
        true, // is_move
        None,
    )
    .await
    .expect("start interactive move-into");

    assert!(
        wait_until(|| !events.conflicts.lock_ignore_poison().is_empty()).await,
        "the collision must prompt"
    );
    resolve_write_conflict(&start.operation_id, ConflictResolution::Skip, false);

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // Something was skipped → the move invariant keeps the source intact.
    assert!(
        src_root.join("d/a.txt").exists() && src_root.join("d/b.txt").exists(),
        "an interactive move with a skipped collision must NOT delete the source"
    );
}

#[tokio::test]
async fn interactive_dir_vs_dir_merges_without_prompting() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    // The archive already holds directory `d` (implied by `d/keep.txt`).
    write_multi_zip(&archive, &[("d/keep.txt", b"keep")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/new.txt"), b"new").expect("w1");

    let (events, _op_id) = start_interactive_copy_into(&archive, &src_root, "d").await;

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the merge should complete with no prompt"
    );
    // The directory collision merged silently — no prompt fired.
    assert!(
        events.conflicts.lock_ignore_poison().is_empty(),
        "dir-vs-dir must merge WITHOUT a conflict prompt"
    );
    assert_eq!(read_entry(&archive, "d/new.txt").as_deref(), Some(b"new".as_slice()));
    assert_eq!(read_entry(&archive, "d/keep.txt").as_deref(), Some(b"keep".as_slice()));
    // Merging into a dir that ALREADY exists in the archive must NOT synthesize a
    // redundant explicit `d/` directory entry (the mkdir guard skips existing dirs).
    let file = std::fs::File::open(&archive).expect("open");
    let mut zip = ZipArchive::new(file).expect("zip");
    assert!(
        zip.by_name("d/").is_err(),
        "a merge into a pre-existing archive dir must not add a redundant explicit dir entry"
    );
}
