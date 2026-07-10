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

use super::types::{WriteOperationError, WriteOperationType};

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

/// Map a write op's terminal `Result` error into the journal's `ExecutionStatus`:
/// `None` (success) ⇒ `Done`, a `Cancelled` error ⇒ `Canceled`, anything else ⇒
/// `Failed`. A failed / canceled op still finalizes and stays rollbackable for
/// what it reached (D4). The caller passes `result.as_ref().err()` (or, for a
/// `WriteFailure`, `.map(|f| &f.error)`).
pub(super) fn execution_status_from_error(err: Option<&WriteOperationError>) -> ExecutionStatus {
    match err {
        None => ExecutionStatus::Done,
        Some(WriteOperationError::Cancelled { .. }) => ExecutionStatus::Canceled,
        Some(_) => ExecutionStatus::Failed,
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
/// `not_rollbackable` until finalize computes eligibility). `item_count` is the
/// provisional planned total (the top-level source count, known before the scan);
/// `finalize_op` refines it to the scanned total. `dest_volume_id` is `Some("root")`
/// for a local copy/move (the destination is on the local FS) and `None` for a
/// delete/trash (no destination volume).
pub(super) fn open_local_op(
    op_id: &str,
    kind: OpKind,
    initiator: Initiator,
    item_count: u64,
    dest_volume_id: Option<&str>,
) {
    journal_open(OpenOperation {
        op_id: op_id.to_string(),
        kind,
        initiator,
        source_volume_id: Some(DEFAULT_VOLUME_ID.to_string()),
        dest_volume_id: dest_volume_id.map(str::to_string),
        item_count,
        started_at: now_secs(),
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    });
}

/// Open a volume (SMB / MTP / local) managed op in the journal, carrying the REAL
/// source and dest volume ids — the volume-aware sibling of [`open_local_op`],
/// which bakes in `"root"`. A same-volume move passes the one id as both.
pub(super) fn open_volume_op(
    op_id: &str,
    kind: OpKind,
    initiator: Initiator,
    source_volume_id: &str,
    dest_volume_id: Option<&str>,
    item_count: u64,
) {
    journal_open(OpenOperation {
        op_id: op_id.to_string(),
        kind,
        initiator,
        source_volume_id: Some(source_volume_id.to_string()),
        dest_volume_id: dest_volume_id.map(str::to_string),
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
    record_row(
        op_id,
        entry_type,
        RowRole::RollbackUnit,
        DEFAULT_VOLUME_ID,
        source,
        dest.map(|d| (DEFAULT_VOLUME_ID, d)),
        size,
        mtime,
        overwrote,
        outcome,
    );
}

/// Record one `rollback_unit` row on a real (SMB / MTP / local) volume: the
/// volume-aware sibling of [`record_local_leaf`]. `source` lives on
/// `source_volume_id`; `dest` bundles the destination volume id + path (they may
/// differ from the source on a cross-volume copy / move). The local helpers bake
/// in `"root"`; the volume paths must pass the REAL ids so a volume op's rows
/// never journal under `"root"` (the honesty invariant — a wrong volume id would
/// corrupt history silently).
#[allow(clippy::too_many_arguments, reason = "the natural fields of a journal row")]
pub(super) fn record_volume_leaf(
    op_id: &str,
    entry_type: EntryType,
    source_volume_id: &str,
    source: &Path,
    dest: Option<(&str, &Path)>,
    size: Option<i64>,
    mtime: Option<i64>,
    overwrote: bool,
    outcome: ItemOutcome,
) {
    record_row(
        op_id,
        entry_type,
        RowRole::RollbackUnit,
        source_volume_id,
        source,
        dest,
        size,
        mtime,
        overwrote,
        outcome,
    );
}

/// Record the per-file `rollback_unit` rows a volume copy / cross-volume move
/// landed for ONE top-level source. A FILE source records one leaf (`created_files`
/// empty, `dest_root` = the landed dest path, `file_size` known). A DIRECTORY
/// source records one leaf per `created_files` entry (a dest path under
/// `dest_root`), its source rebased onto `source_root` from the tail under
/// `dest_root`; per-inner-file size isn't cheaply available here, so it records
/// `None`. `overwrote` applies to the whole source's leaves — op-wide rollback
/// eligibility is the OR of these, so marking a source that overwrote anything is
/// honest (deleting the copies can't restore an overwritten original). The created
/// directories are journaled separately, after all files, via
/// [`record_created_dirs_on`].
#[allow(clippy::too_many_arguments, reason = "the natural fields of a volume transfer")]
pub(super) fn record_volume_transfer_source(
    op_id: &str,
    source_volume_id: &str,
    source_root: &Path,
    dest_volume_id: &str,
    dest_root: &Path,
    source_is_dir: bool,
    created_files: &[std::path::PathBuf],
    file_size: Option<i64>,
    overwrote: bool,
) {
    if source_is_dir {
        for dest_leaf in created_files {
            let rel = dest_leaf.strip_prefix(dest_root).unwrap_or(dest_leaf);
            let source_leaf = source_root.join(rel);
            record_volume_leaf(
                op_id,
                EntryType::File,
                source_volume_id,
                &source_leaf,
                Some((dest_volume_id, dest_leaf)),
                None,
                None,
                overwrote,
                ItemOutcome::Done,
            );
        }
    } else {
        record_volume_leaf(
            op_id,
            EntryType::File,
            source_volume_id,
            source_root,
            Some((dest_volume_id, dest_root)),
            file_size,
            None,
            overwrote,
            ItemOutcome::Done,
        );
    }
}

/// Record one `search_only` row (a leaf beneath a trashed / same-FS-moved
/// top-level unit — searchable but never a reversal unit, D-granularity).
/// Volume-aware: `source_volume_id` and the optional `dest` volume carry the real
/// ids (the local callers pass `"root"`).
#[allow(clippy::too_many_arguments, reason = "the natural fields of a journal row")]
pub(super) fn record_search_leaf(
    op_id: &str,
    entry_type: EntryType,
    source_volume_id: &str,
    source: &Path,
    dest: Option<(&str, &Path)>,
    size: Option<i64>,
    mtime: Option<i64>,
) {
    record_row(
        op_id,
        entry_type,
        RowRole::SearchOnly,
        source_volume_id,
        source,
        dest,
        size,
        mtime,
        false,
        ItemOutcome::Done,
    );
}

/// The shared record core: split the source (and optional dest) into interned dir
/// prefix + leaf name, carry the explicit volume ids, and hand one [`JournalItem`]
/// to the writer. `seq` is assigned by the journal in recording order.
#[allow(clippy::too_many_arguments, reason = "the natural fields of a journal row")]
fn record_row(
    op_id: &str,
    entry_type: EntryType,
    row_role: RowRole,
    source_volume_id: &str,
    source: &Path,
    dest: Option<(&str, &Path)>,
    size: Option<i64>,
    mtime: Option<i64>,
    overwrote: bool,
    outcome: ItemOutcome,
) {
    let (source_dir, source_name) = local_split(source);
    let (dest_volume_id, dest_dir, dest_name) = match dest {
        Some((vol, d)) => {
            let (dd, dn) = local_split(d);
            (Some(vol.to_string()), Some(dd), Some(dn))
        }
        None => (None, None, None),
    };
    journal_record_items(
        op_id,
        vec![JournalItem {
            seq: 0,
            entry_type,
            row_role,
            source_volume_id: source_volume_id.to_string(),
            source_dir,
            source_name,
            dest_volume_id,
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
    record_created_dirs_on(op_id, DEFAULT_VOLUME_ID, dirs);
}

/// The volume-aware sibling of [`record_created_dirs`]: the created dirs live on
/// the destination volume `dest_volume_id` (may be SMB / MTP). Both the source
/// and dest of each row are the created path on that volume.
pub(super) fn record_created_dirs_on(op_id: &str, dest_volume_id: &str, dirs: &[std::path::PathBuf]) {
    for dir in dirs {
        record_row(
            op_id,
            EntryType::Dir,
            RowRole::RollbackUnit,
            dest_volume_id,
            dir,
            Some((dest_volume_id, dir)),
            None,
            None,
            false,
            ItemOutcome::Done,
        );
    }
}

/// Journal the terminal state of a `run_instant` create (mkdir / mkfile) on
/// volume `volume_id` (`"root"` for the local drive). On success the created path
/// is a net-new item (source == dest, so the M3 rollback removes it if still
/// empty/unchanged); on failure the op finalizes `failed` with no item. The
/// matching open call must have used the same `op_id`.
pub(super) fn journal_instant_create(
    op_id: &str,
    kind: OpKind,
    entry_type: EntryType,
    volume_id: &str,
    created: Option<&Path>,
) {
    match created {
        Some(path) => {
            record_volume_leaf(
                op_id,
                entry_type,
                volume_id,
                path,
                Some((volume_id, path)),
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

/// The op's terminal header aggregates — planned total, completed items, and
/// total bytes — read from the live status cache the queue UI drives. Returned as
/// `(item_count, items_done, bytes_total)`; `item_count` is `None` when no real
/// scanned total is available (an instant op, or a direct-call test that never
/// registered a status row), so finalize keeps the provisional open-time value.
fn header_totals_from_status(op_id: &str) -> (Option<u64>, u64, u64) {
    match super::state::get_operation_status(op_id) {
        Some(status) if status.files_total > 0 => (
            Some(status.files_total as u64),
            status.files_done as u64,
            status.bytes_total,
        ),
        _ => (None, 0, 0),
    }
}

/// Finalize a local-FS op with a non-archive terminal state. Archive ops
/// (compress) finalize through [`finalize_archive_op`] with the driver's subkind.
/// The header aggregates are refined from the status cache (M6 rider) so the alpha
/// dialog renders a real "Copy N items" instead of zero.
pub(super) fn finalize_op(op_id: &str, kind: OpKind, execution_status: ExecutionStatus) {
    let (item_count, items_done, bytes_total) = header_totals_from_status(op_id);
    journal_finalize(
        op_id,
        FinalizeInputs {
            execution_status,
            kind,
            archive_subkind: None,
            net_new: false,
            ended_at: now_secs(),
            item_count,
            items_done,
            bytes_total,
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
            // A compress produces one archive; keep the open-time item_count.
            item_count: None,
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
