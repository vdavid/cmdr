//! IPC commands for the operation log's read side (M4).
//!
//! Thin pass-throughs over [`crate::operation_log::query`]: the business logic
//! (filtering, paging, dir-path resolution) lives in the query module; these
//! commands only open a short-lived read-only connection off the IPC thread and
//! forward the call. The Debug panel (M6) and alpha dialog (M7) consume them.

use tauri::AppHandle;

use crate::operation_log::query::{self, OperationDetail};
use crate::operation_log::store::{OperationLogStoreError, OperationRow, open_read_connection, operation_log_db_path};

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
