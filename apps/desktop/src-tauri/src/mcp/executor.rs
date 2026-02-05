//! Tool execution logic.
//!
//! Handles the execution of MCP tools and returns results.
//! All tools are designed to match user capabilities exactly.

use std::path::Path;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::dialog_state::DialogStateStore;
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

/// Execute a tool by name.
pub fn execute_tool<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        // App commands
        "quit" => execute_quit(app),
        "switch_pane" => execute_switch_pane(app),
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
    app.emit("switch-pane", ())
        .map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!("OK: Switched focus to other pane"))
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

    app.emit("mcp-set-view-mode", json!({"pane": pane, "mode": mode}))
        .map_err(|e| ToolError::internal(e.to_string()))?;
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

    app.emit("mcp-sort", json!({"pane": pane, "by": by, "order": order}))
        .map_err(|e| ToolError::internal(e.to_string()))?;

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

    app.emit("mcp-key", json!({"key": key}))
        .map_err(|e| ToolError::internal(e.to_string()))?;
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
                if !locations.iter().any(|loc| loc.name == volume_name) {
                    let available: Vec<&str> = locations.iter().map(|l| l.name.as_str()).collect();
                    return Err(ToolError::invalid_params(format!(
                        "Volume '{}' not found. Available volumes: {}",
                        volume_name,
                        available.join(", ")
                    )));
                }
            }

            app.emit("mcp-volume-select", json!({"pane": pane, "name": volume_name}))
                .map_err(|e| ToolError::internal(e.to_string()))?;
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

            app.emit("mcp-nav-to-path", json!({"pane": pane, "path": path}))
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!(format!("OK: Navigated {pane} pane to {path}")))
        }
        "move_cursor" => {
            let pane = params
                .get("pane")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
            let to = params
                .get("to")
                .ok_or_else(|| ToolError::invalid_params("Missing 'to' parameter"))?;

            if !["left", "right"].contains(&pane) {
                return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
            }

            // 'to' can be either an integer index or a string filename
            // If it's a number, validate it's non-negative
            if let Some(index) = to.as_i64() {
                if index < 0 {
                    return Err(ToolError::invalid_params("index must be >= 0"));
                }
            } else if to.as_str().is_none() {
                return Err(ToolError::invalid_params(
                    "'to' must be an index (number) or filename (string)",
                ));
            }

            app.emit("mcp-move-cursor", json!({"pane": pane, "to": to}))
                .map_err(|e| ToolError::internal(e.to_string()))?;
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

            app.emit("mcp-scroll-to", json!({"pane": pane, "index": index}))
                .map_err(|e| ToolError::internal(e.to_string()))?;
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
    app.emit("mcp-copy", ())
        .map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!("OK: Copy dialog opened. Waiting for user confirmation."))
}

/// Execute mkdir command.
///
/// Note: We cannot validate whether the current directory is writable because
/// the current directory path is managed by the frontend. The validation happens
/// when the actual mkdir operation is attempted, which will return an appropriate
/// error if the directory is not writable.
fn execute_mkdir<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkdir", ())
        .map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!("OK: Create folder dialog opened."))
}

/// Execute refresh command.
fn execute_refresh<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-refresh", ())
        .map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!("OK: Pane refreshed"))
}

/// Execute the unified select command.
/// Emits event to frontend to manipulate file selection.
fn execute_select_command<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;

    let start = params
        .get("start")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| ToolError::invalid_params("Missing 'start' parameter"))?;

    // count can be a number or the string "all"
    let count_value = params
        .get("count")
        .ok_or_else(|| ToolError::invalid_params("Missing 'count' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if start < 0 {
        return Err(ToolError::invalid_params("start must be >= 0"));
    }

    let count: Value = if let Some(n) = count_value.as_i64() {
        if n < 0 {
            return Err(ToolError::invalid_params("count must be >= 0"));
        }
        json!(n)
    } else if let Some(s) = count_value.as_str() {
        if s == "all" {
            json!("all")
        } else {
            return Err(ToolError::invalid_params("count must be a number or 'all'"));
        }
    } else {
        return Err(ToolError::invalid_params("count must be a number or 'all'"));
    };

    // mode defaults to "replace" if not provided
    let mode = params.get("mode").and_then(|v| v.as_str()).unwrap_or("replace");

    // Validate mode
    if !["replace", "add", "subtract"].contains(&mode) {
        return Err(ToolError::invalid_params(
            "mode must be 'replace', 'add', or 'subtract'",
        ));
    }

    app.emit(
        "mcp-select",
        json!({"pane": pane, "start": start, "count": count, "mode": mode}),
    )
    .map_err(|e| ToolError::internal(e.to_string()))?;

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
    // Track dialog state
    if let Some(store) = app.try_state::<DialogStateStore>() {
        match dialog_type {
            "settings" => store.set_settings_open(true),
            "about" => store.set_about_open(true),
            "volume-picker" => store.set_volume_picker_open(true),
            "file-viewer" => {
                if let Some(p) = path {
                    store.add_file_viewer(p.to_string());
                }
            }
            _ => {}
        }
    }

    match dialog_type {
        "settings" => {
            // Emit event to open settings, optionally with a section
            if let Some(section) = section {
                app.emit_to("main", "open-settings", json!({"section": section}))
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!(format!("OK: Opened settings at {section}")))
            } else {
                app.emit_to("main", "open-settings", ())
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!("OK: Opened settings"))
            }
        }
        "volume-picker" => {
            app.emit("open-volume-picker", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Opened volume picker"))
        }
        "file-viewer" => {
            // If path is provided, open for that file; otherwise, use cursor file
            if let Some(path) = path {
                // Validate that the file exists
                if !Path::new(path).exists() {
                    return Err(ToolError::invalid_params(format!("File does not exist: {}", path)));
                }
                app.emit("open-file-viewer", json!({"path": path}))
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!(format!("OK: Opened file viewer for {path}")))
            } else {
                // Open for file under cursor (validation happens in frontend)
                app.emit("open-file-viewer", ())
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!("OK: Opened file viewer for cursor file"))
            }
        }
        "about" => {
            app.emit("show-about", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Opened about dialog"))
        }
        "confirmation" => Err(ToolError::invalid_params(
            "Cannot open confirmation dialog directly. Use copy or mkdir tools instead.",
        )),
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

/// Execute dialog focus action.
fn execute_dialog_focus<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, path: Option<&str>) -> ToolResult {
    match dialog_type {
        "settings" => {
            app.emit("focus-settings", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Focused settings"))
        }
        "file-viewer" => {
            if let Some(path) = path {
                // Validate that the file exists
                if !Path::new(path).exists() {
                    return Err(ToolError::invalid_params(format!("File does not exist: {}", path)));
                }
                app.emit("focus-file-viewer", json!({"path": path}))
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!(format!("OK: Focused file viewer for {path}")))
            } else {
                // Focus most recently opened file-viewer
                app.emit("focus-file-viewer", ())
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!("OK: Focused most recent file viewer"))
            }
        }
        "volume-picker" => {
            app.emit("focus-volume-picker", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Focused volume picker"))
        }
        "about" => {
            app.emit("focus-about", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Focused about dialog"))
        }
        "confirmation" => {
            app.emit("focus-confirmation", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Focused confirmation dialog"))
        }
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

/// Execute dialog close action.
fn execute_dialog_close<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, path: Option<&str>) -> ToolResult {
    // Track dialog state
    if let Some(store) = app.try_state::<DialogStateStore>() {
        match dialog_type {
            "settings" => store.set_settings_open(false),
            "about" => store.set_about_open(false),
            "volume-picker" => store.set_volume_picker_open(false),
            "confirmation" => store.set_confirmation_open(false),
            "file-viewer" => {
                if let Some(p) = path {
                    store.remove_file_viewer(p);
                } else {
                    store.clear_all_file_viewers();
                }
            }
            _ => {}
        }
    }

    match dialog_type {
        "settings" => {
            app.emit_to("settings", "mcp-settings-close", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Closed settings"))
        }
        "volume-picker" => {
            app.emit("close-volume-picker", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Closed volume picker"))
        }
        "file-viewer" => {
            if let Some(path) = path {
                // Close specific file viewer
                app.emit("close-file-viewer", json!({"path": path}))
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!(format!("OK: Closed file viewer for {path}")))
            } else {
                // Close all file viewers
                app.emit("close-all-file-viewers", ())
                    .map_err(|e| ToolError::internal(e.to_string()))?;
                Ok(json!("OK: Closed all file viewer dialogs"))
            }
        }
        "about" => {
            app.emit("close-about", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!("OK: Closed about dialog"))
        }
        "confirmation" => {
            app.emit("close-confirmation", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
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
