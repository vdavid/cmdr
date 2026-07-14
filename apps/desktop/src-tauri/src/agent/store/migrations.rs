//! The forward-migration ladder for `main.db` — the second in this codebase, mirroring
//! the operation log's (`operation_log/store/migrations.rs`), which proved the pattern.
//!
//! `main.db` lives for years (agent-spec D1/D3), so it never delete-and-recreates on a
//! schema change: it migrates forward. [`run_migrations`] compares the stored
//! `meta.schema_version` against the ladder and, for each step newer than the stored
//! version, runs that step's `up` inside a transaction and bumps the version — stepwise,
//! so a crash between steps leaves a consistent intermediate version the next open
//! resumes from.
//!
//! Rules the ladder enforces:
//! - **Never destroy on a version gap.** A stored version *older* than the ladder
//!   migrates up. A stored version *newer* (a downgrade) is refused with
//!   [`AgentStoreError::SchemaDowngrade`], never wiped — the newer DB may hold data this
//!   build can't represent. Delete-and-recreate is reserved for a genuinely unparseable
//!   file (see `AgentStore::open`).
//! - **Each step is one transaction.** Table/index/trigger creation for version N, then
//!   the version stamp to N, commit together — so a reader never sees version N with N's
//!   schema half-applied.
//!
//! Adding a migration: append a [`Migration`] with the next version and an `up` that
//! transforms the previous schema in place. Never edit or renumber a shipped step
//! (users' DBs already ran it); the `up` runs against whatever the prior steps produced.

use rusqlite::{Connection, Transaction};

use super::AgentStoreError;

/// One forward step in the ladder: bring the schema from `version - 1` to `version`.
/// `up` runs inside a transaction the runner owns; the runner gates on the stored
/// version so it never runs twice for one DB.
pub struct Migration {
    /// The schema version this step produces. Strictly increasing across the ladder;
    /// the highest is the current version.
    pub version: u32,
    /// A short human note for logs. Not load-bearing.
    pub description: &'static str,
    /// Transform the schema from the prior version to [`version`](Self::version).
    pub up: fn(&Transaction<'_>) -> rusqlite::Result<()>,
}

/// The production ladder. Version 1 creates the whole initial schema; later schema
/// changes append steps here.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "initial schema: conversations, messages, messages_fts, cost_meter",
        up: migrate_v1_initial,
    },
    Migration {
        version: 2,
        description: "conversations.last_model for model-change events",
        up: migrate_v2_last_model,
    },
];

/// The meta key holding the integer schema version (as text). Absent ⇒ 0 (a fresh DB
/// that hasn't run any step). The migration anchor, outside the ladder.
pub const SCHEMA_VERSION_KEY: &str = "schema_version";

/// Run the ladder against `conn`, bringing the stored schema version up to the highest
/// in `migrations`. Bootstraps the `meta` table first. Refuses a downgrade; never
/// destroys. Parameterized over `migrations` so the ladder mechanism is tested with
/// synthetic steps independent of the production schema.
pub fn run_migrations(conn: &Connection, migrations: &[Migration]) -> Result<(), AgentStoreError> {
    bootstrap_meta(conn)?;
    let current = read_schema_version(conn)?;
    let target = migrations.iter().map(|m| m.version).max().unwrap_or(0);

    if current > target {
        // A downgrade: the DB was written by a newer build. Refuse — never destroy a
        // newer DB (it may hold data we can't represent). The caller surfaces this; the
        // file stays untouched.
        return Err(AgentStoreError::SchemaDowngrade {
            found: current,
            expected: target,
        });
    }

    // Apply each pending step, oldest first, each in its own transaction so a crash
    // between steps leaves a consistent intermediate version.
    let mut pending: Vec<&Migration> = migrations.iter().filter(|m| m.version > current).collect();
    pending.sort_by_key(|m| m.version);
    for migration in pending {
        let tx = conn.unchecked_transaction()?;
        (migration.up)(&tx)?;
        stamp_schema_version(&tx, migration.version)?;
        tx.commit()?;
        log::info!(
            target: "agent::store",
            "main.db migrated to schema v{} ({})",
            migration.version,
            migration.description
        );
    }
    Ok(())
}

/// Stamp the schema version into `meta`. Runs inside the migration step's transaction so
/// the version and the schema change commit atomically.
fn stamp_schema_version(conn: &Connection, version: u32) -> Result<(), AgentStoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![SCHEMA_VERSION_KEY, version.to_string()],
    )?;
    Ok(())
}

/// Create the `meta` key/value table if absent. Idempotent and safe on every open; it's
/// the anchor the ladder reads the version from.
fn bootstrap_meta(conn: &Connection) -> Result<(), AgentStoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        ) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Read the stored schema version (absent ⇒ 0).
pub(super) fn read_schema_version(conn: &Connection) -> Result<u32, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![SCHEMA_VERSION_KEY], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(v)) => Ok(v.parse::<u32>().unwrap_or(0)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(0),
    }
}

/// Version 1: the initial schema. Conversations, per-message rows with typed
/// `content_blocks` JSON, an external-content FTS5 index over message text kept in sync
/// by triggers, and a per-day/per-thread/per-model cost meter. All classification
/// columns are TEXT tokens ([`super::super::types`] / the LLM seam) so the DB stays
/// `sqlite3`-inspectable. Rationale for each table: `DETAILS.md`.
fn migrate_v1_initial(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        CREATE TABLE conversations (
            id         INTEGER PRIMARY KEY,
            title      TEXT    NOT NULL,          -- generated from first message; user-renamable
            created_at INTEGER NOT NULL,          -- unix secs
            updated_at INTEGER NOT NULL,
            archived   INTEGER NOT NULL DEFAULT 0,-- 0/1 flag + filter; no delete in v1
            origin     TEXT                       -- nullable snake_case token; NULL = user-started
        );
        CREATE INDEX conversations_updated ON conversations (archived, updated_at DESC, id DESC);

        CREATE TABLE messages (
            id                INTEGER PRIMARY KEY,
            conversation_id   INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            seq               INTEGER NOT NULL,        -- per-conversation ordinal
            role              TEXT    NOT NULL,        -- token: system|user|assistant|tool
            content_blocks    TEXT    NOT NULL,        -- JSON: ordered typed parts; opaque provider
                                                       -- state rides here and NEVER crosses to the frontend
            text_for_search   TEXT    NOT NULL DEFAULT '', -- plain user+assistant text, extracted at insert
            prompt_tokens     INTEGER,                 -- nullable; assistant turns only
            completion_tokens INTEGER,
            created_at        INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX messages_conv_seq ON messages (conversation_id, seq);

        -- External-content FTS5 over message text. `content='messages'` means the index
        -- stores no copy of the text; it points at `messages.id` (content_rowid). The
        -- three triggers keep it synced: an edit re-indexes, a delete de-indexes.
        CREATE VIRTUAL TABLE messages_fts USING fts5 (
            text_for_search,
            content='messages',
            content_rowid='id'
        );
        CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, text_for_search) VALUES (new.id, new.text_for_search);
        END;
        CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, text_for_search) VALUES('delete', old.id, old.text_for_search);
        END;
        CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, text_for_search) VALUES('delete', old.id, old.text_for_search);
            INSERT INTO messages_fts(rowid, text_for_search) VALUES (new.id, new.text_for_search);
        END;

        -- Per-day, per-thread, per-model token + cost rollup. `conversation_id` is NOT
        -- NULL: SQLite treats NULLs as distinct in a PK/UNIQUE, so a nullable column
        -- inside the PK breaks ON CONFLICT DO UPDATE (every write inserts a duplicate
        -- instead of upserting). One row per real thread; the per-day cross-thread
        -- rollup is computed at query time (SUM ... GROUP BY day).
        CREATE TABLE cost_meter (
            day               TEXT    NOT NULL,        -- YYYY-MM-DD, local day
            conversation_id   INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            provider          TEXT    NOT NULL,        -- ProviderTag token
            model             TEXT    NOT NULL,
            prompt_tokens     INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            cost_micros       INTEGER NOT NULL DEFAULT 0,  -- integer micro-USD; honest estimate
            priced            INTEGER NOT NULL DEFAULT 1,  -- 0 when the model wasn't in the price table
            PRIMARY KEY (day, conversation_id, provider, model)
        );
        ",
    )
}

/// Version 2: `conversations.last_model` — the model name the conversation's most recent
/// completed turn (or recorded model-change event) used. NULL means no turn has run yet.
/// The chat runtime compares against it to insert honest "switched to X" event rows when
/// the effective model changes between turns.
fn migrate_v2_last_model(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("ALTER TABLE conversations ADD COLUMN last_model TEXT;")
}
