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

const SCHEMA_VERSION: &str = "13";

/// Meta key for the per-volume epoch counter (TEXT, like all meta values).
///
/// Bumped on every continuity break; a scan/reconcile *stamps* listed dirs with
/// the current epoch but does not bump it. Absent ⇒ treat as epoch 1 (a volume
/// with no recorded epoch behaves as "all current", not "all stale"). See the
/// "Honest sizes" model in `indexing/DETAILS.md`.
pub const CURRENT_EPOCH_KEY: &str = "current_epoch";

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
#[derive(Debug, Clone, Default)]
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

/// Mutable context held during a scan for assigning parent IDs.
///
/// Maintains a `HashMap<PathBuf, i64>` mapping directory paths to their
/// pre-assigned entry IDs. The scanner looks up each entry's parent path
/// in this map to get its `parent_id`, assigns a fresh `id` from
/// `next_id`, and (if the entry is a directory) inserts its own mapping.
///
/// Dropped after the scan completes, freeing ~100 MB for 538K directories.
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
    /// source of truth for ID allocation.
    ///
    /// `is_volume_root`: true for full volume scans (always maps root → ROOT_ID).
    /// When false (subtree scans), resolves the root's actual entry ID from the DB.
    /// Returns an error if the root isn't indexed yet (for example, a subtree scan
    /// racing with an ongoing full scan; the full scan will cover it).
    pub fn new(
        conn: &Connection,
        root: &Path,
        is_volume_root: bool,
        next_id: Arc<AtomicI64>,
    ) -> Result<Self, IndexStoreError> {
        // Only volume-root scans need to create the sentinel; subtree scans
        // run after the full scan has already inserted it, and their connection
        // may be read-only or contending with the writer thread's write lock.
        if is_volume_root {
            ensure_root_sentinel(conn)?;
        }

        let root_id = if is_volume_root {
            ROOT_ID
        } else {
            let root_str = root.to_string_lossy();
            match resolve_path(conn, &root_str)? {
                Some(id) => id,
                None => {
                    // Diagnose which component is missing by walking the path
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
                                    "ScanContext::new: resolve_path({root_str}) failed at \
                                     component \"{component}\" (parent_id={current_id})"
                                );
                                break;
                            }
                            Err(e) => {
                                log::debug!(
                                    "ScanContext::new: resolve_path({root_str}) errored at \
                                     component \"{component}\" (parent_id={current_id}): {e}"
                                );
                                break;
                            }
                        }
                    }
                    return Err(IndexStoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
                }
            }
        };

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
        }
    }
}

impl std::error::Error for IndexStoreError {}

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
#[cfg(test)]
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

/// Resolve a path string to an entry ID by walking component-by-component.
///
/// Returns `None` if any component along the path doesn't exist.
/// The path must be absolute (starting with `/`).
pub fn resolve_path(conn: &Connection, path: &str) -> Result<Option<i64>, IndexStoreError> {
    if path == "/" {
        return Ok(Some(ROOT_ID));
    }

    let path = path.strip_suffix('/').unwrap_or(path);

    let mut current_id = ROOT_ID;
    for component in path.strip_prefix('/').unwrap_or(path).split('/') {
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
mod tests {
    use super::*;

    /// Create an IndexStore backed by a temporary file.
    fn open_temp_store() -> (IndexStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test-index.db");
        let store = IndexStore::open(&db_path).expect("failed to open store");
        (store, dir)
    }

    /// Helper: insert an entry using integer-keyed API. Returns the new ID.
    fn insert_entry(conn: &Connection, parent_id: i64, name: &str, is_dir: bool, size: Option<u64>) -> i64 {
        IndexStore::insert_entry_v2(conn, parent_id, name, is_dir, false, size, size, None, None).unwrap()
    }

    #[test]
    fn schema_creation_and_version() {
        let (store, _dir) = open_temp_store();
        let status = store.get_index_status().unwrap();
        assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
    }

    /// `min_subtree_epoch` survives a `dir_stats` write + read round-trip
    /// (single and batch paths), and defaults to 0 for an un-set row.
    #[test]
    fn dir_stats_min_subtree_epoch_round_trips() {
        let (store, _dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        let a = insert_entry(&conn, ROOT_ID, "a", true, None);
        let b = insert_entry(&conn, ROOT_ID, "b", true, None);

        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: a,
                    recursive_logical_size: 100,
                    min_subtree_epoch: 7,
                    ..Default::default()
                },
                DirStatsById {
                    entry_id: b,
                    recursive_logical_size: 0,
                    min_subtree_epoch: 0,
                    ..Default::default()
                },
            ],
        )
        .unwrap();

        let single = IndexStore::get_dir_stats_by_id(&conn, a).unwrap().unwrap();
        assert_eq!(single.min_subtree_epoch, 7);

        let batch = IndexStore::get_dir_stats_batch_by_ids(&conn, &[a, b]).unwrap();
        assert_eq!(batch[0].as_ref().unwrap().min_subtree_epoch, 7);
        assert_eq!(batch[1].as_ref().unwrap().min_subtree_epoch, 0);
    }

    /// A fresh entry defaults to `listed_epoch = 0`; `mark_dirs_listed` stamps the
    /// given ids and leaves unlisted ones at 0.
    #[test]
    fn mark_dirs_listed_stamps_only_given_ids() {
        let (store, _dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        let a = insert_entry(&conn, ROOT_ID, "a", true, None);
        let b = insert_entry(&conn, ROOT_ID, "b", true, None);

        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(),
            Some(0),
            "default is 0"
        );

        IndexStore::mark_dirs_listed(&conn, &[a], 3).unwrap();
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(),
            Some(3),
            "a stamped"
        );
        assert_eq!(
            IndexStore::get_listed_epoch_by_id(&conn, b).unwrap(),
            Some(0),
            "b untouched"
        );

        // Empty id list is a no-op.
        IndexStore::mark_dirs_listed(&conn, &[], 9).unwrap();
        assert_eq!(IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(), Some(3));
    }

    /// `current_epoch` helpers: absent reads as 1, seed makes it 1, bump increments.
    #[test]
    fn current_epoch_helpers() {
        let (store, _dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        // Absent ⇒ treated as 1 (all current, not all stale).
        assert_eq!(IndexStore::get_meta(&conn, CURRENT_EPOCH_KEY).unwrap(), None);
        assert_eq!(IndexStore::read_current_epoch(&conn).unwrap(), 1);

        // Seeding writes "1" and is idempotent.
        assert_eq!(IndexStore::seed_current_epoch(&conn).unwrap(), 1);
        assert_eq!(
            IndexStore::get_meta(&conn, CURRENT_EPOCH_KEY).unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(
            IndexStore::seed_current_epoch(&conn).unwrap(),
            1,
            "seed leaves existing value"
        );

        // Bump increments and persists.
        assert_eq!(IndexStore::bump_current_epoch(&conn).unwrap(), 2);
        assert_eq!(IndexStore::read_current_epoch(&conn).unwrap(), 2);
    }

    /// `recompute_min_subtree_epoch`: the 0-absorbing min over the dir's own
    /// `listed_epoch` and every child dir's stored `min_subtree_epoch`.
    #[test]
    fn recompute_min_subtree_epoch_cases() {
        let (store, _dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        // An unlisted dir (listed_epoch = 0) is always 0, regardless of children.
        let unlisted = insert_entry(&conn, ROOT_ID, "unlisted", true, None);
        assert_eq!(IndexStore::recompute_min_subtree_epoch(&conn, unlisted).unwrap(), 0);

        // A listed dir with NO child dirs is covered at its own epoch.
        let leaf = insert_entry(&conn, ROOT_ID, "leaf", true, None);
        IndexStore::mark_dirs_listed(&conn, &[leaf], 5).unwrap();
        assert_eq!(
            IndexStore::recompute_min_subtree_epoch(&conn, leaf).unwrap(),
            5,
            "listed-childless ⇒ own epoch"
        );

        // A listed parent with one complete child (epoch 4) and one incomplete
        // child (epoch 0) ⇒ 0 (the 0 absorbs).
        let parent = insert_entry(&conn, ROOT_ID, "parent", true, None);
        IndexStore::mark_dirs_listed(&conn, &[parent], 9).unwrap();
        let complete = insert_entry(&conn, parent, "complete", true, None);
        let incomplete = insert_entry(&conn, parent, "incomplete", true, None);
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: complete,
                    min_subtree_epoch: 4,
                    ..Default::default()
                },
                DirStatsById {
                    entry_id: incomplete,
                    min_subtree_epoch: 0,
                    ..Default::default()
                },
            ],
        )
        .unwrap();
        assert_eq!(
            IndexStore::recompute_min_subtree_epoch(&conn, parent).unwrap(),
            0,
            "an incomplete child absorbs to 0"
        );

        // With both children complete (4 and 6), the parent is the weakest link
        // across self (9) and children ⇒ 4.
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: incomplete,
                min_subtree_epoch: 6,
                ..Default::default()
            }],
        )
        .unwrap();
        assert_eq!(
            IndexStore::recompute_min_subtree_epoch(&conn, parent).unwrap(),
            4,
            "weakest link = min(own=9, 4, 6) = 4"
        );
    }

    /// A schema-version mismatch drops + rebuilds; the rebuilt DB still has the
    /// new v13 columns (a write/read round-trip through them succeeds).
    #[test]
    fn schema_bump_rebuild_has_new_columns() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("bump.db");

        // Open, then stamp a stale version to force a drop+rebuild on reopen.
        {
            let store = IndexStore::open(&db_path).unwrap();
            let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
            IndexStore::update_meta(&conn, "schema_version", "1").unwrap();
        }

        let store = IndexStore::open(&db_path).unwrap();
        assert_eq!(
            store.get_index_status().unwrap().schema_version.as_deref(),
            Some(SCHEMA_VERSION)
        );

        // The new columns exist and round-trip on the rebuilt schema.
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        let a = insert_entry(&conn, ROOT_ID, "a", true, None);
        IndexStore::mark_dirs_listed(&conn, &[a], 5).unwrap();
        assert_eq!(IndexStore::get_listed_epoch_by_id(&conn, a).unwrap(), Some(5));
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: a,
                min_subtree_epoch: 5,
                ..Default::default()
            }],
        )
        .unwrap();
        assert_eq!(
            IndexStore::get_dir_stats_by_id(&conn, a)
                .unwrap()
                .unwrap()
                .min_subtree_epoch,
            5
        );
    }

    /// `apply_pragmas` must set a non-zero `busy_timeout` on both read and
    /// write connections. Without it, concurrent connections fail with
    /// `SQLITE_BUSY` on the first lock contention instead of waiting.
    #[test]
    fn apply_pragmas_sets_busy_timeout_on_both_modes() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        let write_timeout: i64 = write_conn
            .pragma_query_value(None, "busy_timeout", |r| r.get(0))
            .unwrap();
        assert!(
            write_timeout > 0,
            "write connection should have busy_timeout set, got {write_timeout}"
        );

        let read_conn = IndexStore::open_read_connection(store.db_path()).unwrap();
        let read_timeout: i64 = read_conn
            .pragma_query_value(None, "busy_timeout", |r| r.get(0))
            .unwrap();
        assert!(
            read_timeout > 0,
            "read connection should have busy_timeout set, got {read_timeout}"
        );
    }

    /// `open_read_connection` must succeed while another connection holds a
    /// write transaction. The live and replay event loops rely on this to
    /// open their path-resolution connection without racing the writer
    /// thread. Regression: switching this call site to `open_write_connection`
    /// (or removing the `busy_timeout` pragma) makes the open fail on every
    /// concurrent commit, which silently kills the FSEvents receiver and
    /// stops live index updates for the rest of the session.
    #[test]
    fn open_read_connection_succeeds_under_write_lock() {
        let (store, _dir) = open_temp_store();
        let db_path = store.db_path().to_path_buf();
        let writer = IndexStore::open_write_connection(&db_path).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();

        // The read connection should open and be usable while the writer's
        // transaction is still in flight.
        let read = IndexStore::open_read_connection(&db_path).expect("read connection should open under write lock");
        let root = IndexStore::get_entry_by_id(&read, ROOT_ID).unwrap();
        assert!(root.is_some(), "read connection should see committed root sentinel");

        // Release the writer's lock so the tempdir can clean up cleanly.
        writer.execute_batch("ROLLBACK").unwrap();
    }

    #[test]
    fn root_sentinel_exists() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
        let root = IndexStore::get_entry_by_id(&write_conn, ROOT_ID).unwrap();
        assert!(root.is_some());
        let root = root.unwrap();
        assert_eq!(root.id, ROOT_ID);
        assert_eq!(root.parent_id, ROOT_PARENT_ID);
        assert_eq!(root.name, "");
        assert!(root.is_directory);
    }

    #[test]
    fn insert_and_list_entries() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let users_id = insert_entry(&write_conn, ROOT_ID, "Users", true, None);
        let test_id = insert_entry(&write_conn, users_id, "test", true, None);
        insert_entry(&write_conn, test_id, "a.txt", false, Some(1024));
        insert_entry(&write_conn, test_id, "docs", true, None);

        let result = store.list_children(test_id).unwrap();
        assert_eq!(result.len(), 2);

        let file = result.iter().find(|e| e.name == "a.txt").unwrap();
        assert!(!file.is_directory);
        assert_eq!(file.logical_size, Some(1024));

        let dir = result.iter().find(|e| e.name == "docs").unwrap();
        assert!(dir.is_directory);
    }

    #[test]
    fn dir_stats_roundtrip() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let users_id = insert_entry(&conn, ROOT_ID, "Users", true, None);
        let test_id = insert_entry(&conn, users_id, "test", true, None);

        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: test_id,
                recursive_logical_size: 50_000,
                recursive_physical_size: 50_000,
                recursive_file_count: 42,
                recursive_dir_count: 5,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();

        let result = IndexStore::get_dir_stats_by_id(&conn, test_id).unwrap().unwrap();
        assert_eq!(result.recursive_logical_size, 50_000);
        assert_eq!(result.recursive_file_count, 42);
        assert_eq!(result.recursive_dir_count, 5);
    }

    #[test]
    fn dir_stats_batch_lookup() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let a_id = insert_entry(&conn, ROOT_ID, "a", true, None);
        let b_id = insert_entry(&conn, ROOT_ID, "b", true, None);

        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: a_id,
                    recursive_logical_size: 100,
                    recursive_physical_size: 100,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
                DirStatsById {
                    entry_id: b_id,
                    recursive_logical_size: 200,
                    recursive_physical_size: 200,
                    recursive_file_count: 2,
                    recursive_dir_count: 1,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
            ],
        )
        .unwrap();

        let result = IndexStore::get_dir_stats_batch_by_ids(&conn, &[a_id, 99999, b_id]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].is_some());
        assert!(result[1].is_none());
        assert!(result[2].is_some());
        assert_eq!(result[0].as_ref().unwrap().recursive_logical_size, 100);
        assert_eq!(result[2].as_ref().unwrap().recursive_logical_size, 200);
    }

    #[test]
    fn meta_roundtrip() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        IndexStore::update_meta(&write_conn, "volume_path", "/").unwrap();
        IndexStore::update_meta(&write_conn, "scan_duration_ms", "1234").unwrap();

        let val = IndexStore::get_meta(&write_conn, "volume_path").unwrap();
        assert_eq!(val.as_deref(), Some("/"));

        let status = store.get_index_status().unwrap();
        assert_eq!(status.volume_path.as_deref(), Some("/"));
        assert_eq!(status.scan_duration_ms.as_deref(), Some("1234"));
    }

    #[test]
    fn read_scan_calibration_reads_seeded_keys() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        IndexStore::update_meta(&write_conn, "total_entries", "5000000").unwrap();
        IndexStore::update_meta(&write_conn, "total_physical_bytes", "905000000000").unwrap();
        IndexStore::update_meta(&write_conn, "scan_duration_ms", "149000").unwrap();

        let calibration = IndexStore::read_scan_calibration(&write_conn).unwrap();
        assert_eq!(calibration.total_entries, Some(5_000_000));
        assert_eq!(calibration.total_physical_bytes, Some(905_000_000_000));
        assert_eq!(calibration.scan_duration_ms, Some(149_000));
    }

    #[test]
    fn read_scan_calibration_missing_keys_are_none() {
        let (store, _dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        // Fresh DB: none of the calibration keys exist yet.
        let calibration = IndexStore::read_scan_calibration(&conn).unwrap();
        assert_eq!(calibration, ScanCalibration::default());
        assert_eq!(calibration.total_entries, None);
        assert_eq!(calibration.total_physical_bytes, None);
        assert_eq!(calibration.scan_duration_ms, None);

        // A non-numeric value also maps to None (parse failure), not an error.
        IndexStore::update_meta(&conn, "total_entries", "not-a-number").unwrap();
        let calibration = IndexStore::read_scan_calibration(&conn).unwrap();
        assert_eq!(calibration.total_entries, None);
    }

    #[test]
    fn children_stats() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let p_id = insert_entry(&conn, ROOT_ID, "p", true, None);
        insert_entry(&conn, p_id, "f1.txt", false, Some(100));
        insert_entry(&conn, p_id, "f2.txt", false, Some(200));
        insert_entry(&conn, p_id, "sub", true, None);

        let (logical_size, physical_size, file_count, dir_count) =
            IndexStore::get_children_stats_by_id(&conn, p_id).unwrap();
        assert_eq!(logical_size, 300);
        assert_eq!(physical_size, 300);
        assert_eq!(file_count, 2);
        assert_eq!(dir_count, 1);
    }

    #[test]
    fn delete_entry_and_subtree() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        // Build tree: /a, /a/b.txt, /a/c, /a/c/d.txt
        let a_id = insert_entry(&write_conn, ROOT_ID, "a", true, None);
        let b_id = insert_entry(&write_conn, a_id, "b.txt", false, Some(10));
        let c_id = insert_entry(&write_conn, a_id, "c", true, None);
        insert_entry(&write_conn, c_id, "d.txt", false, Some(20));

        // Delete single entry
        IndexStore::delete_entry_by_id(&write_conn, b_id).unwrap();
        let children = store.list_children(a_id).unwrap();
        assert_eq!(children.len(), 1); // only c remains

        // Delete subtree
        IndexStore::delete_subtree_by_id(&write_conn, a_id).unwrap();
        let children = store.list_children(a_id).unwrap();
        assert!(children.is_empty());
        let root_children = store.list_children(ROOT_ID).unwrap();
        assert!(root_children.is_empty()); // /a itself is also gone
    }

    #[test]
    fn clear_all_resets_schema() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        insert_entry(&write_conn, ROOT_ID, "x", false, Some(1));

        IndexStore::clear_all(&write_conn).unwrap();

        // Schema version should be re-stamped
        let version = IndexStore::get_meta(&write_conn, "schema_version").unwrap();
        assert_eq!(version.as_deref(), Some(SCHEMA_VERSION));

        // Entries should be gone (except root sentinel)
        let children = store.list_children(ROOT_ID).unwrap();
        assert!(children.is_empty());
    }

    #[test]
    fn schema_mismatch_triggers_reset() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("mismatch.db");

        // Create a store and tamper with the version
        {
            let store = IndexStore::open(&db_path).unwrap();
            let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();
            IndexStore::update_meta(&write_conn, "schema_version", "0").unwrap();
        }

        // Re-open: should detect mismatch and reset
        let store = IndexStore::open(&db_path).unwrap();
        let status = store.get_index_status().unwrap();
        assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
    }

    #[test]
    fn corruption_recovery_deletes_and_recreates() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("corrupt.db");

        // Write garbage to simulate corruption
        std::fs::write(&db_path, b"this is not a sqlite database").unwrap();

        // open() should recover by deleting and recreating
        let store = IndexStore::open(&db_path).unwrap();
        let status = store.get_index_status().unwrap();
        assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
    }

    #[test]
    fn db_file_size_returns_nonzero() {
        let (store, _dir) = open_temp_store();
        let size = store.db_file_size().unwrap();
        assert!(size > 0, "DB file should have nonzero size after creation");
    }

    #[test]
    fn get_all_directory_paths() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let a_id = insert_entry(&conn, ROOT_ID, "a", true, None);
        insert_entry(&conn, ROOT_ID, "b", true, None);
        insert_entry(&conn, a_id, "file.txt", false, Some(100));

        let dirs = IndexStore::get_all_directory_paths(&conn).unwrap();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&"/a".to_string()));
        assert!(dirs.contains(&"/b".to_string()));
    }

    #[test]
    fn empty_batch_operations_are_noops() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        IndexStore::insert_entries_v2_batch(&conn, &[]).unwrap();
        IndexStore::upsert_dir_stats_by_id(&conn, &[]).unwrap();
    }

    #[test]
    fn get_entry_by_id_found() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let test_id = insert_entry(&conn, ROOT_ID, "test", true, None);
        let file_id = IndexStore::insert_entry_v2(
            &conn,
            test_id,
            "file.txt",
            false,
            false,
            Some(512),
            Some(512),
            Some(1700000000),
            None,
        )
        .unwrap();

        let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap();
        assert!(result.is_some());
        let found = result.unwrap();
        assert_eq!(found.name, "file.txt");
        assert_eq!(found.logical_size, Some(512));
        assert!(!found.is_directory);
    }

    #[test]
    fn get_entry_by_id_not_found() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let result = IndexStore::get_entry_by_id(&conn, 99999).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_entry_modifies_in_place() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let test_id = insert_entry(&conn, ROOT_ID, "test", true, None);
        let file_id = IndexStore::insert_entry_v2(
            &conn,
            test_id,
            "file.txt",
            false,
            false,
            Some(100),
            Some(100),
            Some(1000),
            None,
        )
        .unwrap();

        let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
        assert_eq!(result.logical_size, Some(100));

        // Update with new size
        IndexStore::update_entry(&conn, file_id, false, false, Some(200), Some(200), Some(2000), None).unwrap();

        let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
        assert_eq!(result.logical_size, Some(200));
        assert_eq!(result.modified_at, Some(2000));
    }

    #[test]
    fn resolve_path_basic() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Root resolves to ROOT_ID
        assert_eq!(resolve_path(&conn, "/").unwrap(), Some(ROOT_ID));

        // Insert /Users/test
        let users_id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
        let test_id =
            IndexStore::insert_entry_v2(&conn, users_id, "test", true, false, None, None, None, None).unwrap();

        assert_eq!(resolve_path(&conn, "/Users").unwrap(), Some(users_id));
        assert_eq!(resolve_path(&conn, "/Users/test").unwrap(), Some(test_id));
        assert_eq!(resolve_path(&conn, "/nonexistent").unwrap(), None);
        assert_eq!(resolve_path(&conn, "/Users/nonexistent").unwrap(), None);
    }

    #[test]
    fn resolve_path_trailing_slash() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let users_id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
        assert_eq!(resolve_path(&conn, "/Users/").unwrap(), Some(users_id));
    }

    #[test]
    fn insert_entry_v2_and_get_by_id() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let id = IndexStore::insert_entry_v2(
            &conn,
            ROOT_ID,
            "myfile.txt",
            false,
            false,
            Some(4096),
            Some(4096),
            Some(999),
            None,
        )
        .unwrap();
        assert!(id > ROOT_ID);

        let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(entry.name, "myfile.txt");
        assert_eq!(entry.parent_id, ROOT_ID);
        assert!(!entry.is_directory);
        assert_eq!(entry.logical_size, Some(4096));
        assert_eq!(entry.modified_at, Some(999));
    }

    #[test]
    fn list_children_v2() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let dir_id =
            IndexStore::insert_entry_v2(&write_conn, ROOT_ID, "mydir", true, false, None, None, None, None).unwrap();
        IndexStore::insert_entry_v2(
            &write_conn,
            dir_id,
            "a.txt",
            false,
            false,
            Some(100),
            Some(100),
            None,
            None,
        )
        .unwrap();
        IndexStore::insert_entry_v2(
            &write_conn,
            dir_id,
            "b.txt",
            false,
            false,
            Some(200),
            Some(200),
            None,
            None,
        )
        .unwrap();

        let children = store.list_children(dir_id).unwrap();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn update_entry_v2() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let id = IndexStore::insert_entry_v2(
            &conn,
            ROOT_ID,
            "file.txt",
            false,
            false,
            Some(100),
            Some(100),
            Some(1000),
            None,
        )
        .unwrap();

        IndexStore::update_entry(&conn, id, false, false, Some(999), Some(999), Some(2000), None).unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(entry.logical_size, Some(999));
        assert_eq!(entry.modified_at, Some(2000));
    }

    #[test]
    fn rename_and_move_entry() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let dir_a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "dir_a", true, false, None, None, None, None).unwrap();
        let dir_b = IndexStore::insert_entry_v2(&conn, ROOT_ID, "dir_b", true, false, None, None, None, None).unwrap();
        let file_id =
            IndexStore::insert_entry_v2(&conn, dir_a, "old.txt", false, false, Some(50), Some(50), None, None).unwrap();

        // Rename
        IndexStore::rename_entry(&conn, file_id, "new.txt").unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
        assert_eq!(entry.name, "new.txt");

        // Move to dir_b
        IndexStore::move_entry(&conn, file_id, dir_b).unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
        assert_eq!(entry.parent_id, dir_b);
    }

    #[test]
    fn delete_entry_by_id_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let id = IndexStore::insert_entry_v2(
            &conn,
            ROOT_ID,
            "file.txt",
            false,
            false,
            Some(100),
            Some(100),
            None,
            None,
        )
        .unwrap();
        assert!(IndexStore::get_entry_by_id(&conn, id).unwrap().is_some());

        IndexStore::delete_entry_by_id(&conn, id).unwrap();
        assert!(IndexStore::get_entry_by_id(&conn, id).unwrap().is_none());
    }

    #[test]
    fn delete_subtree_by_id_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Build tree: /a/b/c.txt
        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None, None, None).unwrap();
        let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None, None, None).unwrap();
        let c = IndexStore::insert_entry_v2(&conn, b, "c.txt", false, false, Some(42), Some(42), None, None).unwrap();

        // Add dir_stats for a and b
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: a,
                    recursive_logical_size: 42,
                    recursive_physical_size: 42,
                    recursive_file_count: 1,
                    recursive_dir_count: 1,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
                DirStatsById {
                    entry_id: b,
                    recursive_logical_size: 42,
                    recursive_physical_size: 42,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
            ],
        )
        .unwrap();

        // Delete subtree rooted at /a
        IndexStore::delete_subtree_by_id(&conn, a).unwrap();

        assert!(IndexStore::get_entry_by_id(&conn, a).unwrap().is_none());
        assert!(IndexStore::get_entry_by_id(&conn, b).unwrap().is_none());
        assert!(IndexStore::get_entry_by_id(&conn, c).unwrap().is_none());
        assert!(IndexStore::get_dir_stats_by_id(&conn, a).unwrap().is_none());
        assert!(IndexStore::get_dir_stats_by_id(&conn, b).unwrap().is_none());
    }

    #[test]
    fn subtree_totals_by_id() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None, None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, a, "f1.txt", false, false, Some(100), Some(100), None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, a, "f2.txt", false, false, Some(200), Some(200), None, None).unwrap();
        let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None, None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, b, "f3.txt", false, false, Some(300), Some(300), None, None).unwrap();

        let (logical_size, physical_size, file_count, dir_count) =
            IndexStore::get_subtree_totals_by_id(&conn, a).unwrap();
        assert_eq!(logical_size, 600);
        assert_eq!(physical_size, 600);
        assert_eq!(file_count, 3);
        assert_eq!(dir_count, 2); // a + b
    }

    #[test]
    fn dir_stats_by_id_roundtrip() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let dir_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "mydir", true, false, None, None, None, None).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: dir_id,
                recursive_logical_size: 12345,
                recursive_physical_size: 12345,
                recursive_file_count: 10,
                recursive_dir_count: 3,
                recursive_has_symlinks: false,
                min_subtree_epoch: 0,
            }],
        )
        .unwrap();

        let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap().unwrap();
        assert_eq!(stats.recursive_logical_size, 12345);
        assert_eq!(stats.recursive_file_count, 10);
        assert_eq!(stats.recursive_dir_count, 3);
    }

    #[test]
    fn dir_stats_batch_by_ids() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let d1 = IndexStore::insert_entry_v2(&conn, ROOT_ID, "d1", true, false, None, None, None, None).unwrap();
        let d2 = IndexStore::insert_entry_v2(&conn, ROOT_ID, "d2", true, false, None, None, None, None).unwrap();

        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: d1,
                    recursive_logical_size: 100,
                    recursive_physical_size: 100,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
                DirStatsById {
                    entry_id: d2,
                    recursive_logical_size: 200,
                    recursive_physical_size: 200,
                    recursive_file_count: 2,
                    recursive_dir_count: 1,
                    recursive_has_symlinks: false,
                    min_subtree_epoch: 0,
                },
            ],
        )
        .unwrap();

        let result = IndexStore::get_dir_stats_batch_by_ids(&conn, &[d1, 99999, d2]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].is_some());
        assert!(result[1].is_none());
        assert!(result[2].is_some());
        assert_eq!(result[0].as_ref().unwrap().recursive_logical_size, 100);
        assert_eq!(result[2].as_ref().unwrap().recursive_logical_size, 200);
    }

    #[test]
    fn get_next_id() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Root sentinel is id=1, so next should be 2
        let next = IndexStore::get_next_id(&conn).unwrap();
        assert_eq!(next, 2);

        IndexStore::insert_entry_v2(&conn, ROOT_ID, "file.txt", false, false, None, None, None, None).unwrap();
        let next = IndexStore::get_next_id(&conn).unwrap();
        assert!(next >= 3);
    }

    #[test]
    fn reconstruct_path_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        assert_eq!(IndexStore::reconstruct_path(&conn, ROOT_ID).unwrap(), "/");

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
        let foo = IndexStore::insert_entry_v2(&conn, users, "foo", true, false, None, None, None, None).unwrap();
        let file =
            IndexStore::insert_entry_v2(&conn, foo, "bar.txt", false, false, Some(10), Some(10), None, None).unwrap();

        assert_eq!(IndexStore::reconstruct_path(&conn, users).unwrap(), "/Users");
        assert_eq!(IndexStore::reconstruct_path(&conn, foo).unwrap(), "/Users/foo");
        assert_eq!(IndexStore::reconstruct_path(&conn, file).unwrap(), "/Users/foo/bar.txt");
    }

    #[test]
    fn resolve_component_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
        assert_eq!(
            IndexStore::resolve_component(&conn, ROOT_ID, "Users").unwrap(),
            Some(users)
        );
        assert_eq!(
            IndexStore::resolve_component(&conn, ROOT_ID, "nonexistent").unwrap(),
            None
        );
    }

    #[test]
    fn get_parent_id_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
        assert_eq!(IndexStore::get_parent_id(&conn, users).unwrap(), Some(ROOT_ID));
        assert_eq!(IndexStore::get_parent_id(&conn, ROOT_ID).unwrap(), Some(ROOT_PARENT_ID));
        assert_eq!(IndexStore::get_parent_id(&conn, 999999).unwrap(), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn platform_case_collation_macos() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Insert "Users" dir
        let users_id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();

        // Resolve with different case should work on macOS
        assert_eq!(resolve_path(&conn, "/users").unwrap(), Some(users_id));
        assert_eq!(resolve_path(&conn, "/USERS").unwrap(), Some(users_id));
        assert_eq!(resolve_path(&conn, "/Users").unwrap(), Some(users_id));

        // Schema v12 reinstated UNIQUE on (parent_id, name_folded). On macOS
        // `normalize_for_comparison("Users") == normalize_for_comparison("users")`
        // (NFD + case fold), so this insert must collide.
        let result = IndexStore::insert_entry_v2(&conn, ROOT_ID, "users", true, false, None, None, None, None);
        assert!(
            result.is_err(),
            "case-variant insert must collide on the UNIQUE (parent_id, name_folded) index; got {result:?}"
        );
    }

    #[test]
    fn insert_entries_v2_batch_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let entries = vec![
            EntryRow {
                id: 100,
                parent_id: ROOT_ID,
                name: "dir1".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 101,
                parent_id: 100,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(42),
                physical_size: Some(42),
                modified_at: Some(1234),
                inode: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).unwrap();

        let entry = IndexStore::get_entry_by_id(&conn, 100).unwrap().unwrap();
        assert_eq!(entry.name, "dir1");
        assert!(entry.is_directory);

        let entry = IndexStore::get_entry_by_id(&conn, 101).unwrap().unwrap();
        assert_eq!(entry.name, "file.txt");
        assert_eq!(entry.logical_size, Some(42));
    }

    // Duplicate (parent_id, name_folded) must be rejected by the schema.
    // The aggregator walks parent_id chains and sums every row; a duplicate would
    // double-count its size into ancestor dir_stats. Schema v12 reinstated the
    // UNIQUE constraint that v5 dropped for collation-cost reasons (since v6,
    // `name_folded` carries pre-folded bytes, so binary collation is fine).
    #[test]
    fn duplicate_parent_name_folded_rejected_individual_insert() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        IndexStore::insert_entry_v2(&conn, ROOT_ID, "dup.txt", false, false, Some(10), Some(10), None, None).unwrap();
        let second =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "dup.txt", false, false, Some(10), Some(10), None, None);
        assert!(
            second.is_err(),
            "second insert with same (parent_id, name_folded) must fail; got {second:?}"
        );
    }

    /// Batch insert uses `INSERT OR IGNORE`: a duplicate `(parent_id, name_folded)`
    /// in the batch (or against an existing row) skips just that row, keeping
    /// every other entry in the batch. The returned `Vec<bool>` flags which
    /// rows actually landed. This replaces the previous roll-back-the-whole-batch
    /// behavior, which silently dropped ~2000 unrelated entries every time a
    /// scan encountered two siblings with colliding `name_folded` (case-sensitive
    /// volumes, NFC/NFD duplicates, etc.).
    #[test]
    fn duplicate_parent_name_folded_skipped_in_batch_insert() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let entries = vec![
            EntryRow {
                id: 100,
                parent_id: ROOT_ID,
                name: "dup.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(10),
                physical_size: Some(10),
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 101,
                parent_id: ROOT_ID,
                name: "dup.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(20),
                physical_size: Some(20),
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 102,
                parent_id: ROOT_ID,
                name: "unrelated.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(30),
                physical_size: Some(30),
                modified_at: None,
                inode: None,
            },
        ];
        let inserted = IndexStore::insert_entries_v2_batch(&conn, &entries).unwrap();
        assert_eq!(inserted, vec![true, false, true]);

        // First duplicate wins; the second is dropped; the unrelated entry survives.
        // Without the per-row skip, the savepoint used to roll back ALL THREE.
        assert!(IndexStore::get_entry_by_id(&conn, 100).unwrap().is_some());
        assert!(IndexStore::get_entry_by_id(&conn, 101).unwrap().is_none());
        assert!(IndexStore::get_entry_by_id(&conn, 102).unwrap().is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn resolve_component_case_insensitive() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let users_id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();

        // Different casings should all resolve to the same ID
        assert_eq!(
            IndexStore::resolve_component(&conn, ROOT_ID, "users").unwrap(),
            Some(users_id)
        );
        assert_eq!(
            IndexStore::resolve_component(&conn, ROOT_ID, "USERS").unwrap(),
            Some(users_id)
        );
        assert_eq!(
            IndexStore::resolve_component(&conn, ROOT_ID, "uSeRs").unwrap(),
            Some(users_id)
        );

        // Nonexistent name returns None
        assert_eq!(
            IndexStore::resolve_component(&conn, ROOT_ID, "nonexistent").unwrap(),
            None
        );
    }

    #[test]
    fn name_folded_populated_on_single_insert() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let name = "MyFolder";
        let id = IndexStore::insert_entry_v2(&conn, ROOT_ID, name, true, false, None, None, None, None).unwrap();

        let folded: String = conn
            .query_row("SELECT name_folded FROM entries WHERE id = ?1", params![id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(folded, normalize_for_comparison(name));
    }

    #[test]
    fn name_folded_populated_on_batch_insert() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let entries = vec![
            EntryRow {
                id: 200,
                parent_id: ROOT_ID,
                name: "Documents".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: None,
            },
            EntryRow {
                id: 201,
                parent_id: 200,
                name: "Café.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(10),
                physical_size: Some(10),
                modified_at: None,
                inode: None,
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).unwrap();

        for e in &entries {
            let folded: String = conn
                .query_row("SELECT name_folded FROM entries WHERE id = ?1", params![e.id], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(folded, normalize_for_comparison(&e.name));
        }
    }

    #[test]
    fn get_children_stats_by_id_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let dir_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "mydir", true, false, None, None, None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, dir_id, "f1.txt", false, false, Some(100), Some(100), None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, dir_id, "f2.txt", false, false, Some(200), Some(200), None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, dir_id, "subdir", true, false, None, None, None, None).unwrap();

        let (logical_size, physical_size, files, dirs) = IndexStore::get_children_stats_by_id(&conn, dir_id).unwrap();
        assert_eq!(logical_size, 300);
        assert_eq!(physical_size, 300);
        assert_eq!(files, 2);
        assert_eq!(dirs, 1);
    }

    #[test]
    fn deeply_nested_path_resolution() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Create /a/b/c/d/e/f/g/h/i/j (10 levels deep)
        let mut parent_id = ROOT_ID;
        let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        let mut ids = Vec::new();
        for name in &names {
            let id = IndexStore::insert_entry_v2(&conn, parent_id, name, true, false, None, None, None, None).unwrap();
            ids.push(id);
            parent_id = id;
        }

        // Resolve full path
        let path = "/a/b/c/d/e/f/g/h/i/j";
        assert_eq!(resolve_path(&conn, path).unwrap(), Some(*ids.last().unwrap()));

        // Reconstruct from deepest
        let reconstructed = IndexStore::reconstruct_path(&conn, *ids.last().unwrap()).unwrap();
        assert_eq!(reconstructed, path);

        // Partial path
        assert_eq!(resolve_path(&conn, "/a/b/c").unwrap(), Some(ids[2]));
    }

    // ── has_sized_entry_for_inode tests ──────────────────────────────

    /// Helper: insert an entry with explicit inode and size. Returns the new ID.
    fn insert_entry_with_inode(
        conn: &Connection,
        parent_id: i64,
        name: &str,
        size: Option<u64>,
        inode: Option<u64>,
    ) -> i64 {
        IndexStore::insert_entry_v2(conn, parent_id, name, false, false, size, size, None, inode).unwrap()
    }

    #[test]
    fn has_sized_entry_for_inode_returns_false_when_no_entry() {
        let (_store, dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

        let result = IndexStore::has_sized_entry_for_inode(&conn, 12345, None).unwrap();
        assert!(!result);
    }

    #[test]
    fn has_sized_entry_for_inode_returns_true_when_sized_entry_exists() {
        let (_store, dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

        insert_entry_with_inode(&conn, ROOT_ID, "primary.txt", Some(1000), Some(100));

        assert!(IndexStore::has_sized_entry_for_inode(&conn, 100, None).unwrap());
    }

    #[test]
    fn has_sized_entry_for_inode_returns_false_when_sizes_are_null() {
        let (_store, dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

        // Secondary link: same inode but NULL sizes (deduped)
        insert_entry_with_inode(&conn, ROOT_ID, "secondary.txt", None, Some(100));

        assert!(!IndexStore::has_sized_entry_for_inode(&conn, 100, None).unwrap());
    }

    #[test]
    fn has_sized_entry_for_inode_exclude_id_skips_self() {
        let (_store, dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

        let id = insert_entry_with_inode(&conn, ROOT_ID, "only.txt", Some(1000), Some(100));

        // Excluding the only sized entry should return false
        assert!(!IndexStore::has_sized_entry_for_inode(&conn, 100, Some(id)).unwrap());
        // Without excluding, it should return true
        assert!(IndexStore::has_sized_entry_for_inode(&conn, 100, None).unwrap());
    }

    // ====================================================================
    // platform_case_compare / normalize_for_comparison
    //
    // The collation function backs SQLite's `platform_case` collation, which
    // every path-resolution query relies on. cargo-mutants showed the
    // structural mutants `platform_case_compare -> Default::default()` and
    // `normalize_for_comparison -> String::new() / "xyzzy".into()` survive
    // when the only test exercises one direction of equality.
    // ====================================================================

    #[cfg(target_os = "macos")]
    #[test]
    fn platform_case_compare_distinguishes_distinct_names() {
        // Kills: replace platform_case_compare -> Default::default() (which is
        // Ordering::Equal, so every comparison would say "equal"; sort order
        // and SQLite's collation-driven uniqueness would collapse).
        assert_eq!(platform_case_compare("a", "a"), std::cmp::Ordering::Equal);
        assert_eq!(platform_case_compare("a", "b"), std::cmp::Ordering::Less);
        assert_eq!(platform_case_compare("b", "a"), std::cmp::Ordering::Greater);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn platform_case_compare_case_insensitive_on_macos() {
        // APFS is case-preserving but case-insensitive by default. The
        // collation must report equality across case variants for path
        // resolution to work.
        assert_eq!(platform_case_compare("Users", "users"), std::cmp::Ordering::Equal);
        assert_eq!(
            platform_case_compare("README.MD", "readme.md"),
            std::cmp::Ordering::Equal
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn platform_case_compare_normalizes_unicode_nfc_to_nfd() {
        // "é" can be one codepoint (NFC, U+00E9) or two (NFD, U+0065 U+0301).
        // APFS stores NFD; the collation must treat the two representations
        // as equal so a user typing NFC resolves NFD-stored entries.
        let nfc = "café"; // typically NFC in Rust source
        let nfd = "cafe\u{0301}"; // 'e' + combining acute
        // Make sure they're actually different byte sequences (sanity check).
        assert_ne!(nfc.as_bytes(), nfd.as_bytes());
        assert_eq!(
            platform_case_compare(nfc, nfd),
            std::cmp::Ordering::Equal,
            "NFC and NFD forms of 'café' must compare equal on APFS"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn platform_case_compare_is_binary_off_macos() {
        // Linux ext4/btrfs: exact byte comparison, NOT case-folded.
        assert_eq!(platform_case_compare("a", "a"), std::cmp::Ordering::Equal);
        assert_eq!(platform_case_compare("Users", "users"), std::cmp::Ordering::Less);
        // ('U' = 0x55, 'u' = 0x75 → 'U' < 'u' in ASCII, so "Users" < "users".)
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn normalize_for_comparison_lowercases_and_nfd_normalizes() {
        // Kills: replace normalize_for_comparison -> String::new() / "xyzzy".
        assert_eq!(normalize_for_comparison("Users"), "users");
        let nfc = "café";
        let nfd = "cafe\u{0301}";
        // After normalization, both should be NFD-lowercased and equal.
        assert_eq!(normalize_for_comparison(nfc), normalize_for_comparison(nfd));
        assert!(
            !normalize_for_comparison("hello").is_empty(),
            "normalize_for_comparison must not return an empty string for non-empty input"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn normalize_for_comparison_is_identity_off_macos() {
        assert_eq!(normalize_for_comparison("Users"), "Users");
        assert_eq!(normalize_for_comparison("hello"), "hello");
    }

    // ── platform_case_compare (property-based) ───────────────────────
    //
    // The collation is used on every `entries.name` comparison in the
    // SQLite index. A bug in the comparator would corrupt the index's
    // sort order and, worse, cause `resolve_path` to fail to find
    // entries the user typed in a different case or Unicode form.
    // These properties pin the comparator algebra (reflexivity,
    // antisymmetry, transitivity) plus the platform-specific normalization
    // semantics (NFC≡NFD on macOS, byte-equal off macOS).

    mod platform_case_proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Reflexivity: `cmp(a, a) == Equal` for any string.
            #[test]
            fn reflexivity(s in ".*") {
                prop_assert_eq!(platform_case_compare(&s, &s), std::cmp::Ordering::Equal);
            }

            /// Antisymmetry: `cmp(a, b)` and `cmp(b, a)` must be reverses
            /// of each other.
            #[test]
            fn antisymmetry(a in ".*", b in ".*") {
                let ab = platform_case_compare(&a, &b);
                let ba = platform_case_compare(&b, &a);
                prop_assert_eq!(
                    ab,
                    ba.reverse(),
                    "cmp({:?}, {:?}) = {:?} but cmp({:?}, {:?}) = {:?} should be its reverse",
                    a, b, ab, b, a, ba
                );
            }

            /// Transitivity: if `cmp(a, b) <= 0` and `cmp(b, c) <= 0`,
            /// then `cmp(a, c) <= 0`. We also check the strict-less and
            /// equal flavors.
            #[test]
            fn transitivity(a in ".*", b in ".*", c in ".*") {
                use std::cmp::Ordering::*;
                let ab = platform_case_compare(&a, &b);
                let bc = platform_case_compare(&b, &c);
                let ac = platform_case_compare(&a, &c);
                if ab != Greater && bc != Greater {
                    prop_assert!(
                        ac != Greater,
                        "transitivity violated: cmp(a,b)={:?} cmp(b,c)={:?} cmp(a,c)={:?} for a={:?} b={:?} c={:?}",
                        ab, bc, ac, a, b, c
                    );
                }
                if ab != Less && bc != Less {
                    prop_assert!(
                        ac != Less,
                        "transitivity violated (>=): cmp(a,b)={:?} cmp(b,c)={:?} cmp(a,c)={:?}",
                        ab, bc, ac
                    );
                }
            }
        }

        // On macOS, NFC and NFD forms of the same logical string must
        // compare equal: APFS stores NFD, but users may type NFC, and
        // `resolve_path` must find the stored entry either way.
        #[cfg(target_os = "macos")]
        proptest! {
            #[test]
            fn nfc_equals_nfd_on_macos(s in ".*") {
                use unicode_normalization::UnicodeNormalization;
                let nfc: String = s.nfc().collect();
                let nfd: String = s.nfd().collect();
                prop_assert_eq!(
                    platform_case_compare(&nfc, &nfd),
                    std::cmp::Ordering::Equal,
                    "NFC {:?} and NFD {:?} of {:?} must compare equal on APFS",
                    nfc, nfd, s
                );
            }
        }

        // Off macOS, the comparator is exact byte comparison. We pin
        // this by checking that the result matches `str::cmp`.
        #[cfg(not(target_os = "macos"))]
        proptest! {
            #[test]
            fn matches_byte_cmp_off_macos(a in ".*", b in ".*") {
                prop_assert_eq!(platform_case_compare(&a, &b), a.cmp(&b));
            }
        }
    }

    #[test]
    fn has_sized_entry_for_inode_multiple_entries_one_has_sizes() {
        let (_store, dir) = open_temp_store();
        let conn = IndexStore::open_write_connection(&dir.path().join("test-index.db")).unwrap();

        let primary_id = insert_entry_with_inode(&conn, ROOT_ID, "primary.txt", Some(1000), Some(100));
        let secondary_id = insert_entry_with_inode(&conn, ROOT_ID, "secondary.txt", None, Some(100));

        // From secondary's perspective (exclude self): primary has sizes
        assert!(IndexStore::has_sized_entry_for_inode(&conn, 100, Some(secondary_id)).unwrap());
        // From primary's perspective (exclude self): secondary has no sizes
        assert!(!IndexStore::has_sized_entry_for_inode(&conn, 100, Some(primary_id)).unwrap());
    }
}
