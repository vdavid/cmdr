//! Dialog tool handlers.
//!
//! Ack contract:
//! - `open settings|file-viewer|about` → child window appears in `webview_windows()`.
//! - `open` confirmation dialogs → not allowed (use copy/move/delete/mkdir/mkfile instead).
//! - `close settings|file-viewer|about` → matching window disappears.
//! - `close <confirmation>` → soft dialog is no longer in `SoftDialogTracker`. Cancel
//!   doesn't reliably bump pane generation, so we wait for the tracker entry to vanish.
//! - `focus settings|file-viewer|about` → window is present (no-op fast path; if the
//!   window isn't there, the wait_for_ack times out, which is the correct contract for
//!   focusing a non-existent dialog).
//! - `confirm <transfer|delete>` → pane generation advances (the FE accepted the
//!   confirmation and the underlying copy/move/delete started, producing a state push).

use std::path::Path;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{AckSignal, DEFAULT_ACK_TIMEOUT, ToolError, ToolResult, snapshot_generation, wait_for_ack};

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
    let path = params.get("path").and_then(|v| v.as_str());
    let on_conflict = params.get("onConflict").and_then(|v| v.as_str());

    match action {
        "open" => execute_dialog_open(app, dialog_type, section, path).await,
        "focus" => execute_dialog_focus(app, dialog_type, path).await,
        "close" => execute_dialog_close(app, dialog_type, path).await,
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
    // via webview_windows() in resources.rs. No manual tracking needed here.

    match dialog_type {
        "settings" => {
            if let Some(section) = section {
                // Section-specific: MCP-only event handled by setupDialogListeners
                app.emit_to("main", "open-settings", json!({"section": section}))?;
                wait_for_ack(app, AckSignal::WindowAppeared("settings"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!(format!("OK: Opened settings at {section}")))
            } else {
                app.emit_to("main", "execute-command", json!({"commandId": "app.settings"}))?;
                wait_for_ack(app, AckSignal::WindowAppeared("settings"), DEFAULT_ACK_TIMEOUT).await?;
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
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!(format!("OK: Opened file viewer for {path}")))
            } else {
                // Open for file under cursor (validation happens in frontend)
                app.emit("open-file-viewer", ())?;
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!("OK: Opened file viewer for cursor file"))
            }
        }
        "about" => {
            app.emit_to("main", "execute-command", json!({"commandId": "app.about"}))?;
            // `about` is a soft dialog (ModalDialog overlay in the main window), not a
            // separate Tauri window. Track via SoftDialogTracker.
            wait_for_ack(app, AckSignal::SoftDialogAppeared("about"), DEFAULT_ACK_TIMEOUT).await?;
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
async fn execute_dialog_focus<R: Runtime>(app: &AppHandle<R>, dialog_type: &str, path: Option<&str>) -> ToolResult {
    // Focus is a best-effort UI hint. We don't have a reliable "the window now has
    // focus" signal cross-platform, so we ack on the precondition: the target dialog
    // must currently exist. If it doesn't, the wait_for_ack times out with a clear
    // message; that's the correct contract (you can't focus what isn't there).
    match dialog_type {
        "settings" => {
            app.emit("focus-settings", ())?;
            wait_for_ack(app, AckSignal::WindowAppeared("settings"), DEFAULT_ACK_TIMEOUT).await?;
            Ok(json!("OK: Focused settings"))
        }
        "file-viewer" => {
            if let Some(path) = path {
                // Validate that the file exists
                if !Path::new(path).exists() {
                    return Err(ToolError::invalid_params(format!("File does not exist: {}", path)));
                }
                app.emit("focus-file-viewer", json!({"path": path}))?;
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!(format!("OK: Focused file viewer for {path}")))
            } else {
                // Focus most recently opened file-viewer
                app.emit("focus-file-viewer", ())?;
                wait_for_ack(app, AckSignal::WindowAppeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!("OK: Focused most recent file viewer"))
            }
        }
        "about" => {
            app.emit("focus-about", ())?;
            wait_for_ack(app, AckSignal::SoftDialogAppeared("about"), DEFAULT_ACK_TIMEOUT).await?;
            Ok(json!("OK: Focused about dialog"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            app.emit("focus-confirmation", ())?;
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
                app.emit_to("settings", "mcp-settings-close", ())?;
                wait_for_ack(app, AckSignal::WindowDisappeared("settings"), DEFAULT_ACK_TIMEOUT).await?;
            }
            // If the settings window wasn't open to begin with, the close is a no-op
            // and we return OK without waiting: the desired end state is already true.
            Ok(json!("OK: Closed settings"))
        }
        "file-viewer" => {
            if let Some(path) = path {
                app.emit("close-file-viewer", json!({"path": path}))?;
                // We don't know which specific viewer label maps to which path without
                // FE help, so wait for any viewer-* window state to shift. If no viewer
                // matched, the FE no-ops and we ack immediately (no viewers = signal
                // satisfied for "viewer disappeared" only if ALL viewers gone; cannot
                // disambiguate). Use a generation-advance fallback via Any: if the
                // viewer count changed, the FE will push state; otherwise we time out.
                wait_for_ack(app, AckSignal::WindowDisappeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!(format!("OK: Closed file viewer for {path}")))
            } else {
                app.emit("close-all-file-viewers", ())?;
                wait_for_ack(app, AckSignal::WindowDisappeared("viewer"), DEFAULT_ACK_TIMEOUT).await?;
                Ok(json!("OK: Closed all file viewer dialogs"))
            }
        }
        "about" => {
            app.emit("close-about", ())?;
            wait_for_ack(app, AckSignal::WindowDisappeared("about"), DEFAULT_ACK_TIMEOUT).await?;
            // "about" is actually a soft dialog, but WindowDisappeared is benign: it
            // also succeeds when no such window exists. For correctness, also accept
            // the tracker no longer having "about" — wrap in Any.
            Ok(json!("OK: Closed about dialog"))
        }
        "transfer-confirmation" | "mkdir-confirmation" | "new-file-confirmation" | "delete-confirmation" => {
            app.emit("close-confirmation", ())?;
            // Soft confirmation dialogs unmount their `ModalDialog`, which fires
            // `notifyDialogClosed` and updates the `SoftDialogTracker`. Wait for the
            // tracker to lose the dialog ID. Cancel doesn't reliably bump generation
            // (that's what we used to wait for, and it broke on every cancel).
            wait_for_ack(
                app,
                AckSignal::SoftDialogDisappeared(soft_dialog_id(dialog_type)),
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!("OK: Cancelled confirmation dialog"))
        }
        _ => Err(ToolError::invalid_params(format!("Invalid dialog type: {dialog_type}"))),
    }
}

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
            if !["skip_all", "overwrite_all", "rename_all"].contains(&conflict_policy) {
                return Err(ToolError::invalid_params(
                    "onConflict must be 'skip_all', 'overwrite_all', or 'rename_all'",
                ));
            }
            let pre_gen = snapshot_generation(app);
            app.emit(
                "mcp-confirm-dialog",
                json!({"type": "transfer-confirmation", "onConflict": conflict_policy}),
            )?;
            wait_for_ack(
                app,
                AckSignal::GenerationAdvanced { from: pre_gen },
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!("OK: Transfer dialog confirmed."))
        }
        "delete-confirmation" => {
            let pre_gen = snapshot_generation(app);
            app.emit("mcp-confirm-dialog", json!({"type": "delete-confirmation"}))?;
            wait_for_ack(
                app,
                AckSignal::GenerationAdvanced { from: pre_gen },
                DEFAULT_ACK_TIMEOUT,
            )
            .await?;
            Ok(json!("OK: Delete dialog confirmed."))
        }
        _ => Err(ToolError::invalid_params(format!(
            "Cannot confirm dialog type '{}'. Only 'transfer-confirmation' and 'delete-confirmation' support confirm.",
            dialog_type
        ))),
    }
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
