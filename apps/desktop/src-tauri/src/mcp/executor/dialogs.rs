//! Dialog tool handlers.
//!
//! Ack contract:
//! - `open settings|file-viewer|about` → child window appears in `webview_windows()`.
//! - `open` confirmation dialogs → not allowed (use copy/move/delete/mkdir/mkfile instead).
//! - `close settings` → matching Tauri window disappears.
//! - `close file-viewer` → snapshot the viewer-window count, ack when it drops (so closing one of N
//!   viewers acks without waiting for all to vanish). Returns an `invalid_params` error fast-path
//!   if no viewers are open at all.
//! - `close about` → soft dialog `about` is no longer in `SoftDialogTracker` (`about` is an
//!   overlay, not a separate window).
//! - `close <confirmation>` → soft dialog is no longer in `SoftDialogTracker`. Cancel doesn't
//!   reliably bump pane generation, so we wait for the tracker entry to vanish.
//! - `close <any other registered soft dialog>` → the generic path: validate the id against the
//!   FE-registered known dialogs, emit one `mcp-close-dialog { id }`, and wait for the tracker to
//!   lose the id. The main window routes the id to the dialog's own close via the close registry
//!   (`ModalDialog` / `QueryDialog`). An unregistered id is an honest `invalid_params`, and an
//!   already-closed dialog acks immediately (the tracker doesn't hold it).
//! - `focus settings` → no ack: the backend raises the settings window itself, and a closed one is
//!   an `invalid_params` up front.
//! - `focus file-viewer|about` → window is present (no-op fast path; if the window isn't there, the
//!   wait_for_ack times out, which is the correct contract for focusing a non-existent dialog).
//! - `confirm <transfer|delete>` → the soft dialog is no longer in `SoftDialogTracker`: the FE takes
//!   the confirmation down in the same tick it starts the operation. ❌ Not a pane-generation wait:
//!   a compress, or a copy onto a slow volume, changes nothing in either pane until its first file
//!   lands, so that signal timed out on operations that had started. No such dialog open is an
//!   `invalid_params` up front, since "already gone" would otherwise ack at once.
//! - `confirm quit-confirmation` → no ack: the answer goes to the quit gate, which starts a
//!   teardown that ends the process. `close quit-confirmation` is the other answer ("keep
//!   working"); it waits for the soft dialog to go, but reports that as a typed
//!   `promptClosed` rather than failing, since the gate already took the answer. Both in
//!   `quit.rs`.
//! - `open_search_dialog` → soft dialog `search` appears in `SoftDialogTracker`. The frontend
//!   already calls `notifyDialogOpened('search')` from `SearchDialog.svelte::onMount`. If the
//!   dialog is mid-close when the event arrives, the new mount may race; the ack times out within
//!   the 1500 ms budget and the tool surfaces a clean failure. See plan §5.7 risk register.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_specta::Event as _;

use crate::window_events::{
    CloseAbout, CloseAllFileViewers, CloseConfirmation, CloseFileViewer, ExecuteCommand, FocusAbout, FocusConfirmation,
    FocusFileViewer, McpSettingsClose, OpenFileViewer, OpenSettings,
};

use super::{
    AckSignal, DEFAULT_ACK_TIMEOUT, ToolError, ToolResult, expand_user_path, mcp_round_trip_parsed,
    snapshot_window_count, validate_conflict_policy, validate_path_exists, wait_for_ack,
};
use crate::mcp::dialog_state::SoftDialogTracker;

/// Execute the unified dialog command.
/// Handles opening, focusing, and closing dialogs.
pub async fn execute_dialog_command<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
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
    let path = params.get("path").and_then(|v| v.as_str()).map(expand_user_path);
    let path = path.as_deref();
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str());
    let background = params.get("background").and_then(|v| v.as_bool()).unwrap_or(false);

    match action {
        "open" => execute_dialog_open(app, dialog_type, section, path).await,
        "focus" => execute_dialog_focus(app, dialog_type, path).await,
        "close" => execute_dialog_close(app, dialog_type, path).await,
        "confirm" if background => execute_dialog_confirm_in_background(app, dialog_type, on_conflict).await,
        "confirm" => execute_dialog_confirm(app, dialog_type, on_conflict).await,
        _ => Err(ToolError::invalid_params(format!("Invalid action: {action}"))),
    }
}

/// Execute dialog open action.
async fn execute_dialog_open<R: Runtime>(
    app: &AppHandle<R>,
    dialog_type: &str,
    section: Option<&str>,
    path: Option<&str>,
) -> ToolResult {
    // Window-based dialogs (settings, file-viewer) are tracked automatically
    // via webview_windows() in `resources/mod.rs`. No manual tracking needed here.

    match dialog_type {
        "settings" => {
            if let Some(section) = section {
                // Section-specific: MCP-only event handled by setupDialogListeners
                OpenSettings {
                    section: section.to_string(),
                }
                .emit_to(app, "main")?;
                wait_for_ack(app, AckSignal::WindowAppeared("settings"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!(format!("OK: Opened settings at {section}")))
            } else {
                ExecuteCommand {
                    command_id: "app.settings".to_string(),
                }
                .emit_to(app, "main")?;
                wait_for_ack(app, AckSignal::WindowAppeared("settings"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!("OK: Opened settings"))
            }
        }
        "file-viewer" => {
            // If path is provided, open for that file; otherwise, use cursor file
            if let Some(path) = path {
                // Timed, virtual-path-aware existence check (see executor/mod.rs)
                validate_path_exists(path).await?;
                OpenFileViewer {
                    path: Some(path.to_string()),
                }
                .emit_to(app, "main")?;
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!(format!("OK: Opened file viewer for {path}")))
            } else {
                // Open for file under cursor (validation happens in frontend)
                OpenFileViewer { path: None }.emit_to(app, "main")?;
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!("OK: Opened file viewer for cursor file"))
            }
        }
        "about" => {
            ExecuteCommand {
                command_id: "app.about".to_string(),
            }
            .emit_to(app, "main")?;
            // `about` is a soft dialog (ModalDialog overlay in the main window), not a
            // separate Tauri window. Track via SoftDialogTracker.
            wait_for_ack(app, AckSignal::SoftDialogAppeared("about"), DEFAULT_ACK_TIMEOUT).await?;
            Ok(json!("OK: Opened about dialog"))
        }
        "onboarding" => {
            // Re-entry path. Same command id the menu / palette use, so a single FE
            // handler covers all three surfaces. The wizard is a soft sheet (its own
            // `OnboardingWizard.svelte`, not a ModalDialog consumer), but it calls
            // `notifyDialogOpened('onboarding')` on mount, so SoftDialogTracker fires.
            ExecuteCommand {
                command_id: "cmdr.openOnboarding".to_string(),
            }
            .emit_to(app, "main")?;
            wait_for_ack(app, AckSignal::SoftDialogAppeared("onboarding"), DEFAULT_ACK_TIMEOUT).await?;
            Ok(json!("OK: Opened onboarding wizard"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            Err(ToolError::invalid_params(
                "Cannot open confirmation dialogs directly. Use copy, move, delete, mkdir, or mkfile tools instead.",
            ))
        }
        "quit-confirmation" => Err(ToolError::invalid_params(
            "Cannot open the quit confirmation directly. Call quit: the gate raises it only when operations are still running.",
        )),
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

/// Execute dialog focus action.
async fn execute_dialog_focus<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, path: Option<&str>) -> ToolResult {
    // Focus is a best-effort UI hint. We don't have a reliable "the window now has
    // focus" signal cross-platform, so we ack on the precondition: the target dialog
    // must currently exist. If it doesn't, the wait_for_ack times out with a clear
    // message; that's the correct contract (you can't focus what isn't there).
    match dialog_type {
        "settings" => {
            // Settings is its own window, so the backend raises it directly: no
            // frontend hop, and nothing that can listen or not.
            let window = app.get_webview_window("settings").ok_or_else(|| {
                ToolError::invalid_params("Settings isn't open. Open it first with `dialog open settings`.")
            })?;
            window
                .set_focus()
                .map_err(|e| ToolError::internal(format!("Couldn't focus the settings window: {e}")))?;
            Ok(json!("OK: Focused settings"))
        }
        "file-viewer" => {
            if let Some(path) = path {
                // Timed, virtual-path-aware existence check (see executor/mod.rs)
                validate_path_exists(path).await?;
                FocusFileViewer {
                    path: Some(path.to_string()),
                }
                .emit_to(app, "main")?;
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!(format!("OK: Focused file viewer for {path}")))
            } else {
                // Focus most recently opened file-viewer
                FocusFileViewer { path: None }.emit_to(app, "main")?;
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!("OK: Focused most recent file viewer"))
            }
        }
        "about" => {
            FocusAbout.emit_to(app, "main")?;
            wait_for_ack(app, AckSignal::SoftDialogAppeared("about"), DEFAULT_ACK_TIMEOUT).await?;
            Ok(json!("OK: Focused about dialog"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            FocusConfirmation.emit_to(app, "main")?;
            // Soft dialogs: the tracker is the source of truth.
            wait_for_ack(
                app,
                AckSignal::SoftDialogAppeared(soft_dialog_id(dialog_type)),
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!("OK: Focused confirmation dialog"))
        }
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

/// Execute dialog close action.
async fn execute_dialog_close<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, path: Option<&str>) -> ToolResult {
    // Window-based dialogs are closed via their window; soft dialogs are tracked
    // automatically by the frontend via notify_dialog_closed.

    match dialog_type {
        "settings" => {
            if app.webview_windows().contains_key("settings") {
                McpSettingsClose.emit_to(app, "settings")?;
                wait_for_ack(app, AckSignal::WindowDisappeared("settings"), DEFAULT_ACK_TIMEOUT).await?;
            }
            // If the settings window wasn't open to begin with, the close is a no-op
            // and we return OK without waiting: the desired end state is already true.
            Ok(json!("OK: Closed settings"))
        }
        "file-viewer" => {
            // Snapshot the viewer count first. If zero, fast-fail: there's nothing to
            // close, and waiting for a count drop would just time out at 1500 ms.
            let before = snapshot_window_count(app, "viewer");
            if before == 0 {
                return Err(ToolError::invalid_params("No file viewer windows are open."));
            }
            if let Some(path) = path {
                CloseFileViewer {
                    path: Some(path.to_string()),
                }
                .emit_to(app, "main")?;
                // Closing one of N viewers: ack when the count drops below `before`.
                // If the path doesn't match any open viewer, the count stays put and
                // we time out, which is the right contract (caller asked to close a
                // specific viewer that isn't there).
                wait_for_ack(
                    app,
                    AckSignal::WindowCountBelow {
                        prefix: "viewer",
                        threshold: before,
                    },
                    DEFAULT_ACK_TIMEOUT,
                )
                .await?;
                Ok(json!(format!("OK: Closed file viewer for {path}")))
            } else {
                CloseAllFileViewers.emit_to(app, "main")?;
                // Close-all: ack when zero viewers remain (`count < 1`).
                wait_for_ack(
                    app,
                    AckSignal::WindowCountBelow {
                        prefix: "viewer",
                        threshold: 1,
                    },
                    DEFAULT_ACK_TIMEOUT,
                )
                .await?;
                Ok(json!("OK: Closed all file viewer dialogs"))
            }
        }
        "about" => {
            // `about` is a soft dialog (overlay in the main window), tracked via
            // SoftDialogTracker (id: "about"). If it isn't open, the tracker doesn't
            // hold the id and `SoftDialogDisappeared` returns immediately, so close is
            // a fast no-op in that case, no timeout.
            CloseAbout.emit_to(app, "main")?;
            wait_for_ack(
                app,
                AckSignal::SoftDialogDisappeared("about".to_string()),
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!("OK: Closed about dialog"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            CloseConfirmation.emit_to(app, "main")?;
            // Soft confirmation dialogs unmount their `ModalDialog`, which fires
            // `notifyDialogClosed` and updates the `SoftDialogTracker`. Wait for the
            // tracker to lose the dialog ID. Cancel doesn't reliably bump generation
            // (that's what we used to wait for, and it broke on every cancel).
            wait_for_ack(
                app,
                AckSignal::SoftDialogDisappeared(soft_dialog_id(dialog_type).to_string()),
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!("OK: Cancelled confirmation dialog"))
        }
        // Closing the quit confirmation means "keep working", the same thing Escape and
        // the × mean in the UI. It answers the GATE, not the dialog: the backend owns the
        // decision to exit, and routing an agent's answer through the frontend would make
        // the webview the authority for it (`src-tauri/src/quit/CLAUDE.md`).
        "quit-confirmation" => {
            let mut reply = super::quit::keep_working()?;
            // The gate announced the call-off, so the prompt takes itself down; one that
            // was never up isn't tracked, so this returns at once rather than timing out.
            // ❌ Don't turn a timeout into a refusal: the gate has ALREADY taken the
            // answer and deleted the countdown, and a failure here would send the caller
            // back to `quit` over an answer that landed. A wedged webview costs the
            // typed `promptClosed: false`, not the outcome.
            let closed = wait_for_ack(
                app,
                AckSignal::SoftDialogDisappeared("quit-confirmation".to_string()),
                DEFAULT_ACK_TIMEOUT,
            )
            .await
            .is_ok();
            reply["promptClosed"] = closed.into();
            Ok(reply)
        }
        // Generic close for any OTHER registered soft dialog (whats-new, go-to-path,
        // search, feedback, drive-index-stale, …). Validate the id against the FE-
        // registered known dialogs, then emit the one generic `mcp-close-dialog` event;
        // the main window's router calls the dialog's own close via the close registry.
        // Ack on the tracker losing the id — a dialog that isn't open makes
        // `SoftDialogDisappeared` return immediately (a fast no-op OK), so closing an
        // already-closed dialog succeeds instead of timing out.
        other => execute_generic_dialog_close(app, other).await,
    }
}

/// Close a registered soft dialog by id via the generic `mcp-close-dialog` path.
/// Rejects an id the frontend never registered (an honest "unknown dialog" instead of
/// a silent 1500 ms ack timeout), pointing the caller at the discovery resource.
async fn execute_generic_dialog_close<R: Runtime>(app: &AppHandle<R>, dialog_type: &str) -> ToolResult {
    let is_known = app
        .try_state::<SoftDialogTracker>()
        .is_some_and(|tracker| is_registered_soft_dialog(&tracker.get_known_dialogs(), dialog_type));
    if !is_known {
        return Err(ToolError::invalid_params(format!(
            "Unknown dialog type '{dialog_type}'. Closable dialogs are listed in cmdr://dialogs/available."
        )));
    }
    app.emit("mcp-close-dialog", json!({ "id": dialog_type }))?;
    wait_for_ack(
        app,
        AckSignal::SoftDialogDisappeared(dialog_type.to_string()),
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;
    Ok(json!(format!("OK: Closed {dialog_type} dialog")))
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

/// Asks the frontend to confirm the open `dialog_type`, and waits for that dialog
/// to go away: a confirm the frontend acted on takes the dialog down in the same
/// tick it starts the operation, whatever the panes do afterwards.
///
/// The dialog has to be open FIRST. `SoftDialogDisappeared` is true of a dialog
/// that was never there, so without this check a confirm of nothing would ack.
async fn confirm_open_dialog<R: Runtime>(
    app: &AppHandle<R>,
    dialog_type: &str,
    payload: Value,
) -> Result<(), ToolError> {
    require_open_dialog(app, dialog_type)?;
    app.emit("mcp-confirm-dialog", payload)?;
    wait_for_ack(
        app,
        AckSignal::SoftDialogDisappeared(dialog_type.to_string()),
        DEFAULT_ACK_TIMEOUT,
    )
    .await
}

/// What the frontend says a background confirm's press did. Mirrors the
/// `ProgrammaticConfirmVerdict` in `src/lib/file-explorer/pane/programmatic-confirm.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BackgroundConfirmAck {
    /// The dialog pressed its Background button: the operation is starting with no
    /// progress dialog, and the confirmation is on its way down.
    Pressed,
    /// The delete dialog would delete PERMANENTLY, which never starts out of sight.
    /// The dialog stays open, untouched.
    RefusedPermanentDelete,
}

/// Parse an `mcp-response` for a background confirm. Same `requestId`
/// correlation as the other round-trips; a refusal keeps its typed identity in
/// `refusal`, ❌ never in the message text. A missing `ok` is a failure.
fn parse_background_confirm_response(payload: &str, expected_id: &str) -> Option<Result<BackgroundConfirmAck, String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    if resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Some(Ok(BackgroundConfirmAck::Pressed));
    }
    if resp.get("refusal").and_then(|v| v.as_str()) == Some("permanentDelete") {
        return Some(Ok(BackgroundConfirmAck::RefusedPermanentDelete));
    }
    let err = resp
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown error")
        .to_string();
    Some(Err(err))
}

/// `dialog confirm` with `background: true`: press the open dialog's Background
/// button (F2) instead of Confirm, so the operation starts with no progress
/// dialog. Transfer and trash only, under the dialog's own rules.
///
/// Two steps, because the second can't tell the first's outcomes apart: the
/// frontend first ANSWERS whether it pressed (a permanent delete refuses, and the
/// dialog stays up), then the confirmation goes away like any confirm's does.
async fn execute_dialog_confirm_in_background<R: Runtime>(
    app: &AppHandle<R>,
    dialog_type: &str,
    on_conflict: Option<&str>,
) -> ToolResult {
    let payload = match dialog_type {
        "transfer-confirmation" => {
            let conflict_policy = on_conflict.unwrap_or("skip_all");
            validate_conflict_policy(conflict_policy)?;
            json!({"type": dialog_type, "onConflict": conflict_policy, "startInBackground": true})
        }
        "delete-confirmation" => json!({"type": dialog_type, "startInBackground": true}),
        _ => {
            return Err(ToolError::invalid_params(format!(
                "Cannot confirm '{dialog_type}' in the background. Only 'transfer-confirmation' and a trashing \
                 'delete-confirmation' have a background start."
            )));
        }
    };
    require_open_dialog(app, dialog_type)?;
    let ack = mcp_round_trip_parsed(
        app,
        "mcp-confirm-dialog",
        payload,
        BACKGROUND_CONFIRM_TIMEOUT_SECS,
        parse_background_confirm_response,
    )
    .await?;
    match ack {
        BackgroundConfirmAck::RefusedPermanentDelete => Err(ToolError::invalid_params(
            "The delete dialog would delete permanently, which never runs in the background. Confirm it without \
             background, switch it to trash, or close it.",
        )
        .with_data(json!({ "refusal": "permanentDelete" }))),
        BackgroundConfirmAck::Pressed => {
            wait_for_ack(
                app,
                AckSignal::SoftDialogDisappeared(dialog_type.to_string()),
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!(
                "OK: Started in the background. Find it in cmdr://state operations, or await operation_complete."
            ))
        }
    }
}

/// The frontend answers a background press synchronously, so this only has to
/// outlast a busy main thread, like any other round-trip.
const BACKGROUND_CONFIRM_TIMEOUT_SECS: u64 = 5;

/// Execute dialog confirm action.
/// Programmatically confirms an already-open dialog.
async fn execute_dialog_confirm<R: Runtime>(
    app: &AppHandle<R>,
    dialog_type: &str,
    on_conflict: Option<&str>,
) -> ToolResult {
    match dialog_type {
        "transfer-confirmation" => {
            let conflict_policy = on_conflict.unwrap_or("skip_all");
            validate_conflict_policy(conflict_policy)?;
            confirm_open_dialog(
                app,
                dialog_type,
                json!({"type": "transfer-confirmation", "onConflict": conflict_policy}),
            )
            .await?;
            Ok(json!("OK: Transfer dialog confirmed."))
        }
        "delete-confirmation" => {
            confirm_open_dialog(app, dialog_type, json!({"type": "delete-confirmation"})).await?;
            Ok(json!("OK: Delete dialog confirmed."))
        }
        // Confirming the quit means quitting NOW, skipping the rest of the countdown.
        // Answered on the gate rather than the dialog, and with no ack: the process is on
        // its way out, so there is nothing left to observe. See `quit.rs`.
        "quit-confirmation" => super::quit::confirm_quit(),
        // Confirming this one means supplying a password, which needs a value
        // this tool has nowhere to put. `unlock_archive` is the whole answer.
        "archive-password" => Err(ToolError::invalid_params(
            "The archive-password prompt is answered with the unlock_archive tool, which takes the password. \
             Read cmdr://state dialogs for the archive it's asking about; dialog close cancels it instead.",
        )),
        _ => Err(ToolError::invalid_params(format!(
            "Cannot confirm dialog type '{}'. Only 'transfer-confirmation', 'delete-confirmation', and 'quit-confirmation' support confirm.",
            dialog_type
        ))),
    }
}

/// Execute the `open_search_dialog` tool.
///
/// Emits `mcp-open-search-dialog` with the prefill payload. The main window's
/// `+page.svelte` listener routes prefill values into `search-state.svelte.ts` and
/// flips `showSearchDialog = true`. The dialog mounts and calls
/// `notifyDialogOpened('search')`; we ack on the resulting `SoftDialogAppeared("search")`.
///
/// Per plan §3.11: the result confirms the dialog mounted (not that the search ran).
/// If the dialog is mid-close when the event arrives, the new mount may race; we surface
/// a clean failure from `wait_for_ack` within the 1500 ms budget.
pub async fn execute_open_search_dialog<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    // Strip nulls so the FE sees `undefined` (omitted properties), not `null`.
    // Most JSON-RPC clients serialize missing optional params as `null`, but our
    // FE state setters expect either a real value or "field not present".
    let mut payload = serde_json::Map::new();
    for key in [
        "query",
        "mode",
        "sizeMin",
        "sizeMax",
        "modifiedAfter",
        "modifiedBefore",
        "isDirectory",
        "scope",
        "caseSensitive",
        "excludeSystemDirs",
        "autoRun",
    ] {
        if let Some(v) = params.get(key)
            && !v.is_null()
        {
            payload.insert(key.to_string(), v.clone());
        }
    }

    // Validate `mode` if present.
    if let Some(mode) = payload.get("mode").and_then(|v| v.as_str())
        && !["ai", "filename", "regex"].contains(&mode)
    {
        return Err(ToolError::invalid_params(format!(
            "Invalid mode: '{mode}'. Expected 'ai', 'filename', or 'regex'."
        )));
    }

    app.emit("mcp-open-search-dialog", Value::Object(payload))?;
    wait_for_ack(app, AckSignal::SoftDialogAppeared("search"), DEFAULT_ACK_TIMEOUT).await?;
    Ok(json!("OK: Opened search dialog"))
}

/// Map an MCP confirmation `dialog_type` to its `SoftDialogTracker` ID. The IDs are
/// declared in the Svelte side via `<ModalDialog dialogId="...">` and registered with
/// the backend at startup (`register_known_dialogs`).
fn soft_dialog_id(dialog_type: &str) -> &'static str {
    match dialog_type {
        "transfer-confirmation" => "transfer-confirmation",
        "delete-confirmation" => "delete-confirmation",
        "mkdir-confirmation" => "mkdir-confirmation",
        "new-file-confirmation" => "new-file-confirmation",
        _ => "",
    }
}

/// Whether `dialog_type` is a soft dialog the frontend registered (and so closable via
/// the generic `close` path). Pure over the known list — the FE registers every
/// `SOFT_DIALOG_REGISTRY` id at startup, so an id not in it is one the generic close
/// can't drive (an honest "unknown dialog" error over a silent ack timeout).
fn is_registered_soft_dialog(known: &[crate::mcp::dialog_state::KnownDialog], dialog_type: &str) -> bool {
    known.iter().any(|d| d.id == dialog_type)
}

#[cfg(test)]
#[path = "dialogs_test.rs"]
mod tests;
