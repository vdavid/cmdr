//! SQLite store for the drive index.
//!
//! One DB file per indexed volume. Uses WAL mode for concurrent reads.
//! All writes go through a dedicated writer thread (see `writer.rs`);
//! this module provides the schema, read queries, and static write helpers.
//!
//! ## Schema v2: integer-keyed parent-child tree
//!
//! Entries use an integer primary key (`id`) with a `parent_id` foreign key.
//! The `name` column uses `COLLATE platform_case`, a custom collation registered
//! at connection init that matches the filesystem's case/normalization rules:
//! - **macOS**: case-insensitive + NFD normalization (matching APFS)
//! - **Linux**: binary comparison (matching ext4/btrfs)

use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

// Bump to invalidate on-disk indexes (the cache is disposable: a mismatch deletes
// the DB file + recreates it fresh, no migration). v14 is a forced rebuild, not a schema change: earlier
// builds' reconcile could falsely mark a partial network scan `scan_completed_at`,
// stranding SMB/MTP indexes as "complete" so they'd never rescan. Dropping every
// index on upgrade heals testers to a clean, fully-scanned state with no manual
// Forget.
const SCHEMA_VERSION: &str = "14";

/// Meta key for the per-volume epoch counter (TEXT, like all meta values).
///
/// Bumped on every continuity break; a scan/reconcile *stamps* listed dirs with
/// the current epoch but does not bump it. Absent ⇒ treat as epoch 1 (a volume
/// with no recorded epoch behaves as "all current", not "all stale"). See the
/// "Honest sizes" model in `indexing/DETAILS.md`.
pub const CURRENT_EPOCH_KEY: &str = "current_epoch";

/// Meta key marking that the one-shot `dir_stats` ledger heal has already rebuilt
/// this DB's aggregates. Present ⇒ the heal ran successfully once, so a later
/// launch skips it; absent ⇒ this DB still carries pre-ledger drift and heals on
/// the next full aggregate (the writer-side latch). Only presence matters (the
/// value is a marker). See `indexing/DETAILS.md` § "The dir_stats ledger".
pub const LEDGER_HEAL_KEY: &str = "aggregates_rebuilt_for_ledger";

/// Root entry sentinel ID. All top-level entries have `parent_id = ROOT_ID`.
pub const ROOT_ID: i64 = 1;

/// Parent ID of the root sentinel. No row with this ID exists in the DB.
const ROOT_PARENT_ID: i64 = 0;

// ── Types ────────────────────────────────────────────────────────────

/// Dir stats keyed by path string. Used at the IPC boundary and by
/// the IPC boundary (frontend expects path-keyed dir stats).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DirStats {
    pub path: String,
    pub recursive_size: u64,
    pub recursive_physical_size: u64,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
    /// `true` if any descendant entry (or direct child) is a symlink.
    /// Used by the UI to surface "size omits symlinked content" hints.
    pub recursive_has_symlinks: bool,
    /// `true` while the indexer still has unprocessed writes affecting this
    /// directory or a descendant (a big delete/copy in flight). The frontend
    /// shows a "size updating" hourglass so the number isn't read as settled.
    /// Sourced from the in-memory `pending_sizes` tracker at build time, not the
    /// DB. See `indexing/pending_sizes.rs`.
    pub recursive_size_pending: bool,
    /// Whether `recursive_size` is an exact total (`true`) or a lower bound
    /// (`false`), derived backend-side from the subtree's `min_subtree_epoch`
    /// (`> 0` ⇒ exact). The FE renders an exact size when `true`, a `≥` lower
    /// bound (or `—` when size is 0) when `false`. Raw epochs never cross IPC.
    /// See the "Honest sizes" model in `indexing/DETAILS.md`.
    pub recursive_size_complete: bool,
    /// Whether the (exact) `recursive_size` was computed at an older volume epoch
    /// than the current one (accurate-but-stale). Only meaningful when
    /// `recursive_size_complete` is `true`; drives the muted "stale" treatment.
    pub recursive_size_stale: bool,
}

/// Dir stats keyed by entry ID. Used internally by the integer-keyed store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirStatsById {
    pub entry_id: i64,
    pub recursive_logical_size: u64,
    pub recursive_physical_size: u64,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
    /// `true` if the directory's subtree (including direct children) contains
    /// any symlink entries. Aggregated bottom-up alongside size totals.
    pub recursive_has_symlinks: bool,
    /// Coverage + freshness for this directory's whole subtree, as one integer:
    /// `min` over `{this dir's listed_epoch}` ∪ `{each child dir's
    /// min_subtree_epoch}`. `0` means some directory in the subtree was never
    /// listed (size is a lower bound); `> 0` means the subtree is fully covered
    /// and the value is the oldest listing epoch in it. Rolled up bottom-up by
    /// the aggregator (a separate agent's milestone); stays at its `0` default
    /// until then. See the "Honest sizes" model in `indexing/DETAILS.md`.
    pub min_subtree_epoch: u64,
}

/// A row from the integer-keyed `entries` table. Used as the primary entry
/// type by the scanner (with pre-assigned IDs) and the integer-keyed store API.
#[derive(Debug, Clone)]
pub struct EntryRow {
    pub id: i64,
    pub parent_id: i64,
    pub name: String,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub logical_size: Option<u64>,
    pub physical_size: Option<u64>,
    pub modified_at: Option<u64>,
    pub inode: Option<u64>,
}

/// Resolve the entry id to use as a scan's root, seeding the `ROOT` sentinel for
/// a volume-root scan.
///
/// For a volume-root scan the root is always `ROOT_ID` (the sentinel is created
/// if absent). For a subtree scan the root's actual entry id is resolved from the
/// DB, erroring if it isn't indexed yet (for example a subtree scan racing an
/// ongoing full scan; the full scan will cover it).
///
/// Shared by both scanners. The network (SMB/MTP) `volume_scanner` wraps it in a
/// [`ScanContext`] path→id map (its serial BFS resolves parents by path); the
/// local scanner carries `parent_id` through its parallel walk and needs only the
/// root id from here.
pub fn resolve_scan_root(conn: &Connection, root: &Path, is_volume_root: bool) -> Result<i64, IndexStoreError> {
    if is_volume_root {
        // Only volume-root scans create the sentinel; subtree scans run after the
        // full scan already inserted it, and their connection may be read-only or
        // contending with the writer thread's write lock.
        ensure_root_sentinel(conn)?;
        return Ok(ROOT_ID);
    }

    let root_str = root.to_string_lossy();
    if let Some(id) = resolve_path(conn, &root_str)? {
        return Ok(id);
    }

    // Diagnose which component is missing by walking the path.
    let stripped = root_str.strip_prefix('/').unwrap_or(&root_str);
    let mut current_id = ROOT_ID;
    for component in stripped.split('/') {
        if component.is_empty() {
            continue;
        }
        match IndexStore::resolve_component(conn, current_id, component) {
            Ok(Some(id)) => current_id = id,
            Ok(None) => {
                log::debug!(
                    "resolve_scan_root: resolve_path({root_str}) failed at component \"{component}\" (parent_id={current_id})"
                );
                break;
            }
            Err(e) => {
                log::debug!(
                    "resolve_scan_root: resolve_path({root_str}) errored at component \"{component}\" (parent_id={current_id}): {e}"
                );
                break;
            }
        }
    }
    Err(IndexStoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
}

/// Mutable context held during a network (SMB/MTP) scan for assigning parent IDs.
///
/// Maintains a `HashMap<PathBuf, i64>` mapping directory paths to their
/// pre-assigned entry IDs. The `volume_scanner`'s serial BFS looks up each
/// entry's parent path in this map to get its `parent_id`, assigns a fresh `id`
/// from `next_id`, and (if the entry is a directory) inserts its own mapping. The
/// LOCAL scanner does NOT use this — it carries `parent_id` through its parallel
/// walk, so it never builds a whole-volume path map.
pub struct ScanContext {
    /// Map from directory absolute path to its assigned entry ID.
    pub dir_ids: std::collections::HashMap<PathBuf, i64>,
    /// Shared ID counter. Atomically incremented to allocate unique IDs.
    /// Owned by `IndexWriter`, shared with all scanners and the writer thread.
    next_id: Arc<AtomicI64>,
}

impl ScanContext {
    /// Create a new scan context, seeding the map with the root's entry ID.
    ///
    /// `next_id` is the shared atomic counter from `IndexWriter`, the single
    /// source of truth for ID allocation. `is_volume_root` selects root handling;
    /// see [`resolve_scan_root`].
    pub fn new(
        conn: &Connection,
        root: &Path,
        is_volume_root: bool,
        next_id: Arc<AtomicI64>,
    ) -> Result<Self, IndexStoreError> {
        let root_id = resolve_scan_root(conn, root, is_volume_root)?;
        let mut dir_ids = std::collections::HashMap::new();
        dir_ids.insert(root.to_path_buf(), root_id);
        Ok(Self { dir_ids, next_id })
    }

    /// Allocate the next entry ID and advance the counter.
    pub fn alloc_id(&mut self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register a directory path with its assigned ID, so children can
    /// look up their parent_id.
    pub fn register_dir(&mut self, path: PathBuf, id: i64) {
        self.dir_ids.insert(path, id);
    }

    /// Look up the parent_id for an entry given its parent's absolute path.
    pub fn lookup_parent(&self, parent_path: &Path) -> Option<i64> {
        self.dir_ids.get(parent_path).copied()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub schema_version: Option<String>,
    pub volume_path: Option<String>,
    pub scan_completed_at: Option<String>,
    pub scan_duration_ms: Option<String>,
    pub total_entries: Option<String>,
    /// The previous completed scan's summed post-dedup physical bytes (TEXT, like
    /// every meta value). Surfaced for symmetry with `total_entries` and for
    /// debugging; not on the tier-1 critical path.
    pub total_physical_bytes: Option<String>,
    pub last_event_id: Option<String>,
}

/// The previous completed scan's persisted calibration, read from `meta`.
///
/// All fields are `Option` because a first-ever scan (or a DB rebuilt after a
/// schema bump / `clear_index`) has none of these keys yet. The numerator-side
/// live counters are compared against `total_entries` (tier-1 denominator) and
/// `total_physical_bytes` (tier-2 cap tuning); `scan_duration_ms` seeds the ETA.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanCalibration {
    pub total_entries: Option<u64>,
    pub total_physical_bytes: Option<u64>,
    pub scan_duration_ms: Option<u64>,
}

// ── Errors ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum IndexStoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    /// The on-disk DB carries a different `schema_version` than this build. The
    /// cache is disposable, so `IndexStore::open` recreates the file fresh
    /// (delete + recreate, reclaiming disk) rather than migrating. Carries the
    /// found vs expected versions for a clean upgrade log (not a corruption
    /// warning). Raised by `try_open` before the store is constructed, so its
    /// connection drops before `delete_and_recreate` opens a new one.
    SchemaMismatch {
        found: String,
        expected: &'static str,
    },
}

impl From<rusqlite::Error> for IndexStoreError {
    fn from(err: rusqlite::Error) -> Self {
        IndexStoreError::Sqlite(err)
    }
}

impl From<std::io::Error> for IndexStoreError {
    fn from(err: std::io::Error) -> Self {
        IndexStoreError::Io(err)
    }
}

impl std::fmt::Display for IndexStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexStoreError::Sqlite(e) => write!(f, "SQLite error: {e}"),
            IndexStoreError::Io(e) => write!(f, "I/O error: {e}"),
            IndexStoreError::SchemaMismatch { found, expected } => {
                write!(f, "schema version mismatch (found {found}, expected {expected})")
            }
        }
    }
}

impl std::error::Error for IndexStoreError {}

/// A fatal storage failure that stopped a volume's index: the SQLite result codes
/// that classified the DB as unusable (a dead disk, a corrupt file, a full or
/// read-only volume). Carried on the `IndexPhase::Failed` phase (see `state.rs`)
/// and surfaced to the UI and logs so the failure is specific.
///
/// `code` is the primary SQLite result code (for example `SQLITE_IOERR` = 10);
/// `extended_code` is the extended code (for example `SQLITE_IOERR_WRITE`),
/// preserved because [`IndexStoreError`]'s `Display` flattens it away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IndexFailure {
    pub code: i32,
    pub extended_code: i32,
}

impl IndexStoreError {
    /// The SQLite `(primary ErrorCode, extended result code)` when this wraps a
    /// `SqliteFailure`. `None` for non-SQLite errors and for `rusqlite` errors that
    /// carry no ffi code (for example `QueryReturnedNoRows`).
    ///
    /// Classify on THIS, never on the `Display` string (`no-string-matching`):
    /// `Display` writes only `SQLite error: {e}` and drops the numeric extended
    /// code that distinguishes a transient lock from a dead disk.
    pub fn sqlite_code(&self) -> Option<(rusqlite::ErrorCode, i32)> {
        match self {
            IndexStoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _)) => Some((e.code, e.extended_code)),
            _ => None,
        }
    }

    /// Whether this is a FATAL storage-class error: the DB is unusable and every
    /// subsequent read and write will fail the same way, so the index must stop and
    /// fail rather than retry forever (the 12,700-warning livelock this guards
    /// against). Transient contention (`SQLITE_BUSY` / `SQLITE_LOCKED`) is
    /// deliberately NOT fatal: the busy handler already backs those off. Classified
    /// on the typed primary `ErrorCode`.
    pub fn is_fatal_storage_error(&self) -> bool {
        use rusqlite::ErrorCode::{CannotOpen, DatabaseCorrupt, DiskFull, NotADatabase, ReadOnly, SystemIoFailure};
        matches!(
            self.sqlite_code(),
            Some((
                SystemIoFailure     // SQLITE_IOERR*
                    | DatabaseCorrupt   // SQLITE_CORRUPT
                    | CannotOpen        // SQLITE_CANTOPEN
                    | DiskFull          // SQLITE_FULL
                    | ReadOnly          // SQLITE_READONLY
                    | NotADatabase, // SQLITE_NOTADB
                _
            ))
        )
    }

    /// The typed [`IndexFailure`] for the `Failed` phase, if this is a fatal
    /// storage error (else `None`). The primary code is the low byte of the
    /// extended code, matching SQLite's `SQLITE_IOERR == extended & 0xFF`.
    pub fn as_index_failure(&self) -> Option<IndexFailure> {
        if !self.is_fatal_storage_error() {
            return None;
        }
        self.sqlite_code().map(|(_, extended_code)| IndexFailure {
            code: extended_code & 0xFF,
            extended_code,
        })
    }
}

// ── Platform-case collation ──────────────────────────────────────────

/// Register the `platform_case` collation on a connection.
///
/// Must be called on every connection before any table creation or query,
/// because custom collations are not persisted in the DB file.
pub fn register_platform_case_collation(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.create_collation("platform_case", platform_case_compare)?;
    Ok(())
}

/// Compare two strings using the platform's filesystem case/normalization rules.
///
/// - **macOS**: NFD-normalize then case-fold (matching APFS behavior).
/// - **Linux**: binary comparison (matching ext4/btrfs).
#[cfg(target_os = "macos")]
fn platform_case_compare(a: &str, b: &str) -> std::cmp::Ordering {
    use unicode_normalization::UnicodeNormalization;
    let a_norm: String = a.nfd().collect::<String>().to_lowercase();
    let b_norm: String = b.nfd().collect::<String>().to_lowercase();
    a_norm.cmp(&b_norm)
}

#[cfg(not(target_os = "macos"))]
fn platform_case_compare(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

/// Normalize a string for case-insensitive comparison.
#[cfg(target_os = "macos")]
pub fn normalize_for_comparison(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfd().collect::<String>().to_lowercase()
}

#[cfg(not(target_os = "macos"))]
pub fn normalize_for_comparison(s: &str) -> String {
    s.to_string()
}

// ── Schema ───────────────────────────────────────────────────────────

const CREATE_TABLES_SQL: &str = "
    CREATE TABLE IF NOT EXISTS entries (
        id            INTEGER PRIMARY KEY,
        parent_id     INTEGER NOT NULL,
        name          TEXT    NOT NULL COLLATE platform_case,
        name_folded   TEXT    NOT NULL DEFAULT '',
        is_directory  INTEGER NOT NULL DEFAULT 0,
        is_symlink    INTEGER NOT NULL DEFAULT 0,
        logical_size  INTEGER,
        physical_size INTEGER,
        modified_at   INTEGER,
        inode         INTEGER,
        listed_epoch  INTEGER NOT NULL DEFAULT 0
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_parent_name_folded ON entries (parent_id, name_folded);
    CREATE INDEX IF NOT EXISTS idx_inode ON entries (inode);

    CREATE TABLE IF NOT EXISTS dir_stats (
        entry_id                 INTEGER PRIMARY KEY,
        recursive_logical_size   INTEGER NOT NULL DEFAULT 0,
        recursive_physical_size  INTEGER NOT NULL DEFAULT 0,
        recursive_file_count     INTEGER NOT NULL DEFAULT 0,
        recursive_dir_count      INTEGER NOT NULL DEFAULT 0,
        recursive_has_symlinks   INTEGER NOT NULL DEFAULT 0,
        min_subtree_epoch        INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
";

/// Insert the root sentinel entry if it doesn't exist.
fn ensure_root_sentinel(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute(
        "INSERT OR IGNORE INTO entries (id, parent_id, name, name_folded, is_directory) VALUES (?1, ?2, '', '', 1)",
        params![ROOT_ID, ROOT_PARENT_ID],
    )?;
    Ok(())
}

/// Apply WAL-mode pragmas for performance.
fn apply_pragmas(conn: &Connection, readonly: bool) -> Result<(), IndexStoreError> {
    if !readonly {
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;",
        )?;
    }
    // busy_timeout: when another connection holds the write lock, retry for up
    // to 5s instead of returning SQLITE_BUSY immediately. Applies to every open
    // (read and write) because even read-only connections in WAL mode touch the
    // -shm file at startup and can briefly race a writer. Without this, the
    // live event loop was dying on its initial open under transient contention,
    // dropping the FSEvents receiver and silently stopping live index updates
    // for the rest of the session.
    conn.execute_batch(
        "PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -16384;",
    )?;
    Ok(())
}

/// Create tables if they don't exist and insert root sentinel.
fn create_tables(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute_batch(CREATE_TABLES_SQL)?;
    ensure_root_sentinel(conn)?;
    Ok(())
}

/// Drop all index tables and recreate them from scratch.
///
/// Test-only: the live schema-mismatch path recreates the DB FILE (zero
/// freelist) via `IndexStore::delete_and_recreate`, not a DROP on the live file.
/// This stays only for the `#[cfg(test)]` `clear_all` helper.
#[cfg(test)]
fn reset_schema(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS entries;
         DROP TABLE IF EXISTS dir_stats;
         DROP TABLE IF EXISTS meta;",
    )?;
    create_tables(conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        params!["schema_version", SCHEMA_VERSION],
    )?;
    Ok(())
}

// ── Path reconstruction helpers ──────────────────────────────────────

/// Reconstruct the full path for an entry by walking up the parent chain.
///
/// Returns `/` for the root sentinel itself, and `/component/component/...`
/// for all other entries.
fn reconstruct_path(conn: &Connection, entry_id: i64) -> Result<String, IndexStoreError> {
    if entry_id == ROOT_ID {
        return Ok("/".to_string());
    }

    let mut components = Vec::new();
    let mut current_id = entry_id;

    loop {
        if current_id == ROOT_ID || current_id == ROOT_PARENT_ID {
            break;
        }
        let mut stmt = conn.prepare_cached("SELECT parent_id, name FROM entries WHERE id = ?1")?;
        let (parent_id, name): (i64, String) =
            stmt.query_row(params![current_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        components.push(name);
        current_id = parent_id;
    }

    components.reverse();
    Ok(format!("/{}", components.join("/")))
}

/// Resolve a path string to an entry ID by walking component-by-component from
/// the index root (`ROOT_ID`).
///
/// Returns `None` if any component along the path doesn't exist. The path must be
/// absolute (starting with `/`). For a `root` (local-disk) index `ROOT_ID` is `/`,
/// so an absolute filesystem path resolves directly. For a network/MTP index
/// `ROOT_ID` is the volume root, so a mount-absolute path must be mapped into the
/// volume's index path space first (see [`crate::indexing::routing::index_read_path`]).
pub fn resolve_path(conn: &Connection, path: &str) -> Result<Option<i64>, IndexStoreError> {
    resolve_path_under(conn, ROOT_ID, path)
}

/// Resolve a path RELATIVE to a given root entry id by walking
/// component-by-component from `root_id`.
///
/// Returns `None` if any component doesn't exist under that root. A leading `/`
/// on `relative_path` is treated as relative to `root_id` (NOT the index root),
/// and an empty path (`""` or `"/"`) resolves to `root_id` itself.
///
/// This is the root-relative generalization of [`resolve_path`] (which is just
/// `resolve_path_under(conn, ROOT_ID, path)`). It exists because a network/MTP
/// index is rooted at the VOLUME root rather than `/`: once a mount-absolute hot
/// path has had its volume-root prefix stripped to a relative remainder, this
/// walks that remainder from the index's `ROOT_ID`.
pub fn resolve_path_under(
    conn: &Connection,
    root_id: i64,
    relative_path: &str,
) -> Result<Option<i64>, IndexStoreError> {
    let trimmed = relative_path.strip_suffix('/').unwrap_or(relative_path);

    let mut current_id = root_id;
    for component in trimmed.strip_prefix('/').unwrap_or(trimmed).split('/') {
        if component.is_empty() {
            continue;
        }
        match IndexStore::resolve_component(conn, current_id, component)? {
            Some(id) => current_id = id,
            None => return Ok(None),
        }
    }
    Ok(Some(current_id))
}

// ── IndexStore ───────────────────────────────────────────────────────

/// Read-oriented handle to the index database.
///
/// Holds a single read connection (WAL allows concurrent reads from any thread).
/// Write operations use a separate connection obtained via [`IndexStore::open_write_connection`].
pub struct IndexStore {
    db_path: PathBuf,
    read_conn: Connection,
}

/// Runs `f` inside a SQLite savepoint. Releases on success, rolls back on error.
///
/// SAFETY: `name` is interpolated into SQL. Only pass hardcoded string literals.
fn with_savepoint<F, T>(conn: &Connection, name: &str, f: F) -> Result<T, IndexStoreError>
where
    F: FnOnce(&Connection) -> Result<T, IndexStoreError>,
{
    conn.execute_batch(&format!("SAVEPOINT {name}"))?;
    match f(conn) {
        Ok(val) => {
            conn.execute_batch(&format!("RELEASE {name}"))?;
            Ok(val)
        }
        Err(e) => {
            // Rollback failure is intentionally silenced; the savepoint may already
            // be released or the connection may be in an error state.
            let _ = conn.execute_batch(&format!("ROLLBACK TO {name}"));
            Err(e)
        }
    }
}

// ── IndexStore impl (split across submodules) ────────────────────────
//
// The `impl IndexStore` block lives across these submodules (pure code
// movement, grouped by concern). Each is `impl IndexStore { … }` over the
// struct defined above and pulls shared items in via `use super::*`.
mod connection;
mod dir_stats;
mod entries;
mod meta;

/// Reconstruct a path from an in-memory map of `id -> (parent_id, name)`.
/// More efficient than DB queries when reconstructing many paths.
#[cfg(test)]
fn reconstruct_path_from_map(entry_id: i64, map: &std::collections::HashMap<i64, (i64, &str)>) -> String {
    if entry_id == ROOT_ID {
        return "/".to_string();
    }

    let mut components = Vec::new();
    let mut current_id = entry_id;

    loop {
        if current_id == ROOT_ID || current_id == ROOT_PARENT_ID {
            break;
        }
        match map.get(&current_id) {
            Some((parent_id, name)) => {
                components.push(*name);
                current_id = *parent_id;
            }
            None => break,
        }
    }

    components.reverse();
    format!("/{}", components.join("/"))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
