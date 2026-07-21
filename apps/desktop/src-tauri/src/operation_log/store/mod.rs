//! `operation-log.db`: the durable, cross-volume journal store — schema, the
//! migration ladder, dir interning, app-side case folding, and the read
//! functions the dump bin and (later) the query API call.
//!
//! Unlike the drive index and `importance.db` (disposable per-volume caches that
//! delete-and-recreate on a schema bump), this DB lives for years, so it carries
//! two things nothing else here has: a forward-migration ladder
//! (the `migrations` submodule) and app-side case folding with NO custom collation, keeping
//! the file `sqlite3`-inspectable (D1/D2). Full rationale: `DETAILS.md`.
//!
//! One writer thread owns the single write connection
//! ([`OperationLogWriter`](super::writer)); reads go through short-lived
//! read-only connections. This handle ([`OperationLogStore`]) owns the schema
//! lifecycle (migrate on open; delete-and-recreate only a genuinely unparseable
//! file) and offers direct reads for tests and the dump bin.

mod connection;
mod migrations;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, ErrorCode};
use unicode_normalization::UnicodeNormalization;

pub use connection::open_read_connection;
pub(crate) use connection::open_write_connection;
pub use migrations::{MIGRATIONS, Migration, run_migrations};

use super::types::{
    ArchiveSubkind, EntryType, ExecutionStatus, Initiator, ItemOutcome, NotRollbackableReason, OpKind, RollbackState,
    RowRole, SearchCoverage, SearchCoverageReason,
};

/// The durable journal's file name in the app data dir. Single (not per-volume):
/// a cross-volume operation is one operation with one identity (D1).
pub const OPERATION_LOG_DB_FILE: &str = "operation-log.db";

/// Resolve the `operation-log.db` path inside `data_dir`.
pub fn operation_log_db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(OPERATION_LOG_DB_FILE)
}

/// Fold a leaf name to its case-insensitive, normalized search/identity key:
/// Unicode lowercase, then NFC. Done once in Rust at insert (D2), so the DB
/// needs no custom collation and stays inspectable. It's a *record* key, not a
/// live filesystem mirror, so it may differ slightly from a given filesystem's
/// exact case rules — acceptable for history.
pub fn fold_name(name: &str) -> String {
    name.to_lowercase().nfc().collect()
}

/// Errors from the operation-log store.
#[derive(Debug)]
pub enum OperationLogStoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    /// The on-disk schema version is newer than this build's ladder (a
    /// downgrade). Refused, never destroyed — the newer DB may hold data this
    /// build can't represent.
    SchemaDowngrade {
        found: u32,
        expected: u32,
    },
    /// A stored classification token didn't parse to a known enum variant (a
    /// newer schema wrote it, or corruption). Carried for logging only; never
    /// branched on for control flow.
    Decode {
        column: &'static str,
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
/// header)? The one case where delete-and-recreate is the right move — matched on
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
pub struct OperationRow {
    pub op_id: String,
    pub kind: OpKind,
    pub archive_subkind: Option<ArchiveSubkind>,
    pub initiator: Initiator,
    pub execution_status: ExecutionStatus,
    pub rollback_state: RollbackState,
    pub not_rollbackable_reason: Option<NotRollbackableReason>,
    pub rolls_back_op_id: Option<String>,
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
    pub item_id: i64,
    pub op_id: String,
    pub seq: i64,
    pub entry_type: EntryType,
    pub row_role: RowRole,
    pub source_dir_id: i64,
    pub source_name: String,
    pub dest_dir_id: Option<i64>,
    pub dest_name: Option<String>,
    pub size: Option<i64>,
    pub mtime: Option<i64>,
    pub outcome: ItemOutcome,
    pub overwrote: bool,
}

/// A handle to `operation-log.db`, owning a connection for direct reads and the
/// schema lifecycle. The writer thread opens its OWN write connection.
pub struct OperationLogStore {
    db_path: PathBuf,
    conn: Connection,
}

impl OperationLogStore {
    /// Open (or create) the DB, migrating the schema forward to the current
    /// version. A downgrade is refused (returned as an error, DB untouched); a
    /// genuinely unparseable file is deleted and recreated fresh; a transient
    /// error (IO) propagates without destroying anything.
    pub fn open(db_path: &Path) -> Result<Self, OperationLogStoreError> {
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
                log::warn!(target: "operation_log", "operation-log DB is unparseable ({e}); deleting and recreating");
                Self::delete_and_recreate(db_path)
            }
            Err(e) => Err(e),
        }
    }

    fn try_open(db_path: &Path) -> Result<Self, OperationLogStoreError> {
        let conn = open_write_connection(db_path)?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
            conn,
        })
    }

    fn delete_and_recreate(db_path: &Path) -> Result<Self, OperationLogStoreError> {
        if db_path.exists() {
            std::fs::remove_file(db_path)?;
        }
        for sidecar in [db_path.with_extension("db-wal"), db_path.with_extension("db-shm")] {
            if sidecar.exists() {
                let _ = std::fs::remove_file(&sidecar);
            }
        }
        Self::try_open(db_path)
    }

    /// The DB file path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Borrow the connection for direct reads (tests, the dump bin path).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The current on-disk schema version.
    pub fn schema_version(&self) -> Result<u32, OperationLogStoreError> {
        migrations::read_schema_version(&self.conn)
    }
}

// ── Dir interning ────────────────────────────────────────────────────────────

/// Intern a directory path for `volume_id`, returning its `dir_id`. Interns each
/// ancestor once (reused across operations, so a hot directory is stored once
/// forever), then returns the leaf's id. A volume root (empty path) interns a
/// single row with name `""` and NULL parent.
///
/// Runs on a write-capable connection — inside the writer thread's transaction
/// in production, or directly in tests. Idempotent: the same `(volume_id, path)`
/// always returns the same id.
pub fn intern_dir(conn: &Connection, volume_id: &str, path: &str) -> Result<i64, OperationLogStoreError> {
    // The volume root anchors the chain (name ""), so a file directly at the
    // volume root still has a dir to reference.
    let mut parent = intern_one(conn, volume_id, None, "")?;
    for component in path.split('/').filter(|c| !c.is_empty()) {
        parent = intern_one(conn, volume_id, Some(parent), component)?;
    }
    Ok(parent)
}

/// Intern one `(volume_id, parent, name)` dir row, returning its id. Insert-or-
/// ignore against the IFNULL identity index, then read the id back (reliable
/// whether or not the insert did anything).
fn intern_one(
    conn: &Connection,
    volume_id: &str,
    parent: Option<i64>,
    name: &str,
) -> Result<i64, OperationLogStoreError> {
    let folded = fold_name(name);
    conn.prepare_cached(
        "INSERT INTO dirs (volume_id, parent_dir_id, name, name_folded)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (volume_id, IFNULL(parent_dir_id, 0), name_folded) DO NOTHING",
    )?
    .execute(rusqlite::params![volume_id, parent, name, folded])?;

    // `parent.unwrap_or(0)` mirrors the index's IFNULL(parent, 0): dir ids start
    // at 1, so 0 is a safe stand-in for a NULL (root) parent.
    let id = conn
        .prepare_cached(
            "SELECT dir_id FROM dirs
             WHERE volume_id = ?1 AND IFNULL(parent_dir_id, 0) = ?2 AND name_folded = ?3",
        )?
        .query_row(rusqlite::params![volume_id, parent.unwrap_or(0), folded], |row| {
            row.get::<_, i64>(0)
        })?;
    Ok(id)
}

/// Reconstruct a dir's full path by walking `parent_dir_id` to the volume root.
/// The root (name `""`) contributes nothing, so a path renders as `/a/b/c`.
pub fn reconstruct_dir_path(conn: &Connection, dir_id: i64) -> Result<String, OperationLogStoreError> {
    let mut names = Vec::new();
    let mut current = Some(dir_id);
    let mut stmt = conn.prepare_cached("SELECT name, parent_dir_id FROM dirs WHERE dir_id = ?1")?;
    while let Some(id) = current {
        let (name, parent): (String, Option<i64>) =
            stmt.query_row(rusqlite::params![id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        if !name.is_empty() {
            names.push(name);
        }
        current = parent;
    }
    names.reverse();
    Ok(format!("/{}", names.join("/")))
}

// ── Reads (used by the dump bin; the query API adds paged/filtered reads) ──

/// Decode a stored token to a typed enum, or a [`OperationLogStoreError::Decode`].
fn decode<T>(
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

/// Read one operation header by id, or `None` if absent.
pub fn read_operation(conn: &Connection, op_id: &str) -> Result<Option<OperationRow>, OperationLogStoreError> {
    let sql = format!("SELECT {OPERATION_COLUMNS} FROM operations WHERE op_id = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![op_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(map_operation_row(row)?)),
        None => Ok(None),
    }
}

/// The most recent operations by start time (newest first), including
/// unfinished ones.
pub fn recent_operations(conn: &Connection, limit: u32) -> Result<Vec<OperationRow>, OperationLogStoreError> {
    let sql = format!("SELECT {OPERATION_COLUMNS} FROM operations ORDER BY started_at DESC, op_id DESC LIMIT ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![limit])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_operation_row(row)?);
    }
    Ok(out)
}

pub(super) const ITEM_COLUMNS: &str = "item_id, op_id, seq, entry_type, row_role, source_dir_id, source_name, dest_dir_id, \
     dest_name, size, mtime, outcome, overwrote";

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
    pub seq: i64,
    pub entry_type: EntryType,
    pub source_volume_id: String,
    pub source_path: PathBuf,
    pub dest_volume_id: Option<String>,
    pub dest_path: Option<PathBuf>,
    pub size: Option<i64>,
    pub mtime: Option<i64>,
    pub overwrote: bool,
    pub outcome: ItemOutcome,
}

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

fn map_rollback_unit(conn: &Connection, item: &OperationItemRow) -> Result<RollbackUnit, OperationLogStoreError> {
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
    })
}

/// A page of `rollback_unit` rows for an op, newest-`seq`-first (reverse order, so
/// a rollback undoes in inverse order and removes created files before the
/// `entry_type = dir` rows that held them). Streaming: pass `i64::MAX` for the
/// first page, then the smallest `seq` of the returned page (exclusive) as
/// `before_seq` for the next — so the engine never materializes a 1M-row op (D7).
/// `search_only` rows are excluded (they're for search, never reversal).
pub fn read_rollback_units_page(
    conn: &Connection,
    op_id: &str,
    before_seq: i64,
    limit: u32,
) -> Result<Vec<RollbackUnit>, OperationLogStoreError> {
    let sql = format!(
        "SELECT {ITEM_COLUMNS} FROM operation_items \
         WHERE op_id = ?1 AND row_role = ?2 AND outcome = ?3 AND seq < ?4 \
         ORDER BY seq DESC LIMIT ?5"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![
        op_id,
        RowRole::RollbackUnit.as_token(),
        ItemOutcome::Done.as_token(),
        before_seq,
        limit
    ])?;
    let mut items = Vec::new();
    while let Some(row) = rows.next()? {
        items.push(map_item_row(row)?);
    }
    // Resolve dirs after draining the query so the prepared-statement borrow is
    // released (the cache reuses one connection).
    items.iter().map(|item| map_rollback_unit(conn, item)).collect()
}

/// Every operation currently in `rolling_back` — the startup-reconcile input (rollback,
/// Finding 7): each resolves deterministically from its (unfinalized) inverse op's
/// recorded outcomes, or straight back to `rollbackable` when no inverse row exists.
pub fn ops_in_rolling_back(conn: &Connection) -> Result<Vec<OperationRow>, OperationLogStoreError> {
    let sql = format!("SELECT {OPERATION_COLUMNS} FROM operations WHERE rollback_state = ?1");
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![RollbackState::RollingBack.as_token()])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_operation_row(row)?);
    }
    Ok(out)
}

/// The most recent inverse operation reversing `original_op_id` (its
/// `rolls_back_op_id`), or `None` if none was ever opened. Used by startup
/// reconcile to resolve a crashed `rolling_back` op from the inverse's outcomes.
pub fn read_inverse_op(
    conn: &Connection,
    original_op_id: &str,
) -> Result<Option<OperationRow>, OperationLogStoreError> {
    let sql = format!(
        "SELECT {OPERATION_COLUMNS} FROM operations \
         WHERE rolls_back_op_id = ?1 ORDER BY started_at DESC, op_id DESC LIMIT 1"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![original_op_id])?;
    match rows.next()? {
        Some(row) => Ok(Some(map_operation_row(row)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests;
