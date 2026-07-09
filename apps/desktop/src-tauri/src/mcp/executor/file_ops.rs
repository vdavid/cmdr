//! File operation and selection tool handlers.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{
    AckSignal, DEFAULT_ACK_TIMEOUT, PaneStateStore, ToolError, ToolResult, mcp_await_operation_start, mcp_round_trip,
    wait_for_ack,
};
use crate::pluralize::pluralize;

/// Budget for an auto-confirmed file op to report its spawned `operationId`. More
/// generous than the 1500 ms ack budget because the round-trip spans the whole
/// FE flow (open the dialog → auto-confirm → the write-op IPC that mints the id),
/// which starts a scan preview on the way.
const OPERATION_START_TIMEOUT: u64 = 10;

/// Format the OK line for an auto-confirmed op, appending the spawned
/// `operationId` so a follow-up `queue` / `await operation_complete` can target
/// it. `None` (the compress-on-existing-target case that keeps its dialog open)
/// falls back to the bare confirmation.
fn operation_started_ok(verb: &str, operation_id: Option<String>) -> Value {
    match operation_id {
        Some(id) => json!(format!("OK: {verb} started with auto-confirm (operationId: {id}).")),
        None => json!(format!("OK: {verb} started with auto-confirm.")),
    }
}

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
/// - `autoConfirm: true` → round-trip: wait for the FE to reply with the spawned
///   `operationId` (or an error), returned in the OK text so a follow-up `queue`
///   / `await operation_complete` can target it.
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

    if auto_confirm {
        // Round-trip: the FE replies with the spawned operationId (or acks without
        // one). Replaces the generation ack so the tool returns the exact id.
        let operation_id = mcp_await_operation_start(
            app,
            "mcp-copy",
            json!({"autoConfirm": true, "onConflict": on_conflict}),
            OPERATION_START_TIMEOUT,
        )
        .await?;
        Ok(operation_started_ok("Copy", operation_id))
    } else {
        app.emit("mcp-copy", json!({"autoConfirm": false, "onConflict": on_conflict}))?;
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

    if auto_confirm {
        let operation_id = mcp_await_operation_start(
            app,
            "mcp-move",
            json!({"autoConfirm": true, "onConflict": on_conflict}),
            OPERATION_START_TIMEOUT,
        )
        .await?;
        Ok(operation_started_ok("Move", operation_id))
    } else {
        app.emit("mcp-move", json!({"autoConfirm": false, "onConflict": on_conflict}))?;
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
    // No `onConflict` param, unlike copy/move: compress creates ONE new archive, so
    // there are no inner-file conflicts to resolve, and an existing TARGET archive is
    // the dialog's overwrite affordance, not a policy. A param here would imply
    // behavior the backend doesn't have.
    if auto_confirm {
        // The FE replies with the spawned operationId, OR acks without one when the
        // target archive already exists (compress mode keeps its dialog open rather
        // than silently overwriting). `operation_started_ok(None)` covers that arm.
        let operation_id = mcp_await_operation_start(
            app,
            "mcp-compress",
            json!({"autoConfirm": true}),
            OPERATION_START_TIMEOUT,
        )
        .await?;
        match operation_id {
            Some(id) => Ok(json!(format!(
                "OK: Compress started with auto-confirm (operationId: {id})."
            ))),
            None => Ok(json!(
                "OK: The confirmation dialog opened because the target archive already exists; confirm it to overwrite."
            )),
        }
    } else {
        app.emit("mcp-compress", json!({"autoConfirm": false}))?;
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
/// The optional `mode` (`trash` | `delete`) presets the trash/permanent choice.
/// Omitted, the FE applies its per-volume default (trash where supported, forced
/// permanent on volumes without a trash and inside archives). `permanent` only
/// rides the event when `mode` is given, so the FE default stays the single
/// source of the volume clamp (`no-string-matching`: a typed bool crosses IPC).
///
/// Ack contract:
/// - `autoConfirm: true` → round-trip returning the spawned `operationId`; the FE
///   routes to `trash_files` vs `delete_files` by the effective permanent flag.
/// - `autoConfirm: false` → `delete-confirmation` soft dialog appears, its toggle
///   preset to `mode`.
///
/// `check_operation_has_target` fast-fails the cases the FE would silently drop.
pub async fn execute_delete<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    check_operation_has_target(app, "delete")?;
    // `autoConfirm: true` skips the user's confirmation dialog; the POST-handler token gate
    // (`tool_call_requires_token` in `mcp/server.rs`) is what protects this now — it flags
    // destructive auto-confirm (and the `dialog` confirm action), not the whole server.
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);
    let permanent = delete_permanent_from_mode(params)?;

    if auto_confirm {
        let mut payload = json!({"autoConfirm": true});
        if let Some(p) = permanent {
            payload["permanent"] = json!(p);
        }
        let operation_id = mcp_await_operation_start(app, "mcp-delete", payload, OPERATION_START_TIMEOUT).await?;
        Ok(operation_started_ok("Delete", operation_id))
    } else {
        let mut payload = json!({"autoConfirm": false});
        if let Some(p) = permanent {
            payload["permanent"] = json!(p);
        }
        app.emit("mcp-delete", payload)?;
        wait_for_ack(
            app,
            AckSignal::SoftDialogAppeared("delete-confirmation"),
            DEFAULT_ACK_TIMEOUT,
        )
        .await?;
        Ok(json!("OK: Delete dialog opened. Waiting for user confirmation."))
    }
}

/// Map the `delete` tool's `mode` param to the FE's `permanent` flag: `trash` →
/// keep (false), `delete` → permanent (true), absent → `None` (FE volume default).
fn delete_permanent_from_mode(params: &Value) -> Result<Option<bool>, ToolError> {
    match params.get("mode").and_then(|v| v.as_str()) {
        None => Ok(None),
        Some("trash") => Ok(Some(false)),
        Some("delete") => Ok(Some(true)),
        Some(other) => Err(ToolError::invalid_params(format!(
            "mode must be 'trash' or 'delete' (got '{other}')"
        ))),
    }
}

/// Execute rename command.
///
/// - `autoConfirm: false` → a round-trip: the FE moves the cursor to the target
///   row and starts the inline rename editor prefilled with `newName` for the
///   user to review (the human-review affordance). Acks once the editor is up.
/// - `autoConfirm: true` (token-gated) → calls the `rename_file` backend directly
///   with `force: false`; the managed op notifies the listing cache, so the pane
///   refreshes on success. Honest errors: name not in the listing, or the target
///   already exists.
///
/// The target is the named item (`name`), else the item under the cursor.
pub async fn execute_rename<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let new_name = params
        .get("newName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'newName' parameter"))?
        .to_string();
    if new_name.trim().is_empty() {
        return Err(ToolError::invalid_params("'newName' must not be empty"));
    }
    if new_name.contains('/') {
        return Err(ToolError::invalid_params(
            "'newName' is a name, not a path — it must not contain '/'",
        ));
    }

    let (pane, _state) = super::target_pane_state(app, params)?;
    let name_param = params.get("name").and_then(|v| v.as_str());
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);

    if auto_confirm {
        // Move the cursor to the target first: `move_cursor`'s FE round-trip flushes
        // the MCP state push (`syncStateToMcpNow`), so the resolution below reads a
        // fresh pane state even right after a nav (the `select` / `move_cursor`
        // precedent). Without a `name` we rename the current cursor item and trust
        // the agent positioned it (that move already flushed).
        if let Some(name) = name_param {
            mcp_round_trip(
                app,
                "mcp-move-cursor",
                json!({"pane": pane, "to": name}),
                "ok".to_string(),
            )
            .await?;
        }
        let (_pane, state) = super::target_pane_state(app, params)?;
        let (current_name, from_path) = resolve_rename_target(&state, name_param)?;
        if current_name == new_name {
            return Err(ToolError::invalid_params(format!(
                "'{new_name}' is already the item's name"
            )));
        }
        let parent = std::path::Path::new(&from_path)
            .parent()
            .ok_or_else(|| ToolError::internal(format!("Couldn't derive the parent of {from_path}")))?;
        let to_path = parent.join(&new_name).to_string_lossy().into_owned();
        let volume_id = state.volume_id.clone();
        crate::commands::rename::rename_file(from_path, to_path, false, volume_id)
            .await
            .map_err(|e| ToolError::internal(e.message))?;
        Ok(json!(format!("OK: Renamed to {new_name}.")))
    } else {
        // Resolution lives in the FE (it holds the live listing): the mcp-rename
        // handler moves the cursor to `name` (erroring honestly if it's not in the
        // listing) and starts the editor prefilled with `newName`.
        let mut payload = json!({"pane": pane, "newName": new_name});
        if let Some(name) = name_param {
            payload["name"] = json!(name);
        }
        mcp_round_trip(
            app,
            "mcp-rename",
            payload,
            format!("OK: Rename editor opened, prefilled with {new_name}. Waiting for the user to confirm."),
        )
        .await
    }
}

/// Resolve the single rename target to its `(current_name, path)`: the named item
/// (`name`), else the item under the cursor. Off the pane state (freshly flushed
/// by the caller's cursor move), so a name not in the listing, or a cursor outside
/// the loaded window / on `..`, is an honest error.
fn resolve_rename_target(
    state: &crate::mcp::pane_state::PaneState,
    name: Option<&str>,
) -> Result<(String, String), ToolError> {
    if let Some(name) = name {
        let entry = state
            .files
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| ToolError::invalid_params(format!("'{name}' isn't in the listing")))?;
        return Ok((entry.name.clone(), entry.path.clone()));
    }
    let entry = state
        .cursor_index
        .checked_sub(state.loaded_start)
        .and_then(|i| state.files.get(i))
        .ok_or_else(|| ToolError::invalid_params("the cursor isn't on a listed item; pass 'name'"))?;
    if entry.name == ".." {
        return Err(ToolError::invalid_params(
            "the cursor is on the parent entry (..); pass 'name'",
        ));
    }
    Ok((entry.name.clone(), entry.path.clone()))
}

/// Execute mkdir command.
///
/// - No `name` → opens the naming dialog (unchanged).
/// - `name` only → opens the dialog prefilled with the name.
/// - `name` + `autoConfirm` → creates the folder directly on the focused pane
///   (`create_directory`), erroring honestly on a name conflict.
///
/// Ack for the dialog paths: `mkdir-confirmation` soft dialog appears.
pub async fn execute_mkdir<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    execute_create(app, params, "mcp-mkdir", "mkdir-confirmation", CreateKind::Directory).await
}

/// Execute mkfile command. Same `name` / `autoConfirm` shape as [`execute_mkdir`];
/// ack for the dialog paths: `new-file-confirmation` soft dialog appears.
pub async fn execute_mkfile<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    execute_create(app, params, "mcp-mkfile", "new-file-confirmation", CreateKind::File).await
}

#[derive(Clone, Copy)]
enum CreateKind {
    Directory,
    File,
}

/// Shared body for `mkdir` / `mkfile`.
///
/// The direct-create path (`autoConfirm`) is a round-trip: the FE creates using
/// its LIVE focused-pane path + volumeId (never a backend `PaneStateStore` read,
/// which lags a nav by the debounced sync — reading it could create in the pane's
/// previous directory), then replies OK or an honest conflict error. The dialog
/// path emits the create event (prefilled when `name` is given) and waits for the
/// naming dialog to mount.
async fn execute_create<R: Runtime>(
    app: &AppHandle<R>,
    params: &Value,
    event: &str,
    ack_dialog: &'static str,
    kind: CreateKind,
) -> ToolResult {
    let name = params.get("name").and_then(|v| v.as_str()).map(str::to_string);
    let auto_confirm = params.get("autoConfirm").and_then(|v| v.as_bool()).unwrap_or(false);

    if auto_confirm {
        let name = name.ok_or_else(|| ToolError::invalid_params("autoConfirm requires a 'name'"))?;
        if name.trim().is_empty() {
            return Err(ToolError::invalid_params("'name' must not be empty"));
        }
        if name.contains('/') {
            return Err(ToolError::invalid_params(
                "'name' is a name, not a path — it must not contain '/'",
            ));
        }
        let noun = match kind {
            CreateKind::Directory => "folder",
            CreateKind::File => "file",
        };
        return mcp_round_trip(
            app,
            event,
            json!({ "name": name, "autoConfirm": true }),
            format!("OK: Created {noun} {name}."),
        )
        .await;
    }

    // Dialog path: prefill the name when given, else open with the FE's default.
    let payload = match name {
        Some(name) => json!({ "name": name }),
        None => json!({}),
    };
    app.emit(event, payload)?;
    wait_for_ack(app, AckSignal::SoftDialogAppeared(ack_dialog), DEFAULT_ACK_TIMEOUT).await?;
    let what = match kind {
        CreateKind::Directory => "Create folder",
        CreateKind::File => "Create file",
    };
    Ok(json!(format!("OK: {what} dialog opened.")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_mode_maps_to_permanent_flag() {
        // Omitted → None (the FE applies its per-volume default).
        assert_eq!(delete_permanent_from_mode(&json!({})).unwrap(), None);
        // trash → keep (false); delete → permanent (true).
        assert_eq!(
            delete_permanent_from_mode(&json!({"mode": "trash"})).unwrap(),
            Some(false)
        );
        assert_eq!(
            delete_permanent_from_mode(&json!({"mode": "delete"})).unwrap(),
            Some(true)
        );
        // An unknown mode is an honest error, not a silent default.
        assert!(delete_permanent_from_mode(&json!({"mode": "bin"})).is_err());
    }
}
