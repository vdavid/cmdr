//! What a local copy leaves on disk when one item in the batch fails.
//!
//! The rule the volume engine already follows: a failure keeps every file that
//! landed and cleans only the partial. These cells hold the local engine to it,
//! because an auto-rollback here deletes files that already REPLACED the user's
//! originals, and the original is gone by then.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::file_system::volume::CopyScanResult;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::{
    CachedScanResult, ConflictResolutionResponse, FileInfo, insert_scan_result,
};
use crate::file_system::write_operations::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteConflictEvent, WriteConflictResolvedEvent, WriteSettledEvent,
};

fn make_state(progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)))
}

/// Answers the one conflict this file's folder→file cells raise with Overwrite,
/// which is the explicit consent a blanket policy deliberately doesn't have
/// (`conflict::blanket_resolution_across_types`). Everything else is collected.
struct AnswerOverwrite {
    inner: CollectorEventSink,
    state: Arc<WriteOperationState>,
    answered: AtomicBool,
}

impl OperationEventSink for AnswerOverwrite {
    fn emit_conflict(&self, e: WriteConflictEvent) {
        let clash = e.conflict_id;
        self.inner.emit_conflict(e);
        if !self.answered.swap(true, Ordering::SeqCst) {
            let _ = self.state.conflict_slot.answer(
                clash,
                ConflictResolutionResponse {
                    resolution: ConflictResolution::Overwrite,
                    apply_to_all: false,
                },
            );
        }
    }
    fn emit_progress(&self, e: WriteProgressEvent) {
        self.inner.emit_progress(e);
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_settled(&self, e: WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: DryRunResult) {}
}

/// A batch whose FIRST item overwrites the user's file and whose SECOND one
/// can't be read, run through the copy engine.
///
/// The fault is a source that vanishes between the scan and the copy, which is
/// the one failure a test can inject on any machine: an unreadable file only
/// stops a non-root user, and the Linux container lane runs as root. Seeding the
/// preview cache is what lets the file be there for the scan and gone for the
/// copy, exactly like a source somebody deletes (or unplugs) mid-transfer.
///
/// Answers the destination directory so a cell can ask what survived.
fn copy_batch_whose_second_item_fails(preview_id: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let first = src_dir.join("a.txt");
    let second = src_dir.join("b.txt");
    fs::write(&first, b"THE NEW BYTES").unwrap();
    fs::write(&second, b"about to vanish").unwrap();
    fs::write(dst_dir.join("a.txt"), b"the old bytes").unwrap();

    let sources = vec![first.clone(), second.clone()];
    let files: Vec<FileInfo> = sources
        .iter()
        .map(|path| {
            let metadata = fs::symlink_metadata(path).expect("the source exists while the scan runs");
            FileInfo::new(path.clone(), src_dir.clone(), &metadata)
        })
        .collect();
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    let per_source = sources
        .iter()
        .map(|path| {
            (
                path.clone(),
                CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: fs::symlink_metadata(path).unwrap().len(),
                    dedup_bytes: fs::symlink_metadata(path).unwrap().len(),
                    top_level_is_directory: false,
                },
            )
        })
        .collect();
    insert_scan_result(
        preview_id.to_string(),
        CachedScanResult::from_local_walk(
            sources.clone(),
            files,
            Vec::new(),
            total_bytes,
            total_bytes,
            per_source,
            None,
        ),
    );

    // The scan has been taken; now the second source goes away.
    fs::remove_file(&second).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        preview_id: Some(preview_id.to_string()),
        ..Default::default()
    };

    let result = copy_files_with_progress_inner(&*events, preview_id, &state, &sources, &dst_dir, &config);
    assert!(result.is_err(), "the vanished source must fail the copy: {result:?}");
    (tmp, dst_dir)
}

/// A mid-batch failure must not delete the file that already overwrote the
/// user's destination: the original was set aside and removed the moment the
/// new bytes landed, so deleting the replacement leaves NEITHER copy.
///
/// Pre-fix `PostLoopIntent::Failed` ran `reverse_copy_transaction`, which pops
/// every ledger entry and removes it. The recorded identity is the file that
/// just landed, so the recheck passes and the deletion goes through.
#[test]
fn a_mid_batch_failure_keeps_the_file_that_replaced_the_users_original() {
    let (_tmp, dst_dir) = copy_batch_whose_second_item_fails("op-local-copy-failure-keeps-landed");

    let landed = dst_dir.join("a.txt");
    assert!(
        landed.is_file(),
        "the file that replaced the user's original must survive the failure"
    );
    assert_eq!(
        fs::read(&landed).unwrap(),
        b"THE NEW BYTES",
        "and it must still hold the new bytes"
    );
}

/// The same failure must leave no `.cmdr-tmp-*` litter behind: what the failed
/// item was writing is the one thing the error path does clean.
#[test]
fn a_mid_batch_failure_leaves_no_partial_behind() {
    let (_tmp, dst_dir) = copy_batch_whose_second_item_fails("op-local-copy-failure-no-partial");

    let leftovers: Vec<_> = fs::read_dir(&dst_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .filter(|name| name.contains(".cmdr-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "the failed write's partial must be cleaned; found {leftovers:?}"
    );
}

/// A folder replacing one of the user's files, with the batch failing before the
/// subtree is complete.
///
/// `src/thing/` holds two leaves and `dst/thing` is the user's FILE. The clash is
/// answered Overwrite, so `displace_with_directory` renames the file aside and
/// stands the directory there; the first leaf lands; the second source is gone by
/// the time the copy reaches it, which fails the batch with the folder still half
/// filled.
///
/// Answers the destination directory so a cell can ask what survived, plus the
/// error the operation reported.
fn folder_over_file_copy_whose_second_leaf_fails(
    preview_id: &str,
) -> (tempfile::TempDir, PathBuf, WriteOperationError) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_root.join("thing")).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    let first = src_root.join("thing/a.txt");
    let second = src_root.join("thing/b.txt");
    fs::write(&first, b"source-a").unwrap();
    fs::write(&second, b"source-b").unwrap();
    fs::write(dst_dir.join("thing"), b"the user's only copy").unwrap();

    let source = src_root.join("thing");
    let files: Vec<FileInfo> = [&first, &second]
        .iter()
        .map(|path| {
            let metadata = fs::symlink_metadata(path).expect("the leaf exists while the scan runs");
            FileInfo::new((*path).clone(), src_root.clone(), &metadata)
        })
        .collect();
    let total_bytes: u64 = files.iter().map(|f| f.size).sum();
    insert_scan_result(
        preview_id.to_string(),
        CachedScanResult::from_local_walk(
            vec![source.clone()],
            files,
            vec![source.clone()],
            total_bytes,
            total_bytes,
            vec![(
                source.clone(),
                CopyScanResult {
                    file_count: 2,
                    dir_count: 1,
                    total_bytes,
                    dedup_bytes: total_bytes,
                    top_level_is_directory: true,
                },
            )],
            None,
        ),
    );

    // The scan has been taken; now the second leaf goes away.
    fs::remove_file(&second).unwrap();

    let state = make_state(0);
    let events = AnswerOverwrite {
        inner: CollectorEventSink::new(),
        state: Arc::clone(&state),
        answered: AtomicBool::new(false),
    };
    // Stop, because a BLANKET Overwrite never crosses types; the sink answers the
    // prompt, which is the consent that does.
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Stop,
        preview_id: Some(preview_id.to_string()),
        ..Default::default()
    };

    let result = copy_files_with_progress_inner(&events, preview_id, &state, &[source], &dst_dir, &config);
    let error = result.expect_err("the vanished leaf must fail the copy");
    (tmp, dst_dir, error)
}

/// The failure keeps the folder, so the file it displaced can't simply be thrown
/// away: it comes back beside the folder under a ` (recovered)` name.
///
/// Pre-fix `PostLoopIntent::Failed` committed the transaction, and
/// `CopyTransaction::commit` discards every aside — so the user's only copy of
/// `thing` was deleted to make room for a directory holding one of two files.
#[test]
fn a_failed_folder_over_file_overwrite_keeps_the_users_file_beside_the_folder() {
    let (_tmp, dst_dir, _error) =
        folder_over_file_copy_whose_second_leaf_fails("op-local-folder-over-file-failure-keeps-aside");

    let folder = dst_dir.join("thing");
    assert!(folder.is_dir(), "the folder that replaced the file keeps the name");
    assert_eq!(
        fs::read(folder.join("a.txt")).unwrap(),
        b"source-a",
        "the leaf that landed stays"
    );

    let recovered = dst_dir.join("thing (recovered)");
    assert!(
        recovered.is_file(),
        "the displaced file must survive as a sibling; dst holds {:?}",
        fs::read_dir(&dst_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        fs::read(&recovered).unwrap(),
        b"the user's only copy",
        "and it must still hold its own bytes"
    );
}

/// Moving the file is only half of it: the failure the user reads has to say
/// where it went, as typed data rather than a sentence anyone has to parse.
#[test]
fn a_failed_folder_over_file_overwrite_names_the_recovered_path() {
    let (_tmp, dst_dir, error) =
        folder_over_file_copy_whose_second_leaf_fails("op-local-folder-over-file-failure-names-path");

    let WriteOperationError::OriginalsKeptAside { cause, recovered } = error else {
        panic!("a failure that renamed the user's file has to say so: {error:?}");
    };
    assert!(
        matches!(*cause, WriteOperationError::SourceNotFound { .. }),
        "and it keeps what actually failed, so the dialog can still give that error's own advice: {cause:?}"
    );
    let named: Vec<&str> = recovered.iter().map(|r| r.kept_at.as_str()).collect();
    assert_eq!(
        named,
        vec![dst_dir.join("thing (recovered)").display().to_string()],
        "the recovered path is what the user needs to find their file"
    );
    assert_eq!(
        recovered[0].path,
        dst_dir.join("thing").display().to_string(),
        "alongside the name it used to have"
    );
}
