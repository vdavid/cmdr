//! Tool execution logic.
//!
//! Handles the execution of MCP tools and returns results.
//! All tools are designed to match user capabilities exactly.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Listener, Manager, Runtime};

use super::pane_state::PaneStateStore;
use super::protocol::{INTERNAL_ERROR, INVALID_PARAMS};
use crate::commands::ui::toggle_hidden_files;

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
    let tx = std::sync::Mutex::new(Some(tx));
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
        "quit" => execute_quit(app),
        "switch_pane" => execute_switch_pane(app),
        "swap_panes" => execute_swap_panes(app),
        // View commands
        "toggle_hidden" => execute_toggle_hidden(app),
        "set_view_mode" => execute_set_view_mode(app, params),
        "sort" => execute_sort(app, params),
        // Navigation commands (no params)
        "open_under_cursor" | "nav_to_parent" | "nav_back" | "nav_forward" => execute_nav_command(app, name),
        // Navigation commands (with params)
        "select_volume" | "nav_to_path" | "move_cursor" | "scroll_to" => {
            execute_nav_command_with_params(app, name, params).await
        }
        // Tab commands
        "tab" => execute_tab(app, params),
        // File operation commands
        "copy" => execute_copy(app, params),
        "move" => execute_move(app, params),
        "delete" => execute_delete(app, params),
        "mkdir" => execute_mkdir(app),
        "mkfile" => execute_mkfile(app),
        "refresh" => execute_refresh(app),
        // Selection command
        "select" => execute_select_command(app, params),
        // Dialog command
        "dialog" => execute_dialog_command(app, params),
        // Search commands
        "search" => execute_search(params).await,
        "ai_search" => execute_ai_search(params).await,
        // Settings commands
        "set_setting" => execute_set_setting(app, params).await,
        // Async wait
        "await" => execute_await(app, params).await,
        _ => Err(ToolError::invalid_params(format!("Unknown tool: {name}"))),
    }
}

/// Execute quit command.
fn execute_quit<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.exit(0);
    Ok(json!("OK: Quitting application"))
}

/// Execute switch_pane command.
fn execute_switch_pane<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    // Update the MCP store immediately so the state is correct when read back.
    // The frontend will also update via its own updateFocusedPane call, but that's async.
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let current = store.get_focused_pane();
        let new_pane = if current == "left" { "right" } else { "left" };
        store.set_focused_pane(new_pane.to_string());
    }
    app.emit_to("main", "execute-command", json!({"commandId": "pane.switch"}))?;
    Ok(json!("OK: Switched focus to other pane"))
}

/// Execute swap_panes command.
fn execute_swap_panes<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    // Swap MCP pane state immediately so reads reflect the new layout
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let left = store.get_left();
        let right = store.get_right();
        store.set_left(right);
        store.set_right(left);
    }
    app.emit_to("main", "execute-command", json!({"commandId": "pane.swap"}))?;
    Ok(json!("OK: Swapped left and right panes"))
}

/// Execute unified tab command.
fn execute_tab<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
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

    let tab_id = params.get("tab_id").and_then(|v| v.as_str());

    // Resolve tab_id: required for activate, defaults to active tab for others
    let resolved_tab_id = match action {
        "activate" => tab_id
            .ok_or_else(|| ToolError::invalid_params("'tab_id' is required for activate"))?
            .to_string(),
        "new" => String::new(), // not used
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

    match action {
        "new" => {
            app.emit("mcp-tab", json!({"action": "new", "pane": pane}))?;
            Ok(json!(format!("OK: Creating new tab in {} pane", pane)))
        }
        "close" => {
            app.emit(
                "mcp-tab",
                json!({"action": "close", "pane": pane, "tabId": resolved_tab_id}),
            )?;
            Ok(json!(format!("OK: Closing tab {} in {} pane", resolved_tab_id, pane)))
        }
        "close_others" => {
            app.emit(
                "mcp-tab",
                json!({"action": "close_others", "pane": pane, "tabId": resolved_tab_id}),
            )?;
            Ok(json!(format!(
                "OK: Closing other tabs in {} pane (keeping {} and pinned)",
                pane, resolved_tab_id
            )))
        }
        "activate" => {
            app.emit(
                "mcp-tab",
                json!({"action": "activate", "pane": pane, "tabId": resolved_tab_id}),
            )?;
            Ok(json!(format!(
                "OK: Switched to tab {} in {} pane",
                resolved_tab_id, pane
            )))
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
            Ok(json!(format!("OK: {} tab {} in {} pane", verb, resolved_tab_id, pane)))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown tab action: {}", action))),
    }
}

/// Execute toggle_hidden command.
fn execute_toggle_hidden<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    let result = toggle_hidden_files(app.clone()).map_err(ToolError::internal)?;
    let state = if result { "visible" } else { "hidden" };
    Ok(json!(format!("OK: Hidden files now {state}")))
}

/// Execute set_view_mode command.
fn execute_set_view_mode<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'mode' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if !["brief", "full"].contains(&mode) {
        return Err(ToolError::invalid_params("mode must be 'brief' or 'full'"));
    }

    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_focused_pane(pane.to_string());
    }

    app.emit("mcp-set-view-mode", json!({"pane": pane, "mode": mode}))?;
    Ok(json!(format!("OK: Set {pane} pane to {mode} view")))
}

/// Execute unified sort command.
fn execute_sort<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let by = params
        .get("by")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'by' parameter"))?;
    let order = params
        .get("order")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'order' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if !["name", "ext", "size", "modified", "created"].contains(&by) {
        return Err(ToolError::invalid_params(
            "by must be 'name', 'ext', 'size', 'modified', or 'created'",
        ));
    }
    if !["asc", "desc"].contains(&order) {
        return Err(ToolError::invalid_params("order must be 'asc' or 'desc'"));
    }

    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_focused_pane(pane.to_string());
    }

    app.emit("mcp-sort", json!({"pane": pane, "by": by, "order": order}))?;

    let order_name = if order == "asc" { "ascending" } else { "descending" };
    Ok(json!(format!("OK: Sorted {pane} pane by {by} ({order_name})")))
}

/// Execute a navigation command without parameters.
/// These emit keyboard-equivalent events to the frontend.
fn execute_nav_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    let key = match name {
        "open_under_cursor" => "Enter",
        "nav_to_parent" => "Backspace",
        "nav_back" => "GoBack",       // Custom event, handled by frontend
        "nav_forward" => "GoForward", // Custom event
        _ => return Err(ToolError::invalid_params(format!("Unknown nav command: {name}"))),
    };

    let action = match name {
        "open_under_cursor" => "Opened item under cursor",
        "nav_to_parent" => "Navigated to parent directory",
        "nav_back" => "Navigated back",
        "nav_forward" => "Navigated forward",
        _ => "Navigation action completed",
    };

    app.emit("mcp-key", json!({"key": key}))?;
    Ok(json!(format!("OK: {action}")))
}

/// Execute a navigation command with parameters.
async fn execute_nav_command_with_params<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        "select_volume" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let volume_name = params
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'name' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            // Validate that the volume exists
            #[cfg(target_os = "macos")]
            {
                let locations = crate::volumes::list_locations();
                let is_virtual = volume_name == "Network";
                let is_local = locations.iter().any(|loc| loc.name == volume_name);

                // Check MTP volumes if not a local or virtual volume
                let is_mtp = if !is_virtual && !is_local {
                    let devices = crate::mtp::connection::connection_manager()
                        .get_all_connected_devices()
                        .await;
                    devices.iter().any(|d| {
                        let has_multiple = d.storages.len() > 1;
                        let device_name = d
                            .device
                            .product
                            .as_deref()
                            .or(d.device.manufacturer.as_deref())
                            .unwrap_or(&d.device.id);
                        d.storages.iter().any(|s| {
                            let name = if has_multiple {
                                format!("{} - {}", device_name, s.name)
                            } else {
                                device_name.to_string()
                            };
                            name == volume_name
                        })
                    })
                } else {
                    false
                };

                if !is_virtual && !is_local && !is_mtp {
                    let mut available: Vec<&str> = locations.iter().map(|l| l.name.as_str()).collect();
                    available.push("Network");
                    return Err(ToolError::invalid_params(format!(
                        "Volume '{}' not found. Available volumes: {}",
                        volume_name,
                        available.join(", ")
                    )));
                }
            }

            let store = app
                .try_state::<PaneStateStore>()
                .ok_or_else(|| ToolError::internal("Pane state not available"))?;
            store.set_focused_pane(pane.to_string());
            let path_before = match pane {
                "left" => store.get_left().path,
                "right" => store.get_right().path,
                _ => unreachable!(),
            };
            app.emit("mcp-volume-select", json!({"pane": pane, "name": volume_name}))?;

            // Wait for the target pane's path to change (meaning the volume switch
            // and directory listing completed, and state was pushed to the store).
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            let poll_interval = std::time::Duration::from_millis(250);
            loop {
                let current_path = match pane {
                    "left" => store.get_left().path,
                    "right" => store.get_right().path,
                    _ => unreachable!(),
                };
                if current_path != path_before {
                    break;
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(ToolError::internal(format!(
                        "Timed out waiting for volume '{volume_name}' to load on {pane} pane"
                    )));
                }
                tokio::time::sleep(poll_interval).await;
            }
            Ok(json!(format!("OK: Switched {pane} pane to volume {volume_name}")))
        }
        "nav_to_path" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'path' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            // Validate that the path exists (skip for mtp:// virtual paths)
            if !path.starts_with("mtp://") && !Path::new(path).exists() {
                return Err(ToolError::invalid_params(format!("Path does not exist: {}", path)));
            }

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            mcp_round_trip_with_timeout(
                app,
                "mcp-nav-to-path",
                json!({"pane": pane, "path": path}),
                format!("OK: Navigated {pane} pane to {path}"),
                30,
            )
            .await
        }
        "move_cursor" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            let index_param = params.get("index");
            let filename_param = params.get("filename");

            let to = match (index_param, filename_param) {
                (Some(_), Some(_)) => {
                    return Err(ToolError::invalid_params(
                        "Provide either 'index' or 'filename', not both",
                    ));
                }
                (None, None) => {
                    return Err(ToolError::invalid_params("Provide either 'index' or 'filename'"));
                }
                (Some(idx), None) => {
                    let index = idx
                        .as_i64()
                        .ok_or_else(|| ToolError::invalid_params("'index' must be an integer"))?;
                    if index < 0 {
                        return Err(ToolError::invalid_params("index must be >= 0"));
                    }
                    json!(index)
                }
                (None, Some(name)) => {
                    let filename = name
                        .as_str()
                        .ok_or_else(|| ToolError::invalid_params("'filename' must be a string"))?;
                    json!(filename)
                }
            };

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            mcp_round_trip(
                app,
                "mcp-move-cursor",
                json!({"pane": pane, "to": to}),
                format!("OK: Moved cursor in {pane} pane to {to}"),
            )
            .await
        }
        "scroll_to" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let index = params
                .get("index")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| ToolError::invalid_params("Missing 'index' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }
            if index < 0 {
                return Err(ToolError::invalid_params("index must be >= 0"));
            }

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            app.emit("mcp-scroll-to", json!({"pane": pane, "index": index}))?;
            Ok(json!(format!("OK: Scrolled {pane} pane to index {index}")))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown nav command: {name}"))),
    }
}

/// Execute copy command.
///
/// Note: We cannot validate whether files are selected because selection state
/// is managed by the frontend. The validation happens in the frontend event handler
/// which will show an appropriate error if no files are selected.
fn execute_copy<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str()).unwrap_or("skip_all");

    if auto_confirm && !["skip_all", "overwrite_all", "rename_all"].contains(&on_conflict) {
        return Err(ToolError::invalid_params(
            "onConflict must be 'skip_all', 'overwrite_all', or 'rename_all'",
        ));
    }

    app.emit(
        "mcp-copy",
        json!({"autoConfirm": auto_confirm, "onConflict": on_conflict}),
    )?;

    if auto_confirm {
        Ok(json!("OK: Copy started with auto-confirm."))
    } else {
        Ok(json!("OK: Copy dialog opened. Waiting for user confirmation."))
    }
}

/// Execute move command.
///
/// Note: We cannot validate whether files are selected because selection state
/// is managed by the frontend. The validation happens in the frontend event handler
/// which will show an appropriate error if no files are selected.
fn execute_move<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str()).unwrap_or("skip_all");

    if auto_confirm && !["skip_all", "overwrite_all", "rename_all"].contains(&on_conflict) {
        return Err(ToolError::invalid_params(
            "onConflict must be 'skip_all', 'overwrite_all', or 'rename_all'",
        ));
    }

    app.emit(
        "mcp-move",
        json!({"autoConfirm": auto_confirm, "onConflict": on_conflict}),
    )?;

    if auto_confirm {
        Ok(json!("OK: Move started with auto-confirm."))
    } else {
        Ok(json!("OK: Move dialog opened. Waiting for user confirmation."))
    }
}

/// Execute delete command.
///
/// Note: We cannot validate whether files are selected because selection state
/// is managed by the frontend. The validation happens in the frontend event handler
/// which will show an appropriate error if no files are selected.
fn execute_delete<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);

    app.emit("mcp-delete", json!({"autoConfirm": auto_confirm}))?;

    if auto_confirm {
        Ok(json!("OK: Delete started with auto-confirm."))
    } else {
        Ok(json!("OK: Delete dialog opened. Waiting for user confirmation."))
    }
}

/// Execute mkdir command.
///
/// Note: We cannot validate whether the current directory is writable because
/// the current directory path is managed by the frontend. The validation happens
/// when the actual mkdir operation is attempted, which will return an appropriate
/// error if the directory is not writable.
fn execute_mkdir<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkdir", ())?;
    Ok(json!("OK: Create folder dialog opened."))
}

/// Execute mkfile command.
fn execute_mkfile<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkfile", ())?;
    Ok(json!("OK: Create file dialog opened."))
}

/// Execute refresh command.
fn execute_refresh<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-refresh", ())?;
    Ok(json!("OK: Pane refreshed"))
}

/// Execute the unified select command.
/// Emits event to frontend to manipulate file selection.
fn execute_select_command<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }

    let all_param = params.get("all").and_then(|v| v.as_bool());
    let count_param = params.get("count").and_then(|v| v.as_i64());

    let (start, count): (i64, Value) = match (all_param, count_param) {
        (Some(true), Some(_)) => {
            return Err(ToolError::invalid_params("Provide either 'all' or 'count', not both"));
        }
        (Some(true), None) => {
            // Select all: start doesn't matter, frontend handles it
            (0, json!("all"))
        }
        (_, Some(n)) => {
            if n < 0 {
                return Err(ToolError::invalid_params("count must be >= 0"));
            }
            if n == 0 {
                // Clear selection: start doesn't matter
                (0, json!(0))
            } else {
                // Range select: start is required
                let start = params
                    .get("start")
                    .and_then(|v| v.as_i64())
                    .ok_or_else(|| ToolError::invalid_params("'start' is required when count > 0"))?;
                if start < 0 {
                    return Err(ToolError::invalid_params("start must be >= 0"));
                }
                (start, json!(n))
            }
        }
        (_, None) => {
            return Err(ToolError::invalid_params("Provide either 'all' or 'count'"));
        }
    };

    let mode = params.get("mode").and_then(|v| v.as_str()).unwrap_or("replace");
    if !["replace", "add", "subtract"].contains(&mode) {
        return Err(ToolError::invalid_params(
            "mode must be 'replace', 'add', or 'subtract'",
        ));
    }

    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_focused_pane(pane.to_string());
    }

    app.emit(
        "mcp-select",
        json!({"pane": pane, "start": start, "count": count, "mode": mode}),
    )?;

    Ok(json!(format!("OK: Selection updated in {pane} pane")))
}

/// Execute the unified dialog command.
/// Handles opening, focusing, and closing dialogs.
fn execute_dialog_command<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'action' parameter"))?;

    let dialog_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'type' parameter"))?;

    // Normalize dialog type: accept both "copy-confirmation" and "transfer-confirmation"
    let dialog_type = match dialog_type {
        "copy-confirmation" => "transfer-confirmation",
        other => other,
    };

    // Optional params
    let section = params.get("section").and_then(|v| v.as_str());
    let path = params.get("path").and_then(|v| v.as_str());
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str());

    match action {
        "open" => execute_dialog_open(app, dialog_type, section, path),
        "focus" => execute_dialog_focus(app, dialog_type, path),
        "close" => execute_dialog_close(app, dialog_type, path),
        "confirm" => execute_dialog_confirm(app, dialog_type, on_conflict),
        _ => Err(ToolError::invalid_params(format!("Invalid action: {action}"))),
    }
}

/// Execute dialog open action.
fn execute_dialog_open<R: Runtime>(
    app: &AppHandle<R>,
    dialog_type: &str,
    section: Option<&str>,
    path: Option<&str>,
) -> ToolResult {
    // Window-based dialogs (settings, file-viewer) are tracked automatically
    // via webview_windows() in resources.rs. No manual tracking needed here.

    match dialog_type {
        "settings" => {
            if let Some(section) = section {
                // Section-specific: MCP-only event handled by setupDialogListeners
                app.emit_to("main", "open-settings", json!({"section": section}))?;
                Ok(json!(format!("OK: Opened settings at {section}")))
            } else {
                app.emit_to("main", "execute-command", json!({"commandId": "app.settings"}))?;
                Ok(json!("OK: Opened settings"))
            }
        }
        "file-viewer" => {
            // If path is provided, open for that file; otherwise, use cursor file
            if let Some(path) = path {
                // Validate that the file exists
                if !Path::new(path).exists() {
                    return Err(ToolError::invalid_params(format!("File does not exist: {}", path)));
                }
                app.emit("open-file-viewer", json!({"path": path}))?;
                Ok(json!(format!("OK: Opened file viewer for {path}")))
            } else {
                // Open for file under cursor (validation happens in frontend)
                app.emit("open-file-viewer", ())?;
                Ok(json!("OK: Opened file viewer for cursor file"))
            }
        }
        "about" => {
            app.emit_to("main", "execute-command", json!({"commandId": "app.about"}))?;
            Ok(json!("OK: Opened about dialog"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            Err(ToolError::invalid_params(
                "Cannot open confirmation dialogs directly. Use copy, move, delete, mkdir, or mkfile tools instead.",
            ))
        }
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

/// Execute dialog focus action.
fn execute_dialog_focus<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, path: Option<&str>) -> ToolResult {
    match dialog_type {
        "settings" => {
            app.emit("focus-settings", ())?;
            Ok(json!("OK: Focused settings"))
        }
        "file-viewer" => {
            if let Some(path) = path {
                // Validate that the file exists
                if !Path::new(path).exists() {
                    return Err(ToolError::invalid_params(format!("File does not exist: {}", path)));
                }
                app.emit("focus-file-viewer", json!({"path": path}))?;
                Ok(json!(format!("OK: Focused file viewer for {path}")))
            } else {
                // Focus most recently opened file-viewer
                app.emit("focus-file-viewer", ())?;
                Ok(json!("OK: Focused most recent file viewer"))
            }
        }
        "about" => {
            app.emit("focus-about", ())?;
            Ok(json!("OK: Focused about dialog"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            app.emit("focus-confirmation", ())?;
            Ok(json!("OK: Focused confirmation dialog"))
        }
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

/// Execute dialog close action.
fn execute_dialog_close<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, path: Option<&str>) -> ToolResult {
    // Window-based dialogs are closed via their window; soft dialogs are tracked
    // automatically by the frontend via notify_dialog_closed.

    match dialog_type {
        "settings" => {
            if app.webview_windows().contains_key("settings") {
                app.emit_to("settings", "mcp-settings-close", ())?;
            }
            Ok(json!("OK: Closed settings"))
        }
        "file-viewer" => {
            if let Some(path) = path {
                app.emit("close-file-viewer", json!({"path": path}))?;
                Ok(json!(format!("OK: Closed file viewer for {path}")))
            } else {
                app.emit("close-all-file-viewers", ())?;
                Ok(json!("OK: Closed all file viewer dialogs"))
            }
        }
        "about" => {
            app.emit("close-about", ())?;
            Ok(json!("OK: Closed about dialog"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            app.emit("close-confirmation", ())?;
            Ok(json!("OK: Cancelled confirmation dialog"))
        }
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

/// Execute dialog confirm action.
/// Programmatically confirms an already-open dialog.
fn execute_dialog_confirm<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, on_conflict: Option<&str>) -> ToolResult {
    match dialog_type {
        "transfer-confirmation" => {
            let conflict_policy = on_conflict.unwrap_or("skip_all");
            if !["skip_all", "overwrite_all", "rename_all"].contains(&conflict_policy) {
                return Err(ToolError::invalid_params(
                    "onConflict must be 'skip_all', 'overwrite_all', or 'rename_all'",
                ));
            }
            app.emit(
                "mcp-confirm-dialog",
                json!({"type": "transfer-confirmation", "onConflict": conflict_policy}),
            )?;
            Ok(json!("OK: Transfer dialog confirmed."))
        }
        "delete-confirmation" => {
            app.emit("mcp-confirm-dialog", json!({"type": "delete-confirmation"}))?;
            Ok(json!("OK: Delete dialog confirmed."))
        }
        _ => Err(ToolError::invalid_params(format!(
            "Cannot confirm dialog type '{}'. Only 'transfer-confirmation' and 'delete-confirmation' support confirm.",
            dialog_type
        ))),
    }
}

// ── Await tool ────────────────────────────────────────────────────────

/// Execute the `await` tool: poll PaneStateStore until a condition is met.
async fn execute_await<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let condition = params
        .get("condition")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'condition' parameter"))?;
    let value = params
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'value' parameter"))?;
    let timeout_s = params.get("timeout_s").and_then(|v| v.as_u64()).unwrap_or(15).min(60);
    let after_generation = params.get("after_generation").and_then(|v| v.as_u64());

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if !["has_item", "item_count_gte", "path", "path_contains"].contains(&condition) {
        return Err(ToolError::invalid_params(
            "condition must be 'has_item', 'item_count_gte', 'path', or 'path_contains'",
        ));
    }

    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state not available"))?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_s);
    let poll_interval = std::time::Duration::from_millis(250);

    loop {
        // Check generation gate
        let current_gen = store.get_generation();
        if let Some(min_gen) = after_generation
            && current_gen <= min_gen
        {
            if tokio::time::Instant::now() >= deadline {
                return Err(ToolError::internal(format!(
                    "Timed out after {timeout_s}s waiting for condition '{condition}' = \"{value}\" on {pane} pane (no state update since generation {min_gen})"
                )));
            }
            tokio::time::sleep(poll_interval).await;
            continue;
        }

        let state = match pane {
            "left" => store.get_left(),
            "right" => store.get_right(),
            _ => unreachable!(),
        };

        let matched = match condition {
            "has_item" => state.files.iter().any(|f| f.name == value),
            "item_count_gte" => {
                let min_count: usize = value.parse().unwrap_or(1);
                state.files.len() >= min_count
            }
            "path" => state.path == value,
            "path_contains" => state.path.contains(value),
            _ => unreachable!(),
        };

        if matched {
            // Build a compact state summary to return
            let file_count = state.files.len();
            let first_items: Vec<String> = state
                .files
                .iter()
                .take(20)
                .map(|f| {
                    let kind = if f.is_directory { "d" } else { "f" };
                    format!("{} {}", kind, f.name)
                })
                .collect();

            let result = format!(
                "OK: Condition met (generation {current_gen})\npane: {pane}\npath: {}\nfiles: {} total\nitems:\n{}",
                state.path,
                file_count,
                first_items
                    .iter()
                    .map(|i| format!("  - {i}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            return Ok(json!(result));
        }

        if tokio::time::Instant::now() >= deadline {
            let file_names: Vec<&str> = state.files.iter().take(10).map(|f| f.name.as_str()).collect();
            return Err(ToolError::internal(format!(
                "Timed out after {timeout_s}s waiting for condition '{condition}' = \"{value}\" on {pane} pane. Current path: \"{}\", current generation: {current_gen}, files (first 10): {:?}",
                state.path, file_names
            )));
        }

        tokio::time::sleep(poll_interval).await;
    }
}

// ── Search tools ──────────────────────────────────────────────────────

use crate::search::PatternType;
use crate::search::{
    self, DIALOG_OPEN, SEARCH_INDEX, SearchIndexState, SearchQuery, SearchResult, fill_directory_sizes, format_size,
    format_timestamp, summarize_query,
};

/// Ensure the search index is loaded. Returns the index or an error.
async fn ensure_search_index() -> Result<Arc<search::SearchIndex>, ToolError> {
    // Check if already loaded
    {
        let guard = SEARCH_INDEX.lock().map_err(|e| ToolError::internal(format!("{e}")))?;
        if let Some(ref state) = *guard {
            if state.index.entries.is_empty() && state.index.generation == 0 {
                // Loading sentinel — wait briefly then check again
                log::warn!("MCP ai_search: search index is in loading sentinel state (empty, gen=0), will reload");
            } else {
                log::debug!(
                    "MCP ai_search: search index already loaded, {} entries, gen={}",
                    state.index.entries.len(),
                    state.index.generation
                );
                return Ok(state.index.clone());
            }
        } else {
            log::debug!("MCP ai_search: search index not loaded, will load now");
        }
    }

    // Not loaded — load synchronously via spawn_blocking
    let pool = crate::indexing::get_read_pool().ok_or_else(|| {
        log::error!("MCP ai_search: drive index not available (no read pool)");
        ToolError::internal(
            "Drive index not available. Make sure indexing is enabled and the initial scan has completed.",
        )
    })?;

    DIALOG_OPEN.store(false, Ordering::Relaxed);

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    log::debug!("MCP ai_search: loading search index from DB...");
    let index = tokio::task::spawn_blocking(move || search::load_search_index(&pool, &cancel_clone))
        .await
        .map_err(|e| {
            log::error!("MCP ai_search: search index load spawn_blocking failed: {e}");
            ToolError::internal(format!("Search index load failed: {e}"))
        })?
        .map_err(|e| {
            log::error!("MCP ai_search: search index load failed: {e}");
            ToolError::internal(format!("Search index load failed: {e}"))
        })?;

    log::debug!(
        "MCP ai_search: search index loaded from DB, {} entries",
        index.entries.len()
    );
    let index = Arc::new(index);

    // Store it for reuse (no timers for MCP — one-shot)
    {
        let mut guard = SEARCH_INDEX.lock().map_err(|e| ToolError::internal(format!("{e}")))?;
        *guard = Some(SearchIndexState {
            index: index.clone(),
            idle_timer: None,
            backstop_timer: None,
            load_cancel: Some(cancel),
        });
    }

    Ok(index)
}

/// Parse a human-readable size string into bytes.
/// Supports B, KB, MB, GB, TB (case-insensitive, with or without space).
fn parse_human_size(s: &str) -> Result<u64, ToolError> {
    let s = s.trim();
    // Find where the numeric part ends and the unit begins
    let s_upper = s.to_uppercase();
    let (num_str, unit) = if let Some(pos) = s_upper.find("TB") {
        (&s[..pos], "TB")
    } else if let Some(pos) = s_upper.find("GB") {
        (&s[..pos], "GB")
    } else if let Some(pos) = s_upper.find("MB") {
        (&s[..pos], "MB")
    } else if let Some(pos) = s_upper.find("KB") {
        (&s[..pos], "KB")
    } else if let Some(pos) = s_upper.find('B') {
        (&s[..pos], "B")
    } else {
        // Try parsing as pure number (bytes)
        let n: u64 = s.trim().parse().map_err(|_| {
            ToolError::invalid_params(format!(
                "Couldn't parse size: \"{s}\". Use a format like \"1 MB\" or \"500 KB\"."
            ))
        })?;
        return Ok(n);
    };

    let num: f64 = num_str.trim().parse().map_err(|_| {
        ToolError::invalid_params(format!(
            "Couldn't parse size: \"{s}\". Use a format like \"1 MB\" or \"500 KB\"."
        ))
    })?;

    let multiplier: u64 = match unit {
        "B" => 1,
        "KB" => 1_024,
        "MB" => 1_024 * 1_024,
        "GB" => 1_024 * 1_024 * 1_024,
        "TB" => 1_024 * 1_024 * 1_024 * 1_024,
        _ => unreachable!(),
    };

    Ok((num * multiplier as f64) as u64)
}

/// Format search results as a human-readable table.
fn format_search_results(result: &SearchResult, limit: u32) -> String {
    if result.entries.is_empty() {
        return "No files found matching the query.".to_string();
    }

    let shown = result.entries.len().min(limit as usize);
    let entries = &result.entries[..shown];

    // Compute column widths
    let max_name = entries
        .iter()
        .map(|e| {
            let display_name = if e.is_directory {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            };
            display_name.len()
        })
        .max()
        .unwrap_or(0)
        .max(4);

    let max_parent = entries.iter().map(|e| e.parent_path.len()).max().unwrap_or(0).max(4);

    let mut lines = Vec::with_capacity(entries.len() + 1);
    lines.push(format!("{} of {} results:", shown, result.total_count));

    for entry in entries {
        let display_name = if entry.is_directory {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };

        let size_str = match entry.size {
            Some(s) => format_size(s),
            None => String::new(),
        };

        let date_str = match entry.modified_at {
            Some(ts) => format_timestamp(ts),
            None => String::new(),
        };

        lines.push(format!(
            "  {:<name_w$}  {:<parent_w$}  {:>8}  {}",
            display_name,
            entry.parent_path,
            size_str,
            date_str,
            name_w = max_name,
            parent_w = max_parent,
        ));
    }

    lines.join("\n")
}

/// Run search and post-process (fill dir sizes, post-filter, truncate).
fn run_search_and_postprocess(index: &search::SearchIndex, query: &SearchQuery) -> Result<SearchResult, ToolError> {
    let mut result = search::search(index, query).map_err(ToolError::internal)?;

    // Fill directory sizes from the DB
    if result.entries.iter().any(|e| e.is_directory)
        && let Some(pool) = crate::indexing::get_read_pool()
    {
        fill_directory_sizes(&mut result, &pool);
    }

    // Post-filter: remove directories that don't match size criteria
    let has_size_filter = query.min_size.is_some() || query.max_size.is_some();
    if has_size_filter {
        result.entries.retain(|e| {
            if !e.is_directory {
                return true;
            }
            if let Some(min) = query.min_size {
                match e.size {
                    Some(s) if s >= min => {}
                    _ => return false,
                }
            }
            if let Some(max) = query.max_size {
                match e.size {
                    Some(s) if s <= max => {}
                    _ => return false,
                }
            }
            true
        });
        result.total_count = result.entries.len() as u32;
    }

    // Truncate to limit
    let limit = query.limit.min(1000) as usize;
    if result.entries.len() > limit {
        result.entries.truncate(limit);
    }

    Ok(result)
}

/// Execute the `search` tool.
async fn execute_search(params: &Value) -> ToolResult {
    let pattern = params.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
    let pattern_type = match params.get("pattern_type").and_then(|v| v.as_str()) {
        Some("regex") => PatternType::Regex,
        _ => PatternType::Glob,
    };
    let min_size = params
        .get("min_size")
        .and_then(|v| v.as_str())
        .map(parse_human_size)
        .transpose()?;
    let max_size = params
        .get("max_size")
        .and_then(|v| v.as_str())
        .map(parse_human_size)
        .transpose()?;
    let modified_after = params
        .get("modified_after")
        .and_then(|v| v.as_str())
        .map(search::ai::iso_date_to_timestamp)
        .transpose()
        .map_err(ToolError::invalid_params)?;
    let modified_before = params
        .get("modified_before")
        .and_then(|v| v.as_str())
        .map(search::ai::iso_date_to_timestamp)
        .transpose()
        .map_err(ToolError::invalid_params)?;
    let is_directory = match params.get("type").and_then(|v| v.as_str()) {
        Some("file") => Some(false),
        Some("dir") => Some(true),
        _ => None,
    };
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as u32;

    let index = ensure_search_index().await?;

    // Parse scope if provided
    let scope_str = params.get("scope").and_then(|v| v.as_str());
    let (include_paths, exclude_dir_names) = if let Some(scope) = scope_str {
        let parsed = search::parse_scope(scope);
        let inc = if parsed.include_paths.is_empty() {
            None
        } else {
            Some(parsed.include_paths)
        };
        let exc = if parsed.exclude_patterns.is_empty() {
            None
        } else {
            Some(parsed.exclude_patterns)
        };
        (inc, exc)
    } else {
        (None, None)
    };

    let case_sensitive = params.get("caseSensitive").and_then(|v| v.as_bool());
    let exclude_system_dirs = params.get("excludeSystemDirs").and_then(|v| v.as_bool());

    let mut query = SearchQuery {
        name_pattern: pattern,
        pattern_type,
        min_size,
        max_size,
        modified_after,
        modified_before,
        is_directory,
        include_paths,
        exclude_dir_names,
        include_path_ids: None,
        limit,
        case_sensitive,
        exclude_system_dirs,
    };

    // Resolve include paths to entry IDs via SQLite
    if query.include_paths.as_ref().is_some_and(|p| !p.is_empty())
        && let Some(pool) = crate::indexing::get_read_pool()
    {
        search::resolve_include_paths(&mut query, &pool);
    }

    let query_clone = query.clone();
    let index_clone = index.clone();
    let result = tokio::task::spawn_blocking(move || run_search_and_postprocess(&index_clone, &query_clone))
        .await
        .map_err(|e| ToolError::internal(format!("Search failed: {e}")))??;

    Ok(json!(format_search_results(&result, limit)))
}

/// Build a `SearchQuery` from a `TranslateResult`, merging in caller-provided scope
/// and the LLM-suggested scope, then applying system directory exclusions.
fn build_search_query_from_translate(
    translate_result: &crate::commands::search::TranslateResult,
    scope_str: Option<&str>,
    limit: u32,
) -> SearchQuery {
    // Start with LLM-suggested scope
    let mut include_paths: Option<Vec<String>> = translate_result.query.include_paths.clone();
    let mut exclude_dir_names: Option<Vec<String>> = translate_result.query.exclude_dir_names.clone();

    // Merge caller-provided scope (the explicit `scope` parameter from the MCP request)
    if let Some(scope) = scope_str {
        let parsed = search::parse_scope(scope);
        if !parsed.include_paths.is_empty() {
            include_paths.get_or_insert_with(Vec::new).extend(parsed.include_paths);
        }
        if !parsed.exclude_patterns.is_empty() {
            exclude_dir_names
                .get_or_insert_with(Vec::new)
                .extend(parsed.exclude_patterns);
        }
    }

    SearchQuery {
        name_pattern: translate_result.query.name_pattern.clone(),
        pattern_type: if translate_result.query.pattern_type == "regex" {
            PatternType::Regex
        } else {
            PatternType::Glob
        },
        min_size: translate_result.query.min_size,
        max_size: translate_result.query.max_size,
        modified_after: translate_result.query.modified_after,
        modified_before: translate_result.query.modified_before,
        is_directory: translate_result.query.is_directory,
        include_path_ids: None,
        include_paths,
        exclude_dir_names,
        limit,
        case_sensitive: translate_result.query.case_sensitive,
        exclude_system_dirs: translate_result.query.exclude_system_dirs,
    }
}

/// Execute the `ai_search` tool.
///
/// Single-pass flow: translate natural language → structured query → search.
async fn execute_ai_search(params: &Value) -> ToolResult {
    let natural_query = params.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
        log::warn!("MCP ai_search: missing 'query' parameter, returning error");
        ToolError::invalid_params("Missing 'query' parameter")
    })?;
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
    let scope_str = params.get("scope").and_then(|v| v.as_str());
    let total_t = std::time::Instant::now();
    log::info!("MCP ai_search: handler entered, query={natural_query:?}, limit={limit}, scope={scope_str:?}");

    log::debug!("MCP ai_search: loading search index...");
    let index = match ensure_search_index().await {
        Ok(idx) => {
            log::debug!("MCP ai_search: search index loaded, {} entries", idx.entries.len());
            idx
        }
        Err(e) => {
            log::error!("MCP ai_search: search index load failed: {}", e.message);
            return Err(e);
        }
    };

    // ── Translate query ──────────────────────────────────────────────
    log::debug!("MCP ai_search: calling translate_search_query for query={natural_query:?}");
    let t = std::time::Instant::now();
    let translate_result = match crate::commands::search::translate_search_query(natural_query.to_string()).await {
        Ok(tr) => {
            log::info!(
                "MCP ai_search: translate_search_query succeeded in {:.1}s, pattern={:?}",
                t.elapsed().as_secs_f64(),
                tr.query.name_pattern
            );
            tr
        }
        Err(e) => {
            log::warn!("MCP ai_search: LLM call failed for query={natural_query:?}: {e}");
            return Err(ToolError::internal(format!("AI translation failed: {e}")));
        }
    };

    let mut query = build_search_query_from_translate(&translate_result, scope_str, limit);

    // Resolve include paths to entry IDs via SQLite
    if query.include_paths.as_ref().is_some_and(|p| !p.is_empty())
        && let Some(pool) = crate::indexing::get_read_pool()
    {
        search::resolve_include_paths(&mut query, &pool);
    }

    log::debug!("MCP ai_search: running search...");
    let t = std::time::Instant::now();
    let query_clone = query.clone();
    let index_clone = index.clone();
    let result = match tokio::task::spawn_blocking(move || run_search_and_postprocess(&index_clone, &query_clone)).await
    {
        Ok(Ok(result)) => {
            log::info!(
                "MCP ai_search: search completed in {:.1}s, {} results (total_count={})",
                t.elapsed().as_secs_f64(),
                result.entries.len(),
                result.total_count
            );
            result
        }
        Ok(Err(e)) => {
            log::error!("MCP ai_search: search failed (postprocess): {}", e.message);
            return Err(e);
        }
        Err(e) => {
            log::error!("MCP ai_search: spawn_blocking failed (task join): {e}");
            return Err(ToolError::internal(format!("Search failed: {e}")));
        }
    };

    // ── Fallback: if 0 results and LLM suggested searchPaths, retry without them ──
    let (result, query) = if result.total_count == 0
        && translate_result
            .query
            .include_paths
            .as_ref()
            .is_some_and(|p| !p.is_empty())
    {
        log::info!(
            "MCP ai_search: returned 0 results with searchPaths {:?}, retrying full-drive search",
            translate_result.query.include_paths
        );
        let mut fallback_query = query;
        fallback_query.include_paths = None;
        fallback_query.include_path_ids = None;
        let fallback_query_clone = fallback_query.clone();
        let index_clone = index.clone();
        let t = std::time::Instant::now();
        match tokio::task::spawn_blocking(move || run_search_and_postprocess(&index_clone, &fallback_query_clone)).await
        {
            Ok(Ok(result)) => {
                log::info!(
                    "MCP ai_search: fallback full-drive search completed in {:.1}s, {} results",
                    t.elapsed().as_secs_f64(),
                    result.total_count
                );
                (result, fallback_query)
            }
            Ok(Err(e)) => {
                log::error!("MCP ai_search: fallback search failed: {}", e.message);
                return Err(e);
            }
            Err(e) => {
                log::error!("MCP ai_search: fallback spawn_blocking failed: {e}");
                return Err(ToolError::internal(format!("Search failed: {e}")));
            }
        }
    } else {
        (result, query)
    };

    let interpreted = summarize_query(&query);
    let formatted = format_search_results(&result, limit);
    let caveat_line = translate_result
        .caveat
        .as_deref()
        .map(|c| format!("Note: {c}\n"))
        .unwrap_or_default();
    let output = format!(
        "{} hits\n\nInterpreted query: {interpreted}\n{caveat_line}\n{formatted}",
        result.total_count
    );
    log::info!(
        "MCP ai_search: completed in {:.1}s, output length={}",
        total_t.elapsed().as_secs_f64(),
        output.len()
    );
    Ok(json!(output))
}

/// Execute set_setting command via round-trip to the frontend.
async fn execute_set_setting<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'id' parameter"))?;

    let value = params
        .get("value")
        .ok_or_else(|| ToolError::invalid_params("Missing 'value' parameter"))?;

    mcp_round_trip(
        app,
        "mcp-set-setting",
        json!({"settingId": id, "value": value}),
        format!("OK: Set '{id}' to {value}"),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_error_invalid_params() {
        let err = ToolError::invalid_params("test error");
        assert_eq!(err.code, INVALID_PARAMS);
        assert_eq!(err.message, "test error");
    }

    #[test]
    fn test_tool_error_internal() {
        let err = ToolError::internal("internal error");
        assert_eq!(err.code, INTERNAL_ERROR);
        assert_eq!(err.message, "internal error");
    }

    #[test]
    fn test_path_exists_validation() {
        // Test that Path::new().exists() works as expected for our validation
        assert!(Path::new("/").exists(), "Root should exist");
        assert!(Path::new("/tmp").exists(), "Temp dir should exist");
        assert!(
            !Path::new("/nonexistent/path/that/does/not/exist").exists(),
            "Nonexistent path should not exist"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_volume_list_not_empty() {
        // Verify we can list volumes for validation
        let locations = crate::volumes::list_locations();
        assert!(!locations.is_empty(), "Should have at least one volume");
        // Should have a main volume
        assert!(
            locations
                .iter()
                .any(|l| l.category == crate::volumes::LocationCategory::MainVolume),
            "Should have main volume"
        );
    }

    #[test]
    fn test_parse_human_size_with_space() {
        assert_eq!(parse_human_size("1 MB").unwrap(), 1_048_576);
        assert_eq!(parse_human_size("500 KB").unwrap(), 512_000);
        assert_eq!(parse_human_size("2 GB").unwrap(), 2_147_483_648);
        assert_eq!(parse_human_size("1 TB").unwrap(), 1_099_511_627_776);
        assert_eq!(parse_human_size("100 B").unwrap(), 100);
    }

    #[test]
    fn test_parse_human_size_no_space() {
        assert_eq!(parse_human_size("1MB").unwrap(), 1_048_576);
        assert_eq!(parse_human_size("500KB").unwrap(), 512_000);
        assert_eq!(parse_human_size("2GB").unwrap(), 2_147_483_648);
    }

    #[test]
    fn test_parse_human_size_case_insensitive() {
        assert_eq!(parse_human_size("1 mb").unwrap(), 1_048_576);
        assert_eq!(parse_human_size("500 kb").unwrap(), 512_000);
        assert_eq!(parse_human_size("1 Mb").unwrap(), 1_048_576);
    }

    #[test]
    fn test_parse_human_size_decimal() {
        assert_eq!(parse_human_size("1.5 MB").unwrap(), 1_572_864);
        assert_eq!(parse_human_size("0.5 GB").unwrap(), 536_870_912);
    }

    #[test]
    fn test_parse_human_size_invalid() {
        assert!(parse_human_size("abc").is_err());
        assert!(parse_human_size("MB").is_err());
    }

    #[test]
    fn test_format_search_results_empty() {
        let result = SearchResult {
            entries: Vec::new(),
            total_count: 0,
        };
        assert_eq!(format_search_results(&result, 30), "No files found matching the query.");
    }

    #[test]
    fn test_format_search_results_with_entries() {
        use crate::search::SearchResultEntry;
        let result = SearchResult {
            entries: vec![SearchResultEntry {
                name: "test.pdf".to_string(),
                path: "/Users/test/Documents/test.pdf".to_string(),
                parent_path: "~/Documents".to_string(),
                is_directory: false,
                size: Some(340_000),
                modified_at: Some(1_735_689_600),
                icon_id: "pdf".to_string(),
                entry_id: 1,
            }],
            total_count: 1,
        };
        let formatted = format_search_results(&result, 30);
        assert!(formatted.contains("1 of 1 results:"));
        assert!(formatted.contains("test.pdf"));
        assert!(formatted.contains("~/Documents"));
    }

    #[test]
    fn test_format_search_results_directory_trailing_slash() {
        use crate::search::SearchResultEntry;
        let result = SearchResult {
            entries: vec![SearchResultEntry {
                name: "Projects".to_string(),
                path: "/Users/test/Projects".to_string(),
                parent_path: "~".to_string(),
                is_directory: true,
                size: Some(1_200_000),
                modified_at: Some(1_735_689_600),
                icon_id: "dir".to_string(),
                entry_id: 2,
            }],
            total_count: 1,
        };
        let formatted = format_search_results(&result, 30);
        assert!(formatted.contains("Projects/"));
    }
}
