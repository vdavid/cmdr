//! `operation-log.db`: the durable, cross-volume journal store — schema, the
//! migration ladder, dir interning, app-side case folding, and the read
//! functions the dump bin and (later) the query API call.
//!
//! Unlike the drive index and `importance.db` (disposable per-volume caches that
//! delete-and-recreate on a schema bump), this DB lives for years, so it carries
//! two things nothing else here has: a forward-migration ladder
//! (the `migrations` submodule) and app-side case folding with NO custom collation, keeping
//! the file `sqlite3`-inspectable. Full rationale: `DETAILS.md`.
//!
//! One writer thread owns the single write connection
//! ([`OperationLogWriter`](super::writer)); reads go through short-lived
//! read-only connections. This handle ([`OperationLogStore`]) owns the schema
//! lifecycle (migrate on open; set aside and recreate only a genuinely
//! unparseable file) and offers direct reads for tests and the dump bin.

mod connection;
mod dirs;
mod migrations;
mod tree_indexes;

use std::path::{Path, PathBuf};

use cmdr_fs::entry::TagsChange;
use cmdr_fs::volume::NodeIdentity;
use rusqlite::{Connection, ErrorCode, OptionalExtension};
use unicode_normalization::UnicodeNormalization;

pub use connection::open_read_connection;
pub(crate) use connection::open_write_connection;
pub use dirs::reconstruct_dir_path;
pub(crate) use dirs::{find_dir, intern_child_dir, intern_dir};
pub use migrations::{MIGRATIONS, Migration, run_migrations};
pub(crate) use tree_indexes::{build_tree_item_indexes, tree_item_indexes_exist};

use super::types::{
    ArchiveSubkind, EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState,
    RowRole, SearchCoverage, SearchCoverageReason, SkipReason,
};

/// The durable journal's file name in the app data dir. Single (not per-volume):
/// a cross-volume operation is one operation with one identity.
pub(crate) const OPERATION_LOG_DB_FILE: &str = "operation-log.db";

/// Resolve the `operation-log.db` path inside `data_dir`.
pub fn operation_log_db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(OPERATION_LOG_DB_FILE)
}

/// Fold a name to its case-insensitive, normalized SEARCH key: Unicode lowercase,
/// then NFC. Done once in Rust at insert, so the DB needs no custom collation and
/// stays inspectable. Never an identity: the log can't know a volume's case
/// rules, and on a case-sensitive one two folded-equal names are two entries
/// (#511, #527).
pub fn fold_name(name: &str) -> String {
    name.to_lowercase().nfc().collect()
}

/// Errors from the operation-log store.
#[derive(Debug)]
pub enum OperationLogStoreError {
    /// SQLite refused a statement or an open.
    Sqlite(rusqlite::Error),
    /// The filesystem refused (creating the data dir, removing an unparseable file).
    Io(std::io::Error),
    /// The on-disk schema version is newer than this build's ladder (a
    /// downgrade). Refused, never destroyed — the newer DB may hold data this
    /// build can't represent.
    SchemaDowngrade {
        /// The version the file carries.
        found: u32,
        /// The highest version this build's ladder knows.
        expected: u32,
    },
    /// A stored classification token didn't parse to a known enum variant (a
    /// newer schema wrote it, or corruption). Carried for logging only; never
    /// branched on for control flow.
    Decode {
        /// The column the token came from.
        column: &'static str,
        /// The token as stored.
        value: String,
    },
}

impl std::fmt::Display for OperationLogStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationLogStoreError::Sqlite(e) => write!(f, "operation-log sqlite error: {e}"),
            OperationLogStoreError::Io(e) => write!(f, "operation-log io error: {e}"),
            OperationLogStoreError::SchemaDowngrade { found, expected } => {
                write!(
                    f,
                    "operation-log schema downgrade (found v{found}, this build expects v{expected})"
                )
            }
            OperationLogStoreError::Decode { column, value } => {
                write!(f, "operation-log undecodable token in {column}: {value:?}")
            }
        }
    }
}

impl std::error::Error for OperationLogStoreError {}

impl From<rusqlite::Error> for OperationLogStoreError {
    fn from(e: rusqlite::Error) -> Self {
        OperationLogStoreError::Sqlite(e)
    }
}

impl From<std::io::Error> for OperationLogStoreError {
    fn from(e: std::io::Error) -> Self {
        OperationLogStoreError::Io(e)
    }
}

/// Is this a file we can never parse as a SQLite DB (garbage bytes, corrupt
/// header)? The one case where set-aside-and-recreate is the right move — matched on
/// the typed sqlite error code, never a message string.
fn is_unparseable(err: &OperationLogStoreError) -> bool {
    matches!(
        err,
        OperationLogStoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _))
            if e.code == ErrorCode::NotADatabase || e.code == ErrorCode::DatabaseCorrupt
    )
}

/// A read of one `operations` row, tokens decoded to typed enums. Doubles as the
/// IPC/MCP summary wire type (the query API returns it directly): it carries no
/// interned `dir_id`s, only volume ids and typed fields the frontend can render,
/// so it's safe to serialize. Item rows are NOT (their dir prefixes are ids); the
/// query layer resolves those to paths in a separate view type.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub struct OperationRow {
    pub op_id: String,
    pub kind: OpKind,
    pub archive_subkind: Option<ArchiveSubkind>,
    pub initiator: Initiator,
    pub execution_status: ExecutionStatus,
    pub rollback_state: RollbackState,
    pub not_rollbackable_reason: Option<NotRollbackableReason>,
    pub rolls_back_op_id: Option<String>,
    /// The newest operation that rolls THIS one back, the mirror of
    /// [`Self::rolls_back_op_id`]. `None` until something reverses this operation.
    ///
    /// Not a stored column: every reader resolves it through the partial
    /// `operations_rolls_back` index right after mapping (see
    /// [`fill_inverse_op_ids`]), so no reader can hand out a `None` that means
    /// "I didn't look". While the row reads `rolling_back` this is the id of the
    /// live reversal, which is how the history dialog offers it Pause and Cancel.
    pub inverse_op_id: Option<String>,
    pub source_volume_id: Option<String>,
    pub dest_volume_id: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub item_count: u64,
    pub items_done: u64,
    pub bytes_total: u64,
    pub search_coverage: SearchCoverage,
    pub search_coverage_reason: Option<SearchCoverageReason>,
    pub dev_summary: Option<String>,
}

/// A read of one `operation_items` row. Dir prefixes are ids; call
/// [`reconstruct_dir_path`] to render a full path.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationItemRow {
    /// The row's own key.
    pub item_id: i64,
    /// The operation the item belongs to.
    pub op_id: String,
    /// The item's position within its operation.
    pub seq: i64,
    /// File, directory, or symlink.
    pub entry_type: EntryType,
    /// Whether rollback reverses this row or search only finds it.
    pub row_role: RowRole,
    /// The interned parent directory of the source item.
    pub source_dir_id: i64,
    /// The source item's leaf name.
    pub source_name: String,
    /// The interned parent directory of where the item landed.
    pub dest_dir_id: Option<i64>,
    /// The leaf name it landed under.
    pub dest_name: Option<String>,
    /// The size snapshot, when recorded.
    pub size: Option<i64>,
    /// The mtime snapshot, when recorded.
    pub mtime: Option<i64>,
    /// What happened to the item.
    pub outcome: ItemOutcome,
    /// Whether landing it replaced an existing entry.
    pub overwrote: bool,
    /// Why a rollback left this item alone, when one did. `None` means the reason
    /// wasn't recorded: a row written before the column existed, or an outcome no
    /// rollback produced. Never defaulted — see [`SkipReason`].
    pub rollback_skip_reason: Option<SkipReason>,
    /// Which node the item landed as at its dest, when that was recorded (a trash,
    /// from schema v4 on).
    pub dest_identity: Option<NodeIdentity>,
    /// The Finder-tags value before and after, on a tag write's row (schema v5 on).
    pub tags: Option<TagsChange>,
    /// A DIR row's own folder on the operation's tree axis (schema v6 on), interned
    /// by the writer. `None` on a FILE row and on a DIR row written before v6.
    pub self_dir_id: Option<i64>,
}

/// A handle to `operation-log.db`, owning a connection for direct reads and the
/// schema lifecycle. The writer thread opens its OWN write connection.
pub struct OperationLogStore {
    #[cfg_attr(not(test), allow(dead_code, reason = "read only by the test-gated accessors"))]
    conn: Connection,
}

impl OperationLogStore {
    /// Open (or create) the DB, migrating the schema forward to the current
    /// version. A downgrade is refused (returned as an error, DB untouched); a
    /// genuinely unparseable file is handed to `set_aside` and recreated fresh;
    /// a transient error (IO) propagates without destroying anything.
    ///
    /// `set_aside` is what the host does with the unparseable file first: keeps
    /// it, answering where, or `None` when it couldn't (or keeps nothing: tests
    /// and tools pass `&|_| None`), and the store then deletes it. The app's
    /// answer keeps a copy and tells the person.
    pub fn open(db_path: &Path, set_aside: &dyn Fn(&Path) -> Option<PathBuf>) -> Result<Self, OperationLogStoreError> {
        match Self::try_open(db_path) {
            Ok(store) => Ok(store),
            Err(OperationLogStoreError::SchemaDowngrade { found, expected }) => {
                log::warn!(
                    target: "operation_log",
                    "operation-log DB is newer than this build (found v{found}, expected v{expected}); leaving it untouched"
                );
                Err(OperationLogStoreError::SchemaDowngrade { found, expected })
            }
            Err(e) if is_unparseable(&e) => {
                log::warn!(target: "operation_log", "operation-log DB is unparseable ({e}); setting it aside and recreating");
                Self::set_aside_and_recreate(db_path, set_aside)
            }
            Err(e) => Err(e),
        }
    }

    fn try_open(db_path: &Path) -> Result<Self, OperationLogStoreError> {
        let conn = open_write_connection(db_path)?;
        Ok(Self { conn })
    }

    fn set_aside_and_recreate(
        db_path: &Path,
        set_aside: &dyn Fn(&Path) -> Option<PathBuf>,
    ) -> Result<Self, OperationLogStoreError> {
        // Kept beside the new DB when the host keeps it (and the person hears it
        // reset). A rename that doesn't happen falls back to deleting, the one way
        // forward.
        if db_path.exists() && set_aside(db_path).is_none() {
            std::fs::remove_file(db_path)?;
        }
        for sidecar in [db_path.with_extension("db-wal"), db_path.with_extension("db-shm")] {
            if sidecar.exists() {
                let _ = std::fs::remove_file(&sidecar);
            }
        }
        Self::try_open(db_path)
    }

    /// Borrow the connection for direct reads.
    #[cfg(test)]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The current on-disk schema version.
    #[cfg(test)]
    pub fn schema_version(&self) -> Result<u32, OperationLogStoreError> {
        migrations::read_schema_version(&self.conn)
    }
}

// ── Reads (used by the dump bin; the query API adds paged/filtered reads) ──

/// Decode a stored token to a typed enum, or a [`OperationLogStoreError::Decode`].
pub(super) fn decode<T>(
    value: String,
    parse: impl Fn(&str) -> Option<T>,
    column: &'static str,
) -> Result<T, OperationLogStoreError> {
    match parse(&value) {
        Some(v) => Ok(v),
        None => Err(OperationLogStoreError::Decode { column, value }),
    }
}

/// Decode an optional stored token (NULL ⇒ `None`).
fn decode_opt<T>(
    value: Option<String>,
    parse: impl Fn(&str) -> Option<T>,
    column: &'static str,
) -> Result<Option<T>, OperationLogStoreError> {
    match value {
        Some(v) => decode(v, parse, column).map(Some),
        None => Ok(None),
    }
}

pub(super) const OPERATION_COLUMNS: &str = "op_id, kind, archive_subkind, initiator, execution_status, rollback_state, \
     not_rollbackable_reason, rolls_back_op_id, source_volume_id, dest_volume_id, started_at, ended_at, \
     item_count, items_done, bytes_total, search_coverage, search_coverage_reason, dev_summary";

pub(super) fn map_operation_row(row: &rusqlite::Row<'_>) -> Result<OperationRow, OperationLogStoreError> {
    Ok(OperationRow {
        op_id: row.get(0)?,
        kind: decode(row.get(1)?, OpKind::from_token, "kind")?,
        archive_subkind: decode_opt(row.get(2)?, ArchiveSubkind::from_token, "archive_subkind")?,
        initiator: decode(row.get(3)?, Initiator::from_token, "initiator")?,
        execution_status: decode(row.get(4)?, ExecutionStatus::from_token, "execution_status")?,
        rollback_state: decode(row.get(5)?, RollbackState::from_token, "rollback_state")?,
        not_rollbackable_reason: decode_opt(
            row.get(6)?,
            NotRollbackableReason::from_token,
            "not_rollbackable_reason",
        )?,
        rolls_back_op_id: row.get(7)?,
        // Resolved by `fill_inverse_op_ids` once the query has drained: the mapper
        // holds a `Row`, not the connection it came from.
        inverse_op_id: None,
        source_volume_id: row.get(8)?,
        dest_volume_id: row.get(9)?,
        started_at: row.get(10)?,
        ended_at: row.get(11)?,
        item_count: row.get::<_, i64>(12)? as u64,
        items_done: row.get::<_, i64>(13)? as u64,
        bytes_total: row.get::<_, i64>(14)? as u64,
        search_coverage: decode(row.get(15)?, SearchCoverage::from_token, "search_coverage")?,
        search_coverage_reason: decode_opt(row.get(16)?, SearchCoverageReason::from_token, "search_coverage_reason")?,
        dev_summary: row.get(17)?,
    })
}

/// Fill [`OperationRow::inverse_op_id`] on rows that have already been mapped.
///
/// **Call this from every reader that hands an `OperationRow` out.** The field is
/// the only one the mapper can't populate (it needs the connection, and the
/// query it belongs to is still draining the one statement cache at that point),
/// so leaving it out is how a caller would learn "nothing reverses this" from a
/// reader that simply never asked.
///
/// One prepared statement reused across the batch, each probe served by the
/// partial `operations_rolls_back` index. Newest wins, matching
/// [`read_inverse_op`]: an operation reversed, stopped midway, and reversed again
/// has several inverses, and the live one is the last.
pub(super) fn fill_inverse_op_ids(conn: &Connection, rows: &mut [OperationRow]) -> Result<(), OperationLogStoreError> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut stmt = conn.prepare_cached(
        "SELECT op_id FROM operations WHERE rolls_back_op_id = ?1 ORDER BY started_at DESC, op_id DESC LIMIT 1",
    )?;
    for row in rows.iter_mut() {
        row.inverse_op_id = stmt
            .query_row(rusqlite::params![row.op_id], |r| r.get::<_, String>(0))
            .optional()?;
    }
    Ok(())
}

/// Read one operation header by id, or `None` if absent.
pub fn read_operation(conn: &Connection, op_id: &str) -> Result<Option<OperationRow>, OperationLogStoreError> {
    let sql = format!("SELECT {OPERATION_COLUMNS} FROM operations WHERE op_id = ?1");
    let mut out = {
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(rusqlite::params![op_id])?;
        match rows.next()? {
            Some(row) => vec![map_operation_row(row)?],
            None => return Ok(None),
        }
    };
    fill_inverse_op_ids(conn, &mut out)?;
    Ok(out.pop())
}

/// The most recent operations by start time (newest first), including
/// unfinished ones.
pub fn recent_operations(conn: &Connection, limit: u32) -> Result<Vec<OperationRow>, OperationLogStoreError> {
    let sql = format!("SELECT {OPERATION_COLUMNS} FROM operations ORDER BY started_at DESC, op_id DESC LIMIT ?1");
    let mut out = Vec::new();
    {
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(rusqlite::params![limit])?;
        while let Some(row) = rows.next()? {
            out.push(map_operation_row(row)?);
        }
    }
    fill_inverse_op_ids(conn, &mut out)?;
    Ok(out)
}

pub(super) const ITEM_COLUMNS: &str = "item_id, op_id, seq, entry_type, row_role, source_dir_id, source_name, dest_dir_id, \
     dest_name, size, mtime, outcome, overwrote, rollback_skip_reason, dest_inode, dest_birth_ns, \
     tags_before, tags_after, self_dir_id";

pub(super) fn map_item_row(row: &rusqlite::Row<'_>) -> Result<OperationItemRow, OperationLogStoreError> {
    Ok(OperationItemRow {
        item_id: row.get(0)?,
        op_id: row.get(1)?,
        seq: row.get(2)?,
        entry_type: decode(row.get(3)?, EntryType::from_token, "entry_type")?,
        row_role: decode(row.get(4)?, RowRole::from_token, "row_role")?,
        source_dir_id: row.get(5)?,
        source_name: row.get(6)?,
        dest_dir_id: row.get(7)?,
        dest_name: row.get(8)?,
        size: row.get(9)?,
        mtime: row.get(10)?,
        outcome: decode(row.get(11)?, ItemOutcome::from_token, "outcome")?,
        overwrote: row.get::<_, i64>(12)? != 0,
        rollback_skip_reason: decode_opt(row.get(13)?, SkipReason::from_token, "rollback_skip_reason")?,
        dest_identity: match (row.get::<_, Option<i64>>(14)?, row.get::<_, Option<i64>>(15)?) {
            (Some(inode), Some(birth_nanos)) => Some(NodeIdentity {
                inode: inode as u64,
                birth_nanos,
            }),
            _ => None,
        },
        tags: match (row.get::<_, Option<Vec<u8>>>(16)?, row.get::<_, Option<Vec<u8>>>(17)?) {
            (Some(before), Some(after)) => Some(TagsChange { before, after }),
            _ => None,
        },
        self_dir_id: row.get(18)?,
    })
}

/// Read an operation's items in `seq` order (ascending), up to `limit`.
pub fn read_operation_items(
    conn: &Connection,
    op_id: &str,
    limit: u32,
) -> Result<Vec<OperationItemRow>, OperationLogStoreError> {
    let sql = format!("SELECT {ITEM_COLUMNS} FROM operation_items WHERE op_id = ?1 ORDER BY seq ASC LIMIT ?2");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![op_id, limit])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_item_row(row)?);
    }
    Ok(out)
}

// ── Rollback reads ──────────────────────────────────────────────────────

/// One `rollback_unit` row with its interned dir prefixes resolved to full paths
/// and real volume ids — everything the rollback engine needs to reverse the
/// item without a second query per row. `source` is the item's original location;
/// `dest` (present for copy/move/trash/rename) is where the item ended up (the
/// thing a restore-move brings back, or a remove-inverse deletes).
#[derive(Debug, Clone, PartialEq)]
pub struct RollbackUnit {
    /// The item's position within its operation.
    pub seq: i64,
    /// File, directory, or symlink.
    pub entry_type: EntryType,
    /// The volume of the item's original location.
    pub source_volume_id: String,
    /// The item's original location.
    pub source_path: PathBuf,
    /// The volume it ended up on.
    pub dest_volume_id: Option<String>,
    /// Where it ended up.
    pub dest_path: Option<PathBuf>,
    /// The size snapshot the reversal verifies against.
    pub size: Option<i64>,
    /// The mtime snapshot the reversal verifies against.
    pub mtime: Option<i64>,
    /// Whether landing it replaced an existing entry.
    pub overwrote: bool,
    /// What happened to the item.
    pub outcome: ItemOutcome,
    /// Why an earlier rollback held the item back, when one did. Present only
    /// alongside [`ItemOutcome::Skipped`].
    pub rollback_skip_reason: Option<SkipReason>,
    /// Which node the item landed as at `dest_path`, when that was recorded. A
    /// restore moves back only that node, never a stranger under its name.
    pub dest_identity: Option<NodeIdentity>,
    /// A tag write's value before and after. The undo writes `before` back only
    /// while the file still reads `after`.
    pub tags: Option<TagsChange>,
}

impl RollbackUnit {
    /// Whether a reversal still owes this item an attempt: the operation landed it
    /// (`done`), or an earlier rollback held it back (`skipped` WITH a rollback
    /// verdict). A `skipped` row without one is the operation's own skip, a log entry
    /// that never moved anything. The in-code twin of `AWAITS_REVERSAL_SQL`.
    pub(crate) fn awaits_reversal(&self) -> bool {
        awaits_reversal(self.outcome, self.rollback_skip_reason)
    }
}

/// [`RollbackUnit::awaits_reversal`] over a row's two fields, for readers holding a
/// raw row (the tree's row roles).
pub(crate) fn awaits_reversal(outcome: ItemOutcome, skip_reason: Option<SkipReason>) -> bool {
    match outcome {
        ItemOutcome::Done => true,
        ItemOutcome::Skipped => skip_reason.is_some(),
        ItemOutcome::Failed | ItemOutcome::RolledBack => false,
    }
}

/// The rows a reversal streams, as SQL over `operation_items` (binds `?3` = `done`,
/// `?4` = `skipped`): see [`RollbackUnit::awaits_reversal`]. Retrying a held-back
/// row is what lets "Roll back the rest" pick a partial rollback back up; the per-item recheck
/// keeps that idempotent. A held-back row recorded before the journal kept a reason
/// (pre-v2) reads like the operation's own skip and isn't retried, which is why an
/// empty pass over a partial op stays partial (`rollback.rs`, `resolve_final_state`).
const AWAITS_REVERSAL_SQL: &str = "(outcome = ?3 OR (outcome = ?4 AND rollback_skip_reason IS NOT NULL))";

/// Resolve an interned `dir_id` to `(volume_id, full path)`. The path is
/// [`reconstruct_dir_path`]; the volume id is read from the dir row itself (every
/// dir in a chain shares the volume, so the leaf's row is enough).
pub(super) fn resolve_dir(conn: &Connection, dir_id: i64) -> Result<(String, String), OperationLogStoreError> {
    let volume_id: String = conn
        .prepare_cached("SELECT volume_id FROM dirs WHERE dir_id = ?1")?
        .query_row(rusqlite::params![dir_id], |row| row.get(0))?;
    let path = reconstruct_dir_path(conn, dir_id)?;
    Ok((volume_id, path))
}

/// Join a resolved dir path with a leaf name into a full path. The root renders as
/// `/`, so `join_leaf("/", "a")` is `/a` and `join_leaf("/a/b", "c")` is `/a/b/c`.
pub(super) fn join_leaf(dir_path: &str, name: &str) -> PathBuf {
    PathBuf::from(dir_path).join(name)
}

/// Resolve one raw row's interned dirs into the [`RollbackUnit`] the engine acts on.
/// Two dir resolutions per row, so the engine calls it only for a row it will act on.
pub(crate) fn rollback_unit_of(
    conn: &Connection,
    item: &OperationItemRow,
) -> Result<RollbackUnit, OperationLogStoreError> {
    let (source_volume_id, source_dir) = resolve_dir(conn, item.source_dir_id)?;
    let source_path = join_leaf(&source_dir, &item.source_name);
    let (dest_volume_id, dest_path) = match (item.dest_dir_id, &item.dest_name) {
        (Some(dir_id), Some(name)) => {
            let (vol, dir) = resolve_dir(conn, dir_id)?;
            (Some(vol), Some(join_leaf(&dir, name)))
        }
        _ => (None, None),
    };
    Ok(RollbackUnit {
        seq: item.seq,
        entry_type: item.entry_type,
        source_volume_id,
        source_path,
        dest_volume_id,
        dest_path,
        size: item.size,
        mtime: item.mtime,
        overwrote: item.overwrote,
        outcome: item.outcome,
        rollback_skip_reason: item.rollback_skip_reason,
        dest_identity: item.dest_identity,
        tags: item.tags.clone(),
    })
}

/// A page of `rollback_unit` rows for an op, newest-`seq`-first (reverse order, so
/// a rollback undoes in inverse order and removes created files before the
/// `entry_type = dir` rows that held them). Streaming: pass `i64::MAX` for the
/// first page, then the smallest `seq` of the returned page (exclusive) as
/// `before_seq` for the next — so the engine never materializes a 1M-row op.
/// Only rows that [await reversal](RollbackUnit::awaits_reversal) come back;
/// `search_only` rows are excluded (they're for search, never reversal).
pub fn read_rollback_units_page(
    conn: &Connection,
    op_id: &str,
    before_seq: i64,
    limit: u32,
) -> Result<Vec<RollbackUnit>, OperationLogStoreError> {
    read_rollback_rows_page(conn, op_id, before_seq, limit)?
        .iter()
        .map(|item| rollback_unit_of(conn, item))
        .collect()
}

/// [`read_rollback_units_page`]'s rows before their dirs are resolved: a selective
/// rollback decides each row on these, so a row its selection leaves out never pays
/// for a path ([`rollback_unit_of`] resolves the ones it keeps). Page by the raw
/// rows' smallest `seq`, never the kept ones', or a page that keeps nothing would
/// read again forever.
pub(crate) fn read_rollback_rows_page(
    conn: &Connection,
    op_id: &str,
    before_seq: i64,
    limit: u32,
) -> Result<Vec<OperationItemRow>, OperationLogStoreError> {
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM operation_items \
         WHERE op_id = ?1 AND row_role = ?2 AND {AWAITS_REVERSAL_SQL} AND seq < ?5 \
         ORDER BY seq DESC LIMIT ?6"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![
        op_id,
        RowRole::RollbackUnit.as_token(),
        ItemOutcome::Done.as_token(),
        ItemOutcome::Skipped.as_token(),
        before_seq,
        limit
    ])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(map_item_row(row)?);
    }
    Ok(items)
}

/// What a reversal of `op_id` has ahead of it: how many rows it will credit on its
/// bar, and how many bytes they carry. One aggregate, so a reversal gets honest
/// totals without a scanning phase and without materializing the list
/// [`read_rollback_units_page`] pages through.
///
/// The filters mirror that page query exactly (same `row_role`, same outcomes), or
/// the bar would count rows the loop never visits. It counts every row whose
/// [`UnitRole`](crate::tree::UnitRole) is `Selectable`: each file, and each folder
/// the operation moved, trashed, or made on its own (a trashed folder is one unit
/// the sweep puts back, and a big trash can hold 100,000 of them). It leaves out the
/// folders a copy or move made around its files: the sweep removes those empty
/// leftovers, nothing a person is waiting on. Bytes are files' only, `MAX(size, 0)`,
/// since a folder's `size` is an inode length. The engine credits by the same role
/// (`rollback.rs::credits_progress`), and a selective reversal reads the same counts
/// off the tree summary (`units_selectable`, `unit_bytes_selectable`).
pub(crate) fn read_rollback_progress_totals(
    conn: &Connection,
    op_id: &str,
    kind: OpKind,
) -> Result<(u64, u64), OperationLogStoreError> {
    let sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN entry_type = ?5 THEN MAX(size, 0) ELSE 0 END), 0) \
         FROM operation_items WHERE op_id = ?1 AND row_role = ?2 AND {AWAITS_REVERSAL_SQL} \
         AND NOT (?6 AND entry_type = ?7 AND dest_dir_id IS source_dir_id AND dest_name IS source_name)"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let totals = stmt.query_row(
        rusqlite::params![
            op_id,
            RowRole::RollbackUnit.as_token(),
            ItemOutcome::Done.as_token(),
            ItemOutcome::Skipped.as_token(),
            EntryType::File.as_token(),
            crate::tree::creates_folders_around_files(kind),
            EntryType::Dir.as_token(),
        ],
        |row| Ok((row.get::<_, i64>(0)?.max(0) as u64, row.get::<_, i64>(1)?.max(0) as u64)),
    )?;
    Ok(totals)
}

/// One item row by its `seq`, `None` when the op has none there. Served by the
/// `(op_id, seq)` index.
pub(crate) fn read_item_row(
    conn: &Connection,
    op_id: &str,
    seq: i64,
) -> Result<Option<OperationItemRow>, OperationLogStoreError> {
    let sql = format!("SELECT {ITEM_COLUMNS} FROM operation_items WHERE op_id = ?1 AND seq = ?2");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![op_id, seq])?;
    match rows.next()? {
        Some(row) => Ok(Some(map_item_row(row)?)),
        None => Ok(None),
    }
}

/// [`has_rollback_outcome`]'s query (binds `?2` = `rolled_back`).
const HAS_ROLLBACK_OUTCOME_SQL: &str = "SELECT EXISTS (SELECT 1 FROM operation_items \
     WHERE op_id = ?1 AND (outcome = ?2 OR rollback_skip_reason IS NOT NULL))";

/// [`any_inverse_reversed`]'s query (binds `?2` = `done`).
const ANY_INVERSE_REVERSED_SQL: &str = "SELECT EXISTS (SELECT 1 FROM operations o \
     JOIN operation_items i ON i.op_id = o.op_id WHERE o.rolls_back_op_id = ?1 AND i.outcome = ?2)";

/// Whether a rollback pass already settled any of `op_id`'s rows: reversed one
/// (`rolled_back`) or held one back (a stored `rollback_skip_reason`). An `EXISTS`
/// that stops at the first hit; the row filter isn't index-served past `op_id`, so a
/// miss reads the whole op. The startup reconcile is its only caller, and only for an
/// op a crash left `rolling_back`.
pub(crate) fn has_rollback_outcome(conn: &Connection, op_id: &str) -> Result<bool, OperationLogStoreError> {
    let hit = conn
        .prepare_cached(HAS_ROLLBACK_OUTCOME_SQL)?
        .query_row(rusqlite::params![op_id, ItemOutcome::RolledBack.as_token()], |row| {
            row.get::<_, bool>(0)
        })?;
    Ok(hit)
}

/// Whether ANY inverse of `original_op_id` (every op whose `rolls_back_op_id` names
/// it, not only the newest) journaled a `done` row, meaning it durably reversed
/// something. An `EXISTS` over the `operations_rolls_back` index and each inverse's
/// rows, stopping at the first hit.
pub(crate) fn any_inverse_reversed(conn: &Connection, original_op_id: &str) -> Result<bool, OperationLogStoreError> {
    let hit = conn
        .prepare_cached(ANY_INVERSE_REVERSED_SQL)?
        .query_row(rusqlite::params![original_op_id, ItemOutcome::Done.as_token()], |row| {
            row.get::<_, bool>(0)
        })?;
    Ok(hit)
}

/// Every operation currently in `rolling_back`: the startup-reconcile input
/// (`rollback.rs`), each resolved from its own outcomes and every inverse's.
pub(crate) fn ops_in_rolling_back(conn: &Connection) -> Result<Vec<OperationRow>, OperationLogStoreError> {
    let sql = format!("SELECT {OPERATION_COLUMNS} FROM operations WHERE rollback_state = ?1");
    let mut out = Vec::new();
    {
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(rusqlite::params![RollbackState::RollingBack.as_token()])?;
        while let Some(row) = rows.next()? {
            out.push(map_operation_row(row)?);
        }
    }
    fill_inverse_op_ids(conn, &mut out)?;
    Ok(out)
}

/// Every operation still `queued` or `running`: at open, before any engine runs, each
/// one is an operation a crash interrupted (`interrupted.rs`). A scan of `operations`
/// (no index on the status; the table is small next to its items, and this runs once
/// per launch, as `ops_in_rolling_back` does).
pub(crate) fn ops_left_running(conn: &Connection) -> Result<Vec<OperationRow>, OperationLogStoreError> {
    let sql = format!("SELECT {OPERATION_COLUMNS} FROM operations WHERE execution_status IN (?1, ?2)");
    let mut out = Vec::new();
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![
        ExecutionStatus::Queued.as_token(),
        ExecutionStatus::Running.as_token()
    ])?;
    while let Some(row) = rows.next()? {
        out.push(map_operation_row(row)?);
    }
    Ok(out)
}

/// Whether any of `op_id`'s rows overwrote an existing file: the fact finalize turns
/// into `not_rollbackable(overwrote)`. An `EXISTS` that stops at the first hit; a miss
/// reads the op's rows, which only an interrupted op pays, once.
pub(crate) fn any_row_overwrote(conn: &Connection, op_id: &str) -> Result<bool, OperationLogStoreError> {
    let hit = conn
        .prepare_cached("SELECT EXISTS (SELECT 1 FROM operation_items WHERE op_id = ?1 AND overwrote = 1)")?
        .query_row(rusqlite::params![op_id], |row| row.get::<_, bool>(0))?;
    Ok(hit)
}

/// The most recent inverse operation reversing `original_op_id` (its
/// `rolls_back_op_id`), or `None` if none was ever opened. ❌ Not a reconcile input:
/// an op reversed in several passes has several inverses, and the startup reconcile
/// reads them all ([`any_inverse_reversed`]).
pub fn read_inverse_op(
    conn: &Connection,
    original_op_id: &str,
) -> Result<Option<OperationRow>, OperationLogStoreError> {
    let sql = format!(
        "SELECT {OPERATION_COLUMNS} FROM operations \
         WHERE rolls_back_op_id = ?1 ORDER BY started_at DESC, op_id DESC LIMIT 1"
    );
    let mut out = {
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(rusqlite::params![original_op_id])?;
        match rows.next()? {
            Some(row) => vec![map_operation_row(row)?],
            None => return Ok(None),
        }
    };
    fill_inverse_op_ids(conn, &mut out)?;
    Ok(out.pop())
}

#[cfg(test)]
mod migration_bench;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tree_migration_tests;
