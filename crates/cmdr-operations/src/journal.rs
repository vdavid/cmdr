//! Journaling glue for the write pipeline: brackets an op with the operation-log
//! open/finalize and builds per-item `JournalItem`s at the record points.
//!
//! Each operation's journal lives in its scope ([`OpScope`]), opened once from
//! the engine's `OperationLog` and fixed for the operation's life, so an
//! operation is wholly journaled or wholly not. Every function no-ops for an
//! operation that opened with nothing to journal to (or never opened), and never
//! fails the operation.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cmdr_fs::volume::{DEFAULT_VOLUME_ID, Volume};
use cmdr_operation_log::capture::FinalizeInputs;
use cmdr_operation_log::types::RowRole;
use cmdr_operation_log::types::{
    ArchiveSubkind, EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind,
};
use cmdr_operation_log::writer::{JournalItem, OpenOperation};

use super::ledger::WrittenFile;
use super::scope::OpScope;
use super::types::{WriteOperationError, WriteOperationType};
use cmdr_operation_log::types::{SearchCoverage, SearchCoverageReason};

/// Record item rows for the scope's operation (buffered and batched by the journal).
fn journal_record_items(scope: &OpScope, items: Vec<JournalItem>) {
    if let Some(journal) = scope.journal() {
        journal.record_items(items);
    }
}

/// Downgrade the scope's operation's search coverage (worst wins).
pub(super) fn note_coverage(scope: &OpScope, coverage: SearchCoverage, reason: Option<SearchCoverageReason>) {
    if let Some(journal) = scope.journal() {
        journal.note_coverage(coverage, reason);
    }
}

/// Finalize the scope's operation. The row counts it returns feed the
/// completeness check, which runs inside `cmdr_operation_log::capture::finalize`
/// where they're produced, so nothing here reads them.
fn journal_finalize(scope: &OpScope, inputs: FinalizeInputs) {
    if let Some(journal) = scope.journal() {
        // The row counts feed the completeness check inside `capture::finalize`; these wrappers only pick the kind.
        let _ = journal.clone().finalize(inputs);
    }
}

/// Map the pipeline's op type to the journal taxonomy (1:1). The `archive_edit`
/// subkind + net-new flag are supplied separately by the compress/zip driver
/// which the journal can't derive here.
pub(super) fn op_kind_of(t: WriteOperationType) -> OpKind {
    match t {
        WriteOperationType::Copy => OpKind::Copy,
        WriteOperationType::Move => OpKind::Move,
        WriteOperationType::Delete => OpKind::Delete,
        WriteOperationType::Trash => OpKind::Trash,
        WriteOperationType::Rename => OpKind::Rename,
        WriteOperationType::CreateFolder => OpKind::CreateFolder,
        WriteOperationType::CreateFile => OpKind::CreateFile,
        WriteOperationType::ArchiveEdit | WriteOperationType::Compress => OpKind::ArchiveEdit,
    }
}

/// Map a write op's terminal `Result` error into the journal's `ExecutionStatus`:
/// `None` (success) ⇒ `Done`, a `Cancelled` error ⇒ `Canceled`, anything else ⇒
/// `Failed`. A failed / canceled op still finalizes and stays rollbackable for
/// what it reached. The caller passes `result.as_ref().err()` (or, for a
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
    scope: &OpScope,
    op_id: &str,
    kind: OpKind,
    initiator: Initiator,
    item_count: u64,
    dest_volume_id: Option<&str>,
) {
    scope.open_journal(OpenOperation {
        op_id: op_id.into(),
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
    scope: &OpScope,
    op_id: &str,
    kind: OpKind,
    initiator: Initiator,
    source_volume_id: &str,
    dest_volume_id: Option<&str>,
    item_count: u64,
) {
    scope.open_journal(OpenOperation {
        op_id: op_id.into(),
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
    scope: &OpScope,
    entry_type: EntryType,
    source: &Path,
    dest: Option<&Path>,
    size: Option<i64>,
    mtime: Option<i64>,
    overwrote: bool,
    outcome: ItemOutcome,
) {
    record_row(
        scope,
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
    scope: &OpScope,
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
        scope,
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
/// `dest_root`, carrying the size the leaf was written with. Each leaf's mtime is
/// the landed file's own, from `landed` ([`LandedTimes`]), or `None` where it
/// wasn't read back. `overwrote` applies to the whole source's leaves — op-wide rollback
/// eligibility is the OR of these, so marking a source that overwrote anything is
/// honest (deleting the copies can't restore an overwritten original). The created
/// directories are journaled separately, after all files, via
/// [`record_created_dirs_on`].
#[allow(clippy::too_many_arguments, reason = "the natural fields of a volume transfer")]
pub(super) fn record_volume_transfer_source(
    scope: &OpScope,
    source_volume_id: &str,
    source_root: &Path,
    dest_volume_id: &str,
    dest_root: &Path,
    source_is_dir: bool,
    created_files: &[WrittenFile],
    file_size: Option<i64>,
    landed: &LandedTimes,
    overwrote: bool,
) {
    if source_is_dir {
        for leaf in created_files {
            let rel = leaf.path.strip_prefix(dest_root).unwrap_or(&leaf.path);
            let source_leaf = source_root.join(rel);
            record_volume_leaf(
                scope,
                EntryType::File,
                source_volume_id,
                &source_leaf,
                Some((dest_volume_id, &leaf.path)),
                leaf.identity.recorded_size().and_then(|size| i64::try_from(size).ok()),
                landed.of(&leaf.path),
                overwrote,
                ItemOutcome::Done,
            );
        }
    } else {
        record_volume_leaf(
            scope,
            EntryType::File,
            source_volume_id,
            source_root,
            Some((dest_volume_id, dest_root)),
            file_size,
            landed.of(dest_root),
            overwrote,
            ItemOutcome::Done,
        );
    }
}

/// The modified times a volume transfer's landed files report, read back from the
/// destination right after one top-level source lands, so each row's snapshot is
/// size AND mtime. Size alone passed a file edited in place at the same length (a
/// VM disk image, a SQLite database, a re-saved document), and undoing the copy
/// deleted it with the user's edits (#519).
///
/// The landed file's OWN time, never the source's: a destination that doesn't keep
/// the source's date (S3, WebDAV, an Android phone over MTP) would drift on every
/// file. Read through the same `Volume` the reversal rechecks with, so both sides
/// see one clock at one precision.
///
/// The cost is bounded per SOURCE, never per file: a FILE source asks one
/// `get_metadata`, a DIRECTORY source one `list_directory` per destination folder
/// its files landed in. Only a transfer that journals reads anything back, and a
/// stopped or failed source doesn't (`Default`, so a stop never waits on the share):
/// its rows keep the size-only check. A file the read-back can't place, or a backend
/// that reports no mtime, records `None`, which `verify_snapshot` skips over.
// DEFAULT-OK: empty means "nothing read back", and every lookup then answers `None`,
// the size-only check the row had before; it claims no time for any file.
#[derive(Debug, Default)]
pub(crate) struct LandedTimes(HashMap<PathBuf, i64>);

impl LandedTimes {
    /// Read back the landed times of one top-level source: `landed` for a FILE
    /// source, every `files` entry for a DIRECTORY source.
    pub(crate) async fn read_back(
        dest: &dyn Volume,
        landed: &Path,
        source_is_dir: bool,
        files: &[WrittenFile],
    ) -> Self {
        let mut times = HashMap::new();
        if !source_is_dir {
            if let Some(secs) = dest
                .get_metadata(landed)
                .await
                .ok()
                .and_then(|entry| entry.modified_at)
                .and_then(|secs| i64::try_from(secs).ok())
            {
                times.insert(landed.to_path_buf(), secs);
            }
            return Self(times);
        }
        // Each folder's landed files by name. A listed name is only ever matched
        // against these, never joined onto a path.
        let mut by_folder: BTreeMap<&Path, HashMap<&OsStr, &Path>> = BTreeMap::new();
        for file in files {
            if let (Some(folder), Some(name)) = (file.path.parent(), file.path.file_name()) {
                by_folder.entry(folder).or_default().insert(name, &file.path);
            }
        }
        for (folder, wanted) in by_folder {
            let Ok(entries) = dest.list_directory(folder, None).await else {
                continue;
            };
            for entry in entries.iter().filter(|entry| !entry.is_directory && !entry.is_symlink) {
                let Some(path) = wanted.get(OsStr::new(&entry.name)) else {
                    continue;
                };
                if let Some(secs) = entry.modified_at.and_then(|secs| i64::try_from(secs).ok()) {
                    times.insert(path.to_path_buf(), secs);
                }
            }
        }
        Self(times)
    }

    /// The landed mtime read back for `path`, if one was.
    pub(crate) fn of(&self, path: &Path) -> Option<i64> {
        self.0.get(path).copied()
    }
}

/// Record one `search_only` row (a leaf beneath a trashed / same-FS-moved
/// top-level unit — searchable but never a reversal unit).
/// Volume-aware: `source_volume_id` and the optional `dest` volume carry the real
/// ids (the local callers pass `"root"`).
#[allow(clippy::too_many_arguments, reason = "the natural fields of a journal row")]
pub(super) fn record_search_leaf(
    scope: &OpScope,
    entry_type: EntryType,
    source_volume_id: &str,
    source: &Path,
    dest: Option<(&str, &Path)>,
    size: Option<i64>,
    mtime: Option<i64>,
) {
    record_row(
        scope,
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
    scope: &OpScope,
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
        scope,
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

/// Record the directories a copy created as first-class `dir` rows. Called after
/// the leaf files are recorded, so the dir rows land AFTER their
/// contents in `seq`; the rollback removes files before their dirs. The
/// created path is both source and dest (a copy's rollback removes the dest dir
/// when empty; search matches its name).
pub(super) fn record_created_dirs(scope: &OpScope, dirs: &[PathBuf]) {
    record_created_dirs_on(scope, DEFAULT_VOLUME_ID, dirs);
}

/// The volume-aware sibling of [`record_created_dirs`]: the created dirs live on
/// the destination volume `dest_volume_id` (may be SMB / MTP). Both the source
/// and dest of each row are the created path on that volume.
pub(super) fn record_created_dirs_on(scope: &OpScope, dest_volume_id: &str, dirs: &[PathBuf]) {
    for dir in dirs {
        record_row(
            scope,
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
/// is a net-new item (source == dest, so the rollback removes it if still
/// empty/unchanged); on failure the op finalizes `failed` with no item. The
/// matching open call must have opened on the same `scope`.
pub(super) fn journal_instant_create(
    scope: &OpScope,
    kind: OpKind,
    entry_type: EntryType,
    volume_id: &str,
    created: Option<&Path>,
) {
    match created {
        Some(path) => {
            record_volume_leaf(
                scope,
                entry_type,
                volume_id,
                path,
                Some((volume_id, path)),
                None,
                None,
                false,
                ItemOutcome::Done,
            );
            finalize_op(scope, kind, ExecutionStatus::Done);
        }
        None => finalize_op(scope, kind, ExecutionStatus::Failed),
    }
}

/// Rule this op out of rollback with a typed reason the item rows can't carry —
/// the driver saw something `compute_eligibility` can't derive. Today: a
/// directory MERGE (its one row names a destination that also holds files the op
/// never touched, so reversing it would carry them away) and a cross-FS move
/// whose phase-3 conflict resolution landed the files somewhere other than the
/// destinations phase 2 journaled. No-ops when no journal is installed.
pub(super) fn note_not_rollbackable(scope: &OpScope, reason: NotRollbackableReason) {
    if let Some(journal) = scope.journal() {
        journal.note_not_rollbackable(reason);
    }
}

/// The op's terminal header aggregates — planned total, completed items, and
/// total bytes — read from the live status cache the queue UI drives. Returned as
/// `(item_count, items_done, bytes_total)`; `item_count` is `None` when no real
/// scanned total is available (an instant op, or a direct-call test that never
/// registered a status row), so finalize keeps the provisional open-time value.
fn header_totals_from_status(scope: &OpScope) -> (Option<u64>, u64, u64) {
    let status = scope
        .journal()
        .and_then(|journal| scope.status().snapshot(journal.op_id().as_str(), None));
    match status {
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
/// The header aggregates are refined from the status cache (the header-aggregate rider) so the alpha
/// dialog renders a real "Copy N items" instead of zero.
pub(super) fn finalize_op(scope: &OpScope, kind: OpKind, execution_status: ExecutionStatus) {
    let (item_count, items_done, bytes_total) = header_totals_from_status(scope);
    journal_finalize(
        scope,
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

/// What a fresh compress packed, for its header's aggregates: every entry it
/// planned (files and folders), how many it wrote, and the uncompressed source
/// bytes. The status cache can't answer this at finalize, because both finishing
/// phases clear their totals.
#[derive(Debug, Clone, Copy)]
pub(super) struct PackedTotals {
    pub entries: u64,
    pub entries_done: u64,
    pub source_bytes: u64,
}

/// Finalize an `archive_edit` op, carrying the driver-supplied subkind + net-new
/// flag into eligibility. `packed` is a compress's tally; an in-ZIP edit passes
/// `None` and keeps its open-time header.
pub(super) fn finalize_archive_op(
    scope: &OpScope,
    subkind: ArchiveSubkind,
    net_new: bool,
    execution_status: ExecutionStatus,
    packed: Option<PackedTotals>,
) {
    // A compress that stopped before its plan finished has no entry total yet;
    // `None` keeps the open-time count (its top-level sources).
    let item_count = packed.map(|totals| totals.entries).filter(|entries| *entries > 0);
    journal_finalize(
        scope,
        FinalizeInputs {
            execution_status,
            kind: OpKind::ArchiveEdit,
            archive_subkind: Some(subkind),
            net_new,
            ended_at: now_secs(),
            item_count,
            items_done: packed.map_or(0, |totals| totals.entries_done),
            bytes_total: packed.map_or(0, |totals| totals.source_bytes),
            dev_summary: None,
        },
    );
}

/// The journaling facts an archive-edit driver supplies that the generic pipeline
/// can't derive: the `archive_edit` subkind (compress vs zip-inner edit — both
/// cross IPC as `ArchiveEdit`), whether the archive was net-new (for
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
}

/// Open an archive-edit managed op in the journal. Unlike [`open_local_op`] this
/// records the parent (archive) volume, which may be remote (SMB / MTP). The
/// op opens `not_rollbackable` until [`finalize_archive_op`] computes eligibility
/// from the subkind + net-new flag.
pub(super) fn open_archive_op(scope: &OpScope, op_id: &str, initiator: Initiator, parent_volume_id: &str) {
    open_archive_op_between(scope, op_id, initiator, parent_volume_id, parent_volume_id, 0);
}

/// Open a fresh compress in the journal: the sources live on `source_volume_id`
/// and the archive lands on `parent_volume_id`, which are different volumes for a
/// Mac-to-phone compress. `item_count` is the provisional top-level source count;
/// [`finalize_archive_op`] refines it to what was packed.
pub(super) fn open_compress_op(
    scope: &OpScope,
    op_id: &str,
    initiator: Initiator,
    source_volume_id: &str,
    parent_volume_id: &str,
    item_count: u64,
) {
    open_archive_op_between(scope, op_id, initiator, source_volume_id, parent_volume_id, item_count);
}

fn open_archive_op_between(
    scope: &OpScope,
    op_id: &str,
    initiator: Initiator,
    source_volume_id: &str,
    dest_volume_id: &str,
    item_count: u64,
) {
    scope.open_journal(OpenOperation {
        op_id: op_id.into(),
        kind: OpKind::ArchiveEdit,
        initiator,
        source_volume_id: Some(source_volume_id.to_string()),
        dest_volume_id: Some(dest_volume_id.to_string()),
        item_count,
        started_at: now_secs(),
        rolls_back_op_id: None,
        execution_status: ExecutionStatus::Running,
    });
}

/// Record the archive a compress created as the single `rollback_unit` item: the
/// compress rollback deletes THIS archive if it's still net-new and unchanged
/// (the `size`/`mtime` snapshot is the drift check). The archive lives
/// on `parent_volume_id` (may be remote); `overwrote` is `!net_new`.
pub(super) fn record_compress_archive(
    scope: &OpScope,
    parent_volume_id: &str,
    archive: &Path,
    size: Option<i64>,
    mtime: Option<i64>,
    net_new: bool,
) {
    let (dir, name) = local_split(archive);
    journal_record_items(
        scope,
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
