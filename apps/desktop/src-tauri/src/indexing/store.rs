//! SQLite store for the drive index.
//!
//! One DB file per indexed volume. Uses WAL mode for concurrent reads.
//! All writes go through a dedicated writer thread (see `writer.rs`);
//! this module provides the schema, read queries, and static write helpers.
//!
//! ## Schema v2: integer-keyed parent-child tree
//!
//! Entries use an integer primary key (`id`) with a `parent_id` foreign key.
//! The `name` column uses `COLLATE platform_case` — a custom collation registered
//! at connection init that matches the filesystem's case/normalization rules:
//! - **macOS**: case-insensitive + NFD normalization (matching APFS)
//! - **Linux**: binary comparison (matching ext4/btrfs)
//!
//! **Tooling note**: Opening the DB with the `sqlite3` CLI or any tool that
//! doesn't register the `platform_case` collation will fail on queries touching
//! the `name` column or `idx_parent_name` index.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: &str = "3";

/// Root entry sentinel ID. All top-level entries have `parent_id = ROOT_ID`.
pub const ROOT_ID: i64 = 1;

/// Parent ID of the root sentinel. No row with this ID exists in the DB.
const ROOT_PARENT_ID: i64 = 0;

// ── Types ────────────────────────────────────────────────────────────

/// Dir stats keyed by path string. Used at the IPC boundary and by
/// the IPC boundary (frontend expects path-keyed dir stats).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirStats {
    pub path: String,
    pub recursive_size: u64,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
}

/// Dir stats keyed by entry ID. Used internally by the integer-keyed store.
#[derive(Debug, Clone)]
pub struct DirStatsById {
    pub entry_id: i64,
    pub recursive_size: u64,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
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
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
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
    /// Next ID to assign. Always >= 2 (root sentinel is 1).
    pub next_id: i64,
}

impl ScanContext {
    /// Create a new scan context, seeding the map with the root's entry ID
    /// and fetching `next_id` from the DB.
    ///
    /// `is_volume_root`: true for full volume scans (always maps root → ROOT_ID).
    /// When false (subtree scans), resolves the root's actual entry ID from the DB.
    /// Returns an error if the root isn't indexed yet (for example, a micro-scan
    /// racing with an ongoing full scan — the full scan will cover it).
    pub fn new(conn: &Connection, root: &Path, is_volume_root: bool) -> Result<Self, IndexStoreError> {
        // Only volume-root scans need to create the sentinel — subtree scans
        // run after the full scan has already inserted it, and their connection
        // may be read-only or contending with the writer thread's write lock.
        if is_volume_root {
            ensure_root_sentinel(conn)?;
        }

        let next_id = IndexStore::get_next_id(conn)?;

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
        let id = self.next_id;
        self.next_id += 1;
        id
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub schema_version: Option<String>,
    pub volume_path: Option<String>,
    pub scan_completed_at: Option<String>,
    pub scan_duration_ms: Option<String>,
    pub total_entries: Option<String>,
    pub last_event_id: Option<String>,
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
fn register_platform_case_collation(conn: &Connection) -> Result<(), IndexStoreError> {
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

/// Normalize a string for case-insensitive comparison (for cache keys).
/// Public so `PathResolver` can use the same normalization.
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
        id           INTEGER PRIMARY KEY,
        parent_id    INTEGER NOT NULL,
        name         TEXT    NOT NULL COLLATE platform_case,
        is_directory INTEGER NOT NULL DEFAULT 0,
        is_symlink   INTEGER NOT NULL DEFAULT 0,
        size         INTEGER,
        modified_at  INTEGER
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_parent_name ON entries (parent_id, name);

    CREATE TABLE IF NOT EXISTS dir_stats (
        entry_id             INTEGER PRIMARY KEY,
        recursive_size       INTEGER NOT NULL DEFAULT 0,
        recursive_file_count INTEGER NOT NULL DEFAULT 0,
        recursive_dir_count  INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
";

/// Insert the root sentinel entry if it doesn't exist.
fn ensure_root_sentinel(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute(
        "INSERT OR IGNORE INTO entries (id, parent_id, name, is_directory) VALUES (?1, ?2, '', 1)",
        params![ROOT_ID, ROOT_PARENT_ID],
    )?;
    Ok(())
}

/// Apply WAL-mode pragmas for performance.
fn apply_pragmas(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
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
        let mut stmt =
            conn.prepare_cached("SELECT id FROM entries WHERE parent_id = ?1 AND name = ?2 COLLATE platform_case")?;
        match stmt
            .query_row(params![current_id, component], |row| row.get::<_, i64>(0))
            .optional()?
        {
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

impl IndexStore {
    /// Open (or create) the index database at `db_path`.
    ///
    /// Registers the `platform_case` collation, runs WAL pragmas, creates tables
    /// if missing, and checks the schema version. On version mismatch or corruption
    /// the DB file is deleted and recreated.
    pub fn open(db_path: &Path) -> Result<Self, IndexStoreError> {
        match Self::try_open(db_path) {
            Ok(store) => Ok(store),
            Err(e) => {
                log::warn!("Index DB open failed ({e}), deleting and recreating");
                Self::delete_and_recreate(db_path)
            }
        }
    }

    /// Attempt to open the DB without the delete-and-recreate fallback.
    fn try_open(db_path: &Path) -> Result<Self, IndexStoreError> {
        let conn = Connection::open(db_path)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn)?;
        create_tables(&conn)?;

        // Check schema version
        let version = Self::read_meta_value(&conn, "schema_version")?;
        match version {
            Some(v) if v == SCHEMA_VERSION => { /* all good */ }
            Some(v) => {
                log::warn!("Schema version mismatch (expected {SCHEMA_VERSION}, found {v}), resetting");
                reset_schema(&conn)?;
            }
            None => {
                // Fresh DB, stamp the version
                conn.execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                    params!["schema_version", SCHEMA_VERSION],
                )?;
            }
        }

        Ok(Self {
            db_path: db_path.to_path_buf(),
            read_conn: conn,
        })
    }

    /// Delete the DB file and create a fresh one.
    fn delete_and_recreate(db_path: &Path) -> Result<Self, IndexStoreError> {
        // Remove the main DB file
        if db_path.exists() {
            std::fs::remove_file(db_path)?;
        }
        // Always attempt to remove WAL and SHM sidecars (they can be stale even
        // if the base DB was already deleted).
        let wal = db_path.with_extension("db-wal");
        let shm = db_path.with_extension("db-shm");
        if wal.exists() {
            let _ = std::fs::remove_file(&wal);
        }
        if shm.exists() {
            let _ = std::fs::remove_file(&shm);
        }

        let conn = Connection::open(db_path)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn)?;
        create_tables(&conn)?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params!["schema_version", SCHEMA_VERSION],
        )?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
            read_conn: conn,
        })
    }

    /// Open a separate write connection with WAL pragmas and `platform_case` collation.
    ///
    /// Used by the writer thread; callers own the returned connection.
    pub fn open_write_connection(db_path: &Path) -> Result<Connection, IndexStoreError> {
        let conn = Connection::open(db_path)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn)?;
        Ok(conn)
    }

    /// Open a read-only connection with WAL pragmas and `platform_case` collation.
    ///
    /// Never contends with the writer thread's write lock.
    pub fn open_read_connection(db_path: &Path) -> Result<Connection, IndexStoreError> {
        let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn)?;
        Ok(conn)
    }

    /// Read all meta keys and return the index status.
    pub fn get_index_status(&self) -> Result<IndexStatus, IndexStoreError> {
        Ok(IndexStatus {
            schema_version: Self::read_meta_value(&self.read_conn, "schema_version")?,
            volume_path: Self::read_meta_value(&self.read_conn, "volume_path")?,
            scan_completed_at: Self::read_meta_value(&self.read_conn, "scan_completed_at")?,
            scan_duration_ms: Self::read_meta_value(&self.read_conn, "scan_duration_ms")?,
            total_entries: Self::read_meta_value(&self.read_conn, "total_entries")?,
            last_event_id: Self::read_meta_value(&self.read_conn, "last_event_id")?,
        })
    }

    /// Return the path to the DB file.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Borrow the underlying read connection for direct queries.
    ///
    /// Used by `enrich_entries_with_index` for integer-keyed lookups on the
    /// global read-only store. The connection is WAL-mode, so reads don't
    /// block the writer.
    pub fn read_conn(&self) -> &Connection {
        &self.read_conn
    }

    /// Return the DB file size on disk (bytes).
    pub fn db_file_size(&self) -> Result<u64, IndexStoreError> {
        Ok(std::fs::metadata(&self.db_path)?.len())
    }

    // ── Read methods (integer-keyed, new API) ────────────────────────

    /// List children of a directory by parent entry ID.
    pub fn list_children(&self, parent_id: i64) -> Result<Vec<EntryRow>, IndexStoreError> {
        Self::list_children_on(parent_id, &self.read_conn)
    }

    /// List children of a directory by parent entry ID on a given connection.
    pub fn list_children_on(parent_id: i64, conn: &Connection) -> Result<Vec<EntryRow>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, is_directory, is_symlink, size, modified_at
             FROM entries WHERE parent_id = ?1",
        )?;
        let rows = stmt.query_map(params![parent_id], |row| {
            Ok(EntryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                is_directory: row.get::<_, i32>(3)? != 0,
                is_symlink: row.get::<_, i32>(4)? != 0,
                size: row.get(5)?,
                modified_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List `(id, name)` pairs of child directories for a given parent entry ID.
    ///
    /// Used by `enrich_entries_with_index` to batch-fetch dir_stats for all
    /// subdirectories visible in a listing, then map back by name.
    pub fn list_child_dir_ids_and_names(
        conn: &Connection,
        parent_id: i64,
    ) -> Result<Vec<(i64, String)>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT id, name FROM entries WHERE parent_id = ?1 AND is_directory = 1")?;
        let rows = stmt.query_map(params![parent_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Look up an entry by its integer ID.
    pub fn get_entry_by_id(conn: &Connection, id: i64) -> Result<Option<EntryRow>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, parent_id, name, is_directory, is_symlink, size, modified_at
             FROM entries WHERE id = ?1",
        )?;
        let result = stmt
            .query_row(params![id], |row| {
                Ok(EntryRow {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    is_directory: row.get::<_, i32>(3)? != 0,
                    is_symlink: row.get::<_, i32>(4)? != 0,
                    size: row.get(5)?,
                    modified_at: row.get(6)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Look up dir_stats for a single entry by ID.
    pub fn get_dir_stats_by_id(conn: &Connection, entry_id: i64) -> Result<Option<DirStatsById>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT entry_id, recursive_size, recursive_file_count, recursive_dir_count
             FROM dir_stats WHERE entry_id = ?1",
        )?;
        let result = stmt
            .query_row(params![entry_id], |row| {
                Ok(DirStatsById {
                    entry_id: row.get(0)?,
                    recursive_size: row.get(1)?,
                    recursive_file_count: row.get(2)?,
                    recursive_dir_count: row.get(3)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Batch lookup of dir_stats for multiple entry IDs.
    ///
    /// Returns a `Vec` with the same length as `entry_ids`, where each element
    /// is `Some(DirStatsById)` if found or `None` otherwise.
    pub fn get_dir_stats_batch_by_ids(
        conn: &Connection,
        entry_ids: &[i64],
    ) -> Result<Vec<Option<DirStatsById>>, IndexStoreError> {
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }

        if entry_ids.len() <= 20 {
            return entry_ids
                .iter()
                .map(|id| Self::get_dir_stats_by_id(conn, *id))
                .collect();
        }

        let placeholders: String = (0..entry_ids.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT entry_id, recursive_size, recursive_file_count, recursive_dir_count
             FROM dir_stats WHERE entry_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let param_values: Vec<&dyn rusqlite::types::ToSql> =
            entry_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();

        let rows = stmt.query_map(&*param_values, |row| {
            Ok(DirStatsById {
                entry_id: row.get(0)?,
                recursive_size: row.get(1)?,
                recursive_file_count: row.get(2)?,
                recursive_dir_count: row.get(3)?,
            })
        })?;

        let mut map = std::collections::HashMap::new();
        for row in rows {
            let stats = row?;
            map.insert(stats.entry_id, stats);
        }

        Ok(entry_ids.iter().map(|id| map.remove(id)).collect())
    }

    /// Get the parent ID of an entry.
    pub fn get_parent_id(conn: &Connection, entry_id: i64) -> Result<Option<i64>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT parent_id FROM entries WHERE id = ?1")?;
        let result = stmt
            .query_row(params![entry_id], |row| row.get::<_, i64>(0))
            .optional()?;
        Ok(result)
    }

    /// Resolve a path component under a given parent. Returns the child entry ID.
    pub fn resolve_component(conn: &Connection, parent_id: i64, name: &str) -> Result<Option<i64>, IndexStoreError> {
        let mut stmt =
            conn.prepare_cached("SELECT id FROM entries WHERE parent_id = ?1 AND name = ?2 COLLATE platform_case")?;
        let result = stmt
            .query_row(params![parent_id, name], |row| row.get::<_, i64>(0))
            .optional()?;
        Ok(result)
    }

    /// Reconstruct the full path for an entry by walking up the parent chain.
    #[cfg(test)]
    pub fn reconstruct_path(conn: &Connection, entry_id: i64) -> Result<String, IndexStoreError> {
        reconstruct_path(conn, entry_id)
    }

    // ── Static write helpers (for the writer thread) ─────────────────

    /// Insert a single entry by integer keys. Returns the new entry's ID.
    pub fn insert_entry_v2(
        conn: &Connection,
        parent_id: i64,
        name: &str,
        is_directory: bool,
        is_symlink: bool,
        size: Option<u64>,
        modified_at: Option<u64>,
    ) -> Result<i64, IndexStoreError> {
        conn.execute(
            "INSERT INTO entries (parent_id, name, is_directory, is_symlink, size, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                parent_id,
                name,
                is_directory as i32,
                is_symlink as i32,
                size,
                modified_at
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Batch insert entries with pre-assigned IDs inside a transaction.
    pub fn insert_entries_v2_batch(conn: &Connection, entries: &[EntryRow]) -> Result<(), IndexStoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO entries (id, parent_id, name, is_directory, is_symlink, size, modified_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for e in entries {
                stmt.execute(params![
                    e.id,
                    e.parent_id,
                    e.name,
                    e.is_directory as i32,
                    e.is_symlink as i32,
                    e.size,
                    e.modified_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Update an existing entry by ID.
    pub fn update_entry(
        conn: &Connection,
        id: i64,
        is_directory: bool,
        is_symlink: bool,
        size: Option<u64>,
        modified_at: Option<u64>,
    ) -> Result<(), IndexStoreError> {
        conn.execute(
            "UPDATE entries SET is_directory = ?1, is_symlink = ?2, size = ?3, modified_at = ?4
             WHERE id = ?5",
            params![is_directory as i32, is_symlink as i32, size, modified_at, id],
        )?;
        Ok(())
    }

    /// Rename an entry (update its name).
    #[cfg(test)]
    pub fn rename_entry(conn: &Connection, id: i64, new_name: &str) -> Result<(), IndexStoreError> {
        conn.execute("UPDATE entries SET name = ?1 WHERE id = ?2", params![new_name, id])?;
        Ok(())
    }

    /// Move an entry to a new parent.
    #[cfg(test)]
    pub fn move_entry(conn: &Connection, id: i64, new_parent_id: i64) -> Result<(), IndexStoreError> {
        conn.execute(
            "UPDATE entries SET parent_id = ?1 WHERE id = ?2",
            params![new_parent_id, id],
        )?;
        Ok(())
    }

    /// Batch upsert dir_stats by entry ID inside a transaction.
    pub fn upsert_dir_stats_by_id(conn: &Connection, stats: &[DirStatsById]) -> Result<(), IndexStoreError> {
        if stats.is_empty() {
            return Ok(());
        }
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO dir_stats
                     (entry_id, recursive_size, recursive_file_count, recursive_dir_count)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for s in stats {
                stmt.execute(params![
                    s.entry_id,
                    s.recursive_size,
                    s.recursive_file_count,
                    s.recursive_dir_count,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Set a meta key-value pair.
    pub fn update_meta(conn: &Connection, key: &str, value: &str) -> Result<(), IndexStoreError> {
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get a single meta value by key.
    #[cfg(test)]
    pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>, IndexStoreError> {
        Self::read_meta_value(conn, key)
    }

    /// Get all directory paths from the entries table.
    ///
    /// Reconstructs full paths from the integer-keyed tree for backward compat.
    #[cfg(test)]
    pub fn get_all_directory_paths(conn: &Connection) -> Result<Vec<String>, IndexStoreError> {
        // Collect all directory (id, parent_id, name) tuples, then reconstruct paths in memory
        let mut stmt = conn.prepare("SELECT id, parent_id, name FROM entries WHERE is_directory = 1 AND id != ?1")?;
        let rows = stmt.query_map(params![ROOT_ID], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })?;

        let dir_entries: Vec<(i64, i64, String)> = rows.collect::<Result<Vec<_>, _>>()?;

        // Also include all entries (not just dirs) for path reconstruction of ancestors
        let mut all_stmt = conn.prepare("SELECT id, parent_id, name FROM entries")?;
        let all_rows = all_stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })?;
        let all_entries: Vec<(i64, i64, String)> = all_rows.collect::<Result<Vec<_>, _>>()?;
        let full_map: std::collections::HashMap<i64, (i64, &str)> = all_entries
            .iter()
            .map(|(id, pid, name)| (*id, (*pid, name.as_str())))
            .collect();

        let mut paths = Vec::with_capacity(dir_entries.len());
        for (id, _, _) in &dir_entries {
            let path = reconstruct_path_from_map(*id, &full_map);
            paths.push(path);
        }
        Ok(paths)
    }

    /// Get aggregated child stats for a parent directory by entry ID.
    #[cfg(test)]
    pub fn get_children_stats_by_id(conn: &Connection, parent_id: i64) -> Result<(u64, u64, u64), IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT
                 COALESCE(SUM(CASE WHEN is_directory = 0 THEN size ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN is_directory = 1 THEN 1 ELSE 0 END), 0)
             FROM entries WHERE parent_id = ?1",
        )?;
        let row = stmt.query_row(params![parent_id], |row| {
            Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?))
        })?;
        Ok(row)
    }

    /// Delete a single entry and its dir_stats by ID.
    pub fn delete_entry_by_id(conn: &Connection, id: i64) -> Result<(), IndexStoreError> {
        conn.execute("DELETE FROM dir_stats WHERE entry_id = ?1", params![id])?;
        conn.execute("DELETE FROM entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Delete all descendants of an entry (but not the entry itself) using recursive CTE.
    ///
    /// Used before subtree rescans to prevent orphaned entries. The root entry is kept
    /// because the scanner's `ScanContext` resolves it by path and uses its existing ID.
    pub fn delete_descendants_by_id(conn: &Connection, root_id: i64) -> Result<(), IndexStoreError> {
        // Collect descendant IDs (excluding root) then delete dir_stats and entries
        conn.execute(
            "WITH RECURSIVE descendants(id) AS (
                SELECT id FROM entries WHERE parent_id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN descendants d ON e.parent_id = d.id
            )
            DELETE FROM dir_stats WHERE entry_id IN (SELECT id FROM descendants)",
            params![root_id],
        )?;
        conn.execute(
            "WITH RECURSIVE descendants(id) AS (
                SELECT id FROM entries WHERE parent_id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN descendants d ON e.parent_id = d.id
            )
            DELETE FROM entries WHERE id IN (SELECT id FROM descendants)",
            params![root_id],
        )?;
        Ok(())
    }

    /// Delete an entire subtree by root entry ID using recursive CTE.
    ///
    /// No internal transaction: safe to call inside an outer `BEGIN IMMEDIATE`.
    pub fn delete_subtree_by_id(conn: &Connection, root_id: i64) -> Result<(), IndexStoreError> {
        // Delete dir_stats first to avoid dangling references
        conn.execute(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM entries WHERE id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
            )
            DELETE FROM dir_stats WHERE entry_id IN (SELECT id FROM subtree)",
            params![root_id],
        )?;
        conn.execute(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM entries WHERE id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
            )
            DELETE FROM entries WHERE id IN (SELECT id FROM subtree)",
            params![root_id],
        )?;
        Ok(())
    }

    /// Get total file size, file count, and directory count for a subtree by root entry ID.
    pub fn get_subtree_totals_by_id(conn: &Connection, root_id: i64) -> Result<(u64, u64, u64), IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "WITH RECURSIVE subtree(id) AS (
                SELECT id FROM entries WHERE id = ?1
                UNION ALL
                SELECT e.id FROM entries e JOIN subtree s ON e.parent_id = s.id
            )
            SELECT
                COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN e.size ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN e.is_directory = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN e.is_directory = 1 THEN 1 ELSE 0 END), 0)
            FROM entries e WHERE e.id IN (SELECT id FROM subtree)",
        )?;
        let row = stmt.query_row(params![root_id], |row| {
            Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?))
        })?;
        Ok(row)
    }

    /// Count the total number of entries in the index.
    #[cfg(test)]
    pub fn get_entry_count(conn: &Connection) -> Result<u64, IndexStoreError> {
        let count: u64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Get the next available entry ID. Useful for pre-allocating IDs during scan.
    pub fn get_next_id(conn: &Connection) -> Result<i64, IndexStoreError> {
        let max_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |row| row.get(0))?;
        Ok(max_id + 1)
    }

    /// Drop all tables and recreate the schema (full reset).
    #[cfg(test)]
    pub fn clear_all(conn: &Connection) -> Result<(), IndexStoreError> {
        reset_schema(conn)?;
        Ok(())
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Read a single value from the meta table.
    fn read_meta_value(conn: &Connection, key: &str) -> Result<Option<String>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(val)) => Ok(Some(val)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }
}

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
        IndexStore::insert_entry_v2(conn, parent_id, name, is_dir, false, size, None).unwrap()
    }

    #[test]
    fn schema_creation_and_version() {
        let (store, _dir) = open_temp_store();
        let status = store.get_index_status().unwrap();
        assert_eq!(status.schema_version.as_deref(), Some(SCHEMA_VERSION));
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
        assert_eq!(file.size, Some(1024));

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
                recursive_size: 50_000,
                recursive_file_count: 42,
                recursive_dir_count: 5,
            }],
        )
        .unwrap();

        let result = IndexStore::get_dir_stats_by_id(&conn, test_id).unwrap().unwrap();
        assert_eq!(result.recursive_size, 50_000);
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
                    recursive_size: 100,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                },
                DirStatsById {
                    entry_id: b_id,
                    recursive_size: 200,
                    recursive_file_count: 2,
                    recursive_dir_count: 1,
                },
            ],
        )
        .unwrap();

        let result = IndexStore::get_dir_stats_batch_by_ids(&conn, &[a_id, 99999, b_id]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].is_some());
        assert!(result[1].is_none());
        assert!(result[2].is_some());
        assert_eq!(result[0].as_ref().unwrap().recursive_size, 100);
        assert_eq!(result[2].as_ref().unwrap().recursive_size, 200);
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
    fn children_stats() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let p_id = insert_entry(&conn, ROOT_ID, "p", true, None);
        insert_entry(&conn, p_id, "f1.txt", false, Some(100));
        insert_entry(&conn, p_id, "f2.txt", false, Some(200));
        insert_entry(&conn, p_id, "sub", true, None);

        let (total_size, file_count, dir_count) = IndexStore::get_children_stats_by_id(&conn, p_id).unwrap();
        assert_eq!(total_size, 300);
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
        let file_id =
            IndexStore::insert_entry_v2(&conn, test_id, "file.txt", false, false, Some(512), Some(1700000000)).unwrap();

        let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap();
        assert!(result.is_some());
        let found = result.unwrap();
        assert_eq!(found.name, "file.txt");
        assert_eq!(found.size, Some(512));
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
        let file_id =
            IndexStore::insert_entry_v2(&conn, test_id, "file.txt", false, false, Some(100), Some(1000)).unwrap();

        let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
        assert_eq!(result.size, Some(100));

        // Update with new size
        IndexStore::update_entry(&conn, file_id, false, false, Some(200), Some(2000)).unwrap();

        let result = IndexStore::get_entry_by_id(&conn, file_id).unwrap().unwrap();
        assert_eq!(result.size, Some(200));
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
        let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
        let test_id = IndexStore::insert_entry_v2(&conn, users_id, "test", true, false, None, None).unwrap();

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

        let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
        assert_eq!(resolve_path(&conn, "/Users/").unwrap(), Some(users_id));
    }

    #[test]
    fn insert_entry_v2_and_get_by_id() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "myfile.txt", false, false, Some(4096), Some(999)).unwrap();
        assert!(id > ROOT_ID);

        let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(entry.name, "myfile.txt");
        assert_eq!(entry.parent_id, ROOT_ID);
        assert!(!entry.is_directory);
        assert_eq!(entry.size, Some(4096));
        assert_eq!(entry.modified_at, Some(999));
    }

    #[test]
    fn list_children_v2() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let dir_id = IndexStore::insert_entry_v2(&write_conn, ROOT_ID, "mydir", true, false, None, None).unwrap();
        IndexStore::insert_entry_v2(&write_conn, dir_id, "a.txt", false, false, Some(100), None).unwrap();
        IndexStore::insert_entry_v2(&write_conn, dir_id, "b.txt", false, false, Some(200), None).unwrap();

        let children = store.list_children(dir_id).unwrap();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn update_entry_v2() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "file.txt", false, false, Some(100), Some(1000)).unwrap();

        IndexStore::update_entry(&conn, id, false, false, Some(999), Some(2000)).unwrap();
        let entry = IndexStore::get_entry_by_id(&conn, id).unwrap().unwrap();
        assert_eq!(entry.size, Some(999));
        assert_eq!(entry.modified_at, Some(2000));
    }

    #[test]
    fn rename_and_move_entry() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let dir_a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "dir_a", true, false, None, None).unwrap();
        let dir_b = IndexStore::insert_entry_v2(&conn, ROOT_ID, "dir_b", true, false, None, None).unwrap();
        let file_id = IndexStore::insert_entry_v2(&conn, dir_a, "old.txt", false, false, Some(50), None).unwrap();

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

        let id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "file.txt", false, false, Some(100), None).unwrap();
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
        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None).unwrap();
        let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None).unwrap();
        let c = IndexStore::insert_entry_v2(&conn, b, "c.txt", false, false, Some(42), None).unwrap();

        // Add dir_stats for a and b
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: a,
                    recursive_size: 42,
                    recursive_file_count: 1,
                    recursive_dir_count: 1,
                },
                DirStatsById {
                    entry_id: b,
                    recursive_size: 42,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
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

        let a = IndexStore::insert_entry_v2(&conn, ROOT_ID, "a", true, false, None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, a, "f1.txt", false, false, Some(100), None).unwrap();
        IndexStore::insert_entry_v2(&conn, a, "f2.txt", false, false, Some(200), None).unwrap();
        let b = IndexStore::insert_entry_v2(&conn, a, "b", true, false, None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, b, "f3.txt", false, false, Some(300), None).unwrap();

        let (total_size, file_count, dir_count) = IndexStore::get_subtree_totals_by_id(&conn, a).unwrap();
        assert_eq!(total_size, 600);
        assert_eq!(file_count, 3);
        assert_eq!(dir_count, 2); // a + b
    }

    #[test]
    fn dir_stats_by_id_roundtrip() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let dir_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "mydir", true, false, None, None).unwrap();
        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[DirStatsById {
                entry_id: dir_id,
                recursive_size: 12345,
                recursive_file_count: 10,
                recursive_dir_count: 3,
            }],
        )
        .unwrap();

        let stats = IndexStore::get_dir_stats_by_id(&conn, dir_id).unwrap().unwrap();
        assert_eq!(stats.recursive_size, 12345);
        assert_eq!(stats.recursive_file_count, 10);
        assert_eq!(stats.recursive_dir_count, 3);
    }

    #[test]
    fn dir_stats_batch_by_ids() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let d1 = IndexStore::insert_entry_v2(&conn, ROOT_ID, "d1", true, false, None, None).unwrap();
        let d2 = IndexStore::insert_entry_v2(&conn, ROOT_ID, "d2", true, false, None, None).unwrap();

        IndexStore::upsert_dir_stats_by_id(
            &conn,
            &[
                DirStatsById {
                    entry_id: d1,
                    recursive_size: 100,
                    recursive_file_count: 1,
                    recursive_dir_count: 0,
                },
                DirStatsById {
                    entry_id: d2,
                    recursive_size: 200,
                    recursive_file_count: 2,
                    recursive_dir_count: 1,
                },
            ],
        )
        .unwrap();

        let result = IndexStore::get_dir_stats_batch_by_ids(&conn, &[d1, 99999, d2]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result[0].is_some());
        assert!(result[1].is_none());
        assert!(result[2].is_some());
        assert_eq!(result[0].as_ref().unwrap().recursive_size, 100);
        assert_eq!(result[2].as_ref().unwrap().recursive_size, 200);
    }

    #[test]
    fn get_next_id() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        // Root sentinel is id=1, so next should be 2
        let next = IndexStore::get_next_id(&conn).unwrap();
        assert_eq!(next, 2);

        IndexStore::insert_entry_v2(&conn, ROOT_ID, "file.txt", false, false, None, None).unwrap();
        let next = IndexStore::get_next_id(&conn).unwrap();
        assert!(next >= 3);
    }

    #[test]
    fn reconstruct_path_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        assert_eq!(IndexStore::reconstruct_path(&conn, ROOT_ID).unwrap(), "/");

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
        let foo = IndexStore::insert_entry_v2(&conn, users, "foo", true, false, None, None).unwrap();
        let file = IndexStore::insert_entry_v2(&conn, foo, "bar.txt", false, false, Some(10), None).unwrap();

        assert_eq!(IndexStore::reconstruct_path(&conn, users).unwrap(), "/Users");
        assert_eq!(IndexStore::reconstruct_path(&conn, foo).unwrap(), "/Users/foo");
        assert_eq!(IndexStore::reconstruct_path(&conn, file).unwrap(), "/Users/foo/bar.txt");
    }

    #[test]
    fn resolve_component_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
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

        let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();
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
        let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None).unwrap();

        // Resolve with different case should work on macOS
        assert_eq!(resolve_path(&conn, "/users").unwrap(), Some(users_id));
        assert_eq!(resolve_path(&conn, "/USERS").unwrap(), Some(users_id));
        assert_eq!(resolve_path(&conn, "/Users").unwrap(), Some(users_id));

        // The unique index should prevent inserting a case-variant name under the same parent
        let result = IndexStore::insert_entry_v2(&conn, ROOT_ID, "users", true, false, None, None);
        assert!(
            result.is_err(),
            "Should fail due to case-insensitive unique constraint on macOS"
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
                size: None,
                modified_at: None,
            },
            EntryRow {
                id: 101,
                parent_id: 100,
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(42),
                modified_at: Some(1234),
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &entries).unwrap();

        let entry = IndexStore::get_entry_by_id(&conn, 100).unwrap().unwrap();
        assert_eq!(entry.name, "dir1");
        assert!(entry.is_directory);

        let entry = IndexStore::get_entry_by_id(&conn, 101).unwrap().unwrap();
        assert_eq!(entry.name, "file.txt");
        assert_eq!(entry.size, Some(42));
    }

    #[test]
    fn get_children_stats_by_id_test() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let conn = IndexStore::open_write_connection(&db_path).unwrap();

        let dir_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "mydir", true, false, None, None).unwrap();
        IndexStore::insert_entry_v2(&conn, dir_id, "f1.txt", false, false, Some(100), None).unwrap();
        IndexStore::insert_entry_v2(&conn, dir_id, "f2.txt", false, false, Some(200), None).unwrap();
        IndexStore::insert_entry_v2(&conn, dir_id, "subdir", true, false, None, None).unwrap();

        let (size, files, dirs) = IndexStore::get_children_stats_by_id(&conn, dir_id).unwrap();
        assert_eq!(size, 300);
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
            let id = IndexStore::insert_entry_v2(&conn, parent_id, name, true, false, None, None).unwrap();
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
}
