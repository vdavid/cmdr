//! Pane and tab tool handlers. `quit` lives in `quit.rs`: it routes through the
//! quit gate and has a contract of its own.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_specta::Event as _;

use crate::window_events::ExecuteCommand;

use super::{
    AckSignal, DEFAULT_ACK_TIMEOUT, PaneStateStore, ROUND_TRIP_TIMEOUT_SECS, ToolError, ToolResult,
    mcp_round_trip_parsed, parse_mcp_response, snapshot_generation, wait_for_ack,
};

/// Execute switch_pane command.
pub fn execute_switch_pane<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    // Update the MCP store immediately so the state is correct when read back.
    // The frontend will also update via its own updateFocusedPane call, but that's async.
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let current = store.get_focused_pane();
        let new_pane = if current == "left" { "right" } else { "left" };
        store.set_focused_pane(new_pane.to_string());
    }
    ExecuteCommand {
        command_id: "pane.switch".to_string(),
    }
    .emit_to(app, "main")?;
    Ok(json!("OK: Switched focus to other pane"))
}

/// Execute swap_panes command.
pub fn execute_swap_panes<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    // Swap MCP pane state immediately so reads reflect the new layout
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let left = store.get_left();
        let right = store.get_right();
        store.set_left(right);
        store.set_right(left);
    }
    ExecuteCommand {
        command_id: "pane.swap".to_string(),
    }
    .emit_to(app, "main")?;
    Ok(json!("OK: Swapped left and right panes"))
}

/// Execute unified tab command.
///
/// Ack: pane generation advances after the FE pushes the new tab list via
/// `update_pane_tabs` (which bumps generation specifically for this case). `move` is the
/// exception: it can be refused, so it's a round-trip that names its outcome
/// (`execute_tab_move`).
pub async fn execute_tab<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'action' parameter"))?;
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }

    let tab_id = params.get("tabId").and_then(|v| v.as_str());

    // Resolve tab_id: required for activate, defaults to active tab for others
    let resolved_tab_id = match action {
        "activate" => tab_id
            .ok_or_else(|| ToolError::invalid_params("'tabId' is required for activate"))?
            .to_string(),
        "new" | "reopen" => String::new(), // not used
        _ => {
            // close, close_others, set_pinned: default to active tab
            if let Some(id) = tab_id {
                id.to_string()
            } else if let Some(store) = app.try_state::<PaneStateStore>() {
                let pane_state = match pane {
                    "left" => store.get_left(),
                    "right" => store.get_right(),
                    _ => unreachable!(),
                };
                pane_state
                    .tabs
                    .iter()
                    .find(|t| t.active)
                    .map(|t| t.id.clone())
                    .ok_or_else(|| ToolError::internal("No active tab found"))?
            } else {
                return Err(ToolError::internal("Pane state not available"));
            }
        }
    };

    // Validate tab_id exists (for actions that need it)
    if action != "new"
        && action != "reopen"
        && !resolved_tab_id.is_empty()
        && let Some(store) = app.try_state::<PaneStateStore>()
    {
        let pane_state = match pane {
            "left" => store.get_left(),
            "right" => store.get_right(),
            _ => unreachable!(),
        };
        if !pane_state.tabs.is_empty() && !pane_state.tabs.iter().any(|t| t.id == resolved_tab_id) {
            let available_ids: Vec<&str> = pane_state.tabs.iter().map(|t| t.id.as_str()).collect();
            return Err(ToolError::invalid_params(format!(
                "Tab '{}' not found in {} pane. Available tabs: {}",
                resolved_tab_id,
                pane,
                available_ids.join(", ")
            )));
        }
    }

    if action == "move" {
        return execute_tab_move(app, pane, &resolved_tab_id, params).await;
    }

    let pre_gen = snapshot_generation(app);
    let ok_msg = match action {
        "new" => {
            app.emit("mcp-tab", json!({"action": "new", "pane": pane}))?;
            format!("OK: Creating new tab in {} pane", pane)
        }
        "reopen" => {
            app.emit("mcp-tab", json!({"action": "reopen", "pane": pane}))?;
            format!("OK: Reopening last closed tab in {} pane", pane)
        }
        "close" => {
            app.emit(
                "mcp-tab",
                json!({"action": "close", "pane": pane, "tabId": resolved_tab_id}),
            )?;
            format!("OK: Closing tab {} in {} pane", resolved_tab_id, pane)
        }
        "close_others" => {
            app.emit(
                "mcp-tab",
                json!({"action": "close_others", "pane": pane, "tabId": resolved_tab_id}),
            )?;
            format!(
                "OK: Closing other tabs in {} pane (keeping {} and pinned)",
                pane, resolved_tab_id
            )
        }
        "activate" => {
            app.emit(
                "mcp-tab",
                json!({"action": "activate", "pane": pane, "tabId": resolved_tab_id}),
            )?;
            format!("OK: Switched to tab {} in {} pane", resolved_tab_id, pane)
        }
        "set_pinned" => {
            let pinned = params
                .get("pinned")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| ToolError::invalid_params("'pinned' parameter (boolean) is required for set_pinned"))?;
            let verb = if pinned { "Pinned" } else { "Unpinned" };
            app.emit(
                "mcp-tab",
                json!({"action": "set_pinned", "pane": pane, "tabId": resolved_tab_id, "pinned": pinned}),
            )?;
            format!("OK: {} tab {} in {} pane", verb, resolved_tab_id, pane)
        }
        _ => return Err(ToolError::invalid_params(format!("Unknown tab action: {}", action))),
    };

    wait_for_ack(
        app,
        AckSignal::GenerationAdvanced { from: pre_gen },
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;
    Ok(json!(ok_msg))
}

/// What the frontend says a `tab move` did.
///
/// The frontend owns the rules (one `moveTab`, shared with the mouse drag), so it names
/// the outcome and the backend words it. ❌ Never inferred from message text
/// (`error-string-match`): the discriminant is the contract, mirrored by `MoveTabResult`
/// in `apps/desktop/src/lib/file-explorer/tabs/tab-state-manager.svelte.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TabMoveAck {
    /// The tab moved. `to_index` is where it sits now, which is the end when the request
    /// named no index or one past it.
    Moved {
        to_index: u64,
    },
    /// The tab was already there.
    Unchanged,
    Refused(TabMoveRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TabMoveRefusal {
    /// A pinned tab doesn't move.
    Pinned,
    /// A pane keeps at least one tab, so its only tab can't leave it.
    OnlyTab,
    /// The pane the tab was headed for is at the per-pane tab limit.
    TargetFull,
    /// The tab closed between the backend's check and the move.
    NotFound,
}

impl TabMoveRefusal {
    /// The token an agent branches on, in the error's `data.reason`.
    fn reason(self) -> &'static str {
        match self {
            Self::Pinned => "tabPinned",
            Self::OnlyTab => "onlyTab",
            Self::TargetFull => "tabLimitReached",
            Self::NotFound => "tabNotFound",
        }
    }
}

/// Parse an `mcp-response` for a `tab move`, against the request ID we're waiting for.
///
/// Same `requestId` correlation as [`parse_mcp_response`]. A reply with no `outcome` is a
/// decline that happened before the move (no explorer mounted) and keeps its message; an
/// `ok` that names no outcome, or a `moved` with no index, is a failure, so a malformed
/// reply can never become a false OK.
pub(super) fn parse_tab_move_response(payload: &str, expected_id: &str) -> Option<Result<TabMoveAck, String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    Some(match resp.get("outcome").and_then(|v| v.as_str()) {
        Some("moved") => match resp.get("toIndex").and_then(|v| v.as_u64()) {
            Some(to_index) => Ok(TabMoveAck::Moved { to_index }),
            None => Err("The frontend reported a moved tab without its new index".to_string()),
        },
        Some("unchanged") => Ok(TabMoveAck::Unchanged),
        Some("pinned") => Ok(TabMoveAck::Refused(TabMoveRefusal::Pinned)),
        Some("onlyTab") => Ok(TabMoveAck::Refused(TabMoveRefusal::OnlyTab)),
        Some("targetFull") => Ok(TabMoveAck::Refused(TabMoveRefusal::TargetFull)),
        Some("notFound") => Ok(TabMoveAck::Refused(TabMoveRefusal::NotFound)),
        _ => match parse_mcp_response(payload, expected_id)? {
            Ok(()) => Err("The frontend's reply didn't say what the move did".to_string()),
            Err(err) => Err(err),
        },
    })
}

/// Word a `tab move`'s [`TabMoveAck`] for the agent. A refusal is an error carrying the
/// typed `data.reason`, and its sentence says what to do about it.
pub(super) fn tab_move_result(tab_id: &str, from_pane: &str, to_pane: &str, ack: TabMoveAck) -> ToolResult {
    let refusal = match ack {
        TabMoveAck::Moved { to_index } => {
            return Ok(json!(format!(
                "OK: Moved tab {tab_id} to index {to_index} in {to_pane} pane"
            )));
        }
        TabMoveAck::Unchanged => {
            return Ok(json!(format!(
                "OK: Tab {tab_id} is already at that position in {to_pane} pane. Nothing changed."
            )));
        }
        TabMoveAck::Refused(refusal) => refusal,
    };
    let message = match refusal {
        TabMoveRefusal::Pinned => format!(
            "Tab {tab_id} is pinned, and a pinned tab doesn't move. Unpin it first (set_pinned with pinned: false)."
        ),
        TabMoveRefusal::OnlyTab => format!(
            "Tab {tab_id} is the {from_pane} pane's only tab, and a pane keeps at least one. Open another tab there first (action: new)."
        ),
        TabMoveRefusal::TargetFull => {
            format!("The {to_pane} pane is at its tab limit. Close a tab there first, then move tab {tab_id}.")
        }
        TabMoveRefusal::NotFound => {
            format!("Tab {tab_id} is no longer in the {from_pane} pane. Read cmdr://state for the current tabs.")
        }
    };
    Err(ToolError::invalid_params(message).with_data(json!({ "reason": refusal.reason() })))
}

/// `tab move`: reorder a tab within its pane, or move it to the other pane.
///
/// A round-trip on `mcp-tab`, where the other actions wait on a generation ack: the
/// frontend applies the same rules as a tab drag and replies with what happened, after
/// pushing both panes' tab lists, so a `cmdr://state` read right after this returns is
/// current.
async fn execute_tab_move<R: Runtime>(app: &AppHandle<R>, pane: &str, tab_id: &str, params: &Value) -> ToolResult {
    let to_pane = match params.get("toPane") {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_str() {
            Some(side @ ("left" | "right")) => Some(side),
            _ => return Err(ToolError::invalid_params("toPane must be 'left' or 'right'")),
        },
    };
    let to_index = match params.get("toIndex") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .ok_or_else(|| ToolError::invalid_params("'toIndex' must be a non-negative integer"))?,
        ),
    };
    if to_pane.is_none() && to_index.is_none() {
        return Err(ToolError::invalid_params(
            "move needs 'toPane', 'toIndex', or both: say where the tab should go",
        ));
    }
    let to_pane = to_pane.unwrap_or(pane);

    let mut payload = json!({ "action": "move", "pane": pane, "tabId": tab_id, "toPane": to_pane });
    if let Some(index) = to_index {
        payload["toIndex"] = json!(index);
    }
    let ack = mcp_round_trip_parsed(
        app,
        "mcp-tab",
        payload,
        ROUND_TRIP_TIMEOUT_SECS,
        parse_tab_move_response,
    )
    .await?;
    tab_move_result(tab_id, pane, to_pane, ack)
}
