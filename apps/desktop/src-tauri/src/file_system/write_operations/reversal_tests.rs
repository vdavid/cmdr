//! What a reversal acts on, what it refuses to touch, and how it reports the
//! difference.

use super::*;
use std::fs;
use std::path::PathBuf;

/// The everyday case: nothing touched the file since the copy wrote it, so the
/// reversal removes it.
#[test]
fn a_file_still_as_the_copy_wrote_it_is_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("photo.raw");
    fs::write(&file, b"0123456789").unwrap();
    let recorded = WrittenFile::local(file.clone());

    let result = remove_local_file(&recorded, ReversalGuard::SkipDrifted);

    assert!(matches!(result, ItemResult::Reversed));
    assert!(!file.exists());
}

/// The trap size alone can't see: an editor saving via write-temp-then-rename
/// leaves a DIFFERENT file of exactly the same length at the same path. The node
/// id catches it, and the reversal leaves the user's file alone.
#[test]
fn a_same_size_replacement_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("notes.txt");
    fs::write(&file, b"before").unwrap();
    let recorded = WrittenFile::local(file.clone());

    let incoming = tmp.path().join("notes.incoming");
    fs::write(&incoming, b"after!").unwrap();
    fs::rename(&incoming, &file).unwrap();

    let result = remove_local_file(&recorded, ReversalGuard::SkipDrifted);

    assert!(matches!(result, ItemResult::Skipped(SkipReason::Drift)));
    assert!(file.exists(), "somebody else's file must survive");
}

/// A file appended to in place keeps its node id, so the size half of the check
/// is what catches it.
#[test]
fn a_file_edited_in_place_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("log.txt");
    fs::write(&file, b"one line\n").unwrap();
    let recorded = WrittenFile::local(file.clone());
    fs::write(&file, b"one line\nand another\n").unwrap();

    assert!(matches!(
        remove_local_file(&recorded, ReversalGuard::SkipDrifted),
        ItemResult::Skipped(SkipReason::Drift)
    ));
    assert!(file.exists());
}

/// Somebody already removed it. The end state the reversal wanted holds, so this
/// counts as undone rather than as something to tell the user about.
#[test]
fn a_file_already_gone_counts_as_done_rather_than_a_skip() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("gone.txt");
    fs::write(&file, b"here for now").unwrap();
    let recorded = WrittenFile::local(file.clone());
    fs::remove_file(&file).unwrap();

    let mut tally = ReversalTally::default();
    tally.record(remove_local_file(&recorded, ReversalGuard::SkipDrifted), &file);
    let report = tally.into_cancel_rollback();

    assert_eq!(report.reversed, 1, "the desired end state already held");
    assert!(report.skips.is_empty(), "and there's nothing to report about it");
    assert_eq!(report.outcome, CancelRollbackOutcome::RolledBack);
}

/// A write this operation was still making has no complete file to recognize and,
/// by construction, no size. It goes anyway — leaving a truncated file at the
/// destination is exactly what cancelling mid-file exists to prevent.
#[test]
fn a_partial_this_operation_was_writing_goes_without_a_recheck() {
    let tmp = tempfile::tempdir().unwrap();
    let partial = tmp.path().join("half-written.mov");
    fs::write(&partial, b"the first megabyte").unwrap();

    let result = remove_local_file(&WrittenFile::own_partial(partial.clone()), ReversalGuard::SkipDrifted);

    assert!(matches!(result, ItemResult::Reversed));
    assert!(!partial.exists(), "a partial must never be left behind");
}

/// An entry whose stat failed when the ledger recorded it is unprovable, and an
/// unprovable file is one a reversal fails safe on. ❌ Not to be confused with a
/// partial, which has no identity for a very different reason.
#[test]
fn an_entry_with_no_identity_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("mystery.bin");
    fs::write(&file, b"whose is this?").unwrap();
    let recorded = WrittenFile::local_stat(file.clone(), None);

    let result = remove_local_file(&recorded, ReversalGuard::SkipDrifted);

    assert!(matches!(
        result,
        ItemResult::Skipped(SkipReason::UnverifiablePrecondition)
    ));
    assert!(file.exists());
}

/// The panic net removes everything the ledger claims, drift and all: it runs
/// because a thread died mid-copy, where a destination is as likely half-written
/// as complete.
#[test]
fn the_panic_net_removes_even_a_file_that_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("mid-copy.bin");
    fs::write(&file, b"as recorded").unwrap();
    let recorded = WrittenFile::local(file.clone());
    fs::write(&file, b"a very different length now").unwrap();

    assert!(matches!(
        remove_local_file(&recorded, ReversalGuard::Unconditional),
        ItemResult::Reversed
    ));
    assert!(!file.exists());
}

/// The recheck stats the way the ledger recorded: a copied symlink whose target
/// doesn't exist has to be FOUND and removed, not read as already gone and left
/// dangling at the destination.
#[cfg(unix)]
#[test]
fn a_copied_symlink_that_dangles_is_still_recognized() {
    let tmp = tempfile::tempdir().unwrap();
    let link = tmp.path().join("shortcut");
    std::os::unix::fs::symlink(tmp.path().join("nothing-here"), &link).unwrap();
    let recorded = WrittenFile::local(link.clone());

    assert_eq!(recheck_local(&recorded, ReversalGuard::SkipDrifted), Recheck::Act);
    assert!(matches!(
        remove_local_file(&recorded, ReversalGuard::SkipDrifted),
        ItemResult::Reversed
    ));
}

/// A directory this copy created that somebody has since put a file into stays,
/// and says why.
#[test]
fn a_created_directory_someone_put_a_file_into_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("album");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("theirs.txt"), b"mine now").unwrap();

    assert!(matches!(
        remove_local_dir_if_empty(&dir),
        ItemResult::Skipped(SkipReason::DirNotEmpty)
    ));
    assert!(dir.exists());

    fs::remove_file(dir.join("theirs.txt")).unwrap();
    assert!(matches!(remove_local_dir_if_empty(&dir), ItemResult::Reversed));
    assert!(!dir.exists());
}

/// Volumes carry no node id, so size is the whole identity — the same exposure
/// the operation log's own reversal has always carried.
#[test]
fn a_volume_file_the_backend_now_reports_at_another_size_is_left_alone() {
    let recorded = WrittenFile::volume(PathBuf::from("/share/report.pdf"), 4096);

    assert_eq!(recheck_volume(&recorded, Some(4096)), Recheck::Act);
    assert_eq!(
        recheck_volume(&recorded, Some(9000)),
        Recheck::Skip(SkipReason::Drift),
        "a different size is a different file"
    );
    assert_eq!(
        recheck_volume(&recorded, None),
        Recheck::Skip(SkipReason::UnverifiablePrecondition),
        "a backend that won't say fails safe"
    );
}

/// A partial on a volume goes on sight too, without a round trip to size it.
#[test]
fn a_volume_partial_goes_without_a_size() {
    let partial = WrittenFile::own_partial(PathBuf::from("/share/half.mov"));
    assert_eq!(recheck_volume(&partial, None), Recheck::Act);
}

/// The three outcomes a cancel can report, and the boundary between them.
#[test]
fn the_reported_outcome_follows_what_the_reversal_managed() {
    let path = PathBuf::from("/a/one.txt");

    let empty = ReversalTally::default().into_cancel_rollback();
    assert_eq!(
        empty.outcome,
        CancelRollbackOutcome::RolledBack,
        "an empty ledger is vacuously fully reversed"
    );

    let mut full = ReversalTally::default();
    full.record(ItemResult::Reversed, &path);
    assert_eq!(full.into_cancel_rollback().outcome, CancelRollbackOutcome::RolledBack);

    let mut with_a_skip = ReversalTally::default();
    with_a_skip.record(ItemResult::Reversed, &path);
    with_a_skip.record(ItemResult::Skipped(SkipReason::Drift), &path);
    let report = with_a_skip.into_cancel_rollback();
    assert_eq!(report.outcome, CancelRollbackOutcome::PartiallyRolledBack);
    assert_eq!(report.reversed, 1);
    assert_eq!(report.skips.len(), 1);
    assert_eq!(report.skips[0].count, 1);
    assert_eq!(report.skips[0].example_name, "one.txt");

    let mut stopped_at_once = ReversalTally::default();
    stopped_at_once.mark_canceled();
    assert_eq!(
        stopped_at_once.into_cancel_rollback().outcome,
        CancelRollbackOutcome::NotRolledBack,
        "a reversal stopped before it reached an item undid nothing"
    );

    let mut stopped_partway = ReversalTally::default();
    stopped_partway.record(ItemResult::Reversed, &path);
    stopped_partway.mark_canceled();
    assert_eq!(
        stopped_partway.into_cancel_rollback().outcome,
        CancelRollbackOutcome::PartiallyRolledBack
    );
}
