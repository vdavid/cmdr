//! The on-disk shape of an index DB: the schema version the cache is keyed on,
//! the `meta` keys the rest of the subsystem stamps, the table DDL, the root
//! sentinel, and the pragmas every connection opens with.
//!
//! There are no migrations by design: the index is a disposable cache, so a
//! schema change bumps [`SCHEMA_VERSION`] and the next open rebuilds the file
//! (`connection.rs`).

use rusqlite::{Connection, params};

use super::IndexStoreError;

// Bump to invalidate on-disk indexes (the cache is disposable: a mismatch deletes
// the DB file + recreates it fresh, no migration). v16 makes that marker a
// `entries.unreadable_cause`, so a directory nothing will read says WHY: the OS
// refused us (which granting Full Disk Access fixes) or we refuse it on purpose (a
// NAS snapshot tree). The two are different sentences, and a boolean couldn't tell
// them apart.
pub(super) const SCHEMA_VERSION: &str = "16";

/// Meta key for the per-volume epoch counter (TEXT, like all meta values).
///
/// Bumped on every continuity break; a scan/reconcile *stamps* listed dirs with
/// the current epoch but does not bump it. Absent ⇒ treat as epoch 1 (a volume
/// with no recorded epoch behaves as "all current", not "all stale"). See the
/// "Honest sizes" model in `indexing/DETAILS.md`.
pub(crate) const CURRENT_EPOCH_KEY: &str = "current_epoch";

/// Meta key recording that the user turned drive indexing ON for this volume.
///
/// The positive half of per-drive intent, and the reason a drive interrupted part
/// way through its FIRST index still comes back: completion can't stand in for the
/// choice, because `scan_completed_at` is absent both before a first scan finishes
/// and for the whole of every later rescan. Written when a start is asked for, not
/// when one finishes. Only presence matters (the value is a marker).
pub(crate) const USER_ENABLED_KEY: &str = "user_enabled";

/// Meta key recording that the user turned drive indexing OFF for this volume.
///
/// The negative half, and an unconditional veto: a reconnect must never turn back
/// on what the user turned off. Written ONLY by the explicit disable command,
/// never by a teardown that happens to stop an index (eject, unmount, the master
/// switch, the memory watchdog).
///
/// Exactly one of the two is ever present: `IndexStore::set_drive_index_intent`
/// writes them as a pair.
pub(crate) const USER_DISABLED_KEY: &str = "user_disabled";

/// Meta key marking that this DB's `dir_stats` are known to agree with `entries`:
/// a full aggregate rebuilt them and nothing has knowingly drifted them since.
/// Present ⇒ a later launch skips the heal; absent ⇒ the aggregates are UNPAID
/// (pre-ledger drift, or a bulk walk that suppressed propagation and never ran
/// its terminal aggregate) and the next launch heals them via the writer-side
/// latch. Only presence matters (the value is a marker). See
/// `indexing/DETAILS.md` § "The dir_stats ledger".
pub(crate) const LEDGER_HEAL_KEY: &str = "aggregates_rebuilt_for_ledger";

/// Meta key recording WHICH NAS system-dir exclusion list this DB was BUILT
/// against (the value is the list's fingerprint), written when a network scan
/// truncates. Absent or stale ⇒ the index may still carry rows beneath a directory
/// today's scanner refuses to walk, so the next load rebuilds it from scratch.
/// Storing the fingerprint rather than a bare "done" flag is what makes GROWING
/// the list re-arm every existing index. See
/// `indexing/network_scanner/DETAILS.md` § "NAS snapshot/system dirs aren't recursed".
pub(crate) const SYSTEM_DIR_EXCLUSIONS_KEY: &str = "system_dir_exclusions_built_for";

/// Meta key recording WHICH scan-exclusion policy this DB was BUILT against (the
/// value is the policy's fingerprint), written right after a truncating full walk.
///
/// A policy-excluded directory gets no `entries` row at all, so it drives nothing
/// to `0` and its parents read as fully covered. That is correct only while the
/// policy is the one the rows were written under: if a release REMOVES an entry,
/// the subtrees it used to skip stay row-less, their parents still read as
/// covered, and they become permanently invisible to search with nothing to
/// re-walk them. Absent or stale ⇒ every coverage claim in this DB is unknown, so
/// [`crate::indexing::read::coverage`] hands the whole scope to the walk. Storing
/// the fingerprint rather than a bare flag is what makes CHANGING the policy
/// re-arm every existing index. Same shape as [`SYSTEM_DIR_EXCLUSIONS_KEY`], which
/// covers the NAS list; this one covers the local tiers.
pub(crate) const EXCLUSION_POLICY_KEY: &str = "exclusion_policy_built_for";

/// Root entry sentinel ID. All top-level entries have `parent_id = ROOT_ID`.
pub const ROOT_ID: i64 = 1;

/// Parent ID of the root sentinel. No row with this ID exists in the DB.
pub(super) const ROOT_PARENT_ID: i64 = 0;

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
        listed_epoch  INTEGER NOT NULL DEFAULT 0,
        unreadable_cause INTEGER NOT NULL DEFAULT 0
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
pub(super) fn ensure_root_sentinel(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.execute(
        "INSERT OR IGNORE INTO entries (id, parent_id, name, name_folded, is_directory) VALUES (?1, ?2, '', '', 1)",
        params![ROOT_ID, ROOT_PARENT_ID],
    )?;
    Ok(())
}

/// Apply WAL-mode pragmas for performance.
///
/// The page-cache budget is role-dependent and shared with every other store; see
/// [`cmdr_fs::sqlite_util::apply_page_cache`].
pub(super) fn apply_pragmas(conn: &Connection, readonly: bool) -> Result<(), IndexStoreError> {
    // busy_timeout: when another connection holds the write lock, retry for up
    // to 5s instead of returning SQLITE_BUSY immediately. Applies to every open
    // (read and write) because even read-only connections in WAL mode touch the
    // -shm file at startup and can briefly race a writer. Without this, the
    // live event loop was dying on its initial open under transient contention,
    // dropping the FSEvents receiver and silently stopping live index updates
    // for the rest of the session.
    //
    // FIRST, before anything that takes a lock: `journal_mode = WAL` and the root
    // sentinel insert both need one, and a busy handler that isn't installed yet
    // can't back them off.
    conn.execute_batch(
        "PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;",
    )?;
    cmdr_fs::sqlite_util::apply_page_cache(conn, readonly)?;
    cmdr_fs::sqlite_util::apply_statement_cache(conn, readonly);
    if !readonly {
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;",
        )?;
        // WAL/checkpoint cadence hardening (write connection only; read-only
        // connections never commit or checkpoint, so these would be no-ops there).
        //
        // With `synchronous = NORMAL` + WAL, ordinary commits don't fsync — the
        // fsync barriers happen at CHECKPOINT time (fsync the WAL, copy pages into
        // the main DB, fsync the main DB; an F_FULLFSYNC each on APFS). SQLite's
        // default `wal_autocheckpoint` of 1 000 pages (~4 MiB) fires a PASSIVE
        // checkpoint inline on the committing connection every ~4 MiB of WAL, so a
        // big finalize (the `ComputeAllAggregates` write loop commits ~464 chunked
        // autocommit transactions) crosses that boundary many times and turns
        // finalize into an fsync storm. That is the most likely trigger of the
        // `SQLITE_IOERR` the root index hit mid-scan and never recovered from; the
        // confirm path is the primary+extended SQLite code now logged when a fatal
        // storage error trips `IndexPhase::Failed`. Raise the threshold to 4 000
        // pages (~16 MiB, matching the write connection's
        // `sqlite_util::WRITE_PAGE_CACHE_KIB` page cache) so implicit checkpoints
        // fire ~4x less often (fewer fsync barriers) while the WAL between them
        // stays small enough to keep reads fast. That pairing is why the write
        // budget is 16 MiB: change one and reconsider the other. It's write-side
        // only — read connections never commit or checkpoint, which is why they
        // hold no dirty-page window and run the smaller `READ_PAGE_CACHE_KIB`.
        //
        // `journal_size_limit` caps the on-disk `-wal` file after a checkpoint
        // resets it: a backstop for the window between the 30 s
        // `wal_checkpoint(TRUNCATE)` maintenance ticks (`writer/maintenance.rs`)
        // and the explicit post-scan checkpoint. 64 MiB gives 4x headroom over the
        // autocheckpoint size so the file is reused in place in steady state (no
        // trim/regrow churn), yet a pathological burst — or a long-lived reader
        // blocking checkpoints — can't strand a multi-hundred-MiB `-wal`.
        conn.execute_batch(
            "PRAGMA wal_autocheckpoint = 4000;
             PRAGMA journal_size_limit = 67108864;",
        )?;
    }
    Ok(())
}

/// Create tables if they don't exist and insert root sentinel.
pub(super) fn create_tables(conn: &Connection) -> Result<(), IndexStoreError> {
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
pub(super) fn reset_schema(conn: &Connection) -> Result<(), IndexStoreError> {
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
