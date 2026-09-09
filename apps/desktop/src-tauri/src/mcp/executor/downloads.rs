//! Downloads tool handlers.
//!
//! `go_to_latest_download`: navigate the focused pane to the parent dir of
//! the most recently observed eligible download and move the cursor onto it.
//! Returns the absolute path on success, or a typed error string surfaced
//! through the MCP error channel. Mirrors the user-facing `⌘J` flow without
//! the toast UI — agents drive navigation, the toasts are for humans.

use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::nav::go_to_in_focused_pane;
use super::{ToolError, ToolResult};

/// `go_to_latest_download` MCP tool. No parameters in v1 (the `index`
/// argument from the plan is deferred until the scan fallback returns a
/// sorted list).
///
/// Flow:
/// 1. Resolve the latest eligible download via the same code path as the
///    Tauri command — typed `GoToLatestError` branches map directly onto
///    MCP error responses with descriptive messages.
/// 2. Hand the parent dir + file name to `go_to_in_focused_pane`, the shared
///    navigate-then-cursor primitive (`../nav.rs`) the OS reveal handler uses too.
/// 3. Return the absolute path as the tool result so agents can chain on it.
pub async fn execute_go_to_latest_download<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    let latest = crate::downloads::commands::go_to_latest_download()
        .await
        .map_err(|e| match e {
            crate::downloads::commands::GoToLatestError::WatcherUnavailable => {
                ToolError::internal("Downloads watcher isn't running. Grant Cmdr Full Disk Access and retry.")
            }
            crate::downloads::commands::GoToLatestError::Empty => {
                ToolError::internal("No eligible downloads found in ~/Downloads.")
            }
            crate::downloads::commands::GoToLatestError::DownloadsDirUnresolved => {
                ToolError::internal("Couldn't resolve the Downloads directory.")
            }
        })?;

    go_to_in_focused_pane(app, &latest.parent_dir, std::slice::from_ref(&latest.file_name)).await?;
    Ok(json!(format!("OK: Went to {}", latest.path)))
}
