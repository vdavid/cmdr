//! Tool execution logic.
//!
//! Handles the execution of MCP tools and returns results.
//! All tools are designed to match user capabilities exactly.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::pane_state::PaneStateStore;
use super::protocol::{INTERNAL_ERROR, INVALID_PARAMS};
use crate::commands::ui::{
    copy_to_clipboard, get_info, quick_look, set_view_mode, show_in_finder, toggle_hidden_files,
};

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
        n if n.starts_with("app_") => execute_app_command(app, n),
        // View commands
        n if n.starts_with("view_") => execute_view_command(app, n),
        // Pane commands
        n if n.starts_with("pane_") => execute_pane_command(app, n),
        // Navigation commands
        n if n.starts_with("nav_") => execute_nav_command(app, n),
        // Sort commands
        n if n.starts_with("sort_") => execute_sort_command(app, n),
        // File commands
        n if n.starts_with("file_") => execute_file_command(app, n),
        // Volume commands
        n if n.starts_with("volume_") => execute_volume_command(app, n, params),
        // Context commands
        n if n.starts_with("context_") => execute_context_command(app, n),
        _ => Err(ToolError::invalid_params(format!("Unknown tool: {name}"))),
    }
}

/// Execute an app command.
fn execute_app_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    match name {
        "app_quit" => {
            app.exit(0);
            Ok(json!({"success": true}))
        }
        "app_hide" => {
            // Use macOS NSApplication hide (same as ⌘H)
            #[cfg(target_os = "macos")]
            {
                use objc2::MainThreadMarker;
                use objc2_app_kit::NSApplication;
                if let Some(mtm) = MainThreadMarker::new() {
                    let app_instance = NSApplication::sharedApplication(mtm);
                    app_instance.hide(None);
                }
            }
            Ok(json!({"success": true}))
        }
        "app_about" => {
            app.emit("show-about", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!({"success": true}))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown app command: {name}"))),
    }
}

/// Execute a view command.
fn execute_view_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    match name {
        "view_showHidden" => {
            let result = toggle_hidden_files(app.clone()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "showHiddenFiles": result}))
        }
        "view_briefMode" => {
            set_view_mode(app.clone(), "brief".to_string()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "viewMode": "brief"}))
        }
        "view_fullMode" => {
            set_view_mode(app.clone(), "full".to_string()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "viewMode": "full"}))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown view command: {name}"))),
    }
}

/// Execute a pane command.
fn execute_pane_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    match name {
        "pane_switch" => {
            app.emit("switch-pane", ())
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!({"success": true}))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown pane command: {name}"))),
    }
}

/// Execute a navigation command.
/// These emit keyboard-equivalent events to the frontend.
fn execute_nav_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    let key = match name {
        "nav_open" => "Enter",
        "nav_parent" => "Backspace",
        "nav_back" => "GoBack",       // Custom event, handled by frontend
        "nav_forward" => "GoForward", // Custom event
        "nav_up" => "ArrowUp",
        "nav_down" => "ArrowDown",
        "nav_left" => "ArrowLeft",
        "nav_right" => "ArrowRight",
        "nav_home" => "Home",
        "nav_end" => "End",
        "nav_pageUp" => "PageUp",
        "nav_pageDown" => "PageDown",
        _ => return Err(ToolError::invalid_params(format!("Unknown nav command: {name}"))),
    };

    app.emit("mcp-key", json!({"key": key}))
        .map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!({"success": true, "key": key}))
}

/// Execute a sort command.
fn execute_sort_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    let (action, value) = match name {
        "sort_byName" => ("sortBy", "name"),
        "sort_byExtension" => ("sortBy", "extension"),
        "sort_bySize" => ("sortBy", "size"),
        "sort_byModified" => ("sortBy", "modified"),
        "sort_byCreated" => ("sortBy", "created"),
        "sort_ascending" => ("sortOrder", "asc"),
        "sort_descending" => ("sortOrder", "desc"),
        "sort_toggleOrder" => ("sortOrder", "toggle"),
        _ => return Err(ToolError::invalid_params(format!("Unknown sort command: {name}"))),
    };

    app.emit("mcp-sort", json!({"action": action, "value": value}))
        .map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!({"success": true, "action": action, "value": value}))
}

/// Execute a file command.
/// Gets the selected file from PaneStateStore and operates on it directly.
fn execute_file_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    // Get the selected file from pane state
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state not initialized"))?;

    let focused = store.get_focused_pane();
    let pane = if focused == "right" {
        store.get_right()
    } else {
        store.get_left()
    };

    let selected = pane
        .files
        .get(pane.selected_index)
        .ok_or_else(|| ToolError::internal("No file selected"))?;

    match name {
        "file_showInFinder" => {
            show_in_finder(selected.path.clone()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "path": selected.path}))
        }
        "file_copyPath" => {
            copy_to_clipboard(app.clone(), selected.path.clone()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "copied": selected.path}))
        }
        "file_copyFilename" => {
            copy_to_clipboard(app.clone(), selected.name.clone()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "copied": selected.name}))
        }
        "file_quickLook" => {
            quick_look(selected.path.clone()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "path": selected.path}))
        }
        "file_getInfo" => {
            get_info(selected.path.clone()).map_err(ToolError::internal)?;
            Ok(json!({"success": true, "path": selected.path}))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown file command: {name}"))),
    }
}

/// Execute a volume command.
/// Note: volume listing is now a resource (cmdr://volumes), not a tool.
fn execute_volume_command<R: Runtime>(app: &AppHandle<R>, name: &str, params: &Value) -> ToolResult {
    match name {
        "volume_selectLeft" | "volume_selectRight" => {
            let index = params
                .get("index")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| ToolError::invalid_params("Missing 'index' parameter"))?;

            let pane = if name == "volume_selectLeft" { "left" } else { "right" };

            app.emit("mcp-volume-select", json!({"pane": pane, "index": index}))
                .map_err(|e| ToolError::internal(e.to_string()))?;
            Ok(json!({"success": true, "pane": pane, "index": index}))
        }
        _ => Err(ToolError::invalid_params(format!("Unknown volume command: {name}"))),
    }
}

/// Execute a context command.
fn execute_context_command<R: Runtime>(app: &AppHandle<R>, name: &str) -> ToolResult {
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state not initialized"))?;

    match name {
        "context_getFocusedPane" => {
            let focused = store.get_focused_pane();
            Ok(json!({"focusedPane": focused}))
        }

        "context_getLeftPanePath" => {
            let left = store.get_left();
            Ok(json!({
                "path": left.path,
                "volumeId": left.volume_id,
            }))
        }

        "context_getRightPanePath" => {
            let right = store.get_right();
            Ok(json!({
                "path": right.path,
                "volumeId": right.volume_id,
            }))
        }

        "context_getLeftPaneContent" => {
            let left = store.get_left();
            Ok(json!({
                "path": left.path,
                "files": left.files,
                "selectedIndex": left.selected_index,
                "viewMode": left.view_mode,
                "totalCount": left.files.len(),
            }))
        }

        "context_getRightPaneContent" => {
            let right = store.get_right();
            Ok(json!({
                "path": right.path,
                "files": right.files,
                "selectedIndex": right.selected_index,
                "viewMode": right.view_mode,
                "totalCount": right.files.len(),
            }))
        }

        "context_getSelectedFileInfo" => {
            let focused = store.get_focused_pane();
            let pane = if focused == "right" {
                store.get_right()
            } else {
                store.get_left()
            };

            let selected = pane.files.get(pane.selected_index).cloned();
            match selected {
                Some(file) => Ok(json!({
                    "name": file.name,
                    "path": file.path,
                    "isDirectory": file.is_directory,
                    "size": file.size,
                    "modified": file.modified,
                })),
                None => Ok(json!({"error": "No file selected"})),
            }
        }

        _ => Err(ToolError::invalid_params(format!("Unknown context command: {name}"))),
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
}
