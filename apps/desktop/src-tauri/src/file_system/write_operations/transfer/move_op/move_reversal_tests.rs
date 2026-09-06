//! What a cancelled local move's reversal refuses to touch.
//!
//! A `#[path]` child of `move_op`, like the other suites here, so it can reach
//! `MoveTransaction` directly.

use super::test_support::make_state;
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::CancelRollbackOutcome;

/// The item at the landed path isn't the one this move put there any more, so
/// renaming it back would take somebody else's file away. Its unchanged
/// neighbour still comes home.
#[test]
fn an_item_replaced_at_the_destination_is_not_renamed_back() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("from");
    let into = tmp.path().join("into");
    fs::create_dir_all(&from).unwrap();
    fs::create_dir_all(&into).unwrap();

    let mut move_tx = MoveTransaction::new();
    for name in ["ours.txt", "theirs.txt"] {
        let source = from.join(name);
        let landed = into.join(name);
        fs::write(&source, b"moved").unwrap();
        let stat = fs::symlink_metadata(&source).ok();
        fs::rename(&source, &landed).unwrap();
        move_tx.record(source, WrittenFile::local_stat(landed, stat.as_ref()));
    }

    // Somebody replaces the landed file with one of their own, same length.
    let theirs = into.join("theirs.txt");
    let incoming = into.join("theirs.incoming");
    fs::write(&incoming, b"mine!").unwrap();
    fs::rename(&incoming, &theirs).unwrap();

    let _ = move_tx.rollback();

    assert!(
        theirs.exists(),
        "a replaced destination must stay where it is, not be renamed away"
    );
    assert!(
        !from.join("theirs.txt").exists(),
        "and nothing must land back at its original source"
    );
    assert!(
        from.join("ours.txt").exists(),
        "the unchanged neighbour still comes home"
    );
}

/// Something new sits at the original source now. A rename back would destroy it
/// silently, so the reversal leaves the moved item where it landed.
#[test]
fn a_move_back_never_overwrites_what_now_sits_at_the_original_source() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("from");
    let into = tmp.path().join("into");
    fs::create_dir_all(&from).unwrap();
    fs::create_dir_all(&into).unwrap();

    let source = from.join("notes.txt");
    let landed = into.join("notes.txt");
    fs::write(&source, b"the moved file").unwrap();
    let stat = fs::symlink_metadata(&source).ok();
    fs::rename(&source, &landed).unwrap();

    let mut move_tx = MoveTransaction::new();
    move_tx.record(source.clone(), WrittenFile::local_stat(landed.clone(), stat.as_ref()));

    // The user makes a new file with the same name where the old one used to be.
    fs::write(&source, b"a new file the user just made").unwrap();

    let _ = move_tx.rollback();

    assert_eq!(
        fs::read(&source).unwrap(),
        b"a new file the user just made",
        "the reversal must never rename back over what someone put at the source"
    );
    assert!(
        landed.exists(),
        "and the moved item stays where it landed rather than vanishing"
    );
}

/// A move that only changed a name's case finds its own destination sitting at
/// the original source, because a case-insensitive filesystem folds the two names
/// onto one entry. That's the item itself, not a collision, so the reversal still
/// restores the original spelling.
#[test]
fn a_case_only_rename_back_is_not_a_collision() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("dog.jpg");
    let landed = tmp.path().join("DOG.jpg");
    fs::write(&source, b"a good dog").unwrap();
    let stat = fs::symlink_metadata(&source).ok();
    fs::rename(&source, &landed).unwrap();

    let mut move_tx = MoveTransaction::new();
    move_tx.record(source, WrittenFile::local_stat(landed, stat.as_ref()));
    let report = move_tx.rollback().into_cancel_rollback();

    assert_eq!(
        report.outcome,
        CancelRollbackOutcome::RolledBack,
        "the item at the target IS the item being restored"
    );
    let names: Vec<String> = fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["dog.jpg"], "the original spelling comes back");
}

/// Nothing drifted and nothing took the sources' places, so every item comes home
/// and the reversal says it undid the lot.
#[test]
fn an_untouched_move_restores_every_item() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("from");
    let into = tmp.path().join("into");
    fs::create_dir_all(&from).unwrap();
    fs::create_dir_all(&into).unwrap();

    let mut move_tx = MoveTransaction::new();
    for name in ["one.txt", "two.txt"] {
        let source = from.join(name);
        let landed = into.join(name);
        fs::write(&source, b"moved").unwrap();
        let stat = fs::symlink_metadata(&source).ok();
        fs::rename(&source, &landed).unwrap();
        move_tx.record(source, WrittenFile::local_stat(landed, stat.as_ref()));
    }

    let report = move_tx.rollback().into_cancel_rollback();

    assert_eq!(report.outcome, CancelRollbackOutcome::RolledBack);
    assert_eq!(report.reversed, 2);
    assert!(report.skips.is_empty());
    assert!(from.join("one.txt").exists() && from.join("two.txt").exists());
}

/// A folder-into-folder merge empties each source directory and removes it. The
/// reversal has to put those directories back BEFORE it puts their children
/// back, or every child's rename fails `ENOENT` against a parent that no longer
/// exists: the move stays merged into the destination while the person who
/// clicked "put it back" is told it was undone.
#[test]
fn a_rollback_after_a_merge_recreates_the_source_folders_and_brings_the_children_home() {
    let tmp = tempfile::tempdir().unwrap();
    let src_dir = tmp.path().join("src").join("d");
    let dst_dir = tmp.path().join("dst").join("d");
    fs::create_dir_all(src_dir.join("sub")).unwrap();
    fs::write(src_dir.join("a.txt"), b"A").unwrap();
    fs::write(src_dir.join("sub").join("b.txt"), b"B").unwrap();
    // The destination folder already exists, so the move merges into it.
    fs::create_dir_all(&dst_dir).unwrap();

    let events = CollectorEventSink::new();
    let state = make_state(200);
    let mut move_tx = MoveTransaction::new();
    let mut apply_to_all = ApplyToAll::default();
    let mut files_skipped = 0usize;
    merge_move_directory(
        &src_dir,
        &dst_dir,
        &WriteOperationConfig::default(),
        &events,
        "op-merge-then-rollback",
        &state,
        &mut apply_to_all,
        &mut move_tx,
        &mut files_skipped,
        &mut None,
    )
    .expect("the merge itself succeeds");

    // Precondition: everything crossed, and the emptied source spine is gone.
    assert!(dst_dir.join("a.txt").exists() && dst_dir.join("sub").join("b.txt").exists());
    assert!(!src_dir.exists(), "the merge removes the source folder it emptied");

    let report = move_tx.rollback().into_cancel_rollback();

    assert_eq!(
        report.outcome,
        CancelRollbackOutcome::RolledBack,
        "a merge the reversal fully undid reports itself undone, skips: {:?}",
        report.skips
    );
    assert_eq!(
        fs::read(src_dir.join("a.txt")).unwrap(),
        b"A",
        "the child comes home under a recreated parent"
    );
    assert_eq!(
        fs::read(src_dir.join("sub").join("b.txt")).unwrap(),
        b"B",
        "and so does one nested a level deeper"
    );
    assert!(
        !dst_dir.join("a.txt").exists() && !dst_dir.join("sub").join("b.txt").exists(),
        "nothing this move merged in is left at the destination"
    );
}

/// The item the move landed is already gone. The end state a restore wanted holds
/// as far as the destination is concerned, so it counts as undone rather than as
/// something to warn about.
#[test]
fn a_landed_item_someone_removed_counts_as_done() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("notes.txt");
    let landed = tmp.path().join("moved-notes.txt");
    fs::write(&source, b"content").unwrap();
    let stat = fs::symlink_metadata(&source).ok();
    fs::rename(&source, &landed).unwrap();
    let mut move_tx = MoveTransaction::new();
    move_tx.record(source, WrittenFile::local_stat(landed.clone(), stat.as_ref()));

    fs::remove_file(&landed).unwrap();
    let report = move_tx.rollback().into_cancel_rollback();

    assert_eq!(report.reversed, 1);
    assert!(report.skips.is_empty());
}

/// The restore's own rename refuses to replace, so an entry that appears at the
/// original source AFTER the pre-check stat'd it and found nothing is still
/// safe. Driving `restore_rename` directly is what makes that window
/// deterministic: it is exactly the state the reversal is in once the pre-check
/// has passed.
#[test]
fn a_restore_rename_refuses_to_replace_an_entry_that_appeared_after_the_check() {
    let tmp = tempfile::tempdir().unwrap();
    let landed = tmp.path().join("landed.txt");
    let original_source = tmp.path().join("notes.txt");
    fs::write(&landed, b"the moved file").unwrap();
    // Stands in for the file created in the window between the pre-check and
    // the rename, which no stat of ours can see.
    fs::write(&original_source, b"a new file the user just made").unwrap();

    let result = restore_rename(&landed, &original_source, false);

    assert!(
        matches!(result, ItemResult::Skipped(SkipReason::RestoreTargetOccupied)),
        "an occupied restore target reports itself as occupied, whenever it got occupied"
    );
    assert_eq!(
        fs::read(&original_source).unwrap(),
        b"a new file the user just made",
        "and the entry in the way keeps its bytes"
    );
    assert!(landed.exists(), "the moved item stays where it landed");
}

/// Nothing is in the way, so the same non-replacing rename lands the item back.
#[test]
fn a_restore_rename_lands_when_the_original_source_is_clear() {
    let tmp = tempfile::tempdir().unwrap();
    let landed = tmp.path().join("landed.txt");
    let original_source = tmp.path().join("notes.txt");
    fs::write(&landed, b"the moved file").unwrap();

    assert!(matches!(
        restore_rename(&landed, &original_source, false),
        ItemResult::Reversed
    ));
    assert_eq!(fs::read(&original_source).unwrap(), b"the moved file");
}

/// `force` is the case-only self-collision: the entry the target reports is the
/// item itself, so the rename has to be allowed to land on it.
#[test]
fn a_forced_restore_rename_lands_on_the_item_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let landed = tmp.path().join("DOG.jpg");
    let original_source = tmp.path().join("dog.jpg");
    fs::write(&landed, b"a good dog").unwrap();

    assert!(matches!(
        restore_rename(&landed, &original_source, true),
        ItemResult::Reversed
    ));
    assert_eq!(fs::read(&original_source).unwrap(), b"a good dog");
}
