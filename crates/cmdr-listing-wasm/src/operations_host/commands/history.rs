//! Undo, rollback, and the operation log's reads (`commands/operation_log.rs`
//! in the app), on the real journal and rollback planner.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::rt::task;
use cmdr_operation_log::query::{
    self, OperationTreeRequest, TreeBackfill, TreeBackfillEvent, TreeBackfillStart, TreeBackfillStep,
};
use cmdr_operation_log::rollback::RollbackRefusal;
use cmdr_operation_log::store::{OperationLogStoreError, open_read_connection};
use cmdr_operation_log::types::{Initiator, OperationId};
use cmdr_operations::UndoReport;
use serde::Deserialize;
use serde_json::Value;

use super::super::DemoOperations;
use super::super::wire::{self, emit};
use super::OperationIdsArgs;
use crate::invoke::{answer, ok, page_runtime, parse};

pub(super) const NAMES: &[&str] = &[
    "undo_operations",
    "rollback_operation",
    "get_recent_operation_log_entries",
    "get_operation_log_detail",
    "start_operation_log_tree_backfill",
    "cancel_operation_log_tree_backfill",
    "get_operation_log_tree",
];

impl DemoOperations {
    pub(super) async fn history(&self, command: &str, args: Value) -> Result<Value, Value> {
        let e = &self.engine;
        match command {
            "undo_operations" => {
                let a: OperationIdsArgs = parse(command, args)?;
                let ids: Vec<OperationId> = a.operation_ids.into_iter().map(OperationId::from).collect();
                let report: UndoReport = e.undo_operations(&ids, Initiator::User, self.op_sink()).await;
                ok(report)
            }
            "rollback_operation" => {
                let a: RollbackArgs = parse(command, args)?;
                let dispatched: Result<_, RollbackRefusal> =
                    e.dispatch_rollback(&OperationId::from(a.operation_id), a.initiator, self.op_sink());
                answer(dispatched)
            }
            "get_recent_operation_log_entries" => {
                let a: RecentArgs = parse(command, args)?;
                answer(self.read_log(Vec::new(), |conn| query::recent_operations(conn, a.limit, a.offset)))
            }
            "get_operation_log_detail" => {
                let a: DetailArgs = parse(command, args)?;
                answer(self.read_log(None, |conn| {
                    query::get_operation(conn, &a.operation_id, a.item_limit, a.item_offset)
                }))
            }
            "start_operation_log_tree_backfill" => {
                let a: TreeBackfillArgs = parse(command, args)?;
                answer(self.start_tree_backfill(&a.operation_id))
            }
            "cancel_operation_log_tree_backfill" => {
                let a: TreeBackfillArgs = parse(command, args)?;
                e.operation_log().tree_backfills().cancel(&a.operation_id);
                ok(())
            }
            "get_operation_log_tree" => {
                let a: TreeArgs = parse(command, args)?;
                answer(self.read_log(None, |conn| {
                    // The log's own registry, so an op its backfill is building answers `building`.
                    query::operation_tree(conn, &a.operation_id, &a.request, e.operation_log().tree_backfills())
                }))
            }
            other => Err(Value::String(format!("the demo doesn't answer `{other}`"))),
        }
    }

    /// The desktop's `start_operation_log_tree_backfill`: start, join, or say why
    /// not, and run a started job on the page's event loop, one chunk per turn (the
    /// page has no thread to block), each step out as `operation-log-tree-backfill`.
    fn start_tree_backfill(&self, operation_id: &str) -> Result<TreeBackfillStart, String> {
        let (start, job) = TreeBackfill::start(self.engine.operation_log(), operation_id).map_err(|e| e.to_string())?;
        if let Some(mut job) = job {
            let sink = Arc::clone(&self.sink);
            drop(page_runtime().spawn(async move {
                loop {
                    let step = job.step();
                    let event = TreeBackfillEvent {
                        op_id: job.op_id().to_string(),
                        step,
                    };
                    emit(&*sink, wire::OPERATION_LOG_TREE_BACKFILL, &event);
                    if matches!(step, TreeBackfillStep::Finished(_)) {
                        break;
                    }
                    task::yield_now().await;
                }
            }));
        }
        Ok(start)
    }

    /// A read of the operation log, as the desktop's `with_read_connection`: a
    /// log that never opened is `empty`, an error its message.
    fn read_log<T>(
        &self,
        empty: T,
        read: impl FnOnce(&rusqlite::Connection) -> Result<T, OperationLogStoreError>,
    ) -> Result<T, String> {
        if self.engine.operation_log().writer().is_none() {
            return Ok(empty);
        }
        let conn = open_read_connection(Path::new(&self.log_db)).map_err(|e| e.to_string())?;
        read(&conn).map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RollbackArgs {
    operation_id: String,
    initiator: Initiator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecentArgs {
    limit: u32,
    offset: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailArgs {
    operation_id: String,
    item_limit: u32,
    item_offset: u32,
}

/// The two tree-backfill commands' arguments. Refuses a field it doesn't know, so a
/// frontend that grew one fails loudly here instead of the demo dropping it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TreeBackfillArgs {
    operation_id: String,
}

/// `deny_unknown_fields`, as the request inside it: a field the demo hasn't learned
/// fails loudly instead of being dropped.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TreeArgs {
    operation_id: String,
    request: OperationTreeRequest,
}
