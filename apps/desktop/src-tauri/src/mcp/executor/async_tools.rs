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
