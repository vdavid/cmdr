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

// ── Faithfulness of the ledger: what a volume transfer leaves behind ─────────
//
// A volume op's rows are the only record a later reversal has. These drive the
// real copy body to a terminal state — completed, canceled mid-directory — and
// then ask the real rollback engine to undo it, asserting on the destination
// that's left. Row assertions alone can't tell a faithful ledger from a
// plausible-looking one; it's the reversal acting on the rows that shows the gap.

use super::event_sinks::OperationEventSink;
use crate::file_system::VolumeManager;
use crate::file_system::write_operations::rollback::Reversal;
use crate::operation_log::rollback::{RollbackReport, execute_rollback, rollback_operation};
use crate::operation_log::types::ItemOutcome;

/// A real volume copy, journaled to a real DB, reversible through the real gate.
struct VolumeLoop {
    _journal: TestJournalGuard,
    writer_journal: Arc<WriterJournal>,
    _journal_dir: tempfile::TempDir,
    vm: VolumeManager,
    source: Arc<InMemoryVolume>,
    dest: Arc<InMemoryVolume>,
    op_id: String,
}

impl VolumeLoop {
    fn new(op_id: &str) -> Self {
        let journal_dir = tempfile::tempdir().expect("journal dir");
        let writer = OperationLogWriter::spawn(&operation_log_db_path(journal_dir.path())).expect("spawn writer");
        let writer_journal = Arc::new(WriterJournal::new(writer));
        let journal = TestJournalGuard::install(writer_journal.clone());

        let source = in_memory("Src");
        let dest = in_memory("Dst");
        let vm = VolumeManager::new();
        vm.register("smb-src", Arc::clone(&source) as Arc<dyn Volume>);
        vm.register("smb-dst", Arc::clone(&dest) as Arc<dyn Volume>);

        VolumeLoop {
            _journal: journal,
            writer_journal,
            _journal_dir: journal_dir,
            vm,
            source,
            dest,
            op_id: op_id.to_string(),
        }
    }

    async fn mkdir(&self, path: &str) {
        self.source
            .create_directory(std::path::Path::new(path))
            .await
            .unwrap_or_else(|e| panic!("mkdir {path}: {e:?}"));
    }

    async fn put(&self, path: &str, contents: &[u8]) {
        self.source
            .create_file(std::path::Path::new(path), contents)
            .await
            .unwrap_or_else(|e| panic!("seed {path}: {e:?}"));
    }

    /// Run the real cross-volume copy body to completion, bracketed by the
    /// open/finalize the managed driver arranges.
    async fn copy(&self, sources: &[&str]) {
        self.run_copy(sources, |_| Arc::new(CollectorEventSink::new())).await;
    }

    /// [`Self::copy`], stopped once `after_bytes` have moved — a directory source
    /// interrupted with some children already fully written and one in flight.
    async fn copy_stopping_after_bytes(&self, sources: &[&str], after_bytes: u64) {
        self.run_copy(sources, |intent| {
            Arc::new(StopAfterBytesSink {
                inner: CollectorEventSink::new(),
                intent,
                after_bytes,
            })
        })
        .await;
    }

    /// The shared body: build the op state, let `make_events` hang a sink off its
    /// intent, run the copy, finalize.
    async fn run_copy<F>(&self, sources: &[&str], make_events: F)
    where
        F: FnOnce(Arc<std::sync::atomic::AtomicU8>) -> Arc<dyn OperationEventSink>,
    {
        let state = Arc::new(
            WriteOperationState::new(Duration::from_millis(0)).with_journal_volumes("smb-src".into(), "smb-dst".into()),
        );
        let events = make_events(Arc::clone(&state.intent));
        journal::open_volume_op(
            &self.op_id,
            OpKind::Copy,
            Initiator::User,
            "smb-src",
            Some("smb-dst"),
            sources.len() as u64,
        );
        let paths: Vec<std::path::PathBuf> = sources.iter().map(std::path::PathBuf::from).collect();
        let result = copy_volumes_with_progress(
            events,
            &self.op_id,
            &state,
            Arc::clone(&self.source) as Arc<dyn Volume>,
            &paths,
            Arc::clone(&self.dest) as Arc<dyn Volume>,
            std::path::Path::new("/"),
            &VolumeCopyConfig {
                progress_interval_ms: 0,
                ..VolumeCopyConfig::default()
            },
        )
        .await;
        journal::finalize_op(
            &self.op_id,
            OpKind::Copy,
            journal::execution_status_from_error(result.as_ref().err().map(|f| &f.error)),
        );
    }

    fn items(&self) -> Vec<crate::operation_log::store::OperationItemRow> {
        let conn = open_read_connection(self.writer_journal.writer().db_path()).expect("read conn");
        read_operation_items(&conn, &self.op_id, 1_000).expect("items")
    }

    /// The dest paths this operation journaled, by entry type.
    fn journaled_dests(&self, entry_type: EntryType) -> Vec<String> {
        let conn = open_read_connection(self.writer_journal.writer().db_path()).expect("read conn");
        let detail = crate::operation_log::query::get_operation(&conn, &self.op_id, 1_000, 0)
            .expect("read detail")
            .expect("the copy is journaled");
        let mut out: Vec<String> = detail
            .items
            .iter()
            .filter(|i| i.entry_type == entry_type)
            .filter_map(|i| i.dest_path.clone())
            .collect();
        out.sort();
        out
    }

    /// Ask for the reversal the way the app does: the gate decides, then the engine runs.
    async fn rollback(&self) -> RollbackReport {
        let writer = self.writer_journal.writer();
        let plan = rollback_operation(&self.vm, writer, &self.op_id, |_plan| Ok(())).expect("the copy is reversible");
        let reversal = Reversal::new("volume-journal-test");
        execute_rollback(
            &self.vm,
            writer,
            &plan.original,
            &plan.inverse_op_id,
            Initiator::User,
            reversal.runner(),
        )
        .await
    }

    /// Every FILE path that survives under `root` on the destination volume.
    async fn dest_files_under(&self, root: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_string()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = self.dest.list_directory(std::path::Path::new(&dir), None).await else {
                continue;
            };
            for e in entries {
                let child = if dir == "/" {
                    format!("/{}", e.name)
                } else {
                    format!("{dir}/{}", e.name)
                };
                if e.is_directory {
                    stack.push(child);
                } else {
                    out.push(child);
                }
            }
        }
        out.sort();
        out
    }
}

/// Trips the operation's intent once the copy has moved `after_bytes` bytes, so
/// a directory source is interrupted with some children already fully written.
struct StopAfterBytesSink {
    inner: CollectorEventSink,
    intent: Arc<std::sync::atomic::AtomicU8>,
    after_bytes: u64,
}

impl OperationEventSink for StopAfterBytesSink {
    fn emit_progress(&self, event: crate::file_system::write_operations::types::WriteProgressEvent) {
        if event.phase == crate::file_system::write_operations::types::WriteOperationPhase::Copying
            && event.bytes_done >= self.after_bytes
        {
            // `OperationIntent::Stopped` = 2: keep what's copied, clean the partial.
            self.intent.store(2, std::sync::atomic::Ordering::Relaxed);
        }
        self.inner.emit_progress(event);
    }
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_complete(&self, e: crate::file_system::write_operations::types::WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: crate::file_system::write_operations::types::WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: crate::file_system::write_operations::types::WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: crate::file_system::write_operations::types::WriteConflictEvent) {
        self.inner.emit_conflict(e);
    }
    fn emit_conflict_resolved(&self, e: crate::file_system::write_operations::types::WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, _e: crate::file_system::write_operations::types::WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
}

/// **Gap 3, the user-visible one.** Reversing an SMB/MTP FOLDER copy has to put
/// the folder's inner files back too. Inner leaves recorded no snapshot at all,
/// so every one of them verified as `UnverifiablePrecondition` and skipped: the
/// reversal removed the top level and left the whole tree of copies behind.
///
/// Recording the byte count each leaf already reported is enough — `verify_snapshot`
/// is flat over the row, so a size-only leaf verifies and reverses.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_folder_copy_reverses_every_file_inside_it() {
    let fixture = VolumeLoop::new("op-vol-folder-reverse");
    fixture.mkdir("/album").await;
    fixture.mkdir("/album/inner").await;
    fixture.put("/album/one.txt", b"first").await;
    fixture.put("/album/two.txt", b"second!!").await;
    fixture.put("/album/inner/three.txt", b"third").await;

    fixture.copy(&["/album"]).await;
    assert_eq!(
        fixture.dest_files_under("/album").await,
        vec!["/album/inner/three.txt", "/album/one.txt", "/album/two.txt"],
        "the copy landed the whole folder"
    );

    // Every inner leaf carries the size it was written with, so the recheck has
    // something to verify against.
    let leaves = fixture.items();
    let files: Vec<_> = leaves.iter().filter(|i| i.entry_type == EntryType::File).collect();
    assert_eq!(files.len(), 3, "one row per inner leaf, got {leaves:?}");
    assert!(
        files.iter().all(|i| i.size.is_some()),
        "an inner leaf with no snapshot can never be verified, so it can never be reversed: {files:?}"
    );

    let report = fixture.rollback().await;
    assert_eq!(
        report.skipped, 0,
        "no inner leaf may be skipped as unverifiable, got {report:?}"
    );
    assert_eq!(
        fixture.dest_files_under("/album").await,
        Vec::<String>::new(),
        "every copied file is gone"
    );
    assert!(
        !fixture.dest.exists(std::path::Path::new("/album")).await,
        "and so is the folder the copy created"
    );
}

/// **Gap 1.** A canceled copy keeps what it wrote, including the destination
/// directories it created. Those were journaled only on the success path, so a
/// canceled operation had zero `dir` rows and its reversal left the whole
/// destination skeleton standing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_canceled_volume_copy_still_journals_the_directories_it_created() {
    let fixture = VolumeLoop::new("op-vol-cancel-dirs");
    fixture.mkdir("/album").await;
    for i in 0..4 {
        fixture.put(&format!("/album/f{i}.bin"), &vec![b'x'; 200_000]).await;
    }

    fixture.copy_stopping_after_bytes(&["/album"], 250_000).await;

    assert!(
        fixture.dest.exists(std::path::Path::new("/album")).await,
        "a canceled copy keeps the directory it created"
    );
    let dirs = fixture.journaled_dests(EntryType::Dir);
    assert_eq!(
        dirs,
        vec!["/album".to_string()],
        "the directory the copy created owes a row even though the copy was canceled, got {:?}",
        fixture.items()
    );
}

/// The CONCURRENT driver's half of the same claim. Three top-level sources put the
/// copy on `copy_concurrent.rs` (`source_paths.len() >= 3`), whose interrupted
/// sources come back through `record_failure` rather than the serial `Err` arm —
/// a second place that has to journal, and one the single-source case above
/// never reaches.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interrupted_concurrent_volume_copy_journals_every_child_it_finished() {
    let fixture = VolumeLoop::new("op-vol-interrupted-concurrent");
    for album in ["a", "b", "c"] {
        fixture.mkdir(&format!("/{album}")).await;
        for i in 0..3 {
            fixture.put(&format!("/{album}/f{i}.bin"), &vec![b'x'; 80_000]).await;
        }
    }

    fixture.copy_stopping_after_bytes(&["/a", "/b", "/c"], 200_000).await;

    let survivors = fixture.dest_files_under("/").await;
    assert!(
        !survivors.is_empty(),
        "the fixture must leave at least one finished child behind"
    );
    assert_eq!(
        fixture.journaled_dests(EntryType::File),
        survivors,
        "every destination the canceled copy left behind owes a row, got {:?}",
        fixture.items()
    );
}

/// **Gap 2.** Volume journaling happens per top-level source at COMPLETION, so a
/// directory source interrupted mid-stream contributed no rows at all — every
/// child it had already finished existed on disk and nowhere in the journal, and
/// a reversal from history left them behind.
///
/// The claim under test is the strong one: every destination file that survives
/// the cancel has a row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_interrupted_volume_folder_copy_journals_every_child_it_finished() {
    let fixture = VolumeLoop::new("op-vol-interrupted-children");
    fixture.mkdir("/album").await;
    for i in 0..4 {
        fixture.put(&format!("/album/f{i}.bin"), &vec![b'x'; 200_000]).await;
    }

    fixture.copy_stopping_after_bytes(&["/album"], 250_000).await;

    let survivors = fixture.dest_files_under("/album").await;
    assert!(
        !survivors.is_empty(),
        "the fixture must leave at least one finished child behind"
    );
    let journaled = fixture.journaled_dests(EntryType::File);
    assert_eq!(
        journaled,
        survivors,
        "every destination the canceled copy left behind owes a row, got {:?}",
        fixture.items()
    );
    assert!(
        fixture.items().iter().all(|i| i.outcome == ItemOutcome::Done),
        "the rows name committed files, so they're Done"
    );
}
