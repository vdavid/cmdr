//! Tool execution logic.
//!
//! Handles the execution of MCP tools and returns results.
//! All tools are designed to match user capabilities exactly.

mod app;
mod async_tools;
mod dialogs;
mod file_ops;
mod nav;
mod search;
mod view;

#[cfg(test)]
mod tests;

use std::sync::Mutex;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Listener, Runtime};

use super::pane_state::PaneStateStore;
use super::protocol::{INTERNAL_ERROR, INVALID_PARAMS};

/// Result of tool execution.
pub type ToolResult = Result<Value, ToolError>;

/// Error from tool execution.
#[derive(Debug)]
pub struct ToolError {
    pub code: i32,
    pub message: String,
}

impl ToolError {
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: INVALID_PARAMS,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            code: INTERNAL_ERROR,
            message: msg.into(),
        }
    }
}

impl From<tauri::Error> for ToolError {
    fn from(e: tauri::Error) -> Self {
        Self::internal(e.to_string())
    }
}

/// Emit an event to the frontend and wait for a response (5s timeout).
///
/// The frontend must emit `mcp-response` with `{ requestId, ok, error? }`.
/// Returns `success_msg` on success, or the frontend's error message on failure.
async fn mcp_round_trip<R: Runtime>(
    app: &AppHandle<R>,
    event: &str,
    payload: Value,
    success_msg: String,
) -> ToolResult {
    mcp_round_trip_with_timeout(app, event, payload, success_msg, 5).await
}

/// Like `mcp_round_trip` but with a configurable timeout.
async fn mcp_round_trip_with_timeout<R: Runtime>(
    app: &AppHandle<R>,
    event: &str,
    mut payload: Value,
    success_msg: String,
    timeout_secs: u64,
) -> ToolResult {
    let request_id = uuid::Uuid::new_v4().to_string();
    payload["requestId"] = json!(request_id);

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let expected_id = request_id.clone();

    // Use a Mutex to allow the closure to consume tx exactly once
    let tx = Mutex::new(Some(tx));
    let listener_id = app.listen("mcp-response", move |event| {
        if let Ok(resp) = serde_json::from_str::<Value>(event.payload())
            && resp.get("requestId").and_then(|v| v.as_str()) == Some(&expected_id)
            && let Some(tx) = tx.lock().unwrap().take()
        {
            let result = if resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                Ok(())
            } else {
                let err = resp
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                Err(err)
            };
            let _ = tx.send(result);
        }
    });

    app.emit(event, payload)?;

    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await;
    app.unlisten(listener_id);

    match result {
        Ok(Ok(Ok(()))) => Ok(json!(success_msg)),
        Ok(Ok(Err(err))) => Err(ToolError::internal(err)),
        Ok(Err(_)) => Err(ToolError::internal("Frontend response channel dropped")),
        Err(_) => Err(ToolError::internal(format!(
            "Frontend did not respond within {timeout_secs} seconds"
        ))),
    }
}

/// Execute a tool by name.
pub async fn execute_tool<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        // App commands
        "quit" => app::execute_quit(app),
        "switch_pane" => app::execute_switch_pane(app),
        "swap_panes" => app::execute_swap_panes(app),
        // View commands
        "toggle_hidden" => view::execute_toggle_hidden(app),
        "set_view_mode" => view::execute_set_view_mode(app, params),
        "sort" => view::execute_sort(app, params),
        // Navigation commands (no params)
        "open_under_cursor" | "nav_to_parent" | "nav_back" | "nav_forward" => nav::execute_nav_command(app, name),
        // Navigation commands (with params)
        "select_volume" | "nav_to_path" | "move_cursor" | "scroll_to" => {
            nav::execute_nav_command_with_params(app, name, params).await
        }
        // Tab commands
        "tab" => app::execute_tab(app, params),
        // File operation commands
        "copy" => file_ops::execute_copy(app, params),
        "move" => file_ops::execute_move(app, params),
        "delete" => file_ops::execute_delete(app, params),
        "mkdir" => file_ops::execute_mkdir(app),
        "mkfile" => file_ops::execute_mkfile(app),
        "refresh" => file_ops::execute_refresh(app),
        // Selection command
        "select" => file_ops::execute_select_command(app, params),
        // Dialog command
        "dialog" => dialogs::execute_dialog_command(app, params),
        // Search commands
        "search" => search::execute_search(params).await,
        "ai_search" => search::execute_ai_search(params).await,
        // Settings commands
        "set_setting" => async_tools::execute_set_setting(app, params).await,
        // Network commands
        "connect_to_server" => async_tools::execute_connect_to_server(app, params).await,
        "remove_manual_server" => async_tools::execute_remove_manual_server(app, params),
        // Async wait
        "await" => async_tools::execute_await(app, params).await,
        _ => Err(ToolError::invalid_params(format!("Unknown tool: {name}"))),
    }
}
