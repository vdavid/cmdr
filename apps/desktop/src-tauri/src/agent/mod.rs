//! The agent subsystem: the in-app AI agent whose first user-facing slice is
//! "Ask Cmdr", a read-only chat rail.
//!
//! The subsystem is named after the persistent entity ("the agent"), not the
//! surface, so later proactive slices (proposals, notifications) grow here too.
//! Its modules:
//!
//! - `llm`: the `AgentLlm` seam — the provider-agnostic trait, its
//!   genai-backed impl, the deterministic fake, and the typed message-part model.
//! - `store`: the `main.db` durable store; `start(app)` lands here.
//! - `tools`: the in-process read-only toolset (the agent's registry view).
//! - `chat`: the chat runtime and the pure context-assembly core.
//!
//! See `CLAUDE.md` for must-knows and `DETAILS.md` for the map.

pub mod chat;
pub mod consent;
pub mod llm;
pub mod pricing;
pub mod store;
pub mod tools;
pub mod types;

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use rusqlite::Connection;

/// The managed-state handle to `main.db`: the resolved DB path, plus connection factories
/// the chat runtime opens read/write connections from. Registered by [`start`]. Held
/// as a path (not a live `Connection`, which isn't `Sync`); the chat runtime owns the write-connection
/// lifetime and single-writer discipline.
pub struct AgentDb {
    db_path: PathBuf,
}

impl AgentDb {
    /// The `main.db` path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Open a fresh read-only connection.
    pub fn open_read_connection(&self) -> Result<Connection, store::AgentStoreError> {
        store::open_read_connection(&self.db_path)
    }

    /// Open a fresh write connection (runs the migration ladder, idempotent).
    pub fn open_write_connection(&self) -> Result<Connection, store::AgentStoreError> {
        store::open_write_connection(&self.db_path)
    }
}

/// Open `main.db` and register the [`AgentDb`] handle in managed state so the chat runtime
/// can read and write. Modeled on `operation_log::start`: the store owns the schema
/// lifecycle on open (migrate forward, recreate a genuinely unparseable file, or refuse a
/// downgrade). Failure is non-fatal — the app runs without the agent store rather than
/// refusing to start.
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!(target: "agent::store", "agent store not started: {e}");
            return;
        }
    };
    let db_path = store::main_db_path(&data_dir);

    // Open the store to run the schema lifecycle; the returned handle is dropped (its
    // connection is only needed to migrate). The runtime opens its own connections from
    // the managed path.
    match store::AgentStore::open(&db_path) {
        Ok(_store) => {
            app.manage(AgentDb {
                db_path: db_path.clone(),
            });
            app.manage(tools::propose::rename::RenameProposalStore::default());
            // Register the chat runtime against the same DB so the IPC command is a
            // thin pass-through (`app.state::<chat::runtime::ChatRuntime>()`).
            chat::runtime::register(app, db_path.clone());
            log::debug!(target: "agent::store", "main.db ready at {}", db_path.display());
        }
        Err(e) => log::warn!(target: "agent::store", "main.db not opened: {e}"),
    }
}
