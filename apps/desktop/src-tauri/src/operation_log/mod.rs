//! The operation log: a durable, cross-volume journal of every file mutation,
//! the foundation for search, rollback, and a future undo.
//!
//! This module is the DURABLE STORE (M1): the schema, the forward-migration
//! ladder, the single writer thread, and dir interning. Capture (M2), rollback
//! (M3), search/retention (M4), MCP tools (M5), and the UI (M6/M7) build on it.
//!
//! Unlike every other on-disk store in the app (the drive index and
//! `importance.db` are disposable per-volume caches that delete-and-recreate on
//! a schema bump), this DB lives for years, so it introduces the codebase's
//! first forward-migration ladder (`store`'s migration ladder) and retention
//! discipline. Design rationale, the migration-ladder template, and the capture/
//! rollback contracts: `CLAUDE.md` + `DETAILS.md`.

pub mod store;
pub mod types;
pub mod writer;

use tauri::{AppHandle, Manager};

/// Open `operation-log.db` and spawn its single writer thread, placing the
/// [`OperationLogWriter`](writer::OperationLogWriter) handle in managed state so
/// the capture layer (M2) can journal through it. A single cross-volume writer,
/// no per-volume registry (D1). Failure is non-fatal: the app runs without the
/// journal rather than refusing to start.
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!(target: "operation_log", "operation log not started: {e}");
            return;
        }
    };
    let db_path = store::operation_log_db_path(&data_dir);

    // Open the store first — it owns the schema lifecycle (migrate forward, or
    // recreate a genuinely unparseable file, or refuse a downgrade). The writer
    // then opens its own write connection over the now-current schema.
    if let Err(e) = store::OperationLogStore::open(&db_path) {
        log::warn!(target: "operation_log", "operation log store not opened: {e}");
        return;
    }
    match writer::OperationLogWriter::spawn(&db_path) {
        Ok(writer) => {
            app.manage(writer);
            log::debug!(target: "operation_log", "operation log ready at {}", db_path.display());
        }
        Err(e) => log::warn!(target: "operation_log", "operation log writer not spawned: {e}"),
    }
}
