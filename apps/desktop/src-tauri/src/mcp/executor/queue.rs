//! The `queue` tool: control the operation queue (pause / resume / cancel, and
//! the global pause-all / resume-all).
//!
//! Thin adapter over the typed `write_operations` manager functions (smart
//! backend / thin frontend). It dispatches no FE action and invents no ack — it
//! calls the backend directly and returns OK (the `connect_to_server` /
//! `indexing` precedent, so there is no FE action to ack). These are transient runtime actions on a
//! crash-safe pipeline, so pause / resume / plain cancel are `Open`; only a
//! `rollback: true` cancel (which DELETES already-copied files) is token-gated
//! (`TokenGate::IfRollback`).
//!
//! Discovery of operation ids + their status lives in `cmdr://state` under
//! `operations:` (the two-source join in `resources/operations.rs`).

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
use crate::file_system::write_operations::LifecycleStatus;
use crate::file_system::{
    OperationSnapshot, cancel_operation, cancel_operations, cancel_write_operation, list_operations, pause_all,
    pause_operation, resume_all, resume_operation,
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

/// Reject an id the queue can't act on, so an agent gets an honest refusal
/// instead of a silent no-op (the backend treats an unknown id as a no-op).
/// Benign race: an op that settles between this check and the call would have
/// been a no-op anyway.
fn require_operation_exists(operation_id: &str) -> Result<(), ToolError> {
    if is_controllable(&list_operations(), operation_id) {
        Ok(())
    } else {
        Err(ToolError::invalid_params(format!(
            "Unknown operationId '{operation_id}': it isn't a currently queued, running, or paused operation. See cmdr://state operations."
        )))
    }
}

/// Whether pause / resume / cancel can still do anything to this id.
///
/// Membership in `list_operations()` isn't the test: the snapshot also carries
/// RETAINED FAILURES, which the queue window keeps so it can say why an
/// operation stopped (`write_operations` DETAILS § "Retained failures"). Those
/// ids are long over, so acting on one is precisely the silent no-op this guard
/// exists to turn into a refusal. Typed status match, never a message test.
fn is_controllable(operations: &[OperationSnapshot], operation_id: &str) -> bool {
    operations
        .iter()
        .any(|op| op.operation_id == operation_id && op.status != LifecycleStatus::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::dismiss_failed_operation;
    use crate::file_system::write_operations::{WriteOperationError, WriteOperationType, test_retain_failure};

    fn row(operation_id: &str, status: LifecycleStatus) -> OperationSnapshot {
        OperationSnapshot {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            status,
            source: Some("/Users/me/photos".to_string()),
            destination: Some("Naspolya".to_string()),
            supports_rollback: false,
            error: None,
        }
    }

    /// A retained failure parked in the PROCESS-GLOBAL manager, dropped again on
    /// `Drop` so a failing assertion can't leave it behind for a sibling test
    /// (the reasoning behind `write_operations::test_support::TestOperationGuard`).
    struct RetainedFailureGuard(String);

    impl RetainedFailureGuard {
        fn record(operation_id: &str) -> Self {
            test_retain_failure(
                operation_id,
                WriteOperationType::Copy,
                &WriteOperationError::IoError {
                    path: "/Users/me/photos/a.raw".to_string(),
                    message: "disk went away".to_string(),
                },
            );
            Self(operation_id.to_string())
        }
    }

    impl Drop for RetainedFailureGuard {
        fn drop(&mut self) {
            dismiss_failed_operation(&self.0);
        }
    }

    #[test]
    fn live_operations_are_controllable() {
        let operations = vec![
            row("op-queued", LifecycleStatus::Queued),
            row("op-running", LifecycleStatus::Running),
            row("op-paused", LifecycleStatus::Paused),
        ];
        for id in ["op-queued", "op-running", "op-paused"] {
            assert!(is_controllable(&operations, id), "{id} should still be actionable");
        }
    }

    #[test]
    fn a_retained_failure_is_not_controllable() {
        // It's in the snapshot so the queue window can show why it stopped, but
        // pausing or cancelling it does nothing at all.
        let operations = vec![row("op-failed", LifecycleStatus::Failed)];
        assert!(!is_controllable(&operations, "op-failed"));
    }

    #[test]
    fn an_unknown_id_is_not_controllable() {
        assert!(!is_controllable(
            &[row("op-running", LifecycleStatus::Running)],
            "op-other"
        ));
    }

    #[test]
    fn the_guard_refuses_a_retained_failure_even_though_it_is_listed() {
        let id = format!("mcp-queue-guard-{}", std::process::id());
        let _guard = RetainedFailureGuard::record(&id);

        // The premise: the failure IS in `list_operations()`, so plain membership
        // would wave it through and the tool would answer "OK: Cancelled …" for an
        // operation that ended minutes ago.
        assert!(
            list_operations().iter().any(|op| op.operation_id == id),
            "the retained failure should be on the snapshot"
        );
        assert!(require_operation_exists(&id).is_err(), "the guard must refuse it");
    }
}
