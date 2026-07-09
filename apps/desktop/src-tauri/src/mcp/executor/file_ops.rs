//! File operation and selection tool handlers.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{
    AckSignal, DEFAULT_ACK_TIMEOUT, PaneStateStore, ToolError, ToolResult, mcp_round_trip, snapshot_generation,
    wait_for_ack,
};
use crate::pluralize::pluralize;

/// Pre-checks that a copy/move/delete has something to act on, so an empty operation
/// fails fast with the real cause instead of the generic 1500 ms "frontend may be
/// stalled" ack timeout.
///
/// Mirrors the FE fallback semantics (`file-operation-commands.ts`): a selection wins;
/// with no selection the operation falls back to the cursor file — except when the
/// cursor is on the parent entry (`..`), which the FE silently skips (the dialog never
/// opens, so the ack would time out). When the cursor is outside the loaded window we
/// can't inspect the entry, so we let the FE decide (it has the full listing).
fn check_operation_has_target<R: Runtime>(app: &AppHandle<R>, verb: &str) -> Result<(), ToolError> {
    let Some(store) = app.try_state::<PaneStateStore>() else {
        return Ok(()); // No state synced yet; the FE is the authority.
    };
    let pane = store.get_focused_pane();
    let state = match pane.as_str() {
        "right" => store.get_right(),
        _ => store.get_left(),
    };
    match empty_operation_error(&state, &pane, verb) {
        Some(message) => Err(ToolError::invalid_params(message)),
        None => Ok(()),
    }
}

/// Pure decision core of `check_operation_has_target`. Returns the rejection message
/// for an operation that can't possibly have a target, or `None` to proceed.
pub(super) fn empty_operation_error(
    state: &crate::mcp::pane_state::PaneState,
    pane: &str,
    verb: &str,
) -> Option<String> {
    if state.path.is_empty() {
        return None; // No state push has landed yet; the FE is the authority.
    }
    if !state.selected_indices.is_empty() {
        return None;
    }
    let window_index = state.cursor_index.checked_sub(state.loaded_start);
    if let Some(entry) = window_index.and_then(|i| state.files.get(i))
        && entry.name == ".."
    {
        return Some(format!(
            "Nothing to {verb}: no files are selected in the {pane} pane and the cursor is on the parent entry (..). Use select or move_cursor first."
        ));
    }
    // An empty folder renders no file rows at all (the FE shows an empty-state overlay
    // instead, and skips even the `..` row), so the push carries zero files while
    // `total_files` still counts the parent entry. Nothing is actionable either way.
    if state.files.is_empty() && state.total_files <= 1 {
        return Some(format!(
            "Nothing to {verb}: the {pane} pane shows no files and nothing is selected."
        ));
    }
    None
}

/// Execute copy command.
///
/// Ack contract:
/// - `autoConfirm: true` → pane generation must advance (selection/state push after copy starts).
/// - `autoConfirm: false` → `transfer-confirmation` soft dialog must appear.
///
/// `check_operation_has_target` fast-fails the cases the FE would silently drop
/// (cursor on `..` with nothing selected); everything else is the FE's call.
pub async fn execute_copy<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    check_operation_has_target(app, "copy")?;
    // `autoConfirm: true` skips the user's confirmation dialog. This is safe because the
    // POST-handler boundary gates exactly this case: `tool_call_requires_token` flags
    // destructive auto-confirm (and the `dialog` confirm action), so any caller that reaches
    // here already proved filesystem access by reading the 0o600 `<data_dir>/mcp.token`.
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str()).unwrap_or("skip_all");

    if auto_confirm && !["skip_all", "overwrite_all", "rename_all"].contains(&on_conflict) {
        return Err(ToolError::invalid_params(
            "onConflict must be 'skip_all', 'overwrite_all', or 'rename_all'",
        ));
    }

    let pre_gen = snapshot_generation(app);
    app.emit(
        "mcp-copy",
        json!({"autoConfirm": auto_confirm, "onConflict": on_conflict}),
    )?;

    if auto_confirm {
        wait_for_ack(
            app,
            AckSignal::GenerationAdvanced { from: pre_gen },
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Copy started with auto-confirm."))
    } else {
        wait_for_ack(
            app,
            AckSignal::SoftDialogAppeared("transfer-confirmation"),
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Copy dialog opened. Waiting for user confirmation."))
    }
}

/// Execute move command.
///
/// Ack contract: same as `copy` (transfer-confirmation dialog shape), including the
/// `check_operation_has_target` fast-fail.
pub async fn execute_move<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    check_operation_has_target(app, "move")?;
    // `autoConfirm: true` skips the user's confirmation dialog; the POST-handler token gate
    // (`tool_call_requires_token` in `mcp/server.rs`) is what protects this now — it flags
    // destructive auto-confirm (and the `dialog` confirm action), not the whole server.
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str()).unwrap_or("skip_all");

    if auto_confirm && !["skip_all", "overwrite_all", "rename_all"].contains(&on_conflict) {
        return Err(ToolError::invalid_params(
            "onConflict must be 'skip_all', 'overwrite_all', or 'rename_all'",
        ));
    }

    let pre_gen = snapshot_generation(app);
    app.emit(
        "mcp-move",
        json!({"autoConfirm": auto_confirm, "onConflict": on_conflict}),
    )?;

    if auto_confirm {
        wait_for_ack(
            app,
            AckSignal::GenerationAdvanced { from: pre_gen },
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Move started with auto-confirm."))
    } else {
        wait_for_ack(
            app,
            AckSignal::SoftDialogAppeared("transfer-confirmation"),
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Move dialog opened. Waiting for user confirmation."))
    }
}

/// Execute compress command.
///
/// Opens the SAME transfer dialog as copy/move (in compress mode), packing the
/// cursor item or selection into a new zip at the other pane's path.
///
/// Ack contract:
/// - `autoConfirm: false` → the `transfer-confirmation` soft dialog must appear.
/// - `autoConfirm: true` → EITHER the pane generation advances (the compress
///   started) OR the `transfer-confirmation` dialog appears. The dialog branch is
///   load-bearing: when the target archive already exists, compress mode
///   deliberately aborts the auto-dispatch and keeps the dialog open rather than
///   silently overwriting (never advancing the generation). Waiting on
///   `GenerationAdvanced` alone would hang until timeout in exactly that case.
pub async fn execute_compress<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    check_operation_has_target(app, "compress")?;
    // `autoConfirm: true` skips the user's confirmation dialog; the POST-handler token
    // gate (`tool_call_requires_token` in `mcp/server.rs`) protects this case.
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str()).unwrap_or("skip_all");

    if auto_confirm && !["skip_all", "overwrite_all", "rename_all"].contains(&on_conflict) {
        return Err(ToolError::invalid_params(
            "onConflict must be 'skip_all', 'overwrite_all', or 'rename_all'",
        ));
    }

    let pre_gen = snapshot_generation(app);
    app.emit(
        "mcp-compress",
        json!({"autoConfirm": auto_confirm, "onConflict": on_conflict}),
    )?;

    if auto_confirm {
        wait_for_ack(
            app,
            AckSignal::GenerationAdvancedOrSoftDialog {
                from: pre_gen,
                dialog: "transfer-confirmation",
            },
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!(
            "OK: Compress started with auto-confirm (or the confirmation dialog opened because the target archive already exists)."
        ))
    } else {
        wait_for_ack(
            app,
            AckSignal::SoftDialogAppeared("transfer-confirmation"),
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Compress dialog opened. Waiting for user confirmation."))
    }
}

/// Execute delete command.
///
/// Ack contract:
/// - `autoConfirm: true` → pane generation must advance.
/// - `autoConfirm: false` → `delete-confirmation` soft dialog must appear.
///
/// `check_operation_has_target` fast-fails the cases the FE would silently drop.
pub async fn execute_delete<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    check_operation_has_target(app, "delete")?;
    // `autoConfirm: true` skips the user's confirmation dialog; the POST-handler token gate
    // (`tool_call_requires_token` in `mcp/server.rs`) is what protects this now — it flags
    // destructive auto-confirm (and the `dialog` confirm action), not the whole server.
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);

    let pre_gen = snapshot_generation(app);
    app.emit("mcp-delete", json!({"autoConfirm": auto_confirm}))?;

    if auto_confirm {
        wait_for_ack(
            app,
            AckSignal::GenerationAdvanced { from: pre_gen },
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Delete started with auto-confirm."))
    } else {
        wait_for_ack(
            app,
            AckSignal::SoftDialogAppeared("delete-confirmation"),
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Delete dialog opened. Waiting for user confirmation."))
    }
}

/// Execute mkdir command. Ack: `mkdir-confirmation` soft dialog appears.
///
/// Note: We cannot validate whether the current directory is writable because
/// the current directory path is managed by the frontend. The validation happens
/// when the actual mkdir operation is attempted, which will return an appropriate
/// error if the directory is not writable.
pub async fn execute_mkdir<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkdir", ())?;
    wait_for_ack(
        app,
        AckSignal::SoftDialogAppeared("mkdir-confirmation"),
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;
    Ok(json!("OK: Create folder dialog opened."))
}

/// Execute mkfile command. Ack: `new-file-confirmation` soft dialog appears.
pub async fn execute_mkfile<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    app.emit("mcp-mkfile", ())?;
    wait_for_ack(
        app,
        AckSignal::SoftDialogAppeared("new-file-confirmation"),
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;
    Ok(json!("OK: Create file dialog opened."))
}

/// Execute refresh command.
///
/// A round-trip: the FE forces a backend re-read of the focused pane's listing
/// (the `refresh_listing` IPC — local volumes always re-read; watcher-backed
/// MTP/SMB listings short-circuit, their caches are kept fresh by `notify_mutation`)
/// and replies once it completes. `OK` means "the directory was actually re-read",
/// not "an event was dispatched".
pub async fn execute_refresh<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    mcp_round_trip(app, "mcp-refresh", json!({}), "OK: Pane re-read from disk".to_string()).await
}

/// Execute the unified select command. Ack: pane generation advances (the new
/// selection is pushed via the next `update_*_pane_state`).
/// Emits event to frontend to manipulate file selection.
pub async fn execute_select_command<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }

    // By-name selection: a round-trip — the FE reports names that aren't in the
    // listing back as the error.
    if let Some(names_value) = params.get("names") {
        let names: Vec<&str> = names_value
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        if names.is_empty() || names.len() != names_value.as_array().map(|a| a.len()).unwrap_or(0) {
            return Err(ToolError::invalid_params(
                "'names' must be a non-empty array of strings",
            ));
        }
        if params.get("all").is_some() || params.get("count").is_some() || params.get("start").is_some() {
            return Err(ToolError::invalid_params(
                "Provide either 'names' or 'all'/'count'/'start', not both",
            ));
        }
        let mode = params.get("mode").and_then(|v| v.as_str()).unwrap_or("replace");
        if !["replace", "add", "subtract"].contains(&mode) {
            return Err(ToolError::invalid_params(
                "mode must be 'replace', 'add', or 'subtract'",
            ));
        }
        if let Some(store) = app.try_state::<PaneStateStore>() {
            store.set_focused_pane(pane.to_string());
        }
        let summary = pluralize(names.len() as u64, "file");
        return mcp_round_trip(
            app,
            "mcp-select-names",
            json!({"pane": pane, "names": names, "mode": mode}),
            format!("OK: Selection updated ({mode}, {summary} by name) in {pane} pane"),
        )
        .await;
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

    // Index modes are a round-trip too: the FE applies the selection, flushes the
    // MCP state push, and replies — so `OK` means the backend's PaneStateStore
    // already holds the new selection (a follow-up `copy` reads fresh state), and
    // an unrelated pane push can't false-positive the way `GenerationAdvanced` could.
    mcp_round_trip(
        app,
        "mcp-select",
        json!({"pane": pane, "start": start, "count": count, "mode": mode}),
        format!("OK: Selection updated in {pane} pane"),
    )
    .await
}
