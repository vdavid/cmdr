//! What the local copy's progress-reporting reversal removes, leaves, and shows
//! on the bar while it does it.

use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::ledger::WrittenFile;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{CancelRollback, CancelRollbackOutcome};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::SkipReason;
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Drive a reversal over an already-recorded ledger, plus the one directory the
/// copy created. Returns the frames it emitted and what it reported.
fn reverse(created_dir: &Path, recorded: Vec<WrittenFile>) -> (Vec<WriteProgressEvent>, CancelRollback) {
    let _guard = TestOperationGuard::register("reversal");
    let events = Arc::new(CollectorEventSink::new());
    // A zero interval so every item emits a frame; the throttle isn't what these
    // tests are about.
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));

    let files = recorded.len();
    let mut transaction = CopyTransaction::new();
    for file in recorded {
        transaction.record_file(file);
    }
    transaction.record_dir(created_dir.to_path_buf());

    let tally = rollback_with_progress(
        &mut transaction,
        &*events,
        _guard.id(),
        &state,
        WriteOperationType::Copy,
        files,
        1_000,
        files,
        1_000,
    );
    transaction.commit();

    let frames = events.progress.lock_ignore_poison().clone();
    (frames, tally.into_cancel_rollback())
}

/// The bar reaches its end whether an item was removed or left behind. A bar
/// stranded partway reads as a crash, and a user who thinks the app crashed never
/// reads the summary that would have explained what stayed.
#[test]
fn the_bar_reaches_zero_even_when_the_reversal_leaves_files_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("album");
    fs::create_dir(&dir).unwrap();
    let ours = dir.join("ours.txt");
    let theirs = dir.join("theirs.txt");
    fs::write(&ours, b"what the copy wrote").unwrap();
    fs::write(&theirs, b"what the copy wrote").unwrap();
    let recorded = vec![WrittenFile::local(ours.clone()), WrittenFile::local(theirs.clone())];

    // Somebody else edits one of them after the copy wrote it.
    fs::write(&theirs, b"a different length entirely").unwrap();

    let (frames, report) = reverse(&dir, recorded);

    let last = frames.last().expect("a reversal emits at least one frame");
    assert_eq!(last.files_done, 0, "the file axis lands on zero");
    assert_eq!(last.bytes_done, 0, "and so does the byte axis");
    assert_eq!(report.outcome, CancelRollbackOutcome::PartiallyRolledBack);
    assert!(theirs.exists(), "the changed file stays");
    assert!(!ours.exists(), "its unchanged neighbour goes");

    let reasons: Vec<SkipReason> = report.skips.iter().map(|s| s.reason).collect();
    assert!(reasons.contains(&SkipReason::Drift), "the file it left, and why");
    assert!(
        reasons.contains(&SkipReason::DirNotEmpty),
        "and the directory that file kept alive"
    );
    let drift = report.skips.iter().find(|s| s.reason == SkipReason::Drift).unwrap();
    assert_eq!(drift.example_name, "theirs.txt", "a report can name the file");
}

/// Nothing drifted, so everything the copy wrote comes off — including the
/// directory it created, now that it's empty.
#[test]
fn an_untouched_copy_reverses_completely() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("album");
    fs::create_dir(&dir).unwrap();
    let one = dir.join("one.txt");
    let two = dir.join("two.txt");
    fs::write(&one, b"first").unwrap();
    fs::write(&two, b"second").unwrap();
    let recorded = vec![WrittenFile::local(one), WrittenFile::local(two)];

    let (frames, report) = reverse(&dir, recorded);

    assert_eq!(report.outcome, CancelRollbackOutcome::RolledBack);
    assert!(report.skips.is_empty());
    assert_eq!(report.reversed, 3, "two files and the directory that held them");
    assert!(!dir.exists(), "the created directory goes with its contents");
    assert_eq!(frames.last().unwrap().files_done, 0);
}

/// A partial this operation was still writing goes even though nothing about it
/// can be verified — leaving a truncated file behind is the failure cancelling
/// mid-file exists to prevent.
#[test]
fn a_partial_still_goes_while_a_changed_file_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("album");
    fs::create_dir(&dir).unwrap();
    let partial = dir.join("half-written.mov");
    let changed = dir.join("changed.txt");
    fs::write(&partial, b"the first chunk").unwrap();
    fs::write(&changed, b"as recorded").unwrap();
    let recorded = vec![
        WrittenFile::own_partial(partial.clone()),
        WrittenFile::local(changed.clone()),
    ];
    fs::write(&changed, b"somebody else edited this in place").unwrap();

    let (_frames, report) = reverse(&dir, recorded);

    assert!(!partial.exists(), "a partial must never be left behind");
    assert!(changed.exists(), "a file that changed must never be deleted");
    assert_eq!(report.outcome, CancelRollbackOutcome::PartiallyRolledBack);
}
