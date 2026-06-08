//! App and tab tool handlers.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_specta::Event as _;

use crate::window_events::ExecuteCommand;

use super::{AckSignal, DEFAULT_ACK_TIMEOUT, PaneStateStore, ToolError, ToolResult, snapshot_generation, wait_for_ack};

/// Execute quit command.
pub fn execute_quit<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.exit(0);
    Ok(json!("OK: Quitting application"))
}

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
/// `update_pane_tabs` (which bumps generation specifically for this case).
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
