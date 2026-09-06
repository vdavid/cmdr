//! What the cross-filesystem move's source sweep is allowed to remove.
//!
//! Phase 4 deletes the originals, and the only originals it may delete are the
//! ones Phase 2 staged. Everything here pins that boundary from the outside: a
//! file that arrives in the source while the move runs, an original a Skip left
//! standing, and a symlink that must go as a link rather than as its target.
//!
//! The "appeared after the scan" trigger is reproduced by seeding a scan preview
//! that omits a file which does exist on disk, which is exactly the state a real
//! move is in whenever anything writes into the source after the transfer dialog
//! finished counting.

use super::cross_fs::move_with_staging;
use super::test_support::{make_state, outcomes_for, removal_flags_for};
use super::*;
use crate::file_system::volume::CopyScanResult;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::{CachedScanResult, FileInfo, insert_scan_result};
use crate::file_system::write_operations::types::ConflictResolution;
use crate::ignore_poison::IgnorePoison;

/// Seeds the scan preview a transfer dialog would have cached for `source`,
/// listing exactly `files` and `dirs`. Anything else on disk under `source` is
/// what "appeared after the scan" means to the engine.
fn seed_preview(preview_id: &str, source: &Path, files: &[PathBuf], dirs: Vec<PathBuf>) {
    let source_root = source.parent().expect("a source has a parent").to_path_buf();
    let mut infos = Vec::new();
    let mut total_bytes = 0u64;
    for file in files {
        let metadata = fs::symlink_metadata(file).expect("a scanned file exists on disk");
        total_bytes += metadata.len();
        infos.push(FileInfo::new(file.clone(), source_root.clone(), &metadata));
    }
    insert_scan_result(
        preview_id.to_string(),
        CachedScanResult::from_local_walk(
            vec![source.to_path_buf()],
            infos,
            dirs,
            total_bytes,
            total_bytes,
            vec![(
                source.to_path_buf(),
                CopyScanResult {
                    file_count: files.len(),
                    dir_count: 0,
                    total_bytes,
                    dedup_bytes: total_bytes,
                    top_level_is_directory: source.is_dir(),
                },
            )],
            None,
        ),
    );
}

/// A cross-FS move of `source` into `dst_dir` running off the seeded preview
/// `preview_id`, under Stop (nothing here provokes a conflict prompt).
fn run_preview_backed_move(source: &Path, dst_dir: &Path, preview_id: &str, op_id: &str) -> Arc<CollectorEventSink> {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(0);
    let config = WriteOperationConfig {
        preview_id: Some(preview_id.to_string()),
        ..WriteOperationConfig::default()
    };
    let result = move_with_staging(
        &*events,
        op_id,
        &state,
        std::slice::from_ref(&source.to_path_buf()),
        dst_dir,
        &config,
        0,
    );
    assert!(result.is_ok(), "the move must succeed: {result:?}");
    events
}

/// CRITICAL data-loss regression. A file written into the source after the scan
/// was never staged, so its bytes are not at the destination. Removing the
/// source tree by identity (`remove_dir_all`) destroys it: the download that
/// finished while the transfer dialog was open is gone, and it never existed
/// anywhere else.
#[test]
fn a_file_that_appeared_after_the_scan_survives_the_source_sweep() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    let source = src_root.join("work");
    fs::create_dir_all(source.join("notes")).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let scanned = source.join("notes/report.txt");
    fs::write(&scanned, b"counted by the scan").unwrap();
    seed_preview(
        "sweep-newcomer",
        &source,
        std::slice::from_ref(&scanned),
        vec![source.clone(), source.join("notes")],
    );

    // The download that finished while the transfer dialog was still open.
    let newcomer = source.join("notes/download.zip");
    fs::write(&newcomer, b"arrived after the scan").unwrap();

    run_preview_backed_move(&source, &dst_root, "sweep-newcomer", "op-sweep-newcomer");

    assert!(
        dst_root.join("work/notes/report.txt").is_file(),
        "the scanned file must land at the destination"
    );
    assert!(
        newcomer.exists(),
        "a file that appeared after the scan must survive the source sweep (it was never copied)"
    );
    assert_eq!(fs::read(&newcomer).unwrap(), b"arrived after the scan");
    assert!(
        source.join("notes").is_dir(),
        "the directory holding it must survive too"
    );
    assert!(source.is_dir(), "the source folder must survive, it still holds a file");
    assert!(
        !scanned.exists(),
        "the file that DID land must still be removed from the source"
    );
}

/// The source that kept a newcomer is still sitting in the user's pane, so the
/// operation's verdict on it is `Skipped` with `source_removed: false`. Reading
/// it as `Done`/removed drops the row from the search snapshot and tells the
/// pane to deselect a folder that is still there.
#[test]
fn a_source_that_kept_a_newcomer_ends_on_skipped_and_never_reports_removal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    let source = src_root.join("work");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let scanned = source.join("report.txt");
    fs::write(&scanned, b"counted by the scan").unwrap();
    seed_preview(
        "sweep-newcomer-outcome",
        &source,
        std::slice::from_ref(&scanned),
        vec![source.clone()],
    );
    fs::write(source.join("download.zip"), b"arrived after the scan").unwrap();

    let events = run_preview_backed_move(
        &source,
        &dst_root,
        "sweep-newcomer-outcome",
        "op-sweep-newcomer-outcome",
    );

    assert!(source.is_dir(), "precondition: the newcomer keeps the source folder");
    assert_eq!(
        outcomes_for(&events, &source).last().copied(),
        Some(SourceItemOutcome::Skipped),
        "a source left standing ends on Skipped, whatever it reported while staging"
    );
    assert!(
        !removal_flags_for(&events, &source).contains(&true),
        "a source still on disk must never be reported removed"
    );
}

/// The completion event says how many items stayed behind and where, as typed
/// data the frontend words itself. Without it the move reports a clean success
/// while part of the user's selection is still in the source folder.
#[test]
fn the_completion_event_counts_what_stayed_behind() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    let source = src_root.join("Work");
    fs::create_dir_all(source.join("notes")).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let scanned = source.join("report.txt");
    fs::write(&scanned, b"counted by the scan").unwrap();
    seed_preview(
        "sweep-newcomer-report",
        &source,
        std::slice::from_ref(&scanned),
        vec![source.clone(), source.join("notes")],
    );
    fs::write(source.join("download.zip"), b"arrived after the scan").unwrap();
    fs::write(source.join("notes/todo.md"), b"and so did this").unwrap();

    let events = run_preview_backed_move(&source, &dst_root, "sweep-newcomer-report", "op-sweep-newcomer-report");

    let complete = events.complete.lock_ignore_poison();
    let appeared = complete[0]
        .appeared_during_move
        .as_ref()
        .expect("the move left items behind, so it must say so");
    assert_eq!(appeared.item_count, 2, "both newcomers are counted");
    assert_eq!(appeared.folder_name, "Work");
    assert_eq!(appeared.folder_count, 1);
}

/// The ordinary move reports nothing left behind, so the frontend has no line to
/// show. Without this the "items stayed" notice fires on every clean move.
#[test]
fn a_clean_move_removes_every_source_and_reports_nothing_left_behind() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    let source = src_root.join("work");
    fs::create_dir_all(source.join("notes")).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    fs::write(source.join("report.txt"), b"one").unwrap();
    fs::write(source.join("notes/todo.md"), b"two").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(0);
    let config = WriteOperationConfig::default();
    let result = move_with_staging(
        &*events,
        "op-sweep-clean",
        &state,
        std::slice::from_ref(&source),
        &dst_root,
        &config,
        0,
    );
    assert!(result.is_ok(), "the move must succeed: {result:?}");

    assert!(!source.exists(), "a clean move takes the whole source tree");
    assert!(dst_root.join("work/notes/todo.md").is_file());
    assert_eq!(
        outcomes_for(&events, &source).last().copied(),
        Some(SourceItemOutcome::Done)
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].appeared_during_move.is_none(),
        "nothing appeared, so the completion event says nothing about it"
    );
}

/// A directory merge that skipped a child leaves the source directory standing
/// around that child, so the verdict on it is `Skipped` with `source_removed:
/// false`. `source_removed` is the vanished-path contract the frontend's
/// search-snapshot purge acts on, so reporting the removal the sweep INTENDED
/// drops the row for a folder the user can still open.
#[test]
fn a_source_dir_that_kept_a_skipped_child_never_reports_removal() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let source = src_root.join("photos");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("keep.jpg"), b"source keep").unwrap();
    fs::write(source.join("collide.jpg"), b"source collide").unwrap();
    let dst_dir = dst_root.join("photos");
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(dst_dir.join("collide.jpg"), b"dest collide").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(0);
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..WriteOperationConfig::default()
    };
    let result = move_with_staging(
        &*events,
        "op-sweep-merge-skip",
        &state,
        std::slice::from_ref(&source),
        &dst_root,
        &config,
        0,
    );
    assert!(result.is_ok(), "the move must succeed: {result:?}");

    assert!(source.is_dir(), "precondition: the skipped child keeps the source dir");
    assert!(
        !removal_flags_for(&events, &source).contains(&true),
        "a source dir still on disk must never be reported removed"
    );
    assert_eq!(
        outcomes_for(&events, &source).last().copied(),
        Some(SourceItemOutcome::Skipped)
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].appeared_during_move.is_none(),
        "a Skip the user asked for is not something that appeared mid-move"
    );
}

/// A symlink in the source goes as a LINK. Following it would delete the file it
/// points at, which the user never selected and which the move never copied.
#[test]
fn a_symlinked_source_file_is_removed_as_a_link_and_its_target_is_untouched() {
    use std::os::unix::fs::symlink;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_root = tmp.path().join("dst");
    let source = src_root.join("work");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    // The link's target lives outside the selection entirely.
    let outsider = tmp.path().join("outsider.txt");
    fs::write(&outsider, b"never selected").unwrap();
    symlink(&outsider, source.join("shortcut.txt")).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(0);
    let config = WriteOperationConfig::default();
    let result = move_with_staging(
        &*events,
        "op-sweep-symlink",
        &state,
        std::slice::from_ref(&source),
        &dst_root,
        &config,
        0,
    );
    assert!(result.is_ok(), "the move must succeed: {result:?}");

    assert!(
        fs::symlink_metadata(dst_root.join("work/shortcut.txt"))
            .expect("the link must land")
            .file_type()
            .is_symlink(),
        "the destination gets a symlink, not a copy of the target"
    );
    assert!(!source.exists(), "the source folder and its link are gone");
    assert!(
        outsider.exists(),
        "the link's target must be untouched: the sweep removes links, it never follows them"
    );
    assert_eq!(fs::read(&outsider).unwrap(), b"never selected");
}

/// A Skip while STAGING leaves the original where it is. Two sources sharing a
/// basename both stage into `<staging>/<name>/`, so the second one's children
/// meet the first one's staged files and resolve as a conflict; under Skip they
/// never land, and the sweep must not delete originals whose bytes went nowhere.
///
/// Validation refuses same-basename sources before an operation gets this far
/// (`validation.rs`), so this drives the engine directly: the rule the sweep
/// enforces is "delete what staged", not "delete unless Phase 3 said otherwise".
#[test]
fn a_skip_while_staging_keeps_the_original() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dst_root = tmp.path().join("dst");
    fs::create_dir_all(&dst_root).unwrap();

    let first = tmp.path().join("a/invoices");
    let second = tmp.path().join("b/invoices");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("summary.pdf"), b"the first one").unwrap();
    fs::write(second.join("summary.pdf"), b"the second one, never staged").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(0);
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..WriteOperationConfig::default()
    };
    let result = move_with_staging(
        &*events,
        "op-sweep-staging-skip",
        &state,
        &[first.clone(), second.clone()],
        &dst_root,
        &config,
        0,
    );
    assert!(result.is_ok(), "the move must succeed: {result:?}");

    assert_eq!(
        fs::read(dst_root.join("invoices/summary.pdf")).unwrap(),
        b"the first one",
        "the file that staged first is the one at the destination"
    );
    assert!(
        second.join("summary.pdf").exists(),
        "a Skip while staging means the original never landed: deleting it is data loss"
    );
    assert_eq!(
        fs::read(second.join("summary.pdf")).unwrap(),
        b"the second one, never staged"
    );
}
