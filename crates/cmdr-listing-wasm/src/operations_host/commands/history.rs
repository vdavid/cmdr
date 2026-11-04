//! Undo, rollback, and the operation log's reads (`commands/operation_log.rs`
//! in the app), on the real journal and rollback planner.

use std::path::Path;

use cmdr_operation_log::query;
use cmdr_operation_log::rollback::RollbackRefusal;
use cmdr_operation_log::store::{OperationLogStoreError, open_read_connection};
use cmdr_operation_log::types::{Initiator, OperationId};
use cmdr_operations::UndoReport;
use serde::Deserialize;
use serde_json::Value;

use super::super::DemoOperations;
use super::OperationIdsArgs;
use crate::invoke::{answer, ok, parse};

pub(super) const NAMES: &[&str] = &[
    "undo_operations",
    "rollback_operation",
    "get_recent_operation_log_entries",
    "get_operation_log_detail",
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
            other => Err(Value::String(format!("the demo doesn't answer `{other}`"))),
        }
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
