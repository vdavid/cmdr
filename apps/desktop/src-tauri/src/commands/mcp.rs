//! Tauri commands for live MCP server control.

use tauri::{AppHandle, Runtime};

use crate::mcp;
use crate::security_log::{self, Initiator, SecurityEvent};
use crate::tools;

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
    let was_running = mcp::is_mcp_running();
    let outcome = if enabled {
        let config = mcp::McpConfig::from_settings_and_env(Some(true), Some(port));
        mcp::rebind_interactive(app, config).await?
    } else {
        mcp::stop_mcp_server_and_wait().await;
        mcp::McpServerOutcome::Stopped
    };
    if let Some(event) = toggle_event(was_running, &outcome) {
        security_log::record(event);
    }
    Ok(outcome)
}

/// What the security log notes about a toggle: only a real start or stop. A re-apply that leaves
/// the server as it was (a rebind on a new port, a busy port, off while off) notes nothing.
fn toggle_event(was_running: bool, outcome: &mcp::McpServerOutcome) -> Option<SecurityEvent> {
    match (was_running, outcome) {
        (false, mcp::McpServerOutcome::Running { .. }) => {
            Some(SecurityEvent::McpServerTurnedOn { by: Initiator::User })
        }
        (true, mcp::McpServerOutcome::Stopped) => Some(SecurityEvent::McpServerTurnedOff { by: Initiator::User }),
        _ => None,
    }
}

/// What turning direct mode on or off came to. `survives_restart` is false when this store can't
/// back the setting, so it lasts until Cmdr quits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpDirectModeOutcome {
    pub on: bool,
    pub survives_restart: bool,
}

/// Turn direct mode on or off, called only by `McpServerSection.svelte`: the setting is
/// `mcpSettable: false`, so no tool can reach it. Blocks on the secret store, off the IPC thread.
#[tauri::command]
#[specta::specta]
pub async fn set_mcp_direct_mode(enabled: bool) -> McpDirectModeOutcome {
    let change = tokio::task::spawn_blocking(move || tools::policy::set_direct_mode(enabled))
        .await
        .unwrap_or(tools::policy::DirectModeChange {
            changed: false,
            survives_restart: false,
        });
    if let Some(event) = direct_mode_event(enabled, change.changed) {
        security_log::record(event);
    }
    McpDirectModeOutcome {
        on: tools::policy::direct_mode_setting(),
        survives_restart: change.survives_restart,
    }
}

/// What the security log notes about a direct mode toggle: only a real change.
fn direct_mode_event(on: bool, changed: bool) -> Option<SecurityEvent> {
    changed.then_some(SecurityEvent::McpDirectModeChanged {
        on,
        by: Initiator::User,
    })
}

/// Returns whether the person's direct mode setting is really on in this process, which the JSON
/// value alone doesn't say. An automated run's direct mode doesn't show here.
#[tauri::command]
#[specta::specta]
pub fn get_mcp_direct_mode() -> bool {
    tools::policy::direct_mode_setting()
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

#[cfg(test)]
mod tests {
    use super::*;
    use mcp::McpServerOutcome;

    #[test]
    fn only_a_real_start_or_stop_reaches_the_security_log() {
        let on = Some(SecurityEvent::McpServerTurnedOn { by: Initiator::User });
        let off = Some(SecurityEvent::McpServerTurnedOff { by: Initiator::User });
        assert_eq!(toggle_event(false, &McpServerOutcome::Running { port: 19225 }), on);
        assert_eq!(toggle_event(true, &McpServerOutcome::Stopped), off);
        assert_eq!(toggle_event(true, &McpServerOutcome::Running { port: 19226 }), None);
        assert_eq!(
            toggle_event(false, &McpServerOutcome::PortInUse { requested: 19225 }),
            None
        );
        assert_eq!(toggle_event(false, &McpServerOutcome::Stopped), None);
    }

    #[test]
    fn only_a_real_direct_mode_change_reaches_the_security_log() {
        assert_eq!(
            direct_mode_event(true, true),
            Some(SecurityEvent::McpDirectModeChanged {
                on: true,
                by: Initiator::User
            })
        );
        assert_eq!(
            direct_mode_event(false, true),
            Some(SecurityEvent::McpDirectModeChanged {
                on: false,
                by: Initiator::User
            })
        );
        assert_eq!(direct_mode_event(true, false), None);
        assert_eq!(direct_mode_event(false, false), None);
    }
}
