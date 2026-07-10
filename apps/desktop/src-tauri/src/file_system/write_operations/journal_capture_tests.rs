//! Integration tests for capture at the chokepoint (M2): drive a real local-FS
//! op through the pipeline with a temp-DB journal installed globally, then read
//! back the journaled operation + item rows.
//!
//! These exercise the record points (`single_item.rs`, `walker.rs`,
//! `move_op.rs`) plus the open/finalize bracket, complementing the pure
//! eligibility/completeness unit tests in `operation_log::capture`.

use std::sync::Arc;
use std::time::Duration;

use super::journal;
use super::state::WriteOperationState;
use super::transfer::move_op::move_files_with_progress_inner;
use super::types::{CollectorEventSink, WriteOperationConfig};
use super::{copy_files_with_progress_inner, delete_files_with_progress_inner};

use crate::operation_log::capture::WriterJournal;
use crate::operation_log::store::{open_read_connection, operation_log_db_path, read_operation, read_operation_items};
use crate::operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind, RollbackState, RowRole};
use crate::operation_log::writer::OperationLogWriter;
use crate::operation_log::{clear_journal, set_journal};

/// Install a fresh temp-DB journal as the process-global one and hand back its
/// DB path + the temp dir (kept alive by the caller). nextest isolates each test
/// in its own process, so the global is hermetic.
fn install_journal() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = operation_log_db_path(dir.path());
    let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
    set_journal(Arc::new(WriterJournal::new(writer)));
    (dir, db)
}

fn state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

#[test]
fn grouped_copy_journals_leaf_files_and_created_dir_rows() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    // A source tree: src/a.txt and src/sub/b.txt.
    let src = work.path().join("src");
    std::fs::create_dir_all(src.join("sub")).expect("mk src");
    std::fs::write(src.join("a.txt"), b"aaa").expect("a");
    std::fs::write(src.join("sub").join("b.txt"), b"bbbb").expect("b");
    let dst = work.path().join("dst");
    std::fs::create_dir_all(&dst).expect("mk dst");

    let op_id = "op-copy-smoke";
    journal::open_local_op(op_id, OpKind::Copy, Initiator::User, 0);
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig::default();
    copy_files_with_progress_inner(&events, op_id, &state(), std::slice::from_ref(&src), &dst, &cfg).expect("copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(row.kind, OpKind::Copy);
    assert_eq!(row.execution_status, ExecutionStatus::Done);
    // No overwrite ⇒ rollbackable.
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);

    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    let files: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::File).collect();
    let dirs: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::Dir).collect();
    // Two leaf files (a.txt, b.txt).
    assert_eq!(files.len(), 2, "expected 2 file rows, got {items:?}");
    // At least the two created dirs (dst/src, dst/src/sub).
    assert!(dirs.len() >= 2, "expected created-dir rows, got {items:?}");
    // Every item is a rollback_unit row for a copy.
    assert!(items.iter().all(|i| i.row_role == RowRole::RollbackUnit));
    // Dir rows are sequenced AFTER their contents (Finding 2): every dir seq is
    // greater than every file seq.
    let max_file_seq = files.iter().map(|i| i.seq).max().unwrap();
    let min_dir_seq = dirs.iter().map(|i| i.seq).min().unwrap();
    assert!(
        min_dir_seq > max_file_seq,
        "dir rows must follow file rows in seq (files max {max_file_seq}, dirs min {min_dir_seq})"
    );
}

#[test]
fn overwriting_copy_finalizes_not_rollbackable() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let src = work.path().join("src");
    std::fs::create_dir_all(&src).expect("mk src");
    std::fs::write(src.join("a.txt"), b"new").expect("a");
    // Pre-existing dest that the copy will overwrite.
    let dst = work.path().join("dst");
    std::fs::create_dir_all(dst.join("src")).expect("mk dst");
    std::fs::write(dst.join("src").join("a.txt"), b"old").expect("old");

    let op_id = "op-copy-overwrite";
    journal::open_local_op(op_id, OpKind::Copy, Initiator::User, 0);
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig {
        conflict_resolution: super::ConflictResolution::Overwrite,
        ..Default::default()
    };
    copy_files_with_progress_inner(&events, op_id, &state(), std::slice::from_ref(&src), &dst, &cfg).expect("copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(
        row.rollback_state,
        RollbackState::NotRollbackable,
        "an overwriting copy can't be rolled back (originals gone)"
    );
}

#[test]
fn same_fs_move_journals_the_top_level_item_as_rollback_unit() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    // Source and dest in the same tempdir ⇒ same filesystem ⇒ rename path.
    let src = work.path().join("photos");
    std::fs::create_dir_all(src.join("sub")).expect("mk src");
    std::fs::write(src.join("p.jpg"), b"pic").expect("p");
    let dst = work.path().join("dest");
    std::fs::create_dir_all(&dst).expect("mk dst");

    let op_id = "op-move-smoke";
    journal::open_local_op(op_id, OpKind::Move, Initiator::User, 0);
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig::default();
    move_files_with_progress_inner(&events, op_id, &state(), std::slice::from_ref(&src), &dst, &cfg).expect("move");
    journal::finalize_op(op_id, OpKind::Move, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    assert_eq!(row.kind, OpKind::Move);
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    // A same-FS move records ONE top-level rollback_unit row (the whole `photos`
    // subtree moved by one rename); its `search_only` leaves come from the drive
    // index (M2e), not enumerated here.
    assert_eq!(items.len(), 1, "expected 1 top-level row, got {items:?}");
    assert_eq!(items[0].entry_type, EntryType::Dir);
    assert_eq!(items[0].row_role, RowRole::RollbackUnit);
    assert_eq!(items[0].source_name, "photos");
}

#[test]
fn initiator_threads_from_the_op_into_the_journal_row() {
    // An AI-client-initiated copy records `ai_client`; the default path records
    // `user`. Provenance (D5) crosses as a typed enum, not a string.
    for initiator in [Initiator::User, Initiator::AiClient] {
        let (jdir, jdb) = install_journal();
        let work = tempfile::tempdir().expect("work");
        let src = work.path().join("src");
        std::fs::create_dir_all(&src).expect("mk src");
        std::fs::write(src.join("a.txt"), b"a").expect("a");
        let dst = work.path().join("dst");
        std::fs::create_dir_all(&dst).expect("mk dst");

        let op_id = "op-initiator";
        journal::open_local_op(op_id, OpKind::Copy, initiator, 0);
        let events = CollectorEventSink::new();
        copy_files_with_progress_inner(&events, op_id, &state(), &[src], &dst, &WriteOperationConfig::default())
            .expect("copy");
        journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);
        clear_journal();

        let conn = open_read_connection(&jdb).expect("read conn");
        let row = read_operation(&conn, op_id).expect("read").expect("row");
        assert_eq!(row.initiator, initiator);
        drop(jdir);
    }
}

#[test]
fn delete_journals_search_leaves_and_stays_not_rollbackable() {
    let (_jdir, jdb) = install_journal();
    let work = tempfile::tempdir().expect("work");
    let a = work.path().join("dog.jpg");
    let b = work.path().join("cat.jpg");
    std::fs::write(&a, b"dog").expect("a");
    std::fs::write(&b, b"cat").expect("b");

    let op_id = "op-delete-smoke";
    journal::open_local_op(op_id, OpKind::Delete, Initiator::User, 0);
    let events = CollectorEventSink::new();
    let cfg = WriteOperationConfig::default();
    delete_files_with_progress_inner(&events, op_id, &state(), &[a.clone(), b.clone()], &cfg).expect("delete");
    journal::finalize_op(op_id, OpKind::Delete, ExecutionStatus::Done);
    clear_journal();

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read op").expect("op row");
    // Delete is never rollbackable.
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    // Both deleted leaves are journaled so "when did I delete dog.jpg" is searchable.
    assert_eq!(items.len(), 2, "expected 2 deleted-leaf rows, got {items:?}");
    assert!(items.iter().any(|i| i.source_name == "dog.jpg"));
}
