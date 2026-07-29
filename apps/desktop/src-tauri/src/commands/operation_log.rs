//! IPC commands over the operation log: the read side (the query API), plus undo.
//!
//! The reads are thin pass-throughs over [`crate::operation_log::query`]: the
//! business logic (filtering, paging, dir-path resolution) lives in the query
//! module; these commands only open a short-lived read-only connection off the IPC
//! thread and forward the call. The Debug panel and alpha dialog consume them.
//!
//! [`undo_operations`] is the write side: the frontend-facing entry to the rollback
//! engine (the MCP `operations_rollback` tool is the other consumer). It resolves
//! only when every operation has been reversed, so the caller gets one complete,
//! ordered tally instead of running its own dispatch-then-poll loop.

use tauri::AppHandle;

use crate::file_system::write_operations::rollback::{UndoReport, undo_operations as run_undo};
use crate::operation_log::query::{self, OperationDetail};
use crate::operation_log::store::{OperationLogStoreError, OperationRow, open_read_connection, operation_log_db_path};
use crate::operation_log::types::Initiator;

/// Resolve the `operation-log.db` path and run `read` on a read-only connection,
/// off the IPC thread. A missing/unopened DB (the journal failed to start) yields
/// the read's natural empty result rather than an error, so the UI degrades to
/// "no history" instead of surfacing a failure.
async fn with_read_connection<T, F>(app: AppHandle, empty: T, read: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T, OperationLogStoreError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let data_dir = crate::config::resolved_app_data_dir(&app).map_err(|e| e.to_string())?;
        let db_path = operation_log_db_path(&data_dir);
        if !db_path.exists() {
            return Ok(empty);
        }
        let conn = open_read_connection(&db_path).map_err(|e| e.to_string())?;
        read(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The recent-operations feed (newest first), paged — the alpha UI's "last 50 +
/// load 50 more" and the Debug panel's list.
#[tauri::command]
#[specta::specta]
pub async fn get_recent_operation_log_entries(
    app: AppHandle,
    limit: u32,
    offset: u32,
) -> Result<Vec<OperationRow>, String> {
    with_read_connection(app, Vec::new(), move |conn| {
        query::recent_operations(conn, limit, offset)
    })
    .await
}

/// One operation's header plus a page of its items (dir prefixes resolved to full
/// paths). `None` when the operation is absent.
#[tauri::command]
#[specta::specta]
pub async fn get_operation_log_detail(
    app: AppHandle,
    operation_id: String,
    item_limit: u32,
    item_offset: u32,
) -> Result<Option<OperationDetail>, String> {
    with_read_connection(app, None, move |conn| {
        query::get_operation(conn, &operation_id, item_limit, item_offset)
    })
    .await
}

/// Undo the given operations as one action, **newest first** (a multi-batch rename
/// run is the case this exists for; the order is data-safety-critical, see
/// `rollback::undo_order`). Pass the ids in the order they were APPLIED.
///
/// Resolves once every operation has been reversed, with the full tally: what came
/// back, what was left alone, and per operation. It can take a while — each inverse
/// is a queued managed operation, so it also waits out anything already working the
/// same volume. The user sees the operation queue meanwhile.
///
/// `Initiator::User` throughout: the agent proposed the rename, but undoing it is
/// the user's own action.
#[tauri::command]
#[specta::specta]
pub async fn undo_operations(app: AppHandle, operation_ids: Vec<String>) -> Result<UndoReport, String> {
    Ok(run_undo(&app, &operation_ids, Initiator::User).await)
}
