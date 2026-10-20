//! What the trash does with one item and with a batch: where an item lands, the
//! refusal a batch reports, and the terminal event each ending emits.
//!
//! A `#[path]` child of `trash.rs`, so `super::` here is `trash`.

use super::*;
use crate::testing::TestOperations;
use crate::types::CancelRollbackOutcome;
#[cfg(target_os = "macos")]
use crate::types::TrashRefusedItems;
// Only the macOS-gated cases below build a real directory.
#[cfg(target_os = "macos")]
use cmdr_fs::testing::TestDir;
use std::sync::Arc;
use std::time::Duration;

#[cfg(target_os = "macos")]
fn create_test_dir(name: &str) -> TestDir {
    TestDir::new(&format!("trash_test_{}", name))
}

// ========================================================================
// move_to_trash_sync tests
// ========================================================================

#[cfg(target_os = "macos")]
#[test]
fn test_move_to_trash_sync_file() {
    let tmp = create_test_dir("trash_sync_file");
    let file = tmp.join("test.txt");
    fs::write(&file, "content").unwrap();
    assert!(fs::symlink_metadata(&file).is_ok());

    let result = move_to_trash_sync(&file);
    assert!(result.is_ok());
    assert!(fs::symlink_metadata(&file).is_err());

    // The in-trash location (resultingItemURL) is captured and points into
    // the user's Trash — the rollback restore depends on this dest.
    let in_trash = result.unwrap().expect("macOS reports an in-trash location");
    assert!(
        in_trash.components().any(|c| c.as_os_str() == ".Trash"),
        "expected a ~/.Trash path, got {}",
        in_trash.display()
    );
    assert!(fs::symlink_metadata(&in_trash).is_ok(), "the item exists in Trash");
    let _ = fs::remove_file(&in_trash);
}

// ========================================================================
// trash_dir_for_path tests
// ========================================================================

/// The resolver has to agree with where the trash move actually puts things,
/// or "Go to trash" navigates somewhere the item isn't. Trashing a real file
/// and comparing its recorded location against the resolver is the only check
/// that pins the two together.
#[cfg(target_os = "macos")]
#[test]
fn trash_dir_for_path_matches_where_the_item_actually_landed() {
    let tmp = create_test_dir("trash_dir_resolve");
    let file = tmp.join("test.txt");
    fs::write(&file, "content").unwrap();

    let resolved = trash_dir_for_path(&file).expect("the boot volume has a trash");
    let in_trash = move_to_trash_sync(&file)
        .expect("trash succeeds")
        .expect("macOS reports an in-trash location");

    assert_eq!(
        in_trash.parent(),
        Some(resolved.as_path()),
        "resolver said {}, the item landed in {}",
        resolved.display(),
        in_trash.display()
    );
    let _ = fs::remove_file(&in_trash);
}

/// The case the feature exists for: by the time anyone asks where a trashed item
/// went, its original path is gone. Cocoa refuses to resolve a volume for a path
/// that doesn't exist, so the ancestor walk is what keeps the answer coming.
#[cfg(target_os = "macos")]
#[test]
fn trash_dir_for_path_answers_for_a_path_that_is_already_gone() {
    let tmp = create_test_dir("trash_dir_missing");
    let never_existed = tmp.join("no-such-file.txt");

    let resolved = trash_dir_for_path(&never_existed).expect("the volume still has a trash");
    assert!(
        resolved.components().any(|c| c.as_os_str() == ".Trash"),
        "expected a ~/.Trash path, got {}",
        resolved.display()
    );
}

/// A whole subtree can be gone, not only the leaf (trashing a folder takes its
/// children with it), so the walk must climb as far as it needs to.
#[cfg(target_os = "macos")]
#[test]
fn trash_dir_for_path_climbs_past_several_missing_levels() {
    let tmp = create_test_dir("trash_dir_deep_missing");
    let deep = tmp.join("gone").join("also-gone").join("file.txt");

    assert!(
        trash_dir_for_path(&deep).is_some(),
        "a path several levels below a live ancestor still resolves"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_move_to_trash_sync_directory() {
    let tmp = create_test_dir("trash_sync_dir");
    let dir = tmp.join("subdir");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("inner.txt"), "data").unwrap();

    let result = move_to_trash_sync(&dir);
    assert!(result.is_ok());
    assert!(fs::symlink_metadata(&dir).is_err());
}

#[test]
fn test_move_to_trash_sync_nonexistent() {
    let result = move_to_trash_sync(Path::new("/nonexistent_12345/file.txt"));
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MutationError::NotFound { .. }));
}

#[cfg(target_os = "macos")]
#[test]
fn test_move_to_trash_sync_dangling_symlink() {
    let tmp = create_test_dir("trash_sync_dangling");
    let target = tmp.join("target.txt");
    let link = tmp.join("link.txt");
    fs::write(&target, "data").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    // Remove the target, leaving a dangling symlink
    fs::remove_file(&target).unwrap();

    // The link itself still exists (symlink_metadata succeeds)
    assert!(fs::symlink_metadata(&link).is_ok());
    // But path.exists() would return false (follows symlink)
    assert!(!link.exists());

    // move_to_trash_sync should handle this correctly
    let result = move_to_trash_sync(&link);
    assert!(result.is_ok());
    assert!(fs::symlink_metadata(&link).is_err());
}

// ========================================================================
// trash_files_with_progress tests (via CollectorEventSink)
// ========================================================================

use crate::event_sinks::CollectorEventSink;

/// Empty source list short-circuits: no destructive work, but a
/// `write-complete` event still fires so the FE dialog closes cleanly.
#[test]
fn trash_empty_sources_emits_complete_via_sink() {
    let events = Arc::new(CollectorEventSink::new());
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));

    let result = trash_files_with_progress(&*events, "op-trash-empty", &state, &[], None);
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 0);
    assert_eq!(complete[0].bytes_processed, 0);
    assert!(events.cancelled.lock().unwrap().is_empty());
    assert!(events.errors.lock().unwrap().is_empty());
}

/// Pre-cancel: `Stopped` set before the loop's first iteration. Trash
/// emits `write-cancelled` via the sink and returns
/// `WriteOperationError::Cancelled` without invoking `move_to_trash_sync`.
/// Source path is intentionally bogus — the cancel check fires first, so
/// the path is never stat'd.
#[test]
fn trash_pre_cancel_emits_cancelled_via_sink() {
    let events = Arc::new(CollectorEventSink::new());
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));
    state.intent().set_for_test(crate::OperationIntent::Stopped); // Stopped

    let sources = [PathBuf::from("/nonexistent_trash_test_12345/file.txt")];
    let result = trash_files_with_progress(&*events, "op-trash-cancel", &state, &sources, None);
    assert!(matches!(result, Err(WriteOperationError::Cancelled { .. })));

    let cancelled = events.cancelled.lock().unwrap();
    assert_eq!(cancelled.len(), 1);
    assert_eq!(cancelled[0].files_processed, 0);
    assert_eq!(cancelled[0].rollback.outcome, CancelRollbackOutcome::NotRolledBack);
    assert!(events.complete.lock().unwrap().is_empty());
}

/// All sources missing: trash emits `write-error` via the sink and
/// returns `IoError`. Tests the all-failed branch without invoking
/// `move_to_trash_sync` (the missing-source branch short-circuits
/// before the trash call).
#[test]
fn trash_all_sources_missing_emits_error_via_sink() {
    let events = Arc::new(CollectorEventSink::new());
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));

    let sources = [
        PathBuf::from("/nonexistent_trash_test_aaa/x.txt"),
        PathBuf::from("/nonexistent_trash_test_bbb/y.txt"),
    ];
    let result = trash_files_with_progress(&*events, "op-trash-all-missing", &state, &sources, None);
    // Vanished sources, so `Other`: the dialog must not offer a permission grant
    // as the way through something that isn't a permission problem.
    assert!(matches!(
        result,
        Err(WriteOperationError::TrashRefused {
            item_count: 2,
            reason: TrashRefusalKind::Other,
            ..
        })
    ));

    let errors = events.errors.lock().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(events.complete.lock().unwrap().is_empty());
}

/// One item failing leaves the rest of the batch running, so the operation's
/// own terminal event says nothing about that item. Before this, it said
/// nothing at all: a caller tracking per-source outcomes waited forever for a
/// verdict on a file that was never going to move.
#[test]
fn a_source_trash_could_not_take_reports_itself_as_failed() {
    let events = Arc::new(CollectorEventSink::new());
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));
    let missing = PathBuf::from("/nonexistent_trash_test_ccc/gone.txt");

    let result = trash_files_with_progress(
        &*events,
        "op-trash-one-missing",
        &state,
        std::slice::from_ref(&missing),
        None,
    );
    assert!(matches!(
        result,
        Err(WriteOperationError::TrashRefused {
            item_count: 1,
            reason: TrashRefusalKind::Other,
            ..
        })
    ));

    let items = events.source_items_done.lock().unwrap();
    assert_eq!(items.len(), 1, "the item that couldn't be taken speaks for itself");
    assert_eq!(items[0].source_path, missing.display().to_string());
    assert_eq!(items[0].outcome, SourceItemOutcome::Failed);
    assert!(
        items[0].source_removed,
        "a NotFound source really is gone, so a stale search snapshot may drop it"
    );
}

/// The mixed ending, which is the one that used to lie. Two items in, one taken,
/// one refused: the terminal event reported a plain success with `files_skipped: 0`
/// and said nothing at all about the item still sitting in the pane (David's
/// 2026-09-17 log, a Dropbox online-only file beside an ordinary one). So the
/// completion carries what stayed behind, with its typed reason.
#[cfg(target_os = "macos")]
#[test]
fn a_partly_refused_batch_reports_what_it_left_behind() {
    let events = Arc::new(CollectorEventSink::new());
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));
    let tmp = create_test_dir("trash_partial");
    let taken = tmp.join("taken.txt");
    fs::write(&taken, "content").unwrap();
    let refused = tmp.join("no-such-file.txt");

    let sources = [taken.clone(), refused.clone()];
    let result = trash_files_with_progress(&*events, "op-trash-partial", &state, &sources, None);
    assert!(result.is_ok(), "the item that DID go must not be reported as a failure");

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(
        complete[0].files_processed, 1,
        "only the item that actually went counts"
    );
    assert_eq!(
        complete[0].refused,
        Some(TrashRefusedItems {
            item_count: 1,
            reason: TrashRefusalKind::Other,
        }),
        "a completion that says nothing here reads as a clean success"
    );

    // The pane's accounting stays right: one item gone, one exactly where it was.
    let items = events.source_items_done.lock().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].source_path, taken.display().to_string());
    assert_eq!(items[0].outcome, SourceItemOutcome::Done);
    assert_eq!(items[1].source_path, refused.display().to_string());
    assert_eq!(items[1].outcome, SourceItemOutcome::Failed);
}

/// The other half of the contract: a batch the OS took in full still reports a
/// plain success, so the FE has nothing extra to say about it.
#[cfg(target_os = "macos")]
#[test]
fn a_batch_the_os_took_in_full_reports_nothing_left_behind() {
    let events = Arc::new(CollectorEventSink::new());
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(0)));
    let tmp = create_test_dir("trash_all_taken");
    let first = tmp.join("first.txt");
    let second = tmp.join("second.txt");
    fs::write(&first, "content").unwrap();
    fs::write(&second, "content").unwrap();

    let sources = [first, second];
    let result = trash_files_with_progress(&*events, "op-trash-all-taken", &state, &sources, None);
    assert!(result.is_ok());

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].files_processed, 2);
    assert_eq!(complete[0].refused, None);
}

#[test]
fn test_trash_item_error_captures_path_and_message() {
    let error = TrashItemError {
        path: PathBuf::from("/some/file.txt"),
        message: "Permission denied".to_string(),
        reason: TrashRefusalKind::NotPermitted,
        online_only: false,
    };
    assert_eq!(error.path.display().to_string(), "/some/file.txt");
    assert_eq!(error.message, "Permission denied");
    assert_eq!(error.reason, TrashRefusalKind::NotPermitted);
}

/// A refused batch's summary names each item's path marked in a log line, and reads as
/// `path: why; path: why` everywhere else.
#[test]
fn a_refused_batch_summary_marks_each_path() {
    use cmdr_fs::log_error::LogError;

    let item = |path: &str, message: &str| TrashItemError {
        path: PathBuf::from(path),
        message: message.to_string(),
        reason: TrashRefusalKind::NotPermitted,
        online_only: false,
    };
    let errors = [
        item("Trips/Lake House/a.jpg", "Permission denied"),
        item("Trips/Lake House/b.jpg", "no longer exists"),
    ];
    let summary = refused_summary(&errors);
    assert_eq!(
        summary.to_string(),
        "Trips/Lake House/a.jpg: Permission denied; Trips/Lake House/b.jpg: no longer exists"
    );
    let logged = LogError(&summary).to_string();
    for path in ["Trips/Lake House/a.jpg", "Trips/Lake House/b.jpg"] {
        assert!(logged.contains(&LogPrivate::path(path).to_string()), "{logged:?}");
    }
}

/// A batch reports the reason that opens the most doors, not the most common one:
/// eight vanished files beside one permission refusal is still a batch where a
/// permission grant might be the answer.
#[test]
fn a_batch_reports_the_reason_that_offers_the_user_the_most() {
    let item = |reason| TrashItemError {
        path: PathBuf::from("/some/file.txt"),
        message: String::new(),
        reason,
        online_only: false,
    };

    assert_eq!(strongest_refusal(&[]), TrashRefusalKind::Other);
    assert_eq!(
        strongest_refusal(&[item(TrashRefusalKind::Other), item(TrashRefusalKind::Other)]),
        TrashRefusalKind::Other
    );
    assert_eq!(
        strongest_refusal(&[item(TrashRefusalKind::Other), item(TrashRefusalKind::NoTrashForVolume)]),
        TrashRefusalKind::NoTrashForVolume
    );
    assert_eq!(
        strongest_refusal(&[
            item(TrashRefusalKind::Other),
            item(TrashRefusalKind::NoTrashForVolume),
            item(TrashRefusalKind::NotPermitted),
        ]),
        TrashRefusalKind::NotPermitted
    );
}

/// Sets or clears an item's Finder Locked flag (`uchg`) the way a user would.
#[cfg(target_os = "macos")]
fn set_locked(path: &Path, locked: bool) {
    let flag = if locked { "uchg" } else { "nouchg" };
    let done = std::process::Command::new("chflags").arg(flag).arg(path).status();
    assert!(done.is_ok_and(|status| status.success()), "chflags {flag}");
}

/// A Locked item refuses with the same 513 as any permission refusal, and the
/// "permission" advice was a dead end for it: a permanent delete is refused too.
/// Regression anchor: ERR-JQK62 (cmdr-reports#54).
#[cfg(target_os = "macos")]
#[test]
fn a_locked_item_refuses_as_locked() {
    let tmp = create_test_dir("trash_locked_item");
    let file = tmp.join("locked.pdf");
    fs::write(&file, "content").unwrap();
    set_locked(&file, true);

    let result = move_to_trash_sync(&file);
    set_locked(&file, false);

    assert!(
        matches!(
            result,
            Err(MutationError::TrashRefused {
                reason: TrashRefusalKind::Locked,
                ..
            })
        ),
        "expected a Locked refusal, got {result:?}"
    );
    assert!(file.exists(), "a refused item stays where it was");
}

/// Same 513 for an item whose folder takes no writes, which no permanent delete can
/// get past either, so it gets its own reason and advice.
#[cfg(target_os = "macos")]
#[test]
fn an_item_in_a_folder_that_takes_no_writes_refuses_as_folder_not_writable() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = create_test_dir("trash_folder_not_writable");
    let folder = tmp.join("read-only");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("report.pdf");
    fs::write(&file, "content").unwrap();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();

    let result = move_to_trash_sync(&file);
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(
        matches!(
            result,
            Err(MutationError::TrashRefused {
                reason: TrashRefusalKind::FolderNotWritable,
                ..
            })
        ),
        "expected a FolderNotWritable refusal, got {result:?}"
    );
    assert!(file.exists(), "a refused item stays where it was");
}

/// The two refusals read from the item outrank only the reasons that offer less: a
/// plain permission refusal still wins, because it may be a grant away.
#[test]
fn a_batch_ranks_the_item_level_refusals_between_permission_and_no_trash() {
    let item = |reason| TrashItemError {
        path: PathBuf::from("/some/file.txt"),
        message: String::new(),
        reason,
        online_only: false,
    };

    assert_eq!(
        strongest_refusal(&[
            item(TrashRefusalKind::NoTrashForVolume),
            item(TrashRefusalKind::FolderNotWritable)
        ]),
        TrashRefusalKind::FolderNotWritable
    );
    assert_eq!(
        strongest_refusal(&[
            item(TrashRefusalKind::FolderNotWritable),
            item(TrashRefusalKind::Locked)
        ]),
        TrashRefusalKind::Locked
    );
    assert_eq!(
        strongest_refusal(&[item(TrashRefusalKind::Locked), item(TrashRefusalKind::NotPermitted)]),
        TrashRefusalKind::NotPermitted
    );
}

#[test]
fn test_cancellation_flag_checked_by_state() {
    let engine = TestOperations::new();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(200)));

    assert!(!state.intent().is_cancelled());
    state.intent().set_for_test(crate::OperationIntent::Stopped);
    assert!(state.intent().is_cancelled());
}
