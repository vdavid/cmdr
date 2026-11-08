//! Operation-log tool handlers: read the durable journal, and open the rollback question for
//! a person to answer.
//!
//! `operations_list` / `operations_get` are pure reads over a short-lived
//! read-only connection (the same pattern as `commands/operation_log.rs`: the
//! writer thread owns the one write connection, reads never contend under WAL).
//! `operations_rollback` opens `RollbackConfirmDialog` and answers that a person decides
//! (`tools/actions/DETAILS.md` § `operations_rollback`).
//!
//! Every classification param (`kind`, `initiator`, `executionStatus`,
//! `rollbackState`) and every field of the results crosses the wire as the typed
//! `operation_log::types` enums in their camelCase serde form, never a hand-parsed
//! string: the enum params deserialize with serde, so input
//! tokens match the output the results already serialize.

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};

use super::dialog_confirm::ROLLBACK_CONFIRMATION;
use super::file_ops::open_dialog;
use super::{ToolError, ToolResult};
use crate::operations_host::AppOperationVolumes;
use crate::tools::policy::ToolCtx;
use cmdr_operation_log::query::{self, NameFilter, NameMatch, OperationSearchFilters};
use cmdr_operation_log::rollback::{RollbackRefusal, VolumeLookup, check_rollbackable};
use cmdr_operation_log::store::{OperationLogStoreError, OperationRow, open_read_connection, operation_log_db_path};
use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind, RollbackState};
use cmdr_operations::{LifecycleStatus, OperationSnapshot};

/// Default op page for `operations_list`.
const DEFAULT_LIST_LIMIT: u32 = 50;
/// Default item page for `operations_get`.
const DEFAULT_ITEM_LIMIT: u32 = 200;
/// Upper bound on any page size, so a huge `limit` can't materialize an unbounded
/// result off one MCP call.
const MAX_LIMIT: u32 = 1000;

/// List operations from the journal, filtered and paged, newest first.
///
/// A bare (unfiltered) call reads the recent-operations feed (ordered by start
/// time, so a still-running op sorts first); any filter routes through the search
/// query (ordered by end time). Reading history is not destructive.
pub async fn execute_operations_list<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let (filters, limit, offset) = parse_list_filters(params)?;
    let use_recent = filters_are_empty(&filters);
    let rows = with_read_connection(app, Vec::new(), move |conn| {
        if use_recent {
            query::recent_operations(conn, limit, offset)
        } else {
            query::search_operations(conn, &filters, limit, offset)
        }
    })
    .await?;
    let rows = rows
        .iter()
        .map(operation_as_a_tool_says_it)
        .collect::<Result<Vec<_>, _>>()?;
    // Cut the page to what one tool result may carry (a 1000-row journal page dwarfs an
    // agent's prompt budget) and report both numbers, so the caller can page with `offset`
    // instead of losing rows silently.
    let fitted = super::fit_to_result_budget(rows);
    let (total, returned, truncated) = (fitted.total, fitted.items.len(), fitted.truncated);
    Ok(json!({
        "operations": fitted.items,
        "count": returned,
        "total": total,
        "returned": returned,
        "truncated": truncated,
    }))
}

/// An operation as a tool answers it: `operationId`, the name `operations_get` and
/// `operations_rollback` take, since models handed `opId` guessed wrong. The journal's own row keeps
/// `opId`, which the operation log dialog reads over IPC.
fn operation_as_a_tool_says_it(row: &OperationRow) -> Result<Value, ToolError> {
    let mut value = serde_json::to_value(row).map_err(|e| ToolError::internal(e.to_string()))?;
    if let Some(fields) = value.as_object_mut()
        && let Some(id) = fields.remove("opId")
    {
        fields.insert("operationId".to_string(), id);
    }
    Ok(value)
}

/// One operation's header plus a page of its item rows (full paths, per-item
/// outcome). The `rollbackState` field here is what a rollback poll loop watches.
pub async fn execute_operations_get<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let op_id = required_operation_id(params)?;
    let limit = parse_limit(params, "limit", DEFAULT_ITEM_LIMIT)?;
    let offset = parse_offset(params)?;
    let op_id_for_read = op_id.clone();
    let detail = with_read_connection(app, None, move |conn| {
        query::get_operation(conn, &op_id_for_read, limit, offset)
    })
    .await?;
    match detail {
        Some(mut detail) => {
            // Item rows carry full source + destination paths, so a 200-item page can
            // outgrow one tool result. Cut by size and report it; `totalItems` stays the
            // honest denominator and `offset` fetches the rest.
            let total = detail.total_items as usize;
            let fitted = super::fit_to_result_budget(std::mem::take(&mut detail.items));
            let returned = fitted.items.len();
            detail.items = fitted.items;
            let mut value = serde_json::to_value(&detail).map_err(|e| ToolError::internal(e.to_string()))?;
            value["operation"] = operation_as_a_tool_says_it(&detail.operation)?;
            value["total"] = total.into();
            value["returned"] = returned.into();
            value["truncated"] = (returned < total).into();
            Ok(value)
        }
        None => Err(ToolError::invalid_params(format!("No operation found with id {op_id}"))),
    }
}

/// Open the rollback question for an operation, for a person to answer.
///
/// Whether it CAN roll back is checked here, so a refusal raises no dialog. What it says and
/// whether it's the right moment are the frontend's: `mcp-rollback-dialog` carries the facts
/// its wording is picked from, and its answer says the question is up, or what's in the way
/// (`#lib/file-operations/tool-rollback.svelte.ts`). The answer is typed
/// `{ awaitingPerson, dialog }`: nothing has been reversed.
///
/// Confirming runs the path a person's press runs, and `dialog_confirm` is the only way past
/// them (`dialog_confirm.rs`).
pub async fn execute_operations_rollback<R: Runtime>(app: &AppHandle<R>, ctx: &ToolCtx, params: &Value) -> ToolResult {
    let op_id = required_operation_id(params)?;
    let engine = crate::operations_host::operations()
        .map_err(|_| ToolError::internal("the app isn't wired up to report operation progress yet"))?;

    let snapshot = engine.list_operations();
    let live = snapshot
        .iter()
        .find(|op| op.operation_id == op_id && op.status != LifecycleStatus::Failed);
    let question = if let Some(op) = live {
        live_question(op)?
    } else {
        let id = op_id.clone();
        let row = with_read_connection(app, None, move |conn| query::get_operation(conn, &id, 0, 0))
            .await?
            .map(|detail| detail.operation);
        logged_question(row, &AppOperationVolumes)?
    };
    open_rollback_question(app, ctx, &op_id, question).await
}

/// Asks the frontend to raise the question, and waits until it's on screen.
async fn open_rollback_question<R: Runtime>(
    app: &AppHandle<R>,
    ctx: &ToolCtx,
    op_id: &str,
    question: RollbackQuestion,
) -> ToolResult {
    let mut payload = serde_json::to_value(&question).map_err(|e| ToolError::internal(e.to_string()))?;
    payload["operationId"] = op_id.into();
    open_dialog(
        app,
        "roll back",
        "mcp-rollback-dialog",
        ctx.stamp(payload),
        ROLLBACK_CONFIRMATION,
    )
    .await
}

// ── Pure decision cores (unit-tested) ─────────────────────────────────────────

/// The facts the frontend words a rollback question from. They're facts rather than a chosen
/// variant so the words stay in `reversal-wording.ts`, the one place that picks them.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "target", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum RollbackQuestion {
    /// Still running: confirming takes the existing cancel-with-rollback path.
    Live,
    /// Over: confirming dispatches the rollback engine.
    Logged {
        kind: OpKind,
        /// It's already partly rolled back, so a yes picks that up.
        rest: bool,
    },
}

/// What to ask about a running operation. Its strategy decides here, and whether now's the moment
/// is the frontend's call, on the phase.
fn live_question(op: &OperationSnapshot) -> Result<RollbackQuestion, ToolError> {
    if op.can_reverse() {
        Ok(RollbackQuestion::Live)
    } else {
        Err(live_refusal(op))
    }
}

/// What to ask about a finished operation, through the gate its dispatch will run.
fn logged_question(row: Option<OperationRow>, volumes: &dyn VolumeLookup) -> Result<RollbackQuestion, ToolError> {
    let row = row.ok_or_else(|| refusal_error(&RollbackRefusal::UnknownOperation))?;
    check_rollbackable(volumes, &row).map_err(|refusal| refusal_error(&refusal))?;
    Ok(RollbackQuestion::Logged {
        kind: row.kind,
        rest: row.rollback_state == RollbackState::PartiallyRolledBack,
    })
}

/// A running operation whose strategy can't reverse. `data.refusal` is what a caller branches on.
fn live_refusal(op: &OperationSnapshot) -> ToolError {
    ToolError::invalid_params(format!(
        "Operation {} can't roll back while it runs ({:?}), so no dialog opened. queue cancel stops it and keeps what it wrote.",
        op.operation_id, op.rollback_support
    ))
    .with_data(json!({ "refusal": "cantReverse", "detail": op.rollback_support }))
}

/// The journal's typed refusal as a tool error: `data.refusal` is its `kind`, `data.detail` what
/// it carries (the stored reason, the missing volume). ❌ Never only in the sentence.
fn refusal_error(refusal: &RollbackRefusal) -> ToolError {
    let message = match refusal {
        RollbackRefusal::UnknownOperation => "No operation has that id, so no dialog opened.".to_string(),
        RollbackRefusal::AlreadyRollingBack => "It's already rolling back, so no dialog opened.".to_string(),
        RollbackRefusal::AlreadyRolledBack => "It's already rolled back, so no dialog opened.".to_string(),
        RollbackRefusal::IsARollback => {
            "It's a rollback of another operation, and a rollback can't be rolled back, so no dialog opened."
                .to_string()
        }
        RollbackRefusal::NotRollbackable(reason) => {
            format!("This operation can't be rolled back ({reason:?}), so no dialog opened.")
        }
        RollbackRefusal::VolumeUnavailable { volume_id } => {
            format!("A volume it touched isn't connected ({volume_id}), so no dialog opened.")
        }
        // A tool's rollback always takes the whole operation, so this only words the
        // variant for completeness.
        RollbackRefusal::Selection(reason) => {
            format!("The item selection can't run ({reason:?}), so no dialog opened.")
        }
    };
    let wire = serde_json::to_value(refusal).unwrap_or(Value::Null);
    let mut data = json!({ "refusal": wire.get("kind").cloned().unwrap_or(Value::Null) });
    if let Some(detail) = wire.get("detail") {
        data["detail"] = detail.clone();
    }
    ToolError::invalid_params(message).with_data(data)
}

/// Parse `operations_list`'s filters + paging into the query API's shape, or a
/// typed `invalid_params` on a malformed value.
fn parse_list_filters(params: &Value) -> Result<(OperationSearchFilters, u32, u32), ToolError> {
    let filters = OperationSearchFilters {
        since: params.get("since").and_then(|v| v.as_i64()),
        until: params.get("until").and_then(|v| v.as_i64()),
        name: parse_name_filter(params)?,
        // The tool exposes a single `kind`; the query takes a set (empty ⇒ any).
        kinds: parse_enum_param::<OpKind>(params, "kind")?.into_iter().collect(),
        initiator: parse_enum_param::<Initiator>(params, "initiator")?,
        execution_status: parse_enum_param::<ExecutionStatus>(params, "executionStatus")?,
        rollback_state: parse_enum_param::<RollbackState>(params, "rollbackState")?,
    };
    let limit = parse_limit(params, "limit", DEFAULT_LIST_LIMIT)?;
    let offset = parse_offset(params)?;
    Ok((filters, limit, offset))
}

/// Whether no filter is set — the bare-list case that reads the recent feed.
fn filters_are_empty(f: &OperationSearchFilters) -> bool {
    f.since.is_none()
        && f.until.is_none()
        && f.name.is_none()
        && f.kinds.is_empty()
        && f.initiator.is_none()
        && f.execution_status.is_none()
        && f.rollback_state.is_none()
}

/// A `name` filter, defaulting to prefix match. The name is an exact/prefix match
/// on the folded item name (index-served), NOT a substring search — the schema
/// description says so, since a `contains` scan isn't index-backed.
fn parse_name_filter(params: &Value) -> Result<Option<NameFilter>, ToolError> {
    let Some(text) = params.get("name").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    if text.is_empty() {
        return Ok(None);
    }
    let match_kind = match params.get("nameMatch").and_then(|v| v.as_str()) {
        None | Some("prefix") => NameMatch::Prefix,
        Some("exact") => NameMatch::Exact,
        Some(_) => return Err(ToolError::invalid_params("nameMatch must be 'exact' or 'prefix'")),
    };
    Ok(Some(NameFilter {
        text: text.to_string(),
        match_kind,
    }))
}

/// Deserialize an optional typed-enum param from its camelCase wire token. An
/// absent/null value ⇒ `None`; an unrecognized token ⇒ typed `invalid_params`
/// (the schema `enum` already constrains well-behaved callers).
fn parse_enum_param<T: serde::de::DeserializeOwned>(params: &Value, key: &str) -> Result<Option<T>, ToolError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value::<T>(value.clone())
            .map(Some)
            .map_err(|_| ToolError::invalid_params(format!("Invalid value for '{key}'"))),
    }
}

/// A page limit, clamped to [`MAX_LIMIT`]; absent ⇒ `default`.
fn parse_limit(params: &Value, key: &str, default: u32) -> Result<u32, ToolError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => {
            let n = value
                .as_u64()
                .ok_or_else(|| ToolError::invalid_params(format!("'{key}' must be a non-negative integer")))?;
            Ok((n.min(MAX_LIMIT as u64)) as u32)
        }
    }
}

/// A paging offset; absent ⇒ 0.
fn parse_offset(params: &Value) -> Result<u32, ToolError> {
    match params.get("offset") {
        None | Some(Value::Null) => Ok(0),
        Some(value) => {
            let n = value
                .as_u64()
                .ok_or_else(|| ToolError::invalid_params("'offset' must be a non-negative integer"))?;
            Ok(n.min(u32::MAX as u64) as u32)
        }
    }
}

/// The required `operationId` param.
fn required_operation_id(params: &Value) -> Result<String, ToolError> {
    params
        .get("operationId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::invalid_params("Missing 'operationId' parameter"))
}

/// Resolve `operation-log.db` and run `read` on a short-lived read-only
/// connection, off the MCP task. A missing DB (the journal never opened) yields
/// `empty` so the tool degrades to "no history" rather than erroring.
async fn with_read_connection<R, T, F>(app: &AppHandle<R>, empty: T, read: F) -> Result<T, ToolError>
where
    R: Runtime,
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T, OperationLogStoreError> + Send + 'static,
{
    let app = app.clone();
    tokio::task::spawn_blocking(move || {
        let data_dir = crate::config::resolved_app_data_dir(&app).map_err(ToolError::internal)?;
        let db_path = operation_log_db_path(&data_dir);
        if !db_path.exists() {
            return Ok(empty);
        }
        let conn = open_read_connection(&db_path).map_err(|e| ToolError::internal(e.to_string()))?;
        read(&conn).map_err(|e| ToolError::internal(e.to_string()))
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?
}

#[cfg(test)]
mod tests;
