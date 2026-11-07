//! IPC commands over the operation log: the read side (the query API), plus undo.
//!
//! The reads are thin pass-throughs over [`cmdr_operation_log::query`]: the
//! business logic (filtering, paging, dir-path resolution) lives in the query
//! module; these commands only open a short-lived read-only connection off the IPC
//! thread and forward the call. The Debug panel and alpha dialog consume them.
//!
//! Two write-side entries, for two different surfaces:
//!
//! - [`rollback_operation`] reverses ONE operation and returns as soon as the inverse
//!   is queued. The history dialog's per-row Roll back button uses it: the reversal
//!   belongs to the operation queue from that moment on, and the queue is where the
//!   user watches it.
//! - [`undo_operations`] reverses SEVERAL and resolves only once they're all done,
//!   with the tally. Ask Cmdr's rename undo needs that tally to report what came
//!   back, which a dispatch can't say yet.
//!
//! Both sit on the same engine (`cmdr_operation_log::rollback`). The MCP
//! `operations_rollback` tool opens the question whose yes calls the first one.

use tauri::AppHandle;

use std::sync::Arc;

use tauri_specta::Event as _;

use crate::operations_host::{TauriEventSink, operations, wire};
use cmdr_operation_log::query::{
    self, OperationDetail, OperationTreeRequest, OperationTreeView, TreeBackfill, TreeBackfillEvent, TreeBackfillStart,
    TreeBackfillStep,
};
use cmdr_operation_log::rollback::{RollbackDispatch, RollbackRefusal};
use cmdr_operation_log::store::{OperationLogStoreError, OperationRow, open_read_connection, operation_log_db_path};
use cmdr_operation_log::types::{Initiator, OperationId};
use cmdr_operations::{OperationEventSink, UndoReport};

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

/// Ask for an operation's tree summary, so its item tree can open: start the backfill
/// of an op journaled before the summary existed, join the one running, or say why
/// none is needed or possible (typed). Returns at once; a started or joined backfill
/// reports each step as an `operation-log-tree-backfill` event, ending with its
/// `finished` step. The job runs on the blocking pool, one chunk per step, and stops at
/// the next chunk after `cancel_operation_log_tree_backfill`.
#[tauri::command]
#[specta::specta]
pub async fn start_operation_log_tree_backfill(
    app: AppHandle,
    operation_id: String,
) -> Result<TreeBackfillStart, String> {
    let log = crate::operation_log_host::operation_log().clone();
    let (start, job) = tauri::async_runtime::spawn_blocking(move || TreeBackfill::start(&log, &operation_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if let Some(mut job) = job {
        tauri::async_runtime::spawn_blocking(move || {
            loop {
                let step = job.step();
                let event = TreeBackfillEvent {
                    op_id: job.op_id().to_string(),
                    step,
                };
                if let Err(e) = wire::OperationLogTreeBackfill(event).emit(&app) {
                    log::warn!(target: "operation_log", "couldn't emit a tree backfill step: {e}");
                }
                if matches!(step, TreeBackfillStep::Finished(_)) {
                    break;
                }
            }
        });
    }
    Ok(start)
}

/// Ask an operation's running tree backfill to stop (the dialog closed or moved to
/// another operation). A later start takes the cancel back if it comes first.
#[tauri::command]
#[specta::specta]
pub fn cancel_operation_log_tree_backfill(operation_id: String) {
    crate::operation_log_host::operation_log()
        .tree_backfills()
        .cancel(&operation_id);
}

/// One window of an operation's item tree: the rows under the request's expanded
/// folders from its offset, with its selection normalized and counted
/// (`cmdr_operation_log::query::operation_tree`). `None` when the operation is absent.
#[tauri::command]
#[specta::specta]
pub async fn get_operation_log_tree(
    app: AppHandle,
    operation_id: String,
    request: OperationTreeRequest,
) -> Result<Option<OperationTreeView>, String> {
    // The log's own registry, so an op its backfill is building answers `building`.
    let backfills = Arc::clone(crate::operation_log_host::operation_log().tree_backfills());
    with_read_connection(app, None, move |conn| {
        query::operation_tree(conn, &operation_id, &request, &backfills)
    })
    .await
}

/// Roll one logged operation back, and hand the reversal to the operation queue.
///
/// Returns after DISPATCH, not after the reversal finishes: the inverse is a normal
/// managed operation, so it shows up in the queue and the status corner like any
/// transfer, and that's where the user follows it. The gate runs synchronously
/// before this returns, so an `Ok` means the operation is already recorded as
/// `rolling_back` and the caller can say so without a re-read.
///
/// A domain refusal (unknown / already rolling back / already rolled back / not
/// rollbackable / a volume disconnected) crosses TYPED, so the dialog words it
/// itself instead of parsing a sentence.
///
/// `initiator` is who started the question a person answered: `user` for their own Roll back
/// button, or the agent whose dialog they confirmed. Required, so provenance can't fail open.
#[tauri::command]
#[specta::specta]
pub async fn rollback_operation(
    app: AppHandle,
    operation_id: String,
    initiator: Initiator,
) -> Result<RollbackDispatch, RollbackRefusal> {
    // The gate opens a read connection and reads the operation's row before it
    // spawns anything, so it runs on the blocking pool like every other DB touch here.
    // Before `install` there's no journal, so every operation is unknown.
    let engine = operations().map_err(|_| RollbackRefusal::UnknownOperation)?;
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    tauri::async_runtime::spawn_blocking(move || {
        engine.dispatch_rollback(&OperationId::from(operation_id), initiator, events)
    })
    .await
    // The gate panicked, so nothing was dispatched and nothing changed on disk.
    // `UnknownOperation` is already this module's answer for "we couldn't
    // establish this operation" (an absent writer and an unopenable journal both
    // return it), which is exactly what happened.
    .unwrap_or(Err(RollbackRefusal::UnknownOperation))
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
    let operation_ids: Vec<OperationId> = operation_ids.into_iter().map(OperationId::from).collect();
    let Ok(engine) = operations() else {
        return Ok(UndoReport::all_refused(
            &operation_ids,
            RollbackRefusal::UnknownOperation,
        ));
    };
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    Ok(engine.undo_operations(&operation_ids, Initiator::User, events).await)
}
