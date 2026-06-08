//! Tauri commands for live MCP server control.

use tauri::{AppHandle, Runtime};

use crate::mcp;

/// Start or stop the MCP server based on the user's setting. On enable, binds the requested
/// port exactly: a busy port returns `McpServerOutcome::PortInUse` (server left as it was)
/// rather than silently landing on a different port. On disable, waits for the socket to
/// release so an immediate re-enable on the same port binds cleanly.
#[tauri::command]
#[specta::specta]
pub async fn set_mcp_enabled<R: Runtime + 'static>(
    app: AppHandle<R>,
    enabled: bool,
    port: u16,
) -> Result<mcp::McpServerOutcome, String> {
    if enabled {
        let config = mcp::McpConfig::from_settings_and_env(Some(true), Some(port));
        mcp::rebind_interactive(app, config).await
    } else {
        mcp::stop_mcp_server_and_wait().await;
        Ok(mcp::McpServerOutcome::Stopped)
    }
}

/// Restart the running MCP server on a new port (zero-downtime: the new listener binds
/// before the old one is retired). A busy port leaves the server running on its current
/// port and returns `McpServerOutcome::PortInUse`. No-op (`Stopped`) if the server isn't
/// running — enabling is the toggle's job, not the port stepper's.
#[tauri::command]
#[specta::specta]
pub async fn set_mcp_port<R: Runtime + 'static>(app: AppHandle<R>, port: u16) -> Result<mcp::McpServerOutcome, String> {
    if !mcp::is_mcp_running() {
        return Ok(mcp::McpServerOutcome::Stopped);
    }
    let config = mcp::McpConfig::from_settings_and_env(Some(true), Some(port));
    mcp::rebind_interactive(app, config).await
}

/// Returns whether the MCP server is currently running.
#[tauri::command]
#[specta::specta]
pub fn get_mcp_running() -> bool {
    mcp::is_mcp_running()
}

/// Returns the port the MCP server is actually listening on, or null if not running.
#[tauri::command]
#[specta::specta]
pub fn get_mcp_port() -> Option<u16> {
    mcp::get_mcp_actual_port()
}

/// Returns the per-instance MCP bearer token, or null if the server isn't running.
/// Used by the E2E harness (which runs outside the app) to authenticate `/mcp` requests
/// after fetching it via the Tauri page. The in-app frontend never talks to the HTTP
/// server (it uses the Tauri MCP bridge), so it doesn't need this.
#[tauri::command]
#[specta::specta]
pub fn get_mcp_token() -> Option<String> {
    mcp::current_mcp_token()
}
