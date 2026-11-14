//! IPC commands for Ask Cmdr, the read-only chat rail.
//! Thin pass-throughs: the runtime, store, and context assembly all live in
//! [`crate::agent`]; these commands only bridge the frontend to them.
//!
//! One file per command family, plus the wire shapes they share:
//! - [`views`]: the specta-typed DTOs the commands share, with the pure mappings that build
//!   them (one row's `MessageView` is `agent::chat::message_view`'s, re-exported).
//! - [`chat`]: send + cancel — the send gate (Ask Cmdr's switch, then the slot), the envelope
//!   snapshot, the cancel registry, and the `Channel` bridge.
//! - [`attachments`]: resolving what the user attached, by reference only.
//! - [`bulk_rename`]: review and apply a server-owned rename proposal.
//! - [`conversations`]: thread history (read, list, search, rename, archive).
//! - [`legacy_opt_in`]: the one-time read of Ask Cmdr's old opt-in, for the `askCmdr.enabled`
//!   mapping. Cloud consent's commands live in `ai::cloud_consent`.
//! - [`cost`]: the per-thread footer total and the per-day rollup.
//! - [`memory`]: the settings section's two memory controls (where the notes are, wipe them).
//! - [`suggested_ops`]: what the Suggested ops dialog reads, and the rejection it records.
//! - [`suggested_ops_live`]: the dialog's live per-file facts, a window at a time.
//! - [`suggested_ops_server`]: a server card's approve.
//! - [`wake`]: the status corner's seed read of the wake indicator (settings moves reach the loop
//!   through `settings::store_watch`, not a push).
//!
//! The two connection helpers below are the only shared plumbing: every store-reading
//! command opens a short-lived connection off the IPC thread through them, so a missing
//! store degrades to an empty result instead of a failure.

mod attachments;
mod bulk_rename;
mod chat;
mod conversations;
mod cost;
mod legacy_opt_in;
mod memory;
mod suggested_ops;
mod suggested_ops_live;
mod suggested_ops_server;
mod views;
mod wake;

// Glob re-exports, so each `#[tauri::command]`'s generated companion items (`__cmd__*`,
// `__tauri_command_name_*`, `__specta__fn__*`) come along: the `ipc.rs` manifest registers
// every command by its `crate::commands::agent::<name>` path,
// and a named re-export would leave those hidden items behind. Same pattern as
// `commands/file_system/mod.rs`.
pub use attachments::*;
pub use bulk_rename::*;
pub use chat::*;
pub use conversations::*;
pub use cost::*;
pub use legacy_opt_in::*;
pub use memory::*;
pub use suggested_ops::*;
pub use suggested_ops_live::*;
pub use suggested_ops_server::*;
pub use views::*;
pub use wake::*;

use tauri::{AppHandle, Manager};

use crate::agent::AgentDb;
use crate::agent::store;

const LOG_TARGET: &str = "agent::ipc";

/// The `main.db` path, or `None` when the store never opened. Every command here degrades to
/// an empty answer in that case rather than surfacing a failure the user can't act on.
fn db_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf())
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Open a short-lived WRITE connection to `main.db` off the IPC thread and run `write`
/// (opening a write connection runs the idempotent migration ladder). A missing store
/// (agent start failed) is a silent no-op — there are no conversations to mutate.
async fn with_write_connection<F>(app: AppHandle, write: F) -> Result<(), String>
where
    F: FnOnce(&rusqlite::Connection) -> Result<(), store::AgentStoreError> + Send + 'static,
{
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open_write_connection(&db_path).map_err(|e| e.to_string())?;
        write(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open a short-lived read connection to `main.db` off the IPC thread and run `read`. A
/// missing store (agent start failed) yields the read's `empty` result, so the rail
/// degrades to "no history" rather than surfacing a failure.
async fn with_read_connection<T, F>(app: AppHandle, empty: T, read: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T, store::AgentStoreError> + Send + 'static,
{
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return Ok(empty);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open_read_connection(&db_path).map_err(|e| e.to_string())?;
        read(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
