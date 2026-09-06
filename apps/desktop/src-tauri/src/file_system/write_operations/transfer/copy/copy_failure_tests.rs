//! What a local copy leaves on disk when one item in the batch fails.
//!
//! The rule the volume engine already follows: a failure keeps every file that
//! landed and cleans only the partial. These cells hold the local engine to it,
//! because an auto-rollback here deletes files that already REPLACED the user's
//! originals, and the original is gone by then.

use super::*;
use crate::file_system::volume::CopyScanResult;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::{CachedScanResult, FileInfo, insert_scan_result};

fn make_state(progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)))
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
