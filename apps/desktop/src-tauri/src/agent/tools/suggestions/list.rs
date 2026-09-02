//! The `list_suggestions` tool: what the agent has already put in front of the user, as
//! summaries.
//!
//! `Access::Read`. It answers with sweeps and the groups inside them, each carrying a
//! `COUNT(*)` of its ops. ❌ It never returns individual ops: a group of 60 000 is
//! legitimate, and a tool that listed their paths to say "there are 60 000" would spend the
//! whole turn's budget on the answer. `get_suggestion_group` pages the ops.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use crate::agent::AgentDb;
use crate::agent::store::proposals::{GroupSummary, ProposalSweep, get_sweep, list_groups};
use crate::agent::types::{ProposalStatus, ProposalVerb, Reversibility};
use crate::mcp::{ToolError, ToolResult};
use crate::search::format_timestamp;

/// Which groups to list. Defaults to `pending` — what's waiting on the user is the question
/// nearly every call is asking.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum StatusFilter {
    #[default]
    Pending,
    Approved,
    Interrupted,
    Completed,
    Rejected,
    /// Every status, for "what have you suggested lately?".
    All,
}

impl StatusFilter {
    fn status(self) -> Option<ProposalStatus> {
        match self {
            StatusFilter::Pending => Some(ProposalStatus::Pending),
            StatusFilter::Approved => Some(ProposalStatus::Approved),
            StatusFilter::Interrupted => Some(ProposalStatus::Interrupted),
            StatusFilter::Completed => Some(ProposalStatus::Completed),
            StatusFilter::Rejected => Some(ProposalStatus::Rejected),
            StatusFilter::All => None,
        }
    }

    fn token(self) -> &'static str {
        match self.status() {
            Some(status) => status.as_token(),
            None => "all",
        }
    }
}

/// One group, as a summary. Counts only: the ops themselves are a page away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GroupSummaryOut {
    pub group_id: i64,
    /// The sweep this group belongs to. Amending it later takes both ids, so the pair
    /// travels together wherever a group is read.
    pub sweep_id: i64,
    pub verb: ProposalVerb,
    pub status: ProposalStatus,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub source_volume_id: String,
    /// The shared destination folder, the rename parent, or the archive path. Absent for
    /// trash and delete, which bind no target at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// How far this group could be taken back if the user approves it: a fact the review
    /// dialog discloses, never a reason anything refuses the group.
    pub reversible: Reversibility,
    /// Ops in the live set. This is what would run.
    pub op_count: u64,
    /// Ops the user deselected. Their rows stay, so the record says what was offered.
    #[serde(skip_serializing_if = "is_zero")]
    pub excluded_op_count: u64,
    /// True when a pattern produced this group. Its `displayName` IS that pattern.
    #[serde(skip_serializing_if = "crate::agent::tools::read::is_false")]
    pub from_selector: bool,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

/// One sweep: one agent wake's output, with the groups of it that matched the filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SweepOut {
    pub sweep_id: i64,
    pub created_at: i64,
    /// The date spelled out, because the model can't turn an epoch into one reliably.
    pub created_at_human: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub groups: Vec<GroupSummaryOut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListSuggestionsResult {
    /// The filter that was applied, echoed so the model states what it looked at.
    pub status: String,
    pub sweeps: Vec<SweepOut>,
    /// Groups matching the filter, in total.
    pub total: usize,
    /// Groups in this answer.
    pub returned: usize,
    /// True when the answer left groups out. Say "returned of total" when it is.
    pub truncated: bool,
}

pub fn list_suggestions_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "enum": ["pending", "approved", "interrupted", "completed", "rejected", "all"],
                "description": "Default pending: still waiting on the user. interrupted: the app restarted mid-execution, so the user decides again."
            }
        },
        "additionalProperties": false
    })
}

pub async fn execute_list_suggestions<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let filter: StatusFilter = match params.get("status") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|_| {
            ToolError::invalid_params("status must be pending, approved, interrupted, completed, rejected, or all.")
        })?,
        None => StatusFilter::default(),
    };
    let db_path = app
        .try_state::<AgentDb>()
        .ok_or_else(|| ToolError::internal("Cmdr's suggestion store isn't open yet."))?
        .db_path()
        .to_path_buf();

    let (fitted, sweeps) = tokio::task::spawn_blocking(move || {
        let conn = crate::agent::store::open_read_connection(&db_path)?;
        let groups: Vec<GroupSummaryOut> = list_groups(&conn, filter.status())?
            .into_iter()
            .map(to_group_summary)
            .collect();
        // Cut to what one tool result may carry BEFORE reading sweeps, so a long backlog
        // costs a handful of header reads rather than one per group.
        let fitted = crate::mcp::fit_to_result_budget(groups);
        let mut sweeps = Vec::new();
        for set_id in distinct_set_ids(&fitted.items) {
            if let Some(sweep) = get_sweep(&conn, set_id)? {
                sweeps.push(sweep);
            }
        }
        Ok::<_, crate::agent::store::AgentStoreError>((fitted, sweeps))
    })
    .await
    .map_err(|e| ToolError::internal(e.to_string()))?
    .map_err(|e| ToolError::internal(e.to_string()))?;

    let result = shape_list(filter.token(), fitted.items, fitted.total, fitted.truncated, &sweeps);
    serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
}

/// The set ids of a group page, in the order the groups appear (newest sweep first).
fn distinct_set_ids(groups: &[GroupSummaryOut]) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    for group in groups {
        if !ids.contains(&group.sweep_id) {
            ids.push(group.sweep_id);
        }
    }
    ids
}

/// Nest the group summaries under their sweeps, keeping the order they arrived in (newest
/// first). Pure, so the shape is testable without a database.
///
/// A group whose sweep couldn't be read is dropped rather than shown parentless: the sweep
/// is what dates a suggestion, and an undated one reads as new.
pub(super) fn shape_list(
    status: &str,
    groups: Vec<GroupSummaryOut>,
    total: usize,
    truncated: bool,
    sweeps: &[ProposalSweep],
) -> ListSuggestionsResult {
    let mut out: Vec<SweepOut> = Vec::new();
    let mut returned = 0;
    for group in groups {
        let set_id = group.sweep_id;
        let Some(sweep) = sweeps.iter().find(|sweep| sweep.id == set_id) else {
            continue;
        };
        returned += 1;
        match out.iter_mut().find(|existing| existing.sweep_id == set_id) {
            Some(existing) => existing.groups.push(group),
            None => out.push(SweepOut {
                sweep_id: sweep.id,
                created_at: sweep.created_at,
                created_at_human: format_timestamp(sweep.created_at.max(0) as u64),
                rationale: sweep.rationale.clone(),
                groups: vec![group],
            }),
        }
    }
    ListSuggestionsResult {
        status: status.to_string(),
        sweeps: out,
        total,
        returned,
        truncated,
    }
}

/// One stored group header plus its counts, as the model reads it.
pub(super) fn to_group_summary(summary: GroupSummary) -> GroupSummaryOut {
    let GroupSummary {
        group,
        live_op_count,
        total_op_count,
    } = summary;
    GroupSummaryOut {
        group_id: group.id,
        sweep_id: group.set_id,
        verb: group.verb,
        status: group.status,
        display_name: group.display_name,
        rationale: group.rationale,
        source_volume_id: group.source_volume_id,
        destination: group.destination,
        reversible: group.reversible,
        op_count: live_op_count,
        excluded_op_count: total_op_count.saturating_sub(live_op_count),
        from_selector: group.selector.is_some(),
    }
}
