//! File operation and selection tool handlers.

use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::dialog_confirm::RENAME_EDITOR;
use super::{
    AckSignal, DEFAULT_ACK_TIMEOUT, PaneStateStore, ToolError, ToolResult, mcp_open_dialog, mcp_round_trip,
    refuse_while_dialog_blocks, required_pane_param, target_pane_state, validate_conflict_policy, wait_for_ack,
};
use crate::pluralize::pluralize;

const TRANSFER_DIALOG: &str = "transfer-confirmation";
const DELETE_DIALOG: &str = "delete-confirmation";

/// Budget for the frontend to answer that a dialog opened. It has to outlast the source rows
/// settling (`ROW_SETTLE_BUDGET_MS`, 3 s), and the selection read on top of that.
const DIALOG_OPEN_TIMEOUT: u64 = 10;

/// What a tool that opened a dialog or editor answers: nothing has happened yet, a person decides,
/// and how to find out what they did. The rail reads `awaitingPerson`.
pub(super) fn awaiting_person(dialog: &str) -> Value {
    json!({
        "awaitingPerson": true,
        "dialog": dialog,
        "next": format!(
            "Nothing has happened yet: it's waiting for a person, who may cancel it. While `{dialog}` is listed \
             under dialogs in cmdr://state, they haven't answered. Once it's gone, check cmdr://state operations \
             and the pane's listing for what came of it."
        ),
    })
}

/// Asks the frontend to open a dialog on `pane`, waits for it to say so, and for the tracker to
/// show it, so the next tool sees a dialog to wait for.
pub(super) async fn open_dialog<R: Runtime>(
    app: &AppHandle<R>,
    verb: &str,
    event: &str,
    payload: Value,
    dialog: &'static str,
) -> ToolResult {
    mcp_open_dialog(app, verb, event, payload, DIALOG_OPEN_TIMEOUT).await?;
    wait_for_ack(app, AckSignal::SoftDialogAppeared(dialog), DEFAULT_ACK_TIMEOUT).await?;
    Ok(awaiting_person(dialog))
}

/// The named pane and its state, freshly pushed: the selection a person made a moment ago, or the
/// listing a `nav_to_path` just landed on, may still be on its way here.
async fn fresh_pane_state<R: Runtime>(
    app: &AppHandle<R>,
    params: &Value,
) -> Result<(String, crate::ui_state::pane_state::PaneState), ToolError> {
    let pane = required_pane_param(params)?;
    super::flush_pane_state(app, pane).await?;
    target_pane_state(app, params)
}

/// Pre-checks that a delete or compress has something to act on, so an empty operation fails fast
/// with the real cause.
///
/// Mirrors the FE fallback semantics (`file-operation-commands.ts`): a selection wins; with no
/// selection the operation falls back to the cursor file — except when the cursor is on the parent
/// entry (`..`), which the FE silently skips. When the cursor is outside the loaded window we can't
/// inspect the entry, so we let the FE decide (it has the full listing).
async fn check_operation_has_target<R: Runtime>(
    app: &AppHandle<R>,
    params: &Value,
    verb: &str,
) -> Result<(), ToolError> {
    let (pane, state) = fresh_pane_state(app, params).await?;
    match empty_operation_error(&state, &pane, verb) {
        Some(message) => Err(ToolError::invalid_params(message)),
        None => Ok(()),
    }
}

/// Reads and validates `copy` and `move`'s `onConflict`. ❌ No default: with none named, the dialog
/// opens on the person's saved choice.
fn preset_on_conflict(params: &Value) -> Result<Option<&str>, ToolError> {
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str());
    if let Some(policy) = on_conflict {
        validate_conflict_policy(policy)?;
    }
    Ok(on_conflict)
}

/// The shared body of `copy` and `move`: open the transfer dialog on the named pane's selection.
async fn open_transfer_dialog<R: Runtime>(app: &AppHandle<R>, params: &Value, verb: &str, event: &str) -> ToolResult {
    refuse_while_dialog_blocks(app, verb)?;
    let on_conflict = preset_on_conflict(params)?;
    let (pane, state) = fresh_pane_state(app, params).await?;
    super::refuse_without_selection(&state, &pane, verb)?;
    let mut payload = json!({ "pane": pane });
    if let Some(policy) = on_conflict {
        payload["onConflict"] = policy.into();
    }
    open_dialog(app, verb, event, payload, TRANSFER_DIALOG).await
}

/// Pure decision core of `check_operation_has_target`. Returns the rejection message
/// for an operation that can't possibly have a target, or `None` to proceed.
pub(super) fn empty_operation_error(
    state: &crate::ui_state::pane_state::PaneState,
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
    // Subtract that parent row only where the pane HAS one: a search-results snapshot
    // and a pane at a volume root have none, so a single counted row there is a real
    // file, and refusing it would be a false "shows no files" over something the user
    // can see.
    let actionable_rows = state.total_files.saturating_sub(usize::from(state.has_parent_row));
    if state.files.is_empty() && actionable_rows == 0 {
        return Some(format!(
            "Nothing to {verb}: the {pane} pane shows no files and nothing is selected."
        ));
    }
    None
}

/// Execute copy command: opens the transfer dialog for a person to confirm.
///
/// Ack contract: a round trip the frontend answers once the dialog is up, or with why it isn't, then
/// the `transfer-confirmation` soft dialog must appear.
pub async fn execute_copy<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    open_transfer_dialog(app, params, "copy", "mcp-copy").await
}

/// Execute move command. Same shape as `copy`.
pub async fn execute_move<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    open_transfer_dialog(app, params, "move", "mcp-move").await
}

/// Execute compress command: opens the SAME transfer dialog as copy/move (in compress mode), packing
/// the named pane's cursor item or selection into a new zip at the other pane's path. Same ack contract
/// as `copy`.
pub async fn execute_compress<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    refuse_while_dialog_blocks(app, "compress")?;
    check_operation_has_target(app, params, "compress").await?;
    let pane = required_pane_param(params)?;
    // No `onConflict` param, unlike copy/move: compress creates ONE new archive, so
    // there are no inner-file conflicts to resolve, and an existing TARGET archive is
    // the dialog's overwrite affordance, not a policy. A param here would imply
    // behavior the backend doesn't have.
    open_dialog(
        app,
        "compress",
        "mcp-compress",
        json!({ "pane": pane }),
        TRANSFER_DIALOG,
    )
    .await
}

/// Execute delete command: opens the delete dialog for a person to confirm.
///
/// The optional `mode` (`trash` | `delete`) presets the trash/permanent choice.
/// Omitted, the FE applies its per-volume default (trash where supported, forced
/// permanent on volumes without a trash and inside archives). `permanent` only
/// rides the event when `mode` is given, so the FE default stays the single
/// source of the volume clamp (a typed bool crosses IPC).
///
/// Ack contract: as `copy`, on the `delete-confirmation` soft dialog.
pub async fn execute_delete<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    refuse_while_dialog_blocks(app, "delete")?;
    let permanent = delete_permanent_from_mode(params)?;
    check_operation_has_target(app, params, "delete").await?;
    let mut payload = json!({ "pane": required_pane_param(params)? });
    if let Some(p) = permanent {
        payload["permanent"] = json!(p);
    }
    open_dialog(app, "delete", "mcp-delete", payload, DELETE_DIALOG).await
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

/// Execute rename command: opens the inline editor prefilled with `newName`, for a person to confirm.
///
/// The target is the named item (`name`), else the item under the cursor. Moving the cursor to a
/// named item first flushes the pane state, so the resolution below reads it fresh, and the frontend
/// pins activation to the name resolved here. What a person's F2 refuses gets the same answer, typed:
/// a row the `.git` portal serves here, a read-only pane from the frontend.
pub async fn execute_rename<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    refuse_while_dialog_blocks(app, "rename").map_err(already_editing)?;
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

    let pane = required_pane_param(params)?;
    let name_param = params.get("name").and_then(|v| v.as_str());
    match name_param {
        Some(name) => {
            mcp_round_trip(
                app,
                "mcp-move-cursor",
                json!({"pane": pane, "to": name}),
                "ok".to_string(),
            )
            .await?;
        }
        None => super::flush_pane_state(app, pane).await?,
    }
    let (_pane, state) = target_pane_state(app, params)?;
    let (current_name, from_path) = resolve_rename_target(&state, name_param)?;
    let parent = std::path::Path::new(&from_path)
        .parent()
        .ok_or_else(|| ToolError::internal(format!("Couldn't derive the parent of {from_path}")))?;
    let to_path = parent.join(&new_name).to_string_lossy().into_owned();
    refuse_portal_rename(&from_path, &to_path)?;

    let payload = json!({ "pane": pane, "name": current_name, "newName": new_name });
    open_dialog(app, "rename", "mcp-rename", payload, RENAME_EDITOR).await
}

/// A second rename while the editor is up gets its own typed refusal beside the usual
/// `blockingDialog`: the first one is still waiting, and starting another would silently end it.
fn already_editing(mut refusal: ToolError) -> ToolError {
    let editing = refusal
        .data
        .as_ref()
        .and_then(|data| data.get("blockingDialog"))
        .is_some_and(|blocking| blocking == RENAME_EDITOR);
    if editing {
        refusal.data = Some(json!({ "blockingDialog": RENAME_EDITOR, "refusal": "alreadyEditing" }));
    }
    refusal
}

/// Refuses a rename a person's F2 refuses: one whose source or
/// target the `.git` portal serves. That's every row of a read-only portal pane
/// (`.git/worktrees/`), and a `.git/` listing's category rows, which stand in for
/// git's real `worktrees/` and `branches/` folders at the same path. The engine
/// refuses these too; this answers first, in the agent's terms.
pub(super) fn refuse_portal_rename(from: &str, to: &str) -> Result<(), ToolError> {
    use crate::file_system::git::wiring::portal_serves;

    if portal_serves(std::path::Path::new(from)) || portal_serves(std::path::Path::new(to)) {
        return Err(ToolError::invalid_params(format!(
            "Can't rename '{from}': it's in the read-only git portal (branches, tags, commits, stash, worktrees, and submodules are snapshots, and a person can't rename them either)"
        )));
    }
    Ok(())
}

/// Resolve the single rename target to its `(current_name, path)`: the named item
/// (`name`), else the item under the cursor. Off the pane state (freshly flushed
/// by the caller's cursor move), so a name not in the listing, or a cursor outside
/// the loaded window / on `..`, is an honest error.
fn resolve_rename_target(
    state: &crate::ui_state::pane_state::PaneState,
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
/// - `name` → creates the folder directly on the named pane (`create_directory`),
///   erroring honestly on a name conflict.
/// - No `name` → opens the naming dialog for a person, acked as `copy` is.
pub async fn execute_mkdir<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    execute_create(app, params, "mcp-mkdir", "mkdir-confirmation", CreateKind::Directory).await
}

/// Execute mkfile command. Same `name` shape as [`execute_mkdir`], on the
/// `new-file-confirmation` soft dialog.
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
/// The direct create is a round-trip: the FE creates using the target pane's LIVE path +
/// volumeId (never a backend `PaneStateStore` read, which lags a nav by the debounced
/// sync — reading it could create in the pane's previous directory), then replies OK or
/// an honest conflict error.
async fn execute_create<R: Runtime>(
    app: &AppHandle<R>,
    params: &Value,
    event: &str,
    naming_dialog: &'static str,
    kind: CreateKind,
) -> ToolResult {
    let (verb, noun) = match kind {
        CreateKind::Directory => ("create a folder", "folder"),
        CreateKind::File => ("create a file", "file"),
    };
    refuse_while_dialog_blocks(app, verb)?;
    let pane = required_pane_param(params)?;

    let Some(name) = params.get("name").and_then(|v| v.as_str()) else {
        return open_dialog(app, verb, event, json!({ "pane": pane }), naming_dialog).await;
    };
    if name.trim().is_empty() {
        return Err(ToolError::invalid_params("'name' must not be empty"));
    }
    if name.contains('/') {
        return Err(ToolError::invalid_params(
            "'name' is a name, not a path — it must not contain '/'",
        ));
    }
    let payload = json!({ "name": name, "pane": pane });
    mcp_round_trip(app, event, payload, format!("OK: Created {noun} {name}.")).await
}

/// Execute refresh command.
///
/// A round-trip: the FE forces a backend re-read of the focused pane's listing
/// (the `refresh_listing` IPC with `force: true`, so even a watcher-backed MTP or
/// SMB listing is re-read rather than answered out of the cache) and replies once
/// it completes. `OK` means "the directory was actually re-read", not "an event
/// was dispatched".
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
