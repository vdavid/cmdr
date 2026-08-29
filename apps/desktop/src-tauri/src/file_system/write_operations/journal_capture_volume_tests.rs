//! Volume (SMB / MTP) capture at the chokepoint: drive the REAL volume
//! copy/move/delete bodies with `InMemoryVolume`s and a temp-DB journal, then
//! read the rows back.
//!
//! Two load-bearing themes:
//!
//! - **Honesty of the volume id.** A volume op's rows carry the REAL volume id,
//!   never the local `"root"` the local helpers bake in — a wrong id would
//!   corrupt history silently.
//! - **Faithfulness of the ledger.** Every destination the operation leaves
//!   behind gets a row, whether the operation completed, was canceled, or
//!   failed, and every leaf carries the size a reversal verifies against.
//!
//! The local-FS siblings live in `journal_capture_tests.rs`.

use std::sync::Arc;
use std::time::Duration;

use super::event_sinks::CollectorEventSink;
use super::journal;
use super::state::WriteOperationState;
use super::types::{VolumeCopyConfig, WriteOperationConfig};
use super::{copy_volumes_with_progress, move_volumes_with_progress};

use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::operation_log::TestJournalGuard;
use crate::operation_log::capture::WriterJournal;
use crate::operation_log::store::{open_read_connection, operation_log_db_path, read_operation, read_operation_items};
use crate::operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind, RollbackState, RowRole};
use crate::operation_log::writer::OperationLogWriter;

/// Install a fresh temp-DB journal as the process-global one; see the twin in
/// `journal_capture_tests.rs`.
fn install_journal() -> (TestJournalGuard, tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = operation_log_db_path(dir.path());
    let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
    let guard = TestJournalGuard::install(Arc::new(WriterJournal::new(writer)));
    (guard, dir, db)
}

fn state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(0)))
}

/// Every distinct `volume_id` among the dirs referenced by `op_id`'s item rows.
/// Scoped to the op rather than the whole `dirs` table: under plain `cargo test`
/// a concurrent NON-journal write-op test can journal its own rows into the
/// installed DB (only journal-installing tests serialize on `TestJournalGuard`),
/// and a whole-table read would pick up its `root`-interned dirs.
fn dir_volume_ids(conn: &rusqlite::Connection, op_id: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT d.volume_id FROM dirs d WHERE d.dir_id IN (
                 SELECT source_dir_id FROM operation_items WHERE op_id = ?1
                 UNION
                 SELECT dest_dir_id FROM operation_items WHERE op_id = ?1 AND dest_dir_id IS NOT NULL
             )",
        )
        .expect("prepare");

    stmt.query_map([op_id], |r| r.get::<_, String>(0))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect")
}

fn in_memory(name: &str) -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new(name).with_space_info(1_000_000, 900_000))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_copy_journals_under_the_real_volume_ids_not_root() {
    let (_journal, _jdir, jdb) = install_journal();
    let source = in_memory("Src");
    source
        .create_file(std::path::Path::new("/a.txt"), b"aaa")
        .await
        .expect("a");
    source
        .create_file(std::path::Path::new("/b.txt"), b"bbbb")
        .await
        .expect("b");
    let dest = in_memory("Dst");

    let op_id = "op-vol-copy";
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("smb-src".into(), "smb-dst".into()),
    );
    journal::open_volume_op(op_id, OpKind::Copy, Initiator::AiClient, "smb-src", Some("smb-dst"), 0);
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &state,
        source as Arc<dyn Volume>,
        &[std::path::PathBuf::from("/a.txt"), std::path::PathBuf::from("/b.txt")],
        dest as Arc<dyn Volume>,
        std::path::Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("volume copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    // The operation header carries the REAL volume ids + the AI-client provenance.
    assert_eq!(row.source_volume_id.as_deref(), Some("smb-src"));
    assert_eq!(row.dest_volume_id.as_deref(), Some("smb-dst"));
    assert_eq!(row.initiator, Initiator::AiClient);
    assert_eq!(row.kind, OpKind::Copy);
    // No overwrite ⇒ rollbackable.
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);

    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    assert_eq!(items.len(), 2, "two leaf rows, got {items:?}");
    assert!(items.iter().all(|i| i.row_role == RowRole::RollbackUnit));

    // The honesty invariant: every interned dir is on a REAL volume, never "root".
    let vols = dir_volume_ids(&conn, op_id);
    assert!(
        vols.iter().all(|v| v == "smb-src" || v == "smb-dst"),
        "volume copy dirs must carry the real volume ids, got {vols:?}"
    );
    assert!(
        !vols.iter().any(|v| v == "root"),
        "a volume op must never journal under root"
    );
    assert!(vols.iter().any(|v| v == "smb-src") && vols.iter().any(|v| v == "smb-dst"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overwriting_volume_copy_finalizes_not_rollbackable() {
    let (_journal, _jdir, jdb) = install_journal();
    let source = in_memory("Src");
    source
        .create_file(std::path::Path::new("/dup.txt"), b"new")
        .await
        .expect("src dup");
    let dest = in_memory("Dst");
    // Pre-existing dest file with the same name ⇒ the copy overwrites it.
    dest.create_file(std::path::Path::new("/dup.txt"), b"old")
        .await
        .expect("dst dup");

    let op_id = "op-vol-copy-ow";
    let cfg = VolumeCopyConfig {
        conflict_resolution: super::types::ConflictResolution::Overwrite,
        ..Default::default()
    };
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("smb-src".into(), "smb-dst".into()),
    );
    journal::open_volume_op(op_id, OpKind::Copy, Initiator::User, "smb-src", Some("smb-dst"), 0);
    copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &state,
        source as Arc<dyn Volume>,
        &[std::path::PathBuf::from("/dup.txt")],
        dest as Arc<dyn Volume>,
        std::path::Path::new("/"),
        &cfg,
    )
    .await
    .expect("volume copy");
    journal::finalize_op(op_id, OpKind::Copy, ExecutionStatus::Done);

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    // Overwriting an existing dest ⇒ not rollbackable (the original is gone).
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    assert!(
        items.iter().any(|i| i.overwrote),
        "the overwriting leaf must be flagged, got {items:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_journals_per_leaf_move_rows() {
    let (_journal, _jdir, jdb) = install_journal();
    let source = in_memory("Src");
    source
        .create_file(std::path::Path::new("/one.txt"), b"1")
        .await
        .expect("one");
    source
        .create_file(std::path::Path::new("/two.txt"), b"22")
        .await
        .expect("two");
    let dest = in_memory("Dst");

    let op_id = "op-vol-move";
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("smb-src".into(), "smb-dst".into()),
    );
    journal::open_volume_op(op_id, OpKind::Move, Initiator::User, "smb-src", Some("smb-dst"), 0);
    move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        op_id,
        &state,
        source as Arc<dyn Volume>,
        &[
            std::path::PathBuf::from("/one.txt"),
            std::path::PathBuf::from("/two.txt"),
        ],
        dest as Arc<dyn Volume>,
        std::path::Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("volume move");
    journal::finalize_op(op_id, OpKind::Move, ExecutionStatus::Done);

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    assert_eq!(row.kind, OpKind::Move);
    // A cross-volume move is per-leaf (D-granularity): no overwrite ⇒ rollbackable.
    assert_eq!(row.rollback_state, RollbackState::Rollbackable);

    // Per-leaf rows: one `rollback_unit` per moved FILE, source on the source
    // volume, dest on the dest volume.
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    let files: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::File).collect();
    assert_eq!(files.len(), 2, "one leaf row per moved file, got {items:?}");
    assert!(files.iter().all(|i| i.row_role == RowRole::RollbackUnit));
    let vols = dir_volume_ids(&conn, op_id);
    assert!(
        !vols.iter().any(|v| v == "root"),
        "a cross-volume move must never journal under root"
    );
    assert!(vols.iter().any(|v| v == "smb-src") && vols.iter().any(|v| v == "smb-dst"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_delete_journals_per_leaf_under_the_real_volume_id() {
    let (_journal, _jdir, jdb) = install_journal();
    let volume = in_memory("Src");
    volume
        .create_file(std::path::Path::new("/gone1.txt"), b"x")
        .await
        .expect("g1");
    volume
        .create_file(std::path::Path::new("/gone2.txt"), b"yy")
        .await
        .expect("g2");

    let op_id = "op-vol-delete";
    let state = state();
    journal::open_volume_op(op_id, OpKind::Delete, Initiator::User, "smb-src", None, 0);
    super::delete::delete_volume_files_with_progress_inner(
        volume as Arc<dyn Volume>,
        "smb-src",
        &CollectorEventSink::new(),
        op_id,
        &state,
        &[
            std::path::PathBuf::from("/gone1.txt"),
            std::path::PathBuf::from("/gone2.txt"),
        ],
        &WriteOperationConfig::default(),
    )
    .await
    .expect("volume delete");
    journal::finalize_op(op_id, OpKind::Delete, ExecutionStatus::Done);

    let conn = open_read_connection(&jdb).expect("read conn");
    let row = read_operation(&conn, op_id).expect("read").expect("op row");
    assert_eq!(row.kind, OpKind::Delete);
    // Delete is never rollbackable.
    assert_eq!(row.rollback_state, RollbackState::NotRollbackable);
    let items = read_operation_items(&conn, op_id, 1000).expect("items");
    let files: Vec<_> = items.iter().filter(|i| i.entry_type == EntryType::File).collect();
    assert_eq!(files.len(), 2, "one leaf row per deleted file, got {items:?}");
    let vols = dir_volume_ids(&conn, op_id);
    assert_eq!(
        vols,
        vec!["smb-src".to_string()],
        "the delete must journal under the real volume id"
    );
}
