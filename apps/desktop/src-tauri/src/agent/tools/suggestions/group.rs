//! The `get_suggestion_group` tool: one group's ops, a page at a time.
//!
//! `Access::Read`. The group header comes from one row plus two `COUNT(*)`s, and the ops
//! come from the store's single paged reader, cut again to what one tool result may carry.
//! Both numbers cross the wire (`total` / `returned` / `truncated`) so the model can say
//! what it actually looked at and page for the rest with `offset`.
//!
//! Every per-op number is the CREATION SNAPSHOT: what the index knew when the group was
//! frozen, not what the file is now. The fields say `snapshot` for exactly that reason — a
//! size relayed as current would be a claim nothing here can back.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use super::list::{GroupSummaryOut, to_group_summary};
use crate::agent::AgentDb;
use crate::agent::store::proposals::{GroupSummary, ProposalOp, count_ops, get_group, page_ops};
use crate::agent::types::OpStatus;
use crate::mcp::{ToolError, ToolResult};
use crate::search::{format_size, format_timestamp};

/// The default page. Generous enough that a normal group arrives whole, and the size cut
/// takes over when the paths are long.
const DEFAULT_LIMIT: u32 = 100;
/// The most rows one call may ask for, before the size cut narrows it further.
const MAX_LIMIT: u32 = 500;

/// One proposed op, as the model reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OpOut {
    pub op_id: i64,
    pub path: String,
    /// The name this source becomes. Present only under a rename group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,
    pub status: OpStatus,
    /// The size the index held when this was proposed. Absent when the index had none —
    /// never a zero, which would read as an empty file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_size_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_modified: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_modified_human: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GroupDetail {
    pub found: bool,
    pub group: GroupSummaryOut,
    pub ops: Vec<OpOut>,
    /// Where this page started.
    pub offset: u32,
    /// Every op row the group has, whatever its status.
    pub total: usize,
    pub returned: usize,
    /// True when the page left rows out. Say "returned of total" when it is.
    pub truncated: bool,
}

pub fn get_suggestion_group_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "groupId": { "type": "integer", "description": "The group id from list_suggestions." },
            "offset": { "type": "integer", "description": "Ops to skip; resume with offset + returned." },
            "limit": { "type": "integer", "description": "Default 100, max 500; a page may come back shorter when paths are long." }
        },
        "required": ["groupId"],
        "additionalProperties": false
    })
}

pub async fn execute_get_suggestion_group<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let group_id = params
        .get("groupId")
        .and_then(Value::as_i64)
        .ok_or_else(|| ToolError::invalid_params("Give the groupId to look at."))?;
    let limit = bounded(params, "limit", DEFAULT_LIMIT)?.min(MAX_LIMIT);
    let offset = bounded(params, "offset", 0)?;
    let db_path = app
        .try_state::<AgentDb>()
        .ok_or_else(|| ToolError::internal("Cmdr's suggestion store isn't open yet."))?
        .db_path()
        .to_path_buf();

    let read = tokio::task::spawn_blocking(move || {
        let conn = crate::agent::store::open_read_connection(&db_path)?;
        let Some(group) = get_group(&conn, group_id)? else {
            return Ok::<_, crate::agent::store::AgentStoreError>(None);
        };
        let summary = GroupSummary {
            group,
            live_op_count: count_ops(&conn, group_id, Some(OpStatus::Pending))?,
            total_op_count: count_ops(&conn, group_id, None)?,
        };
        let ops = page_ops(&conn, group_id, limit, offset)?;
        Ok(Some((summary, ops)))
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?
    .map_err(|e| ToolError::internal(e.to_string()))?;

    match read {
        Some((summary, ops)) => {
            let detail = shape_group(summary, ops, offset);
            serde_json::to_value(&detail).map_err(|e| ToolError::internal(e.to_string()))
        }
        // A typed "no such group" rather than an error: the id may simply be one the user
        // already dealt with, and that's an answer, not a fault.
        None => Ok(serde_json::json!({ "found": false, "groupId": group_id })),
    }
}

fn bounded(params: &Value, key: &str, default: u32) -> Result<u32, ToolError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| ToolError::invalid_params(format!("{key} must be a whole number, zero or more."))),
    }
}

/// Shape a header plus one page of ops, cutting the page to the result budget. Pure, so the
/// counts and the snapshot honesty are testable without a database.
pub(super) fn shape_group(summary: GroupSummary, ops: Vec<ProposalOp>, offset: u32) -> GroupDetail {
    let total = summary.total_op_count as usize;
    let group = to_group_summary(summary);
    let fitted = crate::mcp::fit_to_result_budget(ops.into_iter().map(to_op).collect::<Vec<_>>());
    let returned = fitted.items.len();
    GroupDetail {
        found: true,
        group,
        ops: fitted.items,
        offset,
        total,
        returned,
        // Two ways a page falls short of the whole group: the size cut took rows off this
        // page, or the page itself doesn't reach the end. Both mean "there's more".
        truncated: fitted.truncated || (offset as usize + returned) < total,
    }
}

fn to_op(op: ProposalOp) -> OpOut {
    OpOut {
        op_id: op.id,
        path: op.source_path,
        new_name: op.destination,
        status: op.status,
        snapshot_size: op.snapshot_size,
        snapshot_size_human: op.snapshot_size.map(format_size),
        snapshot_modified: op.snapshot_mtime,
        snapshot_modified_human: op.snapshot_mtime.map(|at| format_timestamp(at.max(0) as u64)),
    }
}
