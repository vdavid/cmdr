//! The `dialog_confirm` tool: the one way past a person, for an outside client in direct mode.
//!
//! Ack contract:
//! - `transfer-confirmation`, `delete-confirmation` (with or without `background`) → a round-trip
//!   the START answers: the FE threads the request id into the operation the press starts, which
//!   replies with its `operationId` (or its refusal); a press the dialog refused replies typed
//!   (`refusal`). Then the soft dialog is no longer in `SoftDialogTracker`: the FE takes it down in
//!   the same tick it starts the operation. ❌ Not a pane-generation wait: a compress, or a copy onto
//!   a slow volume, changes nothing in either pane until its first file lands.
//! - `rename`, `mkdir-confirmation`, `new-file-confirmation` → a round-trip the rename or create
//!   itself answers, when it ended or when a slow volume is still on it. Then the tracker loses the
//!   id, unless it's still running: that dialog stays up saying so.
//! - `quit-confirmation` → no ack: the answer goes to the quit gate, which starts a teardown that
//!   ends the process. See `quit.rs`.
//!
//! No such dialog open is an `invalid_params` up front, since `SoftDialogDisappeared` is true of a
//! dialog that was never there.

use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::{
    AckSignal, DEFAULT_ACK_TIMEOUT, OperationStartAck, ToolError, ToolResult, mcp_round_trip_parsed,
    operation_start_result, parse_operation_start_response, validate_conflict_policy, wait_for_ack,
};
use crate::ui_state::dialog_state::SoftDialogTracker;

/// The inline rename editor's id in `SoftDialogTracker`, and in the frontend's `dialog-registry.ts`.
pub(super) const RENAME_EDITOR: &str = "rename";

const TRANSFER: &str = "transfer-confirmation";
const DELETE: &str = "delete-confirmation";
const NEW_FOLDER: &str = "mkdir-confirmation";
const NEW_FILE: &str = "new-file-confirmation";

/// What a confirm led to, as the frontend answers it. The refusals mirror
/// `ProgrammaticConfirmVerdict` in `src/lib/file-explorer/pane/programmatic-confirm.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConfirmAck {
    /// The press reached a start, which answered: with the spawned `operationId`
    /// (`None` for a move that renamed in place), or a gate that refused it.
    Started(OperationStartAck),
    /// A rename or create ended, or a slow volume is still on it.
    Committed { still_running: bool },
    /// A background press on a delete dialog that would delete PERMANENTLY, which
    /// never starts out of sight. The dialog stays open, untouched.
    RefusedPermanentDelete,
    /// The dialog took the press and started nothing, by a guard it holds for a
    /// person too: a refused source, a cloud folder's walk turning the trash into
    /// a delete, a press already in flight. The dialog stays open.
    NotStarted,
    /// The name didn't stick: invalid, taken, or turned down by the volume. The editor or dialog
    /// stays open, and `reason` is the sentence it would show a person.
    NameRefused { reason: String },
    /// The rename copies too much to start unasked, so it moved on to the Move dialog.
    NeedsMoveDialog,
}

/// Parse an `mcp-response` for a confirm. Same `requestId` correlation as the other round-trips; a
/// refusal keeps its typed identity in `refusal`, ❌ never in the message text. Anything else reads
/// as an operation start's answer, so a missing `ok` is a failure.
fn parse_confirm_response(payload: &str, expected_id: &str) -> Option<Result<ConfirmAck, String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    match resp.get("refusal").and_then(|v| v.as_str()) {
        Some("permanentDelete") => return Some(Ok(ConfirmAck::RefusedPermanentDelete)),
        Some("notStarted") => return Some(Ok(ConfirmAck::NotStarted)),
        Some("nameRefused") => {
            let reason = resp.get("reason").and_then(|v| v.as_str()).unwrap_or_default();
            return Some(Ok(ConfirmAck::NameRefused {
                reason: reason.to_string(),
            }));
        }
        Some("needsMoveDialog") => return Some(Ok(ConfirmAck::NeedsMoveDialog)),
        _ => {}
    }
    if resp.get("committed").and_then(|v| v.as_bool()) == Some(true) {
        let still_running = resp.get("stillRunning").and_then(|v| v.as_bool()).unwrap_or(false);
        return Some(Ok(ConfirmAck::Committed { still_running }));
    }
    parse_operation_start_response(payload, expected_id).map(|ack| ack.map(ConfirmAck::Started))
}

/// Budget for a confirm to answer. Same as an auto-confirmed `copy`'s (`file_ops.rs`), and for the
/// same reason: it spans the dialog's wait for its scan preview to start and the write-op IPC that
/// mints the id. A cloud folder's trash that must wait on its walk first asks the round-trip to hold
/// (`holdMs`, see `mcp_round_trip_parsed`), so it still answers with its id. A rename or create
/// answers inside it too: past `MUTATION_REPLY_DEADLINE` the volume's call says it's still running.
const CONFIRM_TIMEOUT_SECS: u64 = 10;

/// Execute the `dialog_confirm` tool: confirm a dialog or the inline rename editor that's open.
pub async fn execute_dialog_confirm<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let dialog_type = params
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'type' parameter"))?;
    let dialog_type = match dialog_type {
        "copy-confirmation" => TRANSFER,
        other => other,
    };
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str());
    // Whatever the dialog: a bad policy refuses every time, rather than only where it would apply.
    if let Some(policy) = on_conflict {
        validate_conflict_policy(policy)?;
    }
    let background = params.get("background").and_then(|v| v.as_bool()).unwrap_or(false);
    let name = params.get("name").and_then(|v| v.as_str());

    if background && dialog_type != TRANSFER && dialog_type != DELETE {
        return Err(ToolError::invalid_params(format!(
            "Cannot confirm '{dialog_type}' in the background. Only '{TRANSFER}' and a trashing '{DELETE}' have a \
             background start."
        )));
    }
    let takes_a_name = matches!(dialog_type, RENAME_EDITOR | NEW_FOLDER | NEW_FILE);
    if name.is_some() && !takes_a_name {
        return Err(ToolError::invalid_params(format!(
            "'name' is for '{RENAME_EDITOR}', '{NEW_FOLDER}', and '{NEW_FILE}', so '{dialog_type}' can't take it."
        )));
    }

    match dialog_type {
        TRANSFER | DELETE => {
            let mut payload = json!({ "type": dialog_type });
            // ❌ No default here: with no policy named, the dialog keeps the one it shows, which may be
            // a person's remembered choice.
            if let Some(policy) = on_conflict.filter(|_| dialog_type == TRANSFER) {
                payload["onConflict"] = policy.into();
            }
            if background {
                payload["startInBackground"] = true.into();
            }
            confirm_and_await_start(app, dialog_type, payload, background).await
        }
        RENAME_EDITOR | NEW_FOLDER | NEW_FILE => commit_name(app, dialog_type, name).await,
        // Confirming the quit means quitting NOW, skipping the rest of the countdown.
        // Answered on the gate rather than the dialog, and with no ack: the process is on
        // its way out, so there is nothing left to observe. See `quit.rs`.
        "quit-confirmation" => super::quit::confirm_quit(),
        // Confirming this one means supplying a password, which needs a value
        // this tool has nowhere to put. `unlock` is the whole answer.
        "archive-password" => Err(ToolError::invalid_params(
            "The archive-password prompt is answered with the unlock tool, which takes the password. \
             Read cmdr://state dialogs for the archive or repo it's asking about; dialog close cancels it instead.",
        )),
        _ => Err(ToolError::invalid_params(format!(
            "Cannot confirm dialog type '{dialog_type}'. Only '{TRANSFER}', '{DELETE}', '{RENAME_EDITOR}', \
             '{NEW_FOLDER}', '{NEW_FILE}', and 'quit-confirmation' can be confirmed."
        ))),
    }
}

/// Refuses up front when `dialog_type` isn't open: `SoftDialogDisappeared` is true
/// of a dialog that was never there, so a confirm of nothing would otherwise ack.
fn require_open_dialog<R: Runtime>(app: &AppHandle<R>, dialog_type: &str) -> Result<(), ToolError> {
    let is_open = app
        .try_state::<SoftDialogTracker>()
        .is_some_and(|tracker| tracker.get_open_types().iter().any(|open| open == dialog_type));
    if is_open {
        Ok(())
    } else {
        Err(ToolError::invalid_params(format!(
            "No {dialog_type} dialog is open to confirm. Read cmdr://state dialogs for what is open."
        )))
    }
}

/// The round-trip every frontend confirm shares.
async fn press<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, payload: Value) -> Result<ConfirmAck, ToolError> {
    require_open_dialog(app, dialog_type)?;
    mcp_round_trip_parsed(
        app,
        "mcp-confirm-dialog",
        payload,
        CONFIRM_TIMEOUT_SECS,
        parse_confirm_response,
    )
    .await
}

/// Word a press that went nowhere. `refusal` in the data is what a caller branches on.
fn refusal_error(dialog_type: &str, ack: &ConfirmAck) -> ToolError {
    let (message, refusal) = match ack {
        ConfirmAck::RefusedPermanentDelete => (
            "The delete dialog would delete permanently, which never runs in the background. Confirm it without \
             background, switch it to trash, or close it."
                .to_string(),
            "permanentDelete",
        ),
        ConfirmAck::NameRefused { reason } => (
            format!("That name didn't stick, and the {dialog_type} dialog is still open: {reason}"),
            "nameRefused",
        ),
        ConfirmAck::NeedsMoveDialog => (
            format!(
                "This rename copies too much to start unasked, so it moved on to the Move dialog. Confirm \
                 {TRANSFER} or close it."
            ),
            "needsMoveDialog",
        ),
        ConfirmAck::NotStarted | ConfirmAck::Started(_) | ConfirmAck::Committed { .. } => (
            format!(
                "The {dialog_type} dialog took the confirm but started nothing, and it's still open. Read \
                 cmdr://state dialogs for what it shows, then confirm again or close it."
            ),
            "notStarted",
        ),
    };
    ToolError::invalid_params(message).with_data(json!({ "refusal": refusal }))
}

/// Presses the open transfer or delete dialog's own confirm, or with `background` its Background
/// button (F2), and answers with the operation that press started, so the caller can
/// `await operation_complete` or `queue` it.
///
/// The frontend threads this request's id into the start, so the answer is the start's own: the
/// spawned `operationId` or the refusal, ❌ never a guess. A press the dialog refused answers at
/// once, typed. Then the confirmation goes away like any confirm's.
async fn confirm_and_await_start<R: Runtime>(
    app: &AppHandle<R>,
    dialog_type: &str,
    payload: Value,
    background: bool,
) -> ToolResult {
    let operation_id = match press(app, dialog_type, payload).await? {
        ConfirmAck::Started(start) => operation_start_result("confirm", start)?,
        other => return Err(refusal_error(dialog_type, &other)),
    };
    wait_for_ack(
        app,
        AckSignal::SoftDialogDisappeared(dialog_type.to_string()),
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;
    Ok(confirm_ok(dialog_type, background, operation_id))
}

/// The OK line for a confirm, naming the started `operationId`. `None` is a move that renamed in
/// place.
fn confirm_ok(dialog_type: &str, background: bool, operation_id: Option<String>) -> Value {
    let noun = if dialog_type == DELETE { "Delete" } else { "Transfer" };
    match (background, operation_id) {
        (false, Some(id)) => json!(format!("OK: {noun} dialog confirmed (operationId: {id}).")),
        (false, None) => json!(format!("OK: {noun} dialog confirmed.")),
        (true, Some(id)) => json!(format!(
            "OK: Started in the background (operationId: {id}). Await operation_complete on it, or find it in \
             cmdr://state operations."
        )),
        (true, None) => json!("OK: Confirmed. It needed no operation, so nothing runs in the background."),
    }
}

/// Commits the inline rename editor or a naming dialog, under `name` when there is one, else what the
/// field holds. The frontend answers when the rename or create ended.
async fn commit_name<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, name: Option<&str>) -> ToolResult {
    let mut payload = json!({ "type": dialog_type });
    if let Some(name) = name {
        if name.trim().is_empty() {
            return Err(ToolError::invalid_params("'name' must not be empty"));
        }
        if name.contains('/') {
            return Err(ToolError::invalid_params(
                "'name' is a name, not a path — it must not contain '/'",
            ));
        }
        payload["name"] = name.into();
    }
    match press(app, dialog_type, payload).await? {
        // The dialog stays up saying so, and goes when the volume answers.
        ConfirmAck::Committed { still_running: true } => Ok(json!(
            "OK: It's still running on a slow volume. It may still land, so check the pane's listing before retrying."
        )),
        ConfirmAck::Committed { still_running: false } => {
            wait_for_ack(
                app,
                AckSignal::SoftDialogDisappeared(dialog_type.to_string()),
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!(format!("OK: {dialog_type} confirmed.")))
        }
        other => Err(refusal_error(dialog_type, &other)),
    }
}

#[cfg(test)]
#[path = "dialog_confirm_test.rs"]
mod tests;
