//! Tauri commands for live MCP server control.

use tauri::{AppHandle, Runtime};

use crate::mcp;

/// Start or stop the MCP server based on the user's setting.
#[tauri::command]
pub async fn set_mcp_enabled<R: Runtime + 'static>(app: AppHandle<R>, enabled: bool, port: u16) -> Result<(), String> {
    if enabled {
        if !mcp::is_mcp_running() {
            let config = mcp::McpConfig::from_settings_and_env(Some(true), Some(port));
            mcp::start_mcp_server(app, config).await?;
        }
    } else {
        mcp::stop_mcp_server();
    }
    Ok(())
}

/// Restart the MCP server on a new port. No-op if the server isn't running.
#[tauri::command]
pub async fn set_mcp_port<R: Runtime + 'static>(app: AppHandle<R>, port: u16) -> Result<(), String> {
    if !mcp::is_mcp_running() {
        return Ok(());
    }

    mcp::stop_mcp_server();
    let config = mcp::McpConfig::from_settings_and_env(Some(true), Some(port));
    mcp::start_mcp_server(app, config).await
}
