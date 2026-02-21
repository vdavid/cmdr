//! SQLite store for the drive index.
//!
//! One DB file per indexed volume. Uses WAL mode for concurrent reads.
//! All writes go through a dedicated writer thread (see `writer.rs`);
//! this module provides the schema, read queries, and static write helpers.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: &str = "1";

// ── Types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirStats {
    pub path: String,
    pub recursive_size: u64,
    pub recursive_file_count: u64,
    pub recursive_dir_count: u64,
}

#[derive(Debug, Clone)]
pub struct ScannedEntry {
    pub path: String,
    pub parent_path: String,
    pub name: String,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
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
    SchemaMismatch { expected: String, found: String },
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
            IndexStoreError::SchemaMismatch { expected, found } => {
                write!(f, "Schema mismatch: expected {expected}, found {found}")
            }
        }
    }
}

impl std::error::Error for IndexStoreError {}

// ── Schema ───────────────────────────────────────────────────────────

const CREATE_TABLES_SQL: &str = "
    CREATE TABLE IF NOT EXISTS entries (
        path         TEXT PRIMARY KEY,
        parent_path  TEXT    NOT NULL,
        name         TEXT    NOT NULL,
        is_directory INTEGER NOT NULL DEFAULT 0,
        is_symlink   INTEGER NOT NULL DEFAULT 0,
        size         INTEGER,
        modified_at  INTEGER
    ) WITHOUT ROWID;

    CREATE INDEX IF NOT EXISTS idx_parent ON entries (parent_path);

    CREATE TABLE IF NOT EXISTS dir_stats (
        path                 TEXT PRIMARY KEY,
        recursive_size       INTEGER NOT NULL DEFAULT 0,
        recursive_file_count INTEGER NOT NULL DEFAULT 0,
        recursive_dir_count  INTEGER NOT NULL DEFAULT 0
    ) WITHOUT ROWID;

    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
";

/// Apply WAL-mode pragmas for performance.
fn apply_pragmas(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -65536;",
    )?;
    Ok(())
}

/// Create tables if they don't exist.
fn create_tables(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute_batch(CREATE_TABLES_SQL)?;
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
    /// Runs WAL pragmas, creates tables if missing, and checks the schema version.
    /// On version mismatch or corruption the DB file is deleted and recreated.
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

    /// Open a separate write connection with WAL pragmas.
    ///
    /// Used by the writer thread; callers own the returned connection.
    pub fn open_write_connection(db_path: &Path) -> Result<Connection, IndexStoreError> {
        let conn = Connection::open(db_path)?;
        apply_pragmas(&conn)?;
        Ok(conn)
    }

    // ── Read methods ─────────────────────────────────────────────────

    /// Look up recursive stats for a single directory.
    pub fn get_dir_stats(&self, path: &str) -> Result<Option<DirStats>, IndexStoreError> {
        let mut stmt = self.read_conn.prepare_cached(
            "SELECT path, recursive_size, recursive_file_count, recursive_dir_count
             FROM dir_stats WHERE path = ?1",
        )?;
        let mut rows = stmt.query_map(params![path], |row| {
            Ok(DirStats {
                path: row.get(0)?,
                recursive_size: row.get(1)?,
                recursive_file_count: row.get(2)?,
                recursive_dir_count: row.get(3)?,
            })
        })?;
        match rows.next() {
            Some(Ok(stats)) => Ok(Some(stats)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Batch lookup of dir_stats for multiple paths.
    ///
    /// Returns a `Vec` with the same length as `paths`, where each element is
    /// `Some(DirStats)` if found or `None` otherwise.
    pub fn get_dir_stats_batch(&self, paths: &[&str]) -> Result<Vec<Option<DirStats>>, IndexStoreError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }

        // For small batches, iterate individually (avoids building dynamic SQL)
        if paths.len() <= 20 {
            return paths.iter().map(|p| self.get_dir_stats(p)).collect();
        }

        // For larger batches, build a single query with IN clause
        let placeholders: String = (0..paths.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT path, recursive_size, recursive_file_count, recursive_dir_count
             FROM dir_stats WHERE path IN ({placeholders})"
        );
        let mut stmt = self.read_conn.prepare(&sql)?;

        let param_values: Vec<&dyn rusqlite::types::ToSql> =
            paths.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();

        let rows = stmt.query_map(&*param_values, |row| {
            Ok(DirStats {
                path: row.get(0)?,
                recursive_size: row.get(1)?,
                recursive_file_count: row.get(2)?,
                recursive_dir_count: row.get(3)?,
            })
        })?;

        // Build a map of path -> DirStats from the results
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let stats = row?;
            map.insert(stats.path.clone(), stats);
        }

        Ok(paths.iter().map(|p| map.remove(*p)).collect())
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

    /// List all entries whose `parent_path` matches the given directory.
    pub fn list_entries_by_parent(&self, parent_path: &str) -> Result<Vec<ScannedEntry>, IndexStoreError> {
        let mut stmt = self.read_conn.prepare_cached(
            "SELECT path, parent_path, name, is_directory, is_symlink, size, modified_at
             FROM entries WHERE parent_path = ?1",
        )?;
        let rows = stmt.query_map(params![parent_path], |row| {
            Ok(ScannedEntry {
                path: row.get(0)?,
                parent_path: row.get(1)?,
                name: row.get(2)?,
                is_directory: row.get::<_, i32>(3)? != 0,
                is_symlink: row.get::<_, i32>(4)? != 0,
                size: row.get(5)?,
                modified_at: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Return the path to the DB file.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Return the DB file size on disk (bytes).
    pub fn db_file_size(&self) -> Result<u64, IndexStoreError> {
        Ok(std::fs::metadata(&self.db_path)?.len())
    }

    // ── Static write helpers (for the writer thread) ─────────────────

    /// Batch insert (or replace) entries inside a transaction.
    pub fn insert_entries_batch(conn: &Connection, entries: &[ScannedEntry]) -> Result<(), IndexStoreError> {
        if entries.is_empty() {
            return Ok(());
        }
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO entries
                     (path, parent_path, name, is_directory, is_symlink, size, modified_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for e in entries {
                stmt.execute(params![
                    e.path,
                    e.parent_path,
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

    /// Batch upsert dir_stats inside a transaction.
    pub fn upsert_dir_stats(conn: &Connection, stats: &[DirStats]) -> Result<(), IndexStoreError> {
        if stats.is_empty() {
            return Ok(());
        }
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO dir_stats
                     (path, recursive_size, recursive_file_count, recursive_dir_count)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for s in stats {
                stmt.execute(params![
                    s.path,
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
    pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>, IndexStoreError> {
        Self::read_meta_value(conn, key)
    }

    /// Get all directory paths from the entries table.
    pub fn get_all_directory_paths(conn: &Connection) -> Result<Vec<String>, IndexStoreError> {
        let mut stmt = conn.prepare("SELECT path FROM entries WHERE is_directory = 1")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get aggregated child stats for a parent directory.
    ///
    /// Returns `(total_file_size, file_count, dir_count)` for direct children only.
    pub fn get_children_stats(conn: &Connection, parent_path: &str) -> Result<(u64, u64, u64), IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT
                 COALESCE(SUM(CASE WHEN is_directory = 0 THEN size ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN is_directory = 0 THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN is_directory = 1 THEN 1 ELSE 0 END), 0)
             FROM entries WHERE parent_path = ?1",
        )?;
        let row = stmt.query_row(params![parent_path], |row| {
            Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?))
        })?;
        Ok(row)
    }

    /// Look up a single entry by path.
    pub fn get_entry(conn: &Connection, path: &str) -> Result<Option<ScannedEntry>, IndexStoreError> {
        let mut stmt = conn.prepare_cached(
            "SELECT path, parent_path, name, is_directory, is_symlink, size, modified_at
             FROM entries WHERE path = ?1",
        )?;
        let result = stmt
            .query_row(params![path], |row| {
                Ok(ScannedEntry {
                    path: row.get(0)?,
                    parent_path: row.get(1)?,
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

    /// Insert or replace a single entry.
    pub fn upsert_entry(conn: &Connection, entry: &ScannedEntry) -> Result<(), IndexStoreError> {
        conn.execute(
            "INSERT OR REPLACE INTO entries
                 (path, parent_path, name, is_directory, is_symlink, size, modified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.path,
                entry.parent_path,
                entry.name,
                entry.is_directory as i32,
                entry.is_symlink as i32,
                entry.size,
                entry.modified_at,
            ],
        )?;
        Ok(())
    }

    /// Delete a single entry and its corresponding dir_stats row.
    pub fn delete_entry(conn: &Connection, path: &str) -> Result<(), IndexStoreError> {
        conn.execute("DELETE FROM entries WHERE path = ?1", params![path])?;
        conn.execute("DELETE FROM dir_stats WHERE path = ?1", params![path])?;
        Ok(())
    }

    /// Delete all entries (and dir_stats) whose path starts with the given prefix.
    pub fn delete_subtree(conn: &Connection, path_prefix: &str) -> Result<(), IndexStoreError> {
        let tx = conn.unchecked_transaction()?;
        // Delete the exact path and everything under it (prefix + '/')
        tx.execute(
            "DELETE FROM entries WHERE path = ?1 OR path LIKE ?2",
            params![path_prefix, format!("{path_prefix}/%")],
        )?;
        tx.execute(
            "DELETE FROM dir_stats WHERE path = ?1 OR path LIKE ?2",
            params![path_prefix, format!("{path_prefix}/%")],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Count the total number of entries in the index.
    pub fn get_entry_count(conn: &Connection) -> Result<u64, IndexStoreError> {
        let count: u64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Get all directory paths whose `parent_path` starts with the given root prefix.
    ///
    /// Used by subtree aggregation to limit computation to a specific subtree.
    pub fn get_directory_paths_under(conn: &Connection, root: &str) -> Result<Vec<String>, IndexStoreError> {
        let mut stmt = conn.prepare(
            "SELECT path FROM entries WHERE is_directory = 1
             AND (path = ?1 OR path LIKE ?2)",
        )?;
        let pattern = format!("{root}/%");
        let rows = stmt.query_map(params![root, pattern], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Drop all tables and recreate the schema (full reset).
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

    #[test]
    fn schema_creation_and_version() {
        let (store, _dir) = open_temp_store();
        let status = store.get_index_status().unwrap();
        assert_eq!(status.schema_version.as_deref(), Some("1"));
    }

    #[test]
    fn insert_and_list_entries() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let entries = vec![
            ScannedEntry {
                path: "/Users/test/a.txt".into(),
                parent_path: "/Users/test".into(),
                name: "a.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(1024),
                modified_at: Some(1700000000),
            },
            ScannedEntry {
                path: "/Users/test/docs".into(),
                parent_path: "/Users/test".into(),
                name: "docs".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: Some(1700000000),
            },
        ];
        IndexStore::insert_entries_batch(&write_conn, &entries).unwrap();

        let result = store.list_entries_by_parent("/Users/test").unwrap();
        assert_eq!(result.len(), 2);

        let file = result.iter().find(|e| e.name == "a.txt").unwrap();
        assert!(!file.is_directory);
        assert_eq!(file.size, Some(1024));

        let dir = result.iter().find(|e| e.name == "docs").unwrap();
        assert!(dir.is_directory);
    }

    #[test]
    fn dir_stats_roundtrip() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let stats = vec![DirStats {
            path: "/Users/test".into(),
            recursive_size: 50_000,
            recursive_file_count: 42,
            recursive_dir_count: 5,
        }];
        IndexStore::upsert_dir_stats(&write_conn, &stats).unwrap();

        let result = store.get_dir_stats("/Users/test").unwrap().unwrap();
        assert_eq!(result.recursive_size, 50_000);
        assert_eq!(result.recursive_file_count, 42);
        assert_eq!(result.recursive_dir_count, 5);
    }

    #[test]
    fn dir_stats_batch_lookup() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let stats = vec![
            DirStats {
                path: "/a".into(),
                recursive_size: 100,
                recursive_file_count: 1,
                recursive_dir_count: 0,
            },
            DirStats {
                path: "/b".into(),
                recursive_size: 200,
                recursive_file_count: 2,
                recursive_dir_count: 1,
            },
        ];
        IndexStore::upsert_dir_stats(&write_conn, &stats).unwrap();

        let result = store.get_dir_stats_batch(&["/a", "/nonexistent", "/b"]).unwrap();
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
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let entries = vec![
            ScannedEntry {
                path: "/p/f1.txt".into(),
                parent_path: "/p".into(),
                name: "f1.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(100),
                modified_at: None,
            },
            ScannedEntry {
                path: "/p/f2.txt".into(),
                parent_path: "/p".into(),
                name: "f2.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(200),
                modified_at: None,
            },
            ScannedEntry {
                path: "/p/sub".into(),
                parent_path: "/p".into(),
                name: "sub".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_batch(&write_conn, &entries).unwrap();

        let (total_size, file_count, dir_count) = IndexStore::get_children_stats(&write_conn, "/p").unwrap();
        assert_eq!(total_size, 300);
        assert_eq!(file_count, 2);
        assert_eq!(dir_count, 1);
    }

    #[test]
    fn delete_entry_and_subtree() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let entries = vec![
            ScannedEntry {
                path: "/a".into(),
                parent_path: "/".into(),
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/a/b.txt".into(),
                parent_path: "/a".into(),
                name: "b.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(10),
                modified_at: None,
            },
            ScannedEntry {
                path: "/a/c".into(),
                parent_path: "/a".into(),
                name: "c".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/a/c/d.txt".into(),
                parent_path: "/a/c".into(),
                name: "d.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(20),
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_batch(&write_conn, &entries).unwrap();

        // Delete single entry
        IndexStore::delete_entry(&write_conn, "/a/b.txt").unwrap();
        let children = store.list_entries_by_parent("/a").unwrap();
        assert_eq!(children.len(), 1); // only /a/c remains

        // Delete subtree
        IndexStore::delete_subtree(&write_conn, "/a").unwrap();
        let children = store.list_entries_by_parent("/a").unwrap();
        assert!(children.is_empty());
        let root_children = store.list_entries_by_parent("/").unwrap();
        assert!(root_children.is_empty()); // /a itself is also gone
    }

    #[test]
    fn clear_all_resets_schema() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let entries = vec![ScannedEntry {
            path: "/x".into(),
            parent_path: "/".into(),
            name: "x".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(1),
            modified_at: None,
        }];
        IndexStore::insert_entries_batch(&write_conn, &entries).unwrap();

        IndexStore::clear_all(&write_conn).unwrap();

        // Schema version should be re-stamped
        let version = IndexStore::get_meta(&write_conn, "schema_version").unwrap();
        assert_eq!(version.as_deref(), Some("1"));

        // Entries should be gone
        let children = store.list_entries_by_parent("/").unwrap();
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
        assert_eq!(status.schema_version.as_deref(), Some("1"));
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
        assert_eq!(status.schema_version.as_deref(), Some("1"));
    }

    #[test]
    fn db_file_size_returns_nonzero() {
        let (store, _dir) = open_temp_store();
        let size = store.db_file_size().unwrap();
        assert!(size > 0, "DB file should have nonzero size after creation");
    }

    #[test]
    fn get_all_directory_paths() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let entries = vec![
            ScannedEntry {
                path: "/a".into(),
                parent_path: "/".into(),
                name: "a".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
            ScannedEntry {
                path: "/a/file.txt".into(),
                parent_path: "/a".into(),
                name: "file.txt".into(),
                is_directory: false,
                is_symlink: false,
                size: Some(100),
                modified_at: None,
            },
            ScannedEntry {
                path: "/b".into(),
                parent_path: "/".into(),
                name: "b".into(),
                is_directory: true,
                is_symlink: false,
                size: None,
                modified_at: None,
            },
        ];
        IndexStore::insert_entries_batch(&write_conn, &entries).unwrap();

        let dirs = IndexStore::get_all_directory_paths(&write_conn).unwrap();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&"/a".to_string()));
        assert!(dirs.contains(&"/b".to_string()));
    }

    #[test]
    fn empty_batch_operations_are_noops() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let write_conn = IndexStore::open_write_connection(&db_path).unwrap();

        // These should succeed without error
        IndexStore::insert_entries_batch(&write_conn, &[]).unwrap();
        IndexStore::upsert_dir_stats(&write_conn, &[]).unwrap();
    }

    #[test]
    fn get_entry_found() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let entry = ScannedEntry {
            path: "/test/file.txt".into(),
            parent_path: "/test".into(),
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(512),
            modified_at: Some(1700000000),
        };
        IndexStore::insert_entries_batch(&write_conn, &[entry]).unwrap();

        let result = IndexStore::get_entry(&write_conn, "/test/file.txt").unwrap();
        assert!(result.is_some());
        let found = result.unwrap();
        assert_eq!(found.name, "file.txt");
        assert_eq!(found.size, Some(512));
        assert!(!found.is_directory);
    }

    #[test]
    fn get_entry_not_found() {
        let (_store, dir) = open_temp_store();
        let db_path = dir.path().join("test-index.db");
        let write_conn = IndexStore::open_write_connection(&db_path).unwrap();

        let result = IndexStore::get_entry(&write_conn, "/nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn upsert_entry_inserts_and_updates() {
        let (store, _dir) = open_temp_store();
        let write_conn = IndexStore::open_write_connection(store.db_path()).unwrap();

        let entry = ScannedEntry {
            path: "/test/file.txt".into(),
            parent_path: "/test".into(),
            name: "file.txt".into(),
            is_directory: false,
            is_symlink: false,
            size: Some(100),
            modified_at: Some(1000),
        };
        IndexStore::upsert_entry(&write_conn, &entry).unwrap();

        let result = IndexStore::get_entry(&write_conn, "/test/file.txt").unwrap().unwrap();
        assert_eq!(result.size, Some(100));

        // Update with new size
        let updated = ScannedEntry {
            size: Some(200),
            modified_at: Some(2000),
            ..entry
        };
        IndexStore::upsert_entry(&write_conn, &updated).unwrap();

        let result = IndexStore::get_entry(&write_conn, "/test/file.txt").unwrap().unwrap();
        assert_eq!(result.size, Some(200));
        assert_eq!(result.modified_at, Some(2000));
    }
}
