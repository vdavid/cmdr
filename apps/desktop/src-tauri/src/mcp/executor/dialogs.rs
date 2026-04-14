//! Dialog tool handlers.

use std::path::Path;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{ToolError, ToolResult};

/// Execute the unified dialog command.
/// Handles opening, focusing, and closing dialogs.
pub fn execute_dialog_command<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
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
