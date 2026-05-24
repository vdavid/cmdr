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
//! - `focus settings|file-viewer|about` → window is present (no-op fast path; if the window isn't
//!   there, the wait_for_ack times out, which is the correct contract for focusing a non-existent
//!   dialog).
//! - `confirm <transfer|delete>` → pane generation advances (the FE accepted the confirmation and
//!   the underlying copy/move/delete started, producing a state push).
//! - `open_search_dialog` → soft dialog `search` appears in `SoftDialogTracker`. The frontend
//!   already calls `notifyDialogOpened('search')` from `SearchDialog.svelte::onMount`. If the
//!   dialog is mid-close when the event arrives, the new mount may race; the ack times out within
//!   the 1500 ms budget and the tool surfaces a clean failure. See plan §5.7 risk register.

use std::path::Path;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{
    AckSignal, DEFAULT_ACK_TIMEOUT, ToolError, ToolResult, snapshot_generation, snapshot_window_count, wait_for_ack,
};

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
        "onboarding" => {
            // Re-entry path. Same command id the menu / palette use, so a single FE
            // handler covers all three surfaces. The wizard is a soft sheet (its own
            // `OnboardingWizard.svelte`, not a ModalDialog consumer), but it calls
            // `notifyDialogOpened('onboarding')` on mount, so SoftDialogTracker fires.
            app.emit_to("main", "execute-command", json!({"commandId": "cmdr.openOnboarding"}))?;
            wait_for_ack(app, AckSignal::SoftDialogAppeared("onboarding"), DEFAULT_ACK_TIMEOUT).await?;
            Ok(json!("OK: Opened onboarding wizard"))
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
            // Snapshot the viewer count first. If zero, fast-fail: there's nothing to
            // close, and waiting for a count drop would just time out at 1500 ms.
            let before = snapshot_window_count(app, "viewer");
            if before == 0 {
                return Err(ToolError::invalid_params("No file viewer windows are open."));
            }
            if let Some(path) = path {
                app.emit("close-file-viewer", json!({"path": path}))?;
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
                app.emit("close-all-file-viewers", ())?;
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
            app.emit("close-about", ())?;
            wait_for_ack(app, AckSignal::SoftDialogDisappeared("about"), DEFAULT_ACK_TIMEOUT).await?;
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
