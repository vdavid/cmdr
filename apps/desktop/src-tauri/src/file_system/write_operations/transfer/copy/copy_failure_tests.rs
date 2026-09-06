//! What a local copy leaves on disk when one item in the batch fails.
//!
//! The rule the volume engine already follows: a failure keeps every file that
//! landed and cleans only the partial. These cells hold the local engine to it,
//! because an auto-rollback here deletes files that already REPLACED the user's
//! originals, and the original is gone by then.

use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;

fn make_state(progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)))
}

/// Makes `path` unreadable, so the copy of it fails with a plain I/O error.
#[cfg(unix)]
fn make_unreadable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
}

/// A mid-batch failure must not delete the file that already overwrote the
/// user's destination: the original was set aside and removed the moment the
/// new bytes landed, so deleting the replacement leaves NEITHER copy.
///
/// Pre-fix `PostLoopIntent::Failed` ran `reverse_copy_transaction`, which pops
/// every ledger entry and removes it. The recorded identity is the file that
/// just landed, so the recheck passes and the deletion goes through.
#[cfg(unix)]
#[test]
fn a_mid_batch_failure_keeps_the_file_that_replaced_the_users_original() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    // First source overwrites an existing destination; second one can't be read.
    let first = src_dir.join("a.txt");
    let second = src_dir.join("b.txt");
    fs::write(&first, b"THE NEW BYTES").unwrap();
    fs::write(&second, b"unreadable").unwrap();
    fs::write(dst_dir.join("a.txt"), b"the old bytes").unwrap();
    make_unreadable(&second);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..Default::default()
    };

    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-failure-keeps-landed",
        &state,
        &[first, second],
        &dst_dir,
        &config,
    );

    assert!(result.is_err(), "the unreadable source must fail the copy");
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

/// The same failure must leave no `.cmdr-tmp-*` litter behind: the partial the
/// failed item was writing is the one thing the error path does clean.
#[cfg(unix)]
#[test]
fn a_mid_batch_failure_leaves_no_partial_behind() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let source = src_dir.join("b.txt");
    fs::write(&source, b"unreadable").unwrap();
    make_unreadable(&source);

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-failure-no-partial",
        &state,
        std::slice::from_ref(&source),
        &dst_dir,
        &config,
    );
    assert!(result.is_err(), "the unreadable source must fail the copy");

    let leftovers: Vec<_> = fs::read_dir(&dst_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "the failed write's partial must be cleaned; found {leftovers:?}"
    );
}
