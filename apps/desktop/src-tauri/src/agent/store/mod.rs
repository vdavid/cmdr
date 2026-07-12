//! `main.db`: the agent's durable store — schema, the migration ladder, connection
//! pragmas, and the conversation / message / FTS / cost-meter query layer.
//!
//! The app's second durable DB, a peer to `operation-log.db` in the app data dir
//! (agent-spec D1/D3). Like the operation log and unlike every disposable per-volume
//! cache here, it lives for years, so it carries a forward-migration ladder
//! (`migrations`) and refuses to wipe on a version bump. It reuses that template exactly;
//! the one net-new piece is the external-content FTS5 index over message text (the
//! operation log folds a column in Rust instead of using FTS5). Full rationale:
//! `DETAILS.md`.
//!
//! [`AgentStore`] owns the schema lifecycle (migrate on open; delete-and-recreate only a
//! genuinely unparseable file; refuse a downgrade). Reads and writes go through
//! connections opened from the DB path (`connection`); the chat runtime (M5) owns the
//! write-connection lifetime.

mod connection;
mod migrations;
mod query;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, ErrorCode};

pub use connection::open_read_connection;
pub(crate) use connection::open_write_connection;
pub use migrations::{MIGRATIONS, Migration, run_migrations};
pub use query::{
    ConversationDetail, ConversationRow, ConversationSearchHit, CostDay, CostRecord, CostSummary, StoredMessage,
    append_message, archive_conversation, cost_summary, create_conversation, get_conversation, list_conversations,
    list_messages, record_cost, rename_conversation, sanitize_fts_query, search_conversations,
};

/// The durable store's file name in the app data dir. Peer to `operation-log.db`.
pub const MAIN_DB_FILE: &str = "main.db";

/// Resolve the `main.db` path inside `data_dir`.
pub fn main_db_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MAIN_DB_FILE)
}

/// Errors from the agent store.
#[derive(Debug)]
pub enum AgentStoreError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    /// The on-disk schema version is newer than this build's ladder (a downgrade).
    /// Refused, never destroyed — the newer DB may hold data this build can't represent.
    SchemaDowngrade {
        found: u32,
        expected: u32,
    },
    /// A stored classification token didn't parse to a known enum variant (a newer
    /// schema wrote it, or corruption). Carried for logging only; never branched on for
    /// control flow.
    Decode {
        column: &'static str,
        value: String,
    },
    /// A persisted `content_blocks` JSON value failed to parse back into the typed
    /// message-part model. Carried for logging only.
    ContentBlocks(serde_json::Error),
}

impl std::fmt::Display for AgentStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStoreError::Sqlite(e) => write!(f, "main.db sqlite error: {e}"),
            AgentStoreError::Io(e) => write!(f, "main.db io error: {e}"),
            AgentStoreError::SchemaDowngrade { found, expected } => {
                write!(
                    f,
                    "main.db schema downgrade (found v{found}, this build expects v{expected})"
                )
            }
            AgentStoreError::Decode { column, value } => {
                write!(f, "main.db undecodable token in {column}: {value:?}")
            }
            AgentStoreError::ContentBlocks(e) => write!(f, "main.db undecodable content_blocks: {e}"),
        }
    }
}

impl std::error::Error for AgentStoreError {}

impl From<rusqlite::Error> for AgentStoreError {
    fn from(e: rusqlite::Error) -> Self {
        AgentStoreError::Sqlite(e)
    }
}

impl From<std::io::Error> for AgentStoreError {
    fn from(e: std::io::Error) -> Self {
        AgentStoreError::Io(e)
    }
}

/// Is this a file we can never parse as a SQLite DB (garbage bytes, corrupt header)? The
/// one case where delete-and-recreate is the right move — matched on the typed sqlite
/// error code, never a message string (`no-string-matching`).
fn is_unparseable(err: &AgentStoreError) -> bool {
    matches!(
        err,
        AgentStoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _))
            if e.code == ErrorCode::NotADatabase || e.code == ErrorCode::DatabaseCorrupt
    )
}

/// A handle to `main.db`, owning a connection for direct reads and the schema lifecycle.
/// The chat runtime opens its own connections for the live read/write path.
pub struct AgentStore {
    db_path: PathBuf,
    conn: Connection,
}

impl AgentStore {
    /// Open (or create) the DB, migrating the schema forward to the current version. A
    /// downgrade is refused (returned as an error, DB untouched); a genuinely unparseable
    /// file is deleted and recreated fresh; a transient error (IO) propagates without
    /// destroying anything.
    pub fn open(db_path: &Path) -> Result<Self, AgentStoreError> {
        match Self::try_open(db_path) {
            Ok(store) => Ok(store),
            Err(AgentStoreError::SchemaDowngrade { found, expected }) => {
                log::warn!(
                    target: "agent::store",
                    "main.db is newer than this build (found v{found}, expected v{expected}); leaving it untouched"
                );
                Err(AgentStoreError::SchemaDowngrade { found, expected })
            }
            Err(e) if is_unparseable(&e) => {
                log::warn!(target: "agent::store", "main.db is unparseable ({e}); deleting and recreating");
                Self::delete_and_recreate(db_path)
            }
            Err(e) => Err(e),
        }
    }

    fn try_open(db_path: &Path) -> Result<Self, AgentStoreError> {
        let conn = open_write_connection(db_path)?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
            conn,
        })
    }

    fn delete_and_recreate(db_path: &Path) -> Result<Self, AgentStoreError> {
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

    /// Borrow the connection for direct reads and writes (the store handle owns a
    /// write-capable connection; tests use it directly).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// The current on-disk schema version.
    pub fn schema_version(&self) -> Result<u32, AgentStoreError> {
        migrations::read_schema_version(&self.conn)
    }
}

#[cfg(test)]
mod tests;
