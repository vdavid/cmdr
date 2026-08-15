//! `IndexStore` lifecycle: open/recreate, connection factories, DB-size and
//! status reads. Pure code movement from the former monolithic `store.rs`.

use super::{
    IndexStatus, IndexStore, IndexStoreError, SCHEMA_VERSION, ScanCalibration, ScanCalibrationKind, ScanCalibrationSet,
    USER_DISABLED_KEY, USER_ENABLED_KEY, apply_pragmas, create_tables, register_platform_case_collation,
};
use rusqlite::{Connection, params};
use std::path::Path;

/// Backoff between `IndexStore::open` retries after transient lock contention, in
/// milliseconds; its length is the retry budget (so three attempts in total).
///
/// Deliberately short: `busy_timeout` (5 s, `apply_pragmas`) already absorbs
/// ordinary contention inside SQLite, so reaching this array means a writer held
/// the lock through a whole busy-handler window (a long checkpoint, another
/// process). These retries are the second line of defense before we hand the
/// caller an error, and they cap the added launch latency at 400 ms.
const OPEN_RETRY_BACKOFF_MS: [u64; 2] = [100, 300];

impl IndexStore {
    /// Open (or create) the index database at `db_path`.
    ///
    /// Registers the `platform_case` collation, runs WAL pragmas, creates tables
    /// if missing, and checks the schema version.
    ///
    /// Failures are classified by typed SQLite code, because deleting is the
    /// destructive branch and needs proof:
    /// - A schema-version mismatch (a clean upgrade) or proven corruption deletes
    ///   the file and recreates it fresh, reclaiming disk with zero freelist.
    /// - Transient lock contention retries with a short backoff, then gives up and
    ///   returns the error. A busy DB is a healthy DB.
    /// - Anything else (a full or read-only volume, a momentary I/O error, an
    ///   unrecognized code) returns the error with the file untouched. A real index
    ///   holds millions of entries and costs tens of minutes to rebuild, so the
    ///   caller reporting a failure always beats silently discarding a good index.
    pub fn open(db_path: &Path) -> Result<Self, IndexStoreError> {
        let mut attempt = 0usize;
        loop {
            match Self::try_open(db_path) {
                Ok(store) => return Ok(store),
                // A schema bump is an expected, clean upgrade, not a failure. Recreate
                // the file fresh (the disposable cache has no migrations) and log it as
                // an upgrade so it reads distinctly from the corruption path below.
                Err(IndexStoreError::SchemaMismatch { found, expected }) => {
                    log::info!(
                        "Index DB schema version changed (found {found}, expected {expected}), recreating index DB"
                    );
                    return Self::delete_and_recreate(db_path);
                }
                Err(e) if e.indicates_corruption() => {
                    log::warn!(
                        "Index DB at {} is corrupt ({e}), deleting and recreating",
                        db_path.display()
                    );
                    return Self::delete_and_recreate(db_path);
                }
                Err(e) if e.is_transient_lock_error() && attempt < OPEN_RETRY_BACKOFF_MS.len() => {
                    let backoff = OPEN_RETRY_BACKOFF_MS[attempt];
                    attempt += 1;
                    log::warn!(
                        "Index DB at {} is locked ({e}), retrying in {backoff} ms (attempt {attempt} of {})",
                        db_path.display(),
                        OPEN_RETRY_BACKOFF_MS.len()
                    );
                    std::thread::sleep(std::time::Duration::from_millis(backoff));
                }
                Err(e) => {
                    log::warn!(
                        "Index DB at {} failed to open ({e}); keeping the file (only corruption is thrown away)",
                        db_path.display()
                    );
                    return Err(e);
                }
            }
        }
    }

    /// Attempt to open the DB without the delete-and-recreate fallback.
    fn try_open(db_path: &Path) -> Result<Self, IndexStoreError> {
        let conn = cmdr_fs::sqlite_util::open(db_path)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn, false)?;
        create_tables(&conn)?;

        // Check schema version
        let version = Self::read_meta_value(&conn, "schema_version")?;
        match version {
            Some(v) if v == SCHEMA_VERSION => { /* all good */ }
            Some(v) => {
                // Hand back to `open`, which deletes + recreates the file fresh
                // (zero freelist) rather than DROP-ing tables on the live file
                // and stranding the freed pages. `conn` is local here and drops
                // before `delete_and_recreate` opens its own connection.
                return Err(IndexStoreError::SchemaMismatch {
                    found: v,
                    expected: SCHEMA_VERSION,
                });
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

        let conn = cmdr_fs::sqlite_util::open(db_path)?;
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
        let conn = cmdr_fs::sqlite_util::open(db_path)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn, false)?;
        Ok(conn)
    }

    /// Open a read-only connection with per-connection pragmas and `platform_case` collation.
    ///
    /// Never contends with the writer thread's write lock.
    pub fn open_read_connection(db_path: &Path) -> Result<Connection, IndexStoreError> {
        let conn = cmdr_fs::sqlite_util::open_read_only(db_path)?;
        register_platform_case_collation(&conn)?;
        apply_pragmas(&conn, true)?;
        Ok(conn)
    }

    /// Whether the persisted index DB at `db_path` records a completed scan.
    ///
    /// ⚠️ Exactly that, and nothing about what the user wants. `scan_completed_at`
    /// is absent for the whole of a first index AND for the whole of every rescan
    /// (`start_scan` deletes it before it walks), so it can't answer "did the user
    /// turn this drive on" — [`Self::user_enabled`] does. Auto-resume reads both
    /// through `master::drive_index_should_run`, where this arm covers the indexes
    /// enabled before the marker existed.
    ///
    /// Deliberately a READ-ONLY probe — never the delete-and-recreate `open` path —
    /// so merely checking a schema-mismatched or locked DB never mutates it. A
    /// missing file, an unreadable/locked DB, or an absent marker all read `false`.
    pub fn persisted_scan_completed(db_path: &Path) -> bool {
        if !db_path.exists() {
            return false;
        }
        match Self::open_read_connection(db_path) {
            Ok(conn) => Self::read_meta_value(&conn, "scan_completed_at")
                .ok()
                .flatten()
                .is_some(),
            Err(e) => {
                log::debug!("persisted_scan_completed({}): read open failed: {e}", db_path.display());
                false
            }
        }
    }

    /// Whether the user explicitly turned indexing ON for this volume (the sticky
    /// [`USER_ENABLED_KEY`] marker).
    ///
    /// The fact [`Self::persisted_scan_completed`] can't stand in for: it's written
    /// when the user asks for the drive, so a first index that never finished, and a
    /// completed index whose rescan is in flight right now, both still read as
    /// enabled. A missing file / unreadable DB / absent marker all read `false`.
    /// READ-ONLY probe.
    pub(crate) fn user_enabled(db_path: &Path) -> bool {
        Self::intent_marker(db_path, USER_ENABLED_KEY)
    }

    /// Whether the user explicitly turned indexing OFF for this volume (the sticky
    /// [`USER_DISABLED_KEY`] marker). Persisted intent that survives a reconnect: the
    /// DB stays on disk for a fast re-enable, but this flag stops the SMB auto-resume
    /// gate from turning back on something the user turned off. A missing file /
    /// unreadable DB / absent marker all read `false`. READ-ONLY probe.
    pub fn user_disabled(db_path: &Path) -> bool {
        Self::intent_marker(db_path, USER_DISABLED_KEY)
    }

    /// Read one of the two sticky per-drive intent markers.
    fn intent_marker(db_path: &Path, key: &str) -> bool {
        if !db_path.exists() {
            return false;
        }
        match Self::open_read_connection(db_path) {
            Ok(conn) => Self::read_meta_value(&conn, key).ok().flatten().as_deref() == Some("1"),
            Err(e) => {
                log::debug!("{key}({}): read open failed: {e}", db_path.display());
                false
            }
        }
    }

    /// Record the user's per-drive indexing choice on the volume's own index DB:
    /// stamp one marker and DELETE the other, so the two can never both hold.
    ///
    /// Writing the pair in one call is what keeps intent a single fact. Splitting it
    /// left every non-SMB transport re-enabling a drive without clearing its veto,
    /// which a later master-switch cycle then read as "the user turned this off".
    ///
    /// Creates the database when the enable is the volume's first ever, because the
    /// choice has to outlive the first index rather than wait for it. ❌ Never
    /// reopens an existing file: `open` deletes and recreates on a schema mismatch,
    /// and throwing a real index away to write one marker is not a trade this call
    /// gets to make.
    ///
    /// Opens a short-lived write connection. Called mid-scan on the one path where a
    /// search already stood a writer up, which is safe for a single `meta` row: it
    /// carries none of the id-counter or accumulator state the single-writer rule
    /// protects, and `busy_timeout` (5 s) absorbs the writer's batch transactions.
    /// ❌ Don't queue it through the writer instead — the marker's whole job is to
    /// survive a crash mid-first-index, and the writer's backlog IS that window.
    pub(crate) fn set_drive_index_intent(db_path: &Path, enabled: bool) -> Result<(), IndexStoreError> {
        if !db_path.exists() {
            drop(Self::open(db_path)?);
        }
        let (set, cleared) = if enabled {
            (USER_ENABLED_KEY, USER_DISABLED_KEY)
        } else {
            (USER_DISABLED_KEY, USER_ENABLED_KEY)
        };
        let conn = Self::open_write_connection(db_path)?;
        Self::update_meta(&conn, set, "1")?;
        conn.execute("DELETE FROM meta WHERE key = ?1", params![cleared])?;
        Ok(())
    }

    /// Persist the volume's mount root (`volume_path` meta) on its index DB.
    ///
    /// The search loader reads this to strip the mount root off scope paths (a
    /// non-root index is mount-relative). Older SMB indexes never wrote it (only the
    /// local scan-completion path did), so `start_indexing_for_smb` heals an existing
    /// DB with this on the next registration — no rescan. ⚠️ Short-lived write
    /// connection: call it only when no writer thread is live for this volume.
    pub fn set_volume_path(db_path: &Path, volume_path: &str) -> Result<(), IndexStoreError> {
        let conn = Self::open_write_connection(db_path)?;
        Self::update_meta(&conn, "volume_path", volume_path)?;
        Ok(())
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

    /// Read every calibration bucket (per-walk-kind plus the unsuffixed
    /// last-scan one) off the given connection, so the caller can pick the
    /// bucket matching the run it's about to start via
    /// [`ScanCalibrationSet::for_kind`].
    ///
    /// Missing or unparseable keys map to `None`. Takes a connection (rather
    /// than `&self`) so `start_scan` can read it off a fresh connection before
    /// truncating; the keys survive `TruncateData` (it preserves `meta`).
    pub(crate) fn read_scan_calibration_set(conn: &Connection) -> Result<ScanCalibrationSet, IndexStoreError> {
        Ok(ScanCalibrationSet {
            full_walk: Self::read_scan_calibration_for(conn, Some(ScanCalibrationKind::FullWalk))?,
            change_check: Self::read_scan_calibration_for(conn, Some(ScanCalibrationKind::ChangeCheck))?,
            any: Self::read_scan_calibration_for(conn, None)?,
        })
    }

    /// One calibration bucket: a walk kind's own keys, or the unsuffixed keys
    /// (`None`) the last completed scan of any kind wrote.
    fn read_scan_calibration_for(
        conn: &Connection,
        kind: Option<ScanCalibrationKind>,
    ) -> Result<ScanCalibration, IndexStoreError> {
        let read_u64 = |base: &str| -> Result<Option<u64>, IndexStoreError> {
            let key = match kind {
                Some(kind) => kind.meta_key(base),
                None => base.to_string(),
            };
            Ok(Self::read_meta_value(conn, &key)?.and_then(|v| v.parse::<u64>().ok()))
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
