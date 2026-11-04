//! The transfer queue: its snapshot, the live tree view, and every control the
//! queue window has (`commands/file_system/operations.rs` in the app).

use cmdr_operation_log::types::OperationId;
use cmdr_operations::{
    PauseOutcome, QueueMoveDirection, QueueMoveOutcome, RetryOutcome, TransferTreeViewError, TransferTreeViewRequest,
};
use serde::Deserialize;
use serde_json::Value;

use super::super::DemoOperations;
use super::{OperationIdArgs, OperationIdsArgs};
use crate::invoke::{answer, ok, parse};

pub(super) const NAMES: &[&str] = &[
    "list_operations",
    "latest_operation_progress",
    "get_transfer_tree_view",
    "release_transfer_tree",
    "cancel_operation",
    "cancel_operations",
    "pause_operation",
    "resume_operation",
    "pause_all",
    "resume_all",
    "move_queued_operation",
    "place_queued_operation",
    "dismiss_failed_operation",
    "retry_failed_operation",
    "dismiss_all_failed_operations",
];

impl DemoOperations {
    pub(super) async fn queue(&self, command: &str, args: Value) -> Result<Value, Value> {
        let e = &self.engine;
        match command {
            "list_operations" => ok(e.list_operations()),
            "latest_operation_progress" => ok(e.latest_operation_progress()),
            "get_transfer_tree_view" => {
                let a: TreeViewArgs = parse(command, args)?;
                let view: Result<_, TransferTreeViewError> =
                    e.transfer_trees().view(&OperationId::from(a.operation_id), &a.request);
                answer(view)
            }
            "release_transfer_tree" => {
                let a: OperationIdArgs = parse(command, args)?;
                e.transfer_trees().release(&OperationId::from(a.operation_id));
                Ok(Value::Null)
            }
            "cancel_operation" => {
                let a: OperationIdArgs = parse(command, args)?;
                e.cancel_operation(&OperationId::from(a.operation_id));
                Ok(Value::Null)
            }
            "cancel_operations" => {
                let a: OperationIdsArgs = parse(command, args)?;
                let ids: Vec<OperationId> = a.operation_ids.into_iter().map(OperationId::from).collect();
                e.cancel_operations(&ids);
                Ok(Value::Null)
            }
            "pause_operation" | "resume_operation" => {
                let a: OperationIdArgs = parse(command, args)?;
                let id = OperationId::from(a.operation_id);
                let outcome: PauseOutcome = if command == "pause_operation" {
                    e.pause_operation(&id)
                } else {
                    e.resume_operation(&id)
                };
                ok(outcome)
            }
            "pause_all" => ok(e.pause_all()),
            "resume_all" => ok(e.resume_all()),
            "move_queued_operation" => {
                let a: QueueMoveArgs = parse(command, args)?;
                let outcome: QueueMoveOutcome =
                    e.move_queued_operation(&OperationId::from(a.operation_id), a.direction);
                ok(outcome)
            }
            "place_queued_operation" => {
                let a: QueuePlaceArgs = parse(command, args)?;
                let outcome: QueueMoveOutcome =
                    e.place_queued_operation(&OperationId::from(a.operation_id), a.position);
                ok(outcome)
            }
            "dismiss_failed_operation" => {
                let a: OperationIdArgs = parse(command, args)?;
                e.dismiss_failed_operation(&OperationId::from(a.operation_id));
                Ok(Value::Null)
            }
            "retry_failed_operation" => {
                let a: OperationIdArgs = parse(command, args)?;
                let outcome: Result<RetryOutcome, _> =
                    e.retry_failed_operation(&OperationId::from(a.operation_id)).await;
                answer(outcome)
            }
            "dismiss_all_failed_operations" => {
                e.dismiss_all_failed_operations();
                Ok(Value::Null)
            }
            other => Err(Value::String(format!("the demo doesn't answer `{other}`"))),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TreeViewArgs {
    operation_id: String,
    request: TransferTreeViewRequest,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueMoveArgs {
    operation_id: String,
    direction: QueueMoveDirection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueuePlaceArgs {
    operation_id: String,
    position: u32,
}
