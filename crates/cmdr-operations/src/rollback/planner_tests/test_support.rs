//! The shared bed the rollback engine's tests drive: a temp-DB journal + writer, a
//! volume registry, seeded journal rows, and `InMemoryVolume` conveniences.
//!
//! Lives apart from `tests.rs` so the sibling test modules (`tests.rs`,
//! `undo_tests.rs`) share ONE rig instead of growing parallel ones — and so neither
//! file has to carry 200 lines of scaffolding.

use std::path::Path;
use std::sync::Arc;

use crate::host::testing::FakeVolumes;
use crate::host::testing::SettablePacing;
use crate::rollback::Reversal;
use crate::testing::TestOperations;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use cmdr_operation_log::rollback::*;
use cmdr_operation_log::store::{
    OperationItemRow, OperationRow, open_read_connection, operation_log_db_path, read_operation, read_operation_items,
    read_rollback_units_page,
};
use cmdr_operation_log::types::{
    EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind, RollbackState, RowRole, SearchCoverage,
};
use cmdr_operation_log::writer::{FinalizeOperation, JournalItem, OpenOperation, OperationLogWriter};

/// A fixed mtime pinned onto seeded files so the recorded snapshot and the live
/// entry agree (verify → Match).
pub(super) const MT: u64 = 1_700_000_000;

// ── Harness ──────────────────────────────────────────────────────────────────

/// A test rig: a writer over a temp-DB journal + a volume registry the engine
/// resolves item volumes through. The temp dir is returned so it outlives the run.
pub(super) struct Rig {
    /// The engine a reversal registers on and its controls ask.
    pub(super) engine: TestOperations,
    /// The engine's rollback pace, which a test sets to hold a reversal open.
    pub(super) pacing: Arc<SettablePacing>,
    pub(super) writer: OperationLogWriter,
    pub(super) vm: FakeVolumes,
    _dir: tempfile::TempDir,
}

impl Rig {
    pub(super) fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = operation_log_db_path(dir.path());
        let writer = OperationLogWriter::spawn(&db).expect("spawn writer");
        let pacing = Arc::new(SettablePacing::default());
        let engine_pacing = Arc::clone(&pacing);
        Rig {
            engine: TestOperations::with(|builder| builder.pacing(engine_pacing)),
            pacing,
            writer,
            vm: FakeVolumes::default(),
            _dir: dir,
        }
    }

    /// A reversal registered under `tag` on this rig's engine.
    pub(super) fn reversal(&self, tag: &str) -> Reversal {
        Reversal::on(&self.engine, tag)
    }

    pub(super) fn register(&self, id: &str, vol: Arc<InMemoryVolume>) {
        self.vm.register(id, vol as Arc<dyn Volume>);
    }

    pub(super) fn read_op(&self, op_id: &str) -> OperationRow {
        let conn = open_read_connection(self.writer.db_path()).expect("read conn");
        read_operation(&conn, op_id).expect("read").expect("op present")
    }

    /// An operation's persisted item rows in `seq` order — how the journal reads back
    /// after a rollback resolved each item.
    pub(super) fn read_items(&self, op_id: &str) -> Vec<OperationItemRow> {
        let conn = open_read_connection(self.writer.db_path()).expect("read conn");
        read_operation_items(&conn, op_id, 1_000).expect("read items")
    }

    /// Seed an operation header + item rows + a terminal `rollback_state`, exactly
    /// as the capture layer would after the op ran.
    pub(super) fn seed(
        &self,
        op_id: &str,
        kind: OpKind,
        src_vol: &str,
        dst_vol: Option<&str>,
        state: RollbackState,
        items: Vec<JournalItem>,
    ) {
        self.seed_at(op_id, kind, src_vol, dst_vol, state, 100, items);
    }

    /// [`Self::seed`] with an explicit `started_at`, for the multi-batch ordering
    /// cases (where relative start time is the whole point).
    #[allow(clippy::too_many_arguments, reason = "the natural fields of a seeded op header")]
    pub(super) fn seed_at(
        &self,
        op_id: &str,
        kind: OpKind,
        src_vol: &str,
        dst_vol: Option<&str>,
        state: RollbackState,
        started_at: i64,
        items: Vec<JournalItem>,
    ) {
        self.writer
            .open_operation(OpenOperation {
                op_id: op_id.into(),
                kind,
                initiator: Initiator::User,
                source_volume_id: Some(src_vol.to_string()),
                dest_volume_id: dst_vol.map(str::to_string),
                item_count: items.len() as u64,
                started_at,
                rolls_back_op_id: None,
                execution_status: ExecutionStatus::Running,
            })
            .expect("open");
        let n = items.len() as u64;
        if !items.is_empty() {
            self.writer.record_items(op_id, items).expect("record");
        }
        self.writer
            .finalize_operation(FinalizeOperation {
                op_id: op_id.to_string(),
                execution_status: ExecutionStatus::Done,
                rollback_state: state,
                not_rollbackable_reason: None,
                archive_subkind: None,
                search_coverage: SearchCoverage::Full,
                search_coverage_reason: None,
                ended_at: 200,
                item_count: None,
                items_done: n,
                bytes_total: 0,
                dev_summary: None,
            })
            .expect("finalize");
        self.writer.flush_blocking().expect("flush");
    }

    /// Put every copied file's ORIGINAL back where the seeded rows say it was copied
    /// from, at the recorded size, on its registered source volume. Undoing a copy
    /// removes it only while its original is still there, so a copy cell that wants
    /// its copies gone seeds the originals too; one that tests the original being
    /// gone leaves this out (`copy_original_tests.rs`).
    pub(super) async fn put_originals(&self, op_id: &str) {
        let conn = open_read_connection(self.writer.db_path()).expect("read conn");
        let units = read_rollback_units_page(&conn, op_id, i64::MAX, 10_000).expect("read units");
        for unit in units.iter().filter(|unit| unit.entry_type == EntryType::File) {
            let volume = self
                .vm
                .get(&unit.source_volume_id)
                .expect("the source volume is registered");
            if let Some(parent) = unit.source_path.parent() {
                volume
                    .create_directory_all(parent)
                    .await
                    .expect("the original's folder");
            }
            let size = unit.size.expect("a copy row records its size") as usize;
            volume
                .create_file(&unit.source_path, &vec![b'o'; size])
                .await
                .expect("the original");
        }
    }

    pub(super) async fn rollback(&self, op_id: &str) -> RollbackReport {
        self.rollback_as(op_id, "inv-1").await
    }

    /// [`Self::rollback`] with an explicit inverse op id, so one rig can reverse
    /// several operations in a chosen order (the multi-batch cases).
    pub(super) async fn rollback_as(&self, op_id: &str, inverse_op_id: &str) -> RollbackReport {
        self.rollback_driven_by(op_id, inverse_op_id, &self.reversal("rollback"))
            .await
    }

    /// [`Self::rollback_as`] against a [`Reversal`] the test holds, so it can stop
    /// it, pause it, or read the frames it emitted while it runs.
    pub(super) async fn rollback_driven_by(
        &self,
        op_id: &str,
        inverse_op_id: &str,
        reversal: &Reversal,
    ) -> RollbackReport {
        let original = self.read_op(op_id);
        execute_rollback(
            &self.vm,
            &self.writer,
            &original,
            inverse_op_id,
            Initiator::User,
            reversal.runner(),
        )
        .await
    }
}

/// An `OperationRow` with every field at a neutral value, for tests that care
/// about one or two fields only.
pub(super) fn blank_op_row() -> OperationRow {
    OperationRow {
        op_id: String::new(),
        kind: OpKind::Rename,
        archive_subkind: None,
        initiator: Initiator::Agent,
        execution_status: ExecutionStatus::Done,
        rollback_state: RollbackState::Rollbackable,
        not_rollbackable_reason: None,
        rolls_back_op_id: None,
        inverse_op_id: None,
        source_volume_id: Some("v".to_string()),
        dest_volume_id: Some("v".to_string()),
        started_at: 0,
        ended_at: None,
        item_count: 0,
        items_done: 0,
        bytes_total: 0,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        dev_summary: None,
    }
}

pub(super) fn split(path: &str) -> (String, String) {
    let p = Path::new(path);
    (
        p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}

/// A `rollback_unit` file row: source on `src_vol`, its landed copy/move on
/// `dst_vol`, with a size + mtime snapshot.
pub(super) fn file_unit(seq: i64, src_vol: &str, src: &str, dst_vol: &str, dst: &str, size: i64) -> JournalItem {
    let (sd, sn) = split(src);
    let (dd, dn) = split(dst);
    JournalItem {
        seq,
        entry_type: EntryType::File,
        row_role: RowRole::RollbackUnit,
        source_volume_id: src_vol.to_string(),
        source_dir: sd,
        source_name: sn,
        dest_volume_id: Some(dst_vol.to_string()),
        dest_dir: Some(dd),
        dest_name: Some(dn),
        size: Some(size),
        mtime: Some(MT as i64),
        outcome: ItemOutcome::Done,
        overwrote: false,
        dest_identity: None,
    }
}

/// A created-directory `rollback_unit` row (source == dest == the created path).
pub(super) fn dir_unit(seq: i64, vol: &str, path: &str) -> JournalItem {
    let (d, n) = split(path);
    JournalItem {
        seq,
        entry_type: EntryType::Dir,
        row_role: RowRole::RollbackUnit,
        source_volume_id: vol.to_string(),
        source_dir: d.clone(),
        source_name: n.clone(),
        dest_volume_id: Some(vol.to_string()),
        dest_dir: Some(d),
        dest_name: Some(n),
        size: None,
        mtime: None,
        outcome: ItemOutcome::Done,
        overwrote: false,
        dest_identity: None,
    }
}

pub(super) async fn put(vol: &InMemoryVolume, path: &str, content: &[u8]) {
    vol.create_file(Path::new(path), content).await.expect("create_file");
    vol.set_modified_at(Path::new(path), Some(MT));
}

pub(super) async fn mkdir(vol: &InMemoryVolume, path: &str) {
    vol.create_directory(Path::new(path)).await.expect("create_directory");
}

pub(super) async fn exists(vol: &InMemoryVolume, path: &str) -> bool {
    vol.exists(Path::new(path)).await
}

pub(super) async fn read(vol: &InMemoryVolume, path: &str) -> Vec<u8> {
    let mut s = vol.open_read_stream(Path::new(path)).await.expect("open stream");
    let mut out = Vec::new();
    while let Some(chunk) = s.next_chunk().await {
        out.extend_from_slice(&chunk.expect("chunk"));
    }
    out
}

// ── A remote place whose root was narrowed ───────────────────────────────────

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use cmdr_fs::volume::ListingProgress;

/// A remote place's volume after an edit narrowed its root: every file stays on
/// the server (`inner`), but a path above the new root is refused as `NotFound`,
/// the way `SftpVolume` and `WebdavVolume` refuse one
/// (`crates/cmdr-sftp/src/volume/paths.rs`). `InMemoryVolume` serves any path it
/// holds, so on its own it can't stand in for that.
pub(super) struct RootFenced {
    inner: Arc<InMemoryVolume>,
    root: PathBuf,
}

impl RootFenced {
    /// `inner`'s files, served through a volume rooted at `root`.
    pub(super) fn over(inner: &Arc<InMemoryVolume>, root: &str) -> Arc<dyn Volume> {
        Arc::new(Self {
            inner: Arc::clone(inner),
            root: PathBuf::from(root),
        })
    }

    fn fence(&self, path: &Path) -> Result<(), VolumeError> {
        if path.starts_with(&self.root) {
            Ok(())
        } else {
            Err(VolumeError::NotFound(path.to_string_lossy().into_owned()))
        }
    }
}

impl Volume for RootFenced {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.fence(path)?;
            self.inner.list_directory(path, on_progress).await
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.fence(path)?;
            self.inner.get_metadata(path).await
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.fence(path)?;
            self.inner.create_directory(path).await
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.fence(path)?;
            self.inner.delete(path).await
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.fence(from)?;
            self.fence(to)?;
            self.inner.rename(from, to, force).await
        })
    }
}
