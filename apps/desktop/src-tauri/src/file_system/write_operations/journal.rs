//! Journaling glue for the write pipeline: brackets a managed op with the
//! operation-log open/finalize and builds per-item `JournalItem`s at the record
//! points.
//!
//! The journal is a process-global reached by `op_id` (see
//! [`crate::operation_log`]), mirroring the op-keyed `update_operation_status`
//! status cache written at these same record points — so these are thin free
//! functions, not threaded state (the D4 deviation recorded in
//! `operation_log/DETAILS.md` § Capture). Every function no-ops when no journal
//! is installed, and never fails the operation.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::file_system::volume::DEFAULT_VOLUME_ID;
use crate::operation_log::capture::FinalizeInputs;
use crate::operation_log::types::RowRole;
use crate::operation_log::types::{ArchiveSubkind, EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind};
use crate::operation_log::writer::{JournalItem, OpenOperation};
use crate::operation_log::{journal_finalize, journal_open, journal_record_items};

use super::types::WriteOperationType;

/// Map the pipeline's op type to the journal taxonomy (1:1). The `archive_edit`
/// subkind + net-new flag are supplied separately by the compress/zip driver
/// (Finding 3), not derivable here.
pub(super) fn op_kind_of(t: WriteOperationType) -> OpKind {
    match t {
        WriteOperationType::Copy => OpKind::Copy,
        WriteOperationType::Move => OpKind::Move,
        WriteOperationType::Delete => OpKind::Delete,
        WriteOperationType::Trash => OpKind::Trash,
        WriteOperationType::Rename => OpKind::Rename,
        WriteOperationType::CreateFolder => OpKind::CreateFolder,
        WriteOperationType::CreateFile => OpKind::CreateFile,
        WriteOperationType::ArchiveEdit => OpKind::ArchiveEdit,
    }
}

/// Seconds since the Unix epoch — the journal's opaque clock (the store owns no
/// clock, so callers supply the time).
pub(super) fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The mtime of `meta` in whole seconds since the epoch, or `None` if the
/// platform can't report it.
pub(super) fn mtime_secs(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// Split a local absolute path into (parent dir string, leaf name) for the
/// journal. The parent path is stored verbatim (interning walks it); a path with
/// no file name yields empty strings.
fn local_split(path: &Path) -> (String, String) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = path
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    (dir, name)
}

/// Open a local-FS managed op in the journal (header only; the schema opens it
/// `not_rollbackable` until finalize computes eligibility).
pub(super) fn open_local_op(op_id: &str, kind: OpKind, initiator: Initiator, item_count: u64) {
    journal_open(OpenOperation {
        op_id: op_id.to_string(),
        kind,
        initiator,
        source_volume_id: Some(DEFAULT_VOLUME_ID.to_string()),
        dest_volume_id: None,
        item_count,
        started_at: now_secs(),
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    });
}

/// Record one local `rollback_unit` row: a copied / cross-FS-moved file (with a
/// dest), a deleted file (no dest), or a trashed / renamed / same-FS-moved
/// top-level item. `seq` is assigned by the journal in recording order.
#[allow(clippy::too_many_arguments, reason = "the natural fields of a journal row")]
pub(super) fn record_local_leaf(
    op_id: &str,
    entry_type: EntryType,
    source: &Path,
    dest: Option<&Path>,
    size: Option<i64>,
    mtime: Option<i64>,
    overwrote: bool,
    outcome: ItemOutcome,
) {
    record_local_row(
        op_id,
        entry_type,
        RowRole::RollbackUnit,
        source,
        dest,
        size,
        mtime,
        overwrote,
        outcome,
    );
}

/// Record one local `search_only` row (a leaf beneath a trashed / same-FS-moved
/// top-level unit — searchable but never a reversal unit, D-granularity).
#[allow(clippy::too_many_arguments, reason = "the natural fields of a journal row")]
pub(super) fn record_local_search_leaf(
    op_id: &str,
    entry_type: EntryType,
    source: &Path,
    dest: Option<&Path>,
    size: Option<i64>,
    mtime: Option<i64>,
) {
    record_local_row(
        op_id,
        entry_type,
        RowRole::SearchOnly,
        source,
        dest,
        size,
        mtime,
        false,
        ItemOutcome::Done,
    );
}

#[allow(clippy::too_many_arguments, reason = "the natural fields of a journal row")]
fn record_local_row(
    op_id: &str,
    entry_type: EntryType,
    row_role: RowRole,
    source: &Path,
    dest: Option<&Path>,
    size: Option<i64>,
    mtime: Option<i64>,
    overwrote: bool,
    outcome: ItemOutcome,
) {
    let (source_dir, source_name) = local_split(source);
    let (dest_dir, dest_name) = match dest {
        Some(d) => {
            let (dd, dn) = local_split(d);
            (Some(dd), Some(dn))
        }
        None => (None, None),
    };
    journal_record_items(
        op_id,
        vec![JournalItem {
            seq: 0,
            entry_type,
            row_role,
            source_volume_id: DEFAULT_VOLUME_ID.to_string(),
            source_dir,
            source_name,
            dest_volume_id: dest.map(|_| DEFAULT_VOLUME_ID.to_string()),
            dest_dir,
            dest_name,
            size,
            mtime,
            outcome,
            overwrote,
        }],
    );
}

/// Record the directories a copy created as first-class `dir` rows (D2, Finding
/// 2). Called after the leaf files are recorded, so the dir rows land AFTER their
/// contents in `seq`; the M3 rollback removes files before their dirs. The
/// created path is both source and dest (a copy's rollback removes the dest dir
/// when empty; search matches its name).
pub(super) fn record_created_dirs(op_id: &str, dirs: &[std::path::PathBuf]) {
    for dir in dirs {
        record_local_row(
            op_id,
            EntryType::Dir,
            RowRole::RollbackUnit,
            dir,
            Some(dir),
            None,
            None,
            false,
            ItemOutcome::Done,
        );
    }
}

/// Journal the terminal state of a `run_instant` create (mkdir / mkfile). On
/// success the created path is a net-new item (source == dest, so the M3 rollback
/// removes it if still empty/unchanged); on failure the op finalizes `failed`
/// with no item. `open_local_op` must have been called with the same `op_id`.
pub(super) fn journal_instant_create(op_id: &str, kind: OpKind, entry_type: EntryType, created: Option<&Path>) {
    match created {
        Some(path) => {
            record_local_leaf(
                op_id,
                entry_type,
                path,
                Some(path),
                None,
                None,
                false,
                ItemOutcome::Done,
            );
            finalize_op(op_id, kind, ExecutionStatus::Done);
        }
        None => finalize_op(op_id, kind, ExecutionStatus::Failed),
    }
}

/// Finalize a local-FS op with a non-archive terminal state. Archive ops
/// (compress) finalize through [`finalize_archive_op`] with the driver's subkind.
pub(super) fn finalize_op(op_id: &str, kind: OpKind, execution_status: ExecutionStatus) {
    journal_finalize(
        op_id,
        FinalizeInputs {
            execution_status,
            kind,
            archive_subkind: None,
            net_new: false,
            ended_at: now_secs(),
            items_done: 0,
            bytes_total: 0,
            dev_summary: None,
        },
    );
}

/// Finalize an `archive_edit` op, carrying the driver-supplied subkind + net-new
/// flag into eligibility (Finding 3).
pub(super) fn finalize_archive_op(
    op_id: &str,
    subkind: ArchiveSubkind,
    net_new: bool,
    execution_status: ExecutionStatus,
) {
    journal_finalize(
        op_id,
        FinalizeInputs {
            execution_status,
            kind: OpKind::ArchiveEdit,
            archive_subkind: Some(subkind),
            net_new,
            ended_at: now_secs(),
            items_done: 0,
            bytes_total: 0,
            dev_summary: None,
        },
    );
}

/// The journaling facts an archive-edit driver supplies that the generic pipeline
/// can't derive: the `archive_edit` subkind (compress vs zip-inner edit — both
/// cross IPC as `ArchiveEdit`, Finding 3), whether the archive was net-new (for
/// compress rollback eligibility), and the provenance. Threaded from the command
/// down into the archive-copy-into deferred, where open + finalize bracket the op.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchiveProvenance {
    pub subkind: ArchiveSubkind,
    /// Compress only: did the archive not exist before (vs overwriting a prior
    /// archive)? Ignored for `Edit`/`Extract`.
    pub net_new: bool,
    pub initiator: Initiator,
}

impl ArchiveProvenance {
    /// A plain copy/move INTO an existing archive: a zip-inner edit, not
    /// rollbackable in v1 (`ZipEditUnsupported`).
    pub(crate) fn edit(initiator: Initiator) -> Self {
        Self {
            subkind: ArchiveSubkind::Edit,
            net_new: false,
            initiator,
        }
    }

    /// A compress: create a NEW archive and pack the sources in. Rollbackable iff
    /// `net_new` (and, at rollback time, unchanged — M3, Finding 5).
    pub(crate) fn compress(net_new: bool, initiator: Initiator) -> Self {
        Self {
            subkind: ArchiveSubkind::Compress,
            net_new,
            initiator,
        }
    }
}

/// Open an archive-edit managed op in the journal. Unlike [`open_local_op`] this
/// records the parent (archive) volume, which may be remote (SMB / MTP). The
/// op opens `not_rollbackable` until [`finalize_archive_op`] computes eligibility
/// from the subkind + net-new flag.
pub(super) fn open_archive_op(op_id: &str, initiator: Initiator, parent_volume_id: &str) {
    journal_open(OpenOperation {
        op_id: op_id.to_string(),
        kind: OpKind::ArchiveEdit,
        initiator,
        source_volume_id: Some(parent_volume_id.to_string()),
        dest_volume_id: Some(parent_volume_id.to_string()),
        item_count: 0,
        started_at: now_secs(),
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    });
}

/// Record the archive a compress created as the single `rollback_unit` item: the
/// compress rollback (M3) deletes THIS archive if it's still net-new and unchanged
/// (the `size`/`mtime` snapshot is the drift check, Finding 5). The archive lives
/// on `parent_volume_id` (may be remote); `overwrote` is `!net_new`.
pub(super) fn record_compress_archive(
    op_id: &str,
    parent_volume_id: &str,
    archive: &Path,
    size: Option<i64>,
    mtime: Option<i64>,
    net_new: bool,
) {
    let (dir, name) = local_split(archive);
    journal_record_items(
        op_id,
        vec![JournalItem {
            seq: 0,
            entry_type: EntryType::File,
            row_role: RowRole::RollbackUnit,
            source_volume_id: parent_volume_id.to_string(),
            source_dir: dir.clone(),
            source_name: name.clone(),
            dest_volume_id: Some(parent_volume_id.to_string()),
            dest_dir: Some(dir),
            dest_name: Some(name),
            size,
            mtime,
            outcome: ItemOutcome::Done,
            overwrote: !net_new,
        }],
    );
}
