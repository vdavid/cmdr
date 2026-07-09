//! Async tool handlers: await, network, and settings.

use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::{PaneStateStore, ToolError, ToolResult, mcp_round_trip};
use crate::indexing::freshness::Freshness;
use crate::mcp::resources::indexing::freshness_token;

// ── Await tool ────────────────────────────────────────────────────────

/// Execute the `await` tool: poll until a condition is met. Pane conditions
/// watch `PaneStateStore`; `index_status` watches a volume's indexing freshness.
pub async fn execute_await<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let condition = params
        .get("condition")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'condition' parameter"))?;
    let timeout_s = params
        .get("timeoutSeconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(15)
        .min(60);

    // The volume-scoped condition takes no pane; branch before pane parsing.
    if condition == "index_status" {
        return await_index_status(params, timeout_s).await;
    }

    let pane = params
        .get("pane")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'pane' parameter (required for this condition)"))?;
    let value = params
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'value' parameter"))?;
    let after_generation = params.get("afterGeneration").and_then(|v| v.as_u64());

    if !["left", "right"].contains(&pane) {
        return Err(ToolError::invalid_params("pane must be 'left' or 'right'"));
    }
    if ![
        "has_item",
        "not_has_item",
        "item_count_gte",
        "item_count_lte",
        "path",
        "path_contains",
    ]
    .contains(&condition)
    {
        return Err(ToolError::invalid_params(
            "condition must be 'has_item', 'not_has_item', 'item_count_gte', 'item_count_lte', 'path', or 'path_contains'",
        ));
    }

    // Expand `~` for path conditions: pane state holds absolute paths, so a literal
    // `~/…` value would never match and the tool would burn its full timeout.
    let value = if matches!(condition, "path" | "path_contains") {
        super::expand_user_path(value)
    } else {
        value.to_string()
    };

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
            "not_has_item" => !state.files.iter().any(|f| f.name == value),
            "item_count_gte" => {
                let min_count: usize = value.parse().unwrap_or(1);
                state.files.len() >= min_count
            }
            "item_count_lte" => {
                let max_count: usize = value.parse().unwrap_or(0);
                state.files.len() <= max_count
            }
            "path" => state.path == value,
            "path_contains" => state.path.contains(value.as_str()),
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

/// Whether a volume's current freshness satisfies an `index_status` await
/// target. Reuses the resource's `freshness_token` so the resource text and the
/// await condition agree by construction; never re-derives freshness (the one
/// transition table lives in `indexing/freshness.rs`).
fn index_status_matches(freshness: Option<Freshness>, target: &str) -> bool {
    freshness_token(freshness) == target
}

/// Pull `volumeId` + status `value` for an `index_status` await, verbatim.
/// Volume ids are used AS-IS — MTP ids embed colons (`mtp-{device}:{storage}`,
/// and the device id may itself hold `:`), so they must never be split.
fn index_status_params(params: &Value) -> Result<(String, String), ToolError> {
    let volume_id = params
        .get("volumeId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("index_status requires a 'volumeId' parameter"))?;
    let value = params
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'value' parameter"))?;
    if !matches!(value, "fresh" | "scanning" | "stale") {
        return Err(ToolError::invalid_params(
            "index_status value must be 'fresh', 'scanning', or 'stale'",
        ));
    }
    Ok((volume_id.to_string(), value.to_string()))
}

/// Poll a volume's indexing freshness until it reaches the target status. Reads
/// the single freshness store (`get_volume_index_status`) each tick — the same
/// store the FE badge reads.
async fn await_index_status(params: &Value, timeout_s: u64) -> ToolResult {
    let (volume_id, target) = index_status_params(params)?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_s);
    let poll_interval = std::time::Duration::from_millis(250);

    loop {
        let freshness = crate::indexing::get_volume_index_status(&volume_id).freshness;
        if index_status_matches(freshness, &target) {
            return Ok(json!(format!(
                "OK: Condition met — volume '{volume_id}' index_status is {}",
                freshness_token(freshness)
            )));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ToolError::internal(format!(
                "Timed out after {timeout_s}s waiting for volume '{volume_id}' index_status = '{target}' (currently {})",
                freshness_token(freshness)
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
        .get("hostId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'hostId' parameter"))?;

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
        .get("volumeId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'volumeId' parameter"))?;

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

#[cfg(test)]
mod index_status_tests {
    use super::*;

    #[test]
    fn matches_each_freshness_token() {
        assert!(index_status_matches(Some(Freshness::Fresh), "fresh"));
        assert!(index_status_matches(Some(Freshness::Scanning), "scanning"));
        assert!(index_status_matches(Some(Freshness::Stale), "stale"));
        // Cross-pairs don't match.
        assert!(!index_status_matches(Some(Freshness::Fresh), "scanning"));
        assert!(!index_status_matches(Some(Freshness::Stale), "fresh"));
        // A never-indexed volume (None) never matches a live-status target.
        assert!(!index_status_matches(None, "fresh"));
        assert!(!index_status_matches(None, "stale"));
    }

    #[test]
    fn preserves_mtp_volume_id_with_colons_verbatim() {
        // The whole point of the two-field form: an MTP volume id whose device id
        // holds colons must round-trip untouched (no `<volumeId>:<status>` packing
        // to naively split).
        let params = json!({ "volumeId": "mtp-AA:BB:CC:65537", "value": "fresh" });
        let (volume_id, status) = index_status_params(&params).expect("valid params");
        assert_eq!(volume_id, "mtp-AA:BB:CC:65537");
        assert_eq!(status, "fresh");
    }

    #[test]
    fn rejects_unknown_status_value() {
        let params = json!({ "volumeId": "root", "value": "bogus" });
        assert!(index_status_params(&params).is_err());
    }

    #[test]
    fn requires_volume_id() {
        let params = json!({ "value": "fresh" });
        assert!(index_status_params(&params).is_err());
    }
}
