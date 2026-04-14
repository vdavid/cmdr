//! File operation and selection tool handlers.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{PaneStateStore, ToolError, ToolResult};

/// Execute copy command.
///
/// Note: We cannot validate whether files are selected because selection state
/// is managed by the frontend. The validation happens in the frontend event handler
/// which will show an appropriate error if no files are selected.
pub fn execute_copy<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
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
pub fn execute_move<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
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
pub fn execute_delete<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
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
pub fn execute_mkdir<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkdir", ())?;
    Ok(json!("OK: Create folder dialog opened."))
}

/// Execute mkfile command.
pub fn execute_mkfile<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkfile", ())?;
    Ok(json!("OK: Create file dialog opened."))
}

/// Execute refresh command.
pub fn execute_refresh<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-refresh", ())?;
    Ok(json!("OK: Pane refreshed"))
}

/// Execute the unified select command.
/// Emits event to frontend to manipulate file selection.
pub fn execute_select_command<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
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
