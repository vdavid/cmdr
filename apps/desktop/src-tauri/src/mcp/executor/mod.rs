//! Tool execution logic.
//!
//! Handles the execution of MCP tools and returns results.
//! All tools are designed to match user capabilities exactly.

mod ack;
// The category handler modules are `pub(crate)` (not private to `executor`) so the generated
// dispatch in the sibling `mcp/tool_registry.rs` can name their `pub` handler fns; a sibling
// can't otherwise reach `executor`'s descendants (E0603). `ack` stays private (executor-internal).
pub(crate) mod app;
pub(crate) mod async_tools;
pub(crate) mod dialogs;
pub(crate) mod downloads;
pub(crate) mod eject;
pub(crate) mod favorites;
pub(crate) mod file_ops;
pub(crate) mod indexing;
pub(crate) mod nav;
pub(crate) mod queue;
pub(crate) mod search;
pub(crate) mod tags;
pub(crate) mod view;

pub(crate) use ack::{
    AckSignal, DEFAULT_ACK_TIMEOUT, NAV_ACK_TIMEOUT, snapshot_generation, snapshot_window_count, wait_for_ack,
};

#[cfg(test)]
mod tests;

use std::sync::Mutex;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Listener, Manager, Runtime};

use super::pane_state::{PaneState, PaneStateStore};
use super::protocol::{INTERNAL_ERROR, INVALID_PARAMS};
use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize;

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

impl From<tauri::Error> for ToolError {
    fn from(e: tauri::Error) -> Self {
        Self::internal(e.to_string())
    }
}

/// Resolve the target pane from an optional `pane` param, defaulting to the
/// focused pane, and return its name plus the last-synced state snapshot.
///
/// Shared by the pane-scoped mutating tools (`tag`, `rename`) that resolve
/// names/selection/cursor to paths off the pane state the FE pushes (the same
/// source `copy`/`move`'s `check_operation_has_target` reads).
pub(super) fn target_pane_state<R: Runtime>(
    app: &AppHandle<R>,
    params: &Value,
) -> Result<(String, PaneState), ToolError> {
    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state not available yet"))?;
    let pane = match params.get("pane").and_then(|v| v.as_str()) {
        Some("left") => "left".to_string(),
        Some("right") => "right".to_string(),
        Some(other) => {
            return Err(ToolError::invalid_params(format!(
                "pane must be 'left' or 'right' (got '{other}')"
            )));
        }
        None => store.get_focused_pane(),
    };
    let state = if pane == "right" {
        store.get_right()
    } else {
        store.get_left()
    };
    Ok((pane, state))
}

/// Validate an optional `pane` param (`left` / `right`), returning `None` when
/// absent. Unlike [`target_pane_state`], this reads no store snapshot: it's for
/// tools whose FE side resolves the pane's LIVE path itself (mkdir/mkfile
/// direct-create), so the backend only validates and forwards the choice, and
/// the FE defaults to its focused pane when `None`.
pub(super) fn optional_pane_param(params: &Value) -> Result<Option<&str>, ToolError> {
    match params.get("pane").and_then(|v| v.as_str()) {
        None => Ok(None),
        Some(pane @ ("left" | "right")) => Ok(Some(pane)),
        Some(other) => Err(ToolError::invalid_params(format!(
            "pane must be 'left' or 'right' (got '{other}')"
        ))),
    }
}

/// Flush the frontend's pending pane-state push for `pane` before a handler reads
/// `PaneStateStore`, so name/selection/cursor resolution sees the LIVE listing.
///
/// `move_cursor` and `select` get this freshness as a side effect (their actions
/// call `syncStateToMcpNow` before replying); a handler that only READS state to
/// resolve targets (`tag`) has no such action, so a bare `nav` leaves the store a
/// debounced-sync behind and a same-named file from the pane's previous directory
/// could resolve as the target. This round-trip closes that window without moving
/// the cursor or changing the selection. The FE `mcp-sync-state` listener replies
/// once the push lands.
pub(super) async fn flush_pane_state<R: Runtime>(app: &AppHandle<R>, pane: &str) -> Result<(), ToolError> {
    mcp_round_trip(app, "mcp-sync-state", json!({ "pane": pane }), "ok".to_string())
        .await
        .map(|_| ())
}

/// Resolve the absolute paths a pane tool should act on: explicit `names` win,
/// else the current selection, else the item under the cursor.
///
/// Paths come from the last-synced pane state, so the loaded window is the
/// authority: a `name` not in the listing, or a selection/cursor outside the
/// loaded window, is an honest error rather than a silent miss.
pub(super) fn resolve_pane_target_paths(state: &PaneState, names: Option<&[String]>) -> Result<Vec<String>, ToolError> {
    if let Some(names) = names {
        if names.is_empty() {
            return Err(ToolError::invalid_params("'names' must be a non-empty array"));
        }
        let mut paths = Vec::with_capacity(names.len());
        let mut missing = Vec::new();
        for name in names {
            match state.files.iter().find(|f| &f.name == name) {
                Some(entry) => paths.push(entry.path.clone()),
                None => missing.push(name.clone()),
            }
        }
        if !missing.is_empty() {
            return Err(ToolError::invalid_params(format!(
                "not in the {} listing: {}",
                if state.path.is_empty() { "current" } else { &state.path },
                missing.join(", ")
            )));
        }
        return Ok(paths);
    }

    if !state.selected_indices.is_empty() {
        let mut paths = Vec::new();
        let mut unresolved = false;
        for &global in &state.selected_indices {
            match global.checked_sub(state.loaded_start).and_then(|i| state.files.get(i)) {
                Some(entry) => paths.push(entry.path.clone()),
                None => unresolved = true,
            }
        }
        if unresolved || paths.is_empty() {
            return Err(ToolError::invalid_params(
                "some selected items aren't in the loaded window; pass 'names' explicitly",
            ));
        }
        return Ok(paths);
    }

    let entry = state
        .cursor_index
        .checked_sub(state.loaded_start)
        .and_then(|i| state.files.get(i))
        .ok_or_else(|| {
            ToolError::invalid_params("nothing is selected and the cursor isn't on a listed item; pass 'names'")
        })?;
    if entry.name == ".." {
        return Err(ToolError::invalid_params(
            "the cursor is on the parent entry (..); select files or pass 'names'",
        ));
    }
    Ok(vec![entry.path.clone()])
}

/// Expands a leading `~` in an agent-supplied path to the user's home directory.
///
/// Every agent-supplied path must pass through this (or `user_path_param`, which wraps it)
/// before validation, comparison, or emission to the frontend — agents routinely send
/// `~/Downloads` and the frontend only understands absolute paths. Virtual paths
/// (`mtp://…`, direct-SMB) never start with `~`, so they pass through untouched.
fn expand_user_path(path: &str) -> String {
    crate::commands::file_system::expand_tilde(path)
}

/// Extracts a required path parameter, expanding a leading `~` via `expand_user_path`.
///
/// Use this instead of raw `params.get(key)` for any param that names a filesystem path.
fn user_path_param(params: &Value, key: &str) -> Result<String, ToolError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(expand_user_path)
        .ok_or_else(|| ToolError::invalid_params(format!("Missing '{key}' parameter")))
}

/// True for scheme-prefixed virtual paths (`mtp://…`, `smb://…`) that don't live on the
/// local filesystem and must skip local existence checks.
fn is_virtual_path(path: &str) -> bool {
    path.split_once("://").is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
    })
}

/// Validates that an agent-supplied path exists, without wedging on a hung mount.
///
/// Virtual paths (see `is_virtual_path`) skip the check — the local filesystem knows
/// nothing about them; the frontend's navigation/open path is the authority there.
/// The local probe runs on the blocking pool under a 2 s timeout because
/// `Path::exists()` on a dead network mount can block indefinitely, and an MCP handler
/// must never do un-timed filesystem I/O (same contract as `commands/util.rs`).
async fn validate_path_exists(path: &str) -> Result<(), ToolError> {
    if is_virtual_path(path) {
        return Ok(());
    }
    let owned = path.to_string();
    let exists = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::task::spawn_blocking(move || std::path::Path::new(&owned).exists()),
    )
    .await;
    match exists {
        Ok(Ok(true)) => Ok(()),
        Ok(Ok(false)) => Err(ToolError::invalid_params(format!("Path does not exist: {path}"))),
        Ok(Err(e)) => Err(ToolError::internal(format!("Path check failed: {e}"))),
        Err(_) => Err(ToolError::internal(format!(
            // allowed-pluralize-noun: "exists" is a verb here, not a plural noun
            "Timed out after two seconds checking whether {path} exists — the volume may be unresponsive"
        ))),
    }
}

/// Emit an event to the frontend and wait for a response (5s timeout).
///
/// The frontend must emit `mcp-response` with `{ requestId, ok, error? }`.
/// Returns `success_msg` on success, or the frontend's error message on failure.
async fn mcp_round_trip<R: Runtime>(
    app: &AppHandle<R>,
    event: &str,
    payload: Value,
    success_msg: String,
) -> ToolResult {
    mcp_round_trip_with_timeout(app, event, payload, success_msg, 5).await
}

/// Parse an `mcp-response` event payload against the request ID we're waiting for.
///
/// This correlation is what makes a round-trip ack **per-request**: it's independent of
/// pane-state pushes (and their byte-identical dedupe), and a reply belonging to some
/// other in-flight request can never satisfy ours.
///
/// Returns `None` when the payload is malformed or carries a different `requestId`,
/// `Some(Ok(()))` on `ok: true`, and `Some(Err(message))` otherwise. A missing `ok`
/// field counts as failure — a malformed reply must never become a false-positive OK.
fn parse_mcp_response(payload: &str, expected_id: &str) -> Option<Result<(), String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    Some(if resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(())
    } else {
        let err = resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error")
            .to_string();
        Err(err)
    })
}

/// Parse an `mcp-response` for an autoConfirm file operation, extracting the
/// spawned `operationId` when present.
///
/// Same `requestId` correlation as [`parse_mcp_response`], but the success arm
/// also carries an optional `operationId` (a string): `Some(Ok(Some(id)))` when
/// the op spawned, `Some(Ok(None))` when the FE acked without spawning one (the
/// compress auto-confirm that keeps its dialog open on an existing target), and
/// `Some(Err(msg))` on failure. `None` for a malformed or mismatched payload.
fn parse_operation_start_response(payload: &str, expected_id: &str) -> Option<Result<Option<String>, String>> {
    let resp = serde_json::from_str::<Value>(payload).ok()?;
    if resp.get("requestId").and_then(|v| v.as_str()) != Some(expected_id) {
        return None;
    }
    if resp.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let operation_id = resp.get("operationId").and_then(|v| v.as_str()).map(str::to_string);
        Some(Ok(operation_id))
    } else {
        let err = resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error")
            .to_string();
        Some(Err(err))
    }
}

/// Emit an autoConfirm file-op event and wait for the FE to reply with the
/// spawned `operationId`.
///
/// Replaces the `GenerationAdvanced` ack for the auto-confirm path: correlating
/// on a per-request id lets the tool return the exact `operationId` the FE minted
/// (so a follow-up `queue` / `await operation_complete` is directly sequenced),
/// where the generation ack could only prove "some pane push happened". Returns
/// the id, or `None` when the FE acked without spawning (compress on an existing
/// target keeps its dialog open). The budget is generous because the flow spans
/// dialog-open → confirm → the write-op IPC that mints the id.
async fn mcp_await_operation_start<R: Runtime>(
    app: &AppHandle<R>,
    event: &str,
    mut payload: Value,
    timeout_secs: u64,
) -> Result<Option<String>, ToolError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    payload["requestId"] = json!(request_id);

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<Option<String>, String>>();
    let expected_id = request_id.clone();

    let tx = Mutex::new(Some(tx));
    let listener_id = app.listen("mcp-response", move |event| {
        if let Some(result) = parse_operation_start_response(event.payload(), &expected_id)
            && let Some(tx) = tx.lock_ignore_poison().take()
        {
            let _ = tx.send(result);
        }
    });

    app.emit(event, payload)?;

    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await;
    app.unlisten(listener_id);

    match result {
        Ok(Ok(Ok(operation_id))) => Ok(operation_id),
        Ok(Ok(Err(err))) => Err(ToolError::internal(err)),
        Ok(Err(_)) => Err(ToolError::internal("Frontend response channel dropped")),
        Err(_) => Err(ToolError::internal(format!(
            "Frontend did not respond within {}",
            pluralize(timeout_secs, "second")
        ))),
    }
}

/// Like `mcp_round_trip` but with a configurable timeout.
async fn mcp_round_trip_with_timeout<R: Runtime>(
    app: &AppHandle<R>,
    event: &str,
    mut payload: Value,
    success_msg: String,
    timeout_secs: u64,
) -> ToolResult {
    let request_id = uuid::Uuid::new_v4().to_string();
    payload["requestId"] = json!(request_id);

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
    let expected_id = request_id.clone();

    // Use a Mutex to allow the closure to consume tx exactly once
    let tx = Mutex::new(Some(tx));
    let listener_id = app.listen("mcp-response", move |event| {
        if let Some(result) = parse_mcp_response(event.payload(), &expected_id)
            && let Some(tx) = tx.lock_ignore_poison().take()
        {
            let _ = tx.send(result);
        }
    });

    app.emit(event, payload)?;

    let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await;
    app.unlisten(listener_id);

    match result {
        Ok(Ok(Ok(()))) => Ok(json!(success_msg)),
        Ok(Ok(Err(err))) => Err(ToolError::internal(err)),
        Ok(Err(_)) => Err(ToolError::internal("Frontend response channel dropped")),
        Err(_) => Err(ToolError::internal(format!(
            "Frontend did not respond within {}",
            pluralize(timeout_secs, "second")
        ))),
    }
}
