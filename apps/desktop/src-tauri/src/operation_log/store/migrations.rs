//! The forward-migration ladder — the first in this codebase, and the template
//! future durable DBs follow.
//!
//! Every other on-disk store here is delete-and-recreate on a version bump (the
//! drive index, `importance.db`) because it's a disposable cache. The operation
//! log lives for years, so it can't wipe on a schema change: it migrates
//! forward. [`run_migrations`] compares the stored `meta.schema_version` against
//! the ladder and, for each step newer than the stored version, runs that step's
//! `up` inside a transaction and bumps the version — stepwise, so a crash between
//! steps leaves a consistent intermediate version the next open resumes from.
//!
//! Rules the ladder enforces:
//! - **Never destroy on a version gap.** A stored version *older* than the ladder
//!   migrates up. A stored version *newer* than the ladder (a downgrade — the
//!   user ran a newer build, then an older one) is refused with
//!   [`OperationLogStoreError::SchemaDowngrade`], never wiped: the newer DB may
//!   hold data this build can't represent, and destroying it loses the user's
//!   history. Delete-and-recreate is reserved for a genuinely unparseable file
//!   (see `OperationLogStore::open`).
//! - **Each step is one transaction.** `ALTER TABLE` / backfill / index creation
//!   for version N, then the version stamp to N, commit together — so a reader
//!   never sees version N with N's schema half-applied.
//!
//! Adding a migration: append a [`Migration`] with the next version and an `up`
//! that transforms the previous schema in place. Never edit a shipped step
//! (users' DBs already ran it); never renumber. The `up` runs against whatever
//! the prior steps produced, so it must be written against that shape, not the
//! latest structs.

use rusqlite::{Connection, Transaction};

use super::OperationLogStoreError;

/// One forward step in the ladder: bring the schema from `version - 1` to
/// `version`. `up` runs inside a transaction the runner owns; it must be
/// idempotent-safe only in the sense that it never runs twice for one DB (the
/// runner gates on the stored version).
pub struct Migration {
    /// The schema version this step produces. Strictly increasing across the
    /// ladder; the highest is the current version.
    pub version: u32,
    /// A short human note for the dump bin and logs. Not load-bearing.
    pub description: &'static str,
    /// Transform the schema from the prior version to [`version`](Self::version).
    pub up: fn(&Transaction<'_>) -> rusqlite::Result<()>,
}

/// The production ladder. Version 1 creates the whole initial schema; later
/// schema changes append steps here.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "initial schema: dirs, operations, operation_items",
    up: migrate_v1_initial,
}];

/// The meta key holding the integer schema version (as text). Absent ⇒ 0 (a
/// fresh DB that hasn't run any step). The migration anchor.
pub const SCHEMA_VERSION_KEY: &str = "schema_version";

/// Run the ladder against `conn`, bringing the stored schema version up to the
/// highest in `migrations`. Bootstraps the `meta` table first (the anchor lives
/// outside the ladder). Refuses a downgrade; never destroys.
///
/// Parameterized over `migrations` so the ladder mechanism is tested with
/// synthetic steps independent of the production schema.
pub fn run_migrations(conn: &Connection, migrations: &[Migration]) -> Result<(), OperationLogStoreError> {
    bootstrap_meta(conn)?;
    let current = read_schema_version(conn)?;
    let target = migrations.iter().map(|m| m.version).max().unwrap_or(0);

    if current > target {
        // A downgrade: the DB was written by a newer build. Refuse — never
        // destroy a newer DB (it may hold data we can't represent). The caller
        // surfaces this; the file stays untouched.
        return Err(OperationLogStoreError::SchemaDowngrade {
            found: current,
            expected: target,
        });
    }

    // Apply each pending step, oldest first, each in its own transaction so a
    // crash between steps leaves a consistent intermediate version.
    let mut pending: Vec<&Migration> = migrations.iter().filter(|m| m.version > current).collect();
    pending.sort_by_key(|m| m.version);
    for migration in pending {
        let tx = conn.unchecked_transaction()?;
        (migration.up)(&tx)?;
        stamp_schema_version(&tx, migration.version)?;
        tx.commit()?;
        log::info!(
            target: "operation_log",
            "operation-log DB migrated to schema v{} ({})",
            migration.version,
            migration.description
        );
    }
    Ok(())
}

/// Stamp the schema version into `meta`. Runs inside the migration step's
/// transaction so the version and the schema change commit atomically.
fn stamp_schema_version(conn: &Connection, version: u32) -> Result<(), OperationLogStoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![SCHEMA_VERSION_KEY, version.to_string()],
    )?;
    Ok(())
}

/// Create the `meta` key/value table if absent. Idempotent and safe on every
/// open; it's the anchor the ladder reads the version from.
fn bootstrap_meta(conn: &Connection) -> Result<(), OperationLogStoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        ) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Read the stored schema version (absent ⇒ 0).
pub(super) fn read_schema_version(conn: &Connection) -> Result<u32, OperationLogStoreError> {
    let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![SCHEMA_VERSION_KEY], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(v)) => Ok(v.parse::<u32>().unwrap_or(0)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(0),
    }
}

/// Version 1: the initial schema. Interned `dirs`, grouped `operations`, per-item
/// `operation_items`. All classification columns are TEXT tokens
/// ([`super::super::types`]) so the DB stays `sqlite3`-inspectable (D2).
fn migrate_v1_initial(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        -- Interned directory prefixes. A 1M-file op under one tree shares a
        -- handful of dirs; interning stores each hot dir once, forever. Item
        -- rows reference a dir_id + a leaf name. `parent_dir_id` is NULL only for
        -- a volume-root row (name ''). The UNIQUE identity uses IFNULL so the two
        -- root interns of one volume dedupe despite SQLite treating NULLs as
        -- distinct in a plain UNIQUE.
        CREATE TABLE dirs (
            dir_id        INTEGER PRIMARY KEY,
            volume_id     TEXT    NOT NULL,
            parent_dir_id INTEGER REFERENCES dirs(dir_id),
            name          TEXT    NOT NULL,
            name_folded   TEXT    NOT NULL
        );
        CREATE UNIQUE INDEX dirs_identity
            ON dirs (volume_id, IFNULL(parent_dir_id, 0), name_folded);

        -- One row per user-level operation, 1:1 with the pipeline's operation_id.
        CREATE TABLE operations (
            op_id                   TEXT    PRIMARY KEY,
            kind                    TEXT    NOT NULL,
            archive_subkind         TEXT,
            initiator               TEXT    NOT NULL,
            execution_status        TEXT    NOT NULL,
            rollback_state          TEXT    NOT NULL,
            not_rollbackable_reason TEXT,
            rolls_back_op_id        TEXT    REFERENCES operations(op_id),
            source_volume_id        TEXT,
            dest_volume_id          TEXT,
            started_at              INTEGER NOT NULL,
            ended_at                INTEGER,
            item_count              INTEGER NOT NULL DEFAULT 0,
            items_done              INTEGER NOT NULL DEFAULT 0,
            bytes_total             INTEGER NOT NULL DEFAULT 0,
            search_coverage         TEXT    NOT NULL DEFAULT 'full',
            search_coverage_reason  TEXT,
            dev_summary             TEXT
        ) WITHOUT ROWID;
        CREATE INDEX operations_ended_at ON operations (ended_at);
        CREATE INDEX operations_rolls_back ON operations (rolls_back_op_id)
            WHERE rolls_back_op_id IS NOT NULL;

        -- Per-item rows. A directory the op created is a first-class row
        -- (entry_type = dir) sequenced after its contents, so a seq DESC rollback
        -- removes files before the dirs that held them.
        CREATE TABLE operation_items (
            item_id            INTEGER PRIMARY KEY,
            op_id              TEXT    NOT NULL REFERENCES operations(op_id),
            seq                INTEGER NOT NULL,
            entry_type         TEXT    NOT NULL,
            row_role           TEXT    NOT NULL,
            source_dir_id      INTEGER NOT NULL REFERENCES dirs(dir_id),
            source_name        TEXT    NOT NULL,
            source_name_folded TEXT    NOT NULL,
            dest_dir_id        INTEGER REFERENCES dirs(dir_id),
            dest_name          TEXT,
            dest_name_folded   TEXT,
            size               INTEGER,
            mtime              INTEGER,
            outcome            TEXT    NOT NULL,
            overwrote          INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX operation_items_op_seq ON operation_items (op_id, seq);
        CREATE INDEX operation_items_source_name ON operation_items (source_name_folded);
        CREATE INDEX operation_items_dest_name ON operation_items (dest_name_folded)
            WHERE dest_name_folded IS NOT NULL;
        ",
    )
}
