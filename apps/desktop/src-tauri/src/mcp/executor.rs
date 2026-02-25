//! Tool execution logic.
//!
//! Handles the execution of MCP tools and returns results.
//! All tools are designed to match user capabilities exactly.

use std::path::Path;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

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

/// Execute a tool by name.
pub fn execute_tool<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
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
            execute_nav_command_with_params(app, name, params)
        }
        // Tab commands
        "activate_tab" => execute_activate_tab(app, params),
        "pin_tab" => execute_pin_tab(app, params),
        // File operation commands
        "copy" => execute_copy(app),
        "mkdir" => execute_mkdir(app),
        "refresh" => execute_refresh(app),
        // Selection command
        "select" => execute_select_command(app, params),
        // Dialog command
        "dialog" => execute_dialog_command(app, params),
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
    app.emit("switch-pane", ())?;
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
    app.emit("swap-panes", ())?;
    Ok(json!("OK: Swapped left and right panes"))
}

/// Execute activate_tab command.
fn execute_activate_tab<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let tab_id = params
        .get("tab_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'tab_id' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }

    // Validate that the tab ID exists in the pane's synced tab list
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let pane_state = match pane {
            "left" => store.get_left(),
            "right" => store.get_right(),
            _ => unreachable!(),
        };
        if !pane_state.tabs.is_empty() && !pane_state.tabs.iter().any(|t| t.id == tab_id) {
            let available_ids: Vec<&str> = pane_state.tabs.iter().map(|t| t.id.as_str()).collect();
            return Err(ToolError::invalid_params(format!(
                "Tab '{}' not found in {} pane. Available tabs: {}",
                tab_id,
                pane,
                available_ids.join(", ")
            )));
        }
    }

    app.emit("mcp-activate-tab", json!({"pane": pane, "tabId": tab_id}))?;
    Ok(json!(format!("OK: Switched to tab {} in {} pane", tab_id, pane)))
}

/// Execute pin_tab command.
fn execute_pin_tab<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let tab_id = params
        .get("tab_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'tab_id' parameter"))?;
    let pinned = params
        .get("pinned")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pinned' parameter (boolean)"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }

    // Validate that the tab ID exists in the pane's synced tab list
    if let Some(store) = app.try_state::<PaneStateStore>() {
        let pane_state = match pane {
            "left" => store.get_left(),
            "right" => store.get_right(),
            _ => unreachable!(),
        };
        if !pane_state.tabs.is_empty() && !pane_state.tabs.iter().any(|t| t.id == tab_id) {
            let available_ids: Vec<&str> = pane_state.tabs.iter().map(|t| t.id.as_str()).collect();
            return Err(ToolError::invalid_params(format!(
                "Tab '{}' not found in {} pane. Available tabs: {}",
                tab_id,
                pane,
                available_ids.join(", ")
            )));
        }
    }

    let action = if pinned { "Pinned" } else { "Unpinned" };
    app.emit("mcp-pin-tab", json!({"pane": pane, "tabId": tab_id, "pinned": pinned}))?;
    Ok(json!(format!("OK: {} tab {} in {} pane", action, tab_id, pane)))
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
fn execute_nav_command_with_params<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
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
                if !is_virtual && !locations.iter().any(|loc| loc.name == volume_name) {
                    let mut available: Vec<&str> = locations.iter().map(|l| l.name.as_str()).collect();
                    available.push("Network");
                    return Err(ToolError::invalid_params(format!(
                        "Volume '{}' not found. Available volumes: {}",
                        volume_name,
                        available.join(", ")
                    )));
                }
            }

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            app.emit("mcp-volume-select", json!({"pane": pane, "name": volume_name}))?;
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

            // Validate that the path exists
            if !Path::new(path).exists() {
                return Err(ToolError::invalid_params(format!("Path does not exist: {}", path)));
            }

            if let Some(store) = app.try_state::<PaneStateStore>() {
                store.set_focused_pane(pane.to_string());
            }

            app.emit("mcp-nav-to-path", json!({"pane": pane, "path": path}))?;
            Ok(json!(format!("OK: Navigated {pane} pane to {path}")))
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

            app.emit("mcp-move-cursor", json!({"pane": pane, "to": to}))?;
            Ok(json!(format!("OK: Moved cursor in {pane} pane to {to}")))
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
fn execute_copy<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-copy", ())?;
    Ok(json!("OK: Copy dialog opened. Waiting for user confirmation."))
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

    // Optional params
    let section = params.get("section").and_then(|v| v.as_str());
    let path = params.get("path").and_then(|v| v.as_str());

    match action {
        "open" => execute_dialog_open(app, dialog_type, section, path),
        "focus" => execute_dialog_focus(app, dialog_type, path),
        "close" => execute_dialog_close(app, dialog_type, path),
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
            // Emit event to open settings, optionally with a section
            if let Some(section) = section {
                app.emit_to("main", "open-settings", json!({"section": section}))?;
                Ok(json!(format!("OK: Opened settings at {section}")))
            } else {
                app.emit_to("main", "open-settings", ())?;
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
            app.emit("show-about", ())?;
            Ok(json!("OK: Opened about dialog"))
        }
        "copy-confirmation" | "mkdir-confirmation" => Err(ToolError::invalid_params(
            "Cannot open confirmation dialogs directly. Use copy or mkdir tools instead.",
        )),
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
        "copy-confirmation" | "mkdir-confirmation" => {
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
        "copy-confirmation" | "mkdir-confirmation" => {
            app.emit("close-confirmation", ())?;
            Ok(json!("OK: Cancelled confirmation dialog"))
        }
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
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
}
