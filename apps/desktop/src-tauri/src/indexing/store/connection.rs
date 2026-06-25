//! `IndexStore` lifecycle: open/recreate, connection factories, DB-size and
//! status reads. Pure code movement from the former monolithic `store.rs`.

use super::*;

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
        apply_pragmas(&conn, false)?;
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
        apply_pragmas(&conn, false)?;
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
        apply_pragmas(&conn, false)?;
        Ok(conn)
    }

    /// Open a read-only connection with per-connection pragmas and `platform_case` collation.
    ///
    /// Never contends with the writer thread's write lock.
    pub fn open_read_connection(db_path: &Path) -> Result<Connection, IndexStoreError> {
        let conn = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn, true)?;
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
            total_physical_bytes: Self::read_meta_value(&self.read_conn, "total_physical_bytes")?,
            last_event_id: Self::read_meta_value(&self.read_conn, "last_event_id")?,
        })
    }

    /// Read the previous completed scan's calibration from `meta` on the given
    /// connection. Missing or unparseable keys map to `None`. Takes a connection
    /// (rather than `&self`) so `start_scan` can read it off a fresh connection
    /// before truncating; the keys survive `TruncateData` (it preserves `meta`).
    pub fn read_scan_calibration(conn: &Connection) -> Result<ScanCalibration, IndexStoreError> {
        let read_u64 = |key: &str| -> Result<Option<u64>, IndexStoreError> {
            Ok(Self::read_meta_value(conn, key)?.and_then(|v| v.parse::<u64>().ok()))
        };
        Ok(ScanCalibration {
            total_entries: read_u64("total_entries")?,
            total_physical_bytes: read_u64("total_physical_bytes")?,
            scan_duration_ms: read_u64("scan_duration_ms")?,
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

    /// Return the total DB size on disk (main file + WAL + SHM sidecars).
    pub fn db_file_size(&self) -> Result<u64, IndexStoreError> {
        let main = std::fs::metadata(&self.db_path)?.len();
        let wal = std::fs::metadata(format!("{}-wal", self.db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
        let shm = std::fs::metadata(format!("{}-shm", self.db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(main + wal + shm)
    }

    /// Return the main DB file size (excluding WAL/SHM).
    pub fn db_main_size(&self) -> Result<u64, IndexStoreError> {
        Ok(std::fs::metadata(&self.db_path)?.len())
    }

    /// Return the WAL file size.
    pub fn db_wal_size(&self) -> Result<u64, IndexStoreError> {
        Ok(std::fs::metadata(format!("{}-wal", self.db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0))
    }

    /// Return SQLite page_count and freelist_count.
    pub fn db_page_stats(conn: &Connection) -> Result<(u64, u64), IndexStoreError> {
        let page_count: u64 = conn.pragma_query_value(None, "page_count", |r| r.get(0))?;
        let freelist: u64 = conn.pragma_query_value(None, "freelist_count", |r| r.get(0))?;
        Ok((page_count, freelist))
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Read a single value from the meta table.
    pub(super) fn read_meta_value(conn: &Connection, key: &str) -> Result<Option<String>, IndexStoreError> {
        let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(val)) => Ok(Some(val)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }
}
