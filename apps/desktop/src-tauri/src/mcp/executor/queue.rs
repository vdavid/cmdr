//! The `queue` tool: control the operation queue (pause / resume / cancel, and
//! the global pause-all / resume-all).
//!
//! Thin adapter over the typed `write_operations` manager functions (smart
//! backend / thin frontend). It dispatches no FE action and invents no ack — it
//! calls the backend directly and returns OK (the `connect_to_server` /
//! `indexing` precedent, plan §3.5). These are transient runtime actions on a
//! crash-safe pipeline, so pause / resume / plain cancel are `Open`; only a
//! `rollback: true` cancel (which DELETES already-copied files) is token-gated
//! (`TokenGate::IfRollback`).
//!
//! Discovery of operation ids + their status lives in `cmdr://state` under
//! `operations:` (the two-source join in `resources/operations.rs`).

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
use crate::file_system::{
    cancel_operation, cancel_operations, cancel_write_operation, list_operations, pause_all, pause_operation,
    resume_all, resume_operation,
};

pub async fn execute_queue(params: &Value) -> ToolResult {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'action' parameter"))?;

    match action {
        "pause_all" => {
            pause_all();
            Ok(json!("OK: Paused every running operation."))
        }
        "resume_all" => {
            resume_all();
            Ok(json!("OK: Resumed every paused operation."))
        }
        "pause" => {
            let id = require_operation_id(params)?;
            require_operation_exists(&id)?;
            pause_operation(&id);
            Ok(json!(format!("OK: Paused operation {id}.")))
        }
        "resume" => {
            let id = require_operation_id(params)?;
            require_operation_exists(&id)?;
            resume_operation(&id);
            Ok(json!(format!("OK: Resumed operation {id}.")))
        }
        "cancel" => execute_cancel(params),
        other => Err(ToolError::invalid_params(format!(
            "action must be 'pause', 'resume', 'cancel', 'pause_all', or 'resume_all' (got '{other}')"
        ))),
    }
}

/// Cancel one or several operations. `rollback: true` (single-op only) routes to
/// the rollback-capable cancel that deletes already-copied files; everything else
/// keeps partials.
fn execute_cancel(params: &Value) -> ToolResult {
    let rollback = params.get("rollback").and_then(|v| v.as_bool()).unwrap_or(false);

    // Multi-op cancel via `operationIds`. Rollback is single-op only (there's no
    // batch rollback backend), so combining the two is an honest error, not a
    // silent partial.
    if let Some(ids_value) = params.get("operationIds") {
        let ids: Vec<String> = ids_value
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            return Err(ToolError::invalid_params(
                "'operationIds' must be a non-empty array of operation id strings",
            ));
        }
        if rollback {
            return Err(ToolError::invalid_params(
                "rollback is only supported for a single operationId, not operationIds",
            ));
        }
        cancel_operations(&ids);
        let summary = crate::pluralize::pluralize(ids.len() as u64, "operation");
        return Ok(json!(format!("OK: Cancelled {summary} (kept already-copied files).")));
    }

    let id = require_operation_id(params)?;
    require_operation_exists(&id)?;
    if rollback {
        cancel_write_operation(&id, true);
        Ok(json!(format!(
            "OK: Cancelled operation {id} and rolled back (deleted already-copied files)."
        )))
    } else {
        cancel_operation(&id);
        Ok(json!(format!(
            "OK: Cancelled operation {id} (kept already-copied files)."
        )))
    }
}

fn require_operation_id(params: &Value) -> Result<String, ToolError> {
    params
        .get("operationId")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| ToolError::invalid_params("This action requires an 'operationId' parameter"))
}

/// Reject an id that isn't currently registered, so an agent gets an honest
/// "unknown operationId" instead of a silent no-op (the backend treats an unknown
/// id as a no-op). Benign race: an op that settles between this check and the
/// call would have been a no-op anyway.
fn require_operation_exists(operation_id: &str) -> Result<(), ToolError> {
    if list_operations().iter().any(|op| op.operation_id == operation_id) {
        Ok(())
    } else {
        Err(ToolError::invalid_params(format!(
            "Unknown operationId '{operation_id}': it isn't a currently queued, running, or paused operation. See cmdr://state operations."
        )))
    }
}
