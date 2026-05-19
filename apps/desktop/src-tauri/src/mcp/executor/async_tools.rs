//! Async tool handlers: await, network, and settings.

use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::{PaneStateStore, ToolError, ToolResult, mcp_round_trip};

// ── Await tool ────────────────────────────────────────────────────────

/// Execute the `await` tool: poll PaneStateStore until a condition is met.
pub async fn execute_await<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter"))?;
    let condition = params
        .get("condition")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'condition' parameter"))?;
    let value = params
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'value' parameter"))?;
    let timeout_s = params.get("timeout_s").and_then(|v| v.as_u64()).unwrap_or(15).min(60);
    let after_generation = params.get("after_generation").and_then(|v| v.as_u64());

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if !["has_item", "item_count_gte", "path", "path_contains"].contains(&condition) {
        return Err(ToolError::invalid_params(
            "condition must be 'has_item', 'item_count_gte', 'path', or 'path_contains'",
        ));
    }

    let store = app
        .try_state::<PaneStateStore>()
        .ok_or_else(|| ToolError::internal("Pane state not available"))?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_s);
    let poll_interval = std::time::Duration::from_millis(250);

    loop {
        // Check generation gate
        let current_gen = store.get_generation();
        if let Some(min_gen) = after_generation
            && current_gen <= min_gen
        {
            if tokio::time::Instant::now() >= deadline {
                return Err(ToolError::internal(format!(
                    "Timed out after {timeout_s}s waiting for condition '{condition}' = \"{value}\" on {pane} pane (no state update since generation {min_gen})"
                )));
            }
            tokio::time::sleep(poll_interval).await;
            continue;
        }

        let state = match pane {
            "left" => store.get_left(),
            "right" => store.get_right(),
            _ => unreachable!(),
        };

        let matched = match condition {
            "has_item" => state.files.iter().any(|f| f.name == value),
            "item_count_gte" => {
                let min_count: usize = value.parse().unwrap_or(1);
                state.files.len() >= min_count
            }
            "path" => state.path == value,
            "path_contains" => state.path.contains(value),
            _ => unreachable!(),
        };

        if matched {
            // Build a compact state summary to return
            let file_count = state.files.len();
            let first_items: Vec<String> = state
                .files
                .iter()
                .take(20)
                .map(|f| {
                    let kind = if f.is_directory { "d" } else { "f" };
                    format!("{} {}", kind, f.name)
                })
                .collect();

            let result = format!(
                "OK: Condition met (generation {current_gen})\npane: {pane}\npath: {}\nfiles: {} total\nitems:\n{}",
                state.path,
                file_count,
                first_items
                    .iter()
                    .map(|i| format!("  - {i}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            return Ok(json!(result));
        }

        if tokio::time::Instant::now() >= deadline {
            let file_names: Vec<&str> = state.files.iter().take(10).map(|f| f.name.as_str()).collect();
            return Err(ToolError::internal(format!(
                "Timed out after {timeout_s}s waiting for condition '{condition}' = \"{value}\" on {pane} pane. Current path: \"{}\", current generation: {current_gen}, files (first 10): {:?}",
                state.path, file_names
            )));
        }

        tokio::time::sleep(poll_interval).await;
    }
}

// ── Network tools ────────────────────────────────────────────────────

/// Execute `connect_to_server`: parse address, TCP check, persist, inject.
pub async fn execute_connect_to_server<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let address = params
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'address' parameter"))?;

    match crate::network::manual_servers::add_manual_server(address, app).await {
        Ok(result) => Ok(json!(format!(
            "OK: Connected to {} (host ID: {})",
            result.host.name, result.host.id
        ))),
        Err(e) => Err(ToolError::internal(e)),
    }
}

/// Execute `remove_manual_server`: remove from storage and discovery state.
pub fn execute_remove_manual_server<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let host_id = params
        .get("host_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'host_id' parameter"))?;

    match crate::network::manual_servers::remove_manual_server(host_id, app) {
        Ok(()) => Ok(json!(format!("OK: Removed server {}", host_id))),
        Err(e) => Err(ToolError::internal(e)),
    }
}

/// Execute `upgrade_smb_to_direct`: upgrade an OS-mounted SMB volume to a direct
/// smb2 session. Thin wrapper around the existing `upgrade_to_smb_volume` Tauri
/// command (same code path that powers the "Connect directly" UI button) so
/// agents get the same behaviour as users: tries stored Keychain credentials,
/// returns a typed result mirroring `UpgradeResult` (Success / CredentialsNeeded
/// / NetworkError). On `CredentialsNeeded`, agents are out of luck for now —
/// credential prompts are interactive; a future tool could accept credentials
/// inline and call `upgrade_to_smb_volume_with_credentials`.
///
/// Only meaningful on macOS / Linux (the underlying command is platform-gated).
/// On other platforms the Tauri stub returns an error; we surface it as an
/// MCP internal error.
pub async fn execute_upgrade_smb_to_direct<R: Runtime>(_app: &AppHandle<R>, params: &Value) -> ToolResult {
    let volume_id = params
        .get("volume_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'volume_id' parameter"))?;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use crate::network::smb_upgrade::UpgradeResult;
        // Calls the inner helper rather than the Tauri command itself because
        // the Tauri command takes concrete `tauri::AppHandle` (= `AppHandle<Wry>`)
        // while the MCP executor's `app` is generic over `Runtime`. The inner
        // function carries all the upgrade logic minus the mDNS kick (which
        // needs a concrete handle). Agents wanting hostname-keyed Keychain
        // creds need mDNS already running — see the tool description.
        match crate::commands::network::upgrade_to_smb_volume_inner(volume_id.to_string()).await {
            Ok(UpgradeResult::Success) => Ok(json!(format!("OK: Upgraded {} to direct smb2", volume_id))),
            Ok(UpgradeResult::CredentialsNeeded {
                server,
                share,
                display_name,
                ..
            }) => {
                let server_label = if display_name.is_empty() { server } else { display_name };
                Ok(json!(format!(
                    "Needs credentials: share={} on {}. Cmdr's Keychain didn't have a working password for this share. \
                     Agents can't prompt; the user has to enter credentials via the UI's 'Connect directly' button. \
                     (If mDNS isn't running, hostname-keyed creds also won't be found; trigger any network UI action first.)",
                    share, server_label
                )))
            }
            Ok(UpgradeResult::NetworkError { message }) => Err(ToolError::internal(format!(
                "Network error while upgrading {}: {}",
                volume_id, message
            ))),
            Err(e) => Err(ToolError::internal(format!(
                "upgrade_to_smb_volume_inner({}) failed: {}",
                volume_id, e
            ))),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = volume_id;
        Err(ToolError::internal(
            "upgrade_smb_to_direct is only supported on macOS and Linux".to_string(),
        ))
    }
}

// ── Settings ─────────────────────────────────────────────────────────

/// Execute set_setting command via round-trip to the frontend.
pub async fn execute_set_setting<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'id' parameter"))?;

    let value = params
        .get("value")
        .ok_or_else(|| ToolError::invalid_params("Missing 'value' parameter"))?;

    mcp_round_trip(
        app,
        "mcp-set-setting",
        json!({"settingId": id, "value": value}),
        format!("OK: Set '{id}' to {value}"),
    )
    .await
}
