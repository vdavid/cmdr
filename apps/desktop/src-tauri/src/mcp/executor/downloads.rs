//! Downloads tool handlers.
//!
//! `reveal_latest_download`: navigate the focused pane to the parent dir of
//! the most recently observed eligible download and move the cursor onto it.
//! Returns the absolute path on success, or a typed error string surfaced
//! through the MCP error channel. Mirrors the user-facing `⌘J` flow without
//! the toast UI — agents drive navigation, the toasts are for humans.

use serde_json::json;
use tauri::{AppHandle, Manager, Runtime};

use super::{PaneStateStore, ToolError, ToolResult, mcp_round_trip, mcp_round_trip_with_timeout};

/// `reveal_latest_download` MCP tool. No parameters in v1 (the `index`
/// argument from the plan is deferred until the scan fallback returns a
/// sorted list).
///
/// Flow:
/// 1. Resolve the latest eligible download via the same code path as the
///    Tauri command — typed `RevealError` branches map directly onto
///    MCP error responses with descriptive messages.
/// 2. Navigate the focused pane to `parent_dir` via `mcp-nav-to-path`
///    (30 s timeout, matches `nav_to_path`'s budget — Downloads is local
///    but the round-trip waits for listing completion).
/// 3. Move the cursor to `file_name` via `mcp-move-cursor` (5 s timeout).
/// 4. Return the absolute path as the tool result so agents can chain on it.
pub async fn execute_reveal_latest_download<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    let revealed = crate::downloads::commands::reveal_latest_download()
        .await
        .map_err(|e| match e {
            crate::downloads::commands::RevealError::WatcherUnavailable => {
                ToolError::internal("Downloads watcher isn't running. Grant Cmdr Full Disk Access and retry.")
            }
            crate::downloads::commands::RevealError::Empty => {
                ToolError::internal("No eligible downloads found in ~/Downloads.")
            }
            crate::downloads::commands::RevealError::DownloadsDirUnresolved => {
                ToolError::internal("Couldn't resolve the Downloads directory.")
            }
        })?;

    let pane = app
        .try_state::<PaneStateStore>()
        .map(|store| store.get_focused_pane())
        .unwrap_or_else(|| "left".to_string());

    // Navigate the focused pane to the parent dir. Reuses the FE's existing
    // `mcp-nav-to-path` handler (the one the `nav_to_path` tool drives), so
    // any volume / listing edge cases the FE already handles apply uniformly.
    mcp_round_trip_with_timeout(
        app,
        "mcp-nav-to-path",
        json!({"pane": pane, "path": revealed.parent_dir}),
        format!("OK: Revealed {}", revealed.path),
        30,
    )
    .await?;

    // Move the cursor onto the revealed file. If the file disappeared
    // between the reveal call and the FE's cursor placement (race against
    // a fresh download that bumped the ring while we were navigating),
    // the FE surfaces the failure through `mcp-response` and we report it
    // as a tool error. Reveal-then-vanish is acceptable to leak through —
    // the navigation completed, only the cursor placement missed.
    mcp_round_trip(
        app,
        "mcp-move-cursor",
        json!({"pane": pane, "to": revealed.file_name}),
        format!("OK: Revealed {}", revealed.path),
    )
    .await
}
