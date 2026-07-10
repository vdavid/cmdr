//! Operation-log tool handlers (the MCP tools): read the durable journal and dispatch a
//! rollback, so an agent can test the whole feature end to end without the FE.
//!
//! `operations_list` / `operations_get` are pure reads over a short-lived
//! read-only connection (the same pattern as `commands/operation_log.rs`: the
//! writer thread owns the one write connection, reads never contend under WAL).
//! `operations_rollback` dispatches the rollback engine and returns after
//! DISPATCH, not completion — the reversal is an async managed op, so the caller
//! polls `operations_get` until the operation's `rollbackState` leaves
//! `rollingBack` (the "dispatch then poll" contract, `mcp/DETAILS.md`).
//!
//! Every classification param (`kind`, `initiator`, `executionStatus`,
//! `rollbackState`) and every field of the results crosses the wire as the typed
//! `operation_log::types` enums in their camelCase serde form, never a hand-parsed
//! string (`no-string-matching`): the enum params deserialize with serde, so input
//! tokens match the output the results already serialize.

use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};

use super::{ToolError, ToolResult};
use crate::operation_log::query::{self, NameFilter, NameMatch, OperationSearchFilters};
use crate::operation_log::store::{OperationLogStoreError, open_read_connection, operation_log_db_path};
use crate::operation_log::types::{ExecutionStatus, Initiator, OpKind, RollbackState};

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
/// query (ordered by end time). Gate `Open`: reading history is not destructive.
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
    let count = rows.len();
    let operations = serde_json::to_value(&rows).map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!({ "operations": operations, "count": count }))
}

/// One operation's header plus a page of its item rows (full paths, per-item
/// outcome). The `rollbackState` field here is what a rollback poll loop watches.
/// Gate `Open`.
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
        Some(detail) => serde_json::to_value(&detail).map_err(|e| ToolError::internal(e.to_string())),
        None => Err(ToolError::invalid_params(format!("No operation found with id {op_id}"))),
    }
}

/// Reverse a logged operation through the rollback engine.
///
/// Requires `autoConfirm: true`, which the `IfAutoConfirm` gate ties to the bearer
/// token — the same threat model as copy/move/delete: a rollback writes to the
/// filesystem, so it must never run unconfirmed. Without a confirmation dialog
/// (an alpha-UI surface), the only safe path over MCP is the token-gated bypass, so a
/// call missing `autoConfirm` is refused rather than acting unconfirmed.
///
/// Returns after DISPATCH: the inverse operation spawns as an async managed op, so
/// the caller polls `operations_get` until this operation's `rollbackState` leaves
/// `rollingBack`. A domain refusal (unknown / already rolling back / not
/// rollbackable / a volume disconnected) comes back as a typed `refusal` body, not
/// a message the caller must parse.
pub async fn execute_operations_rollback<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let op_id = required_operation_id(params)?;
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    if !auto_confirm {
        return Err(ToolError::invalid_params(
            "operations_rollback needs autoConfirm: true. A rollback writes to disk, so \
             (like copy/move/delete) it requires the bearer token; interactive confirmation \
             over MCP isn't available yet.",
        ));
    }

    // `dispatch_rollback` opens a read connection and gates synchronously before
    // spawning the async inverse, so run it off the MCP task like every other
    // blocking DB touch. The token was already validated by the `IfAutoConfirm`
    // gate in `server.rs` before dispatch reached here.
    let app = app.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        crate::file_system::write_operations::rollback::dispatch_rollback(&app, &op_id, Initiator::AiClient)
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?;

    match outcome {
        Ok(dispatch) => Ok(json!({
            "status": "dispatched",
            "inverseOpId": dispatch.inverse_op_id,
            "note": "Reversal runs asynchronously as a managed operation. Poll operations_get on \
                     this operationId until rollbackState leaves 'rollingBack' (settling to \
                     'rolledBack' or 'partiallyRolledBack')."
        })),
        Err(refusal) => {
            let refusal = serde_json::to_value(&refusal).map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!({ "status": "refused", "refusal": refusal }))
        }
    }
}

// ── Pure decision cores (unit-tested) ─────────────────────────────────────────

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
/// description says so, since a `contains` scan isn't index-backed (D8).
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
