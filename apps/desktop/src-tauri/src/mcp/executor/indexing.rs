//! The `indexing` tool: per-drive index control (enable / disable / rescan /
//! forget).
//!
//! Thin adapter over the typed `commands::indexing` functions (smart backend /
//! thin frontend). It dispatches no FE action and invents no ack — it returns
//! the backend result directly (the `connect_to_server` / `remove_manual_server`
//! precedent, so there is no FE action to ack). The one deliberate wait is the ordering contract:
//! `enable` / `rescan` don't return until the volume's freshness has left its
//! pre-scan state, so a follow-up `await index_status <volume> fresh` can't
//! instantly match the pre-rescan Fresh state.
//!
//! Reads / status are NOT an action here: they live in `cmdr://indexing`.

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::commands::indexing::EnableIndexingOutcome;
use crate::commands::indexing::{
    disable_drive_index, enable_drive_index_via_handle, forget_drive_index, rescan_drive_index_via_handle,
};
use crate::indexing::lifecycle::freshness::Freshness;
use crate::mcp::resources::indexing::freshness_token;

/// How long to wait for the scan to start (the ordering contract) before
/// returning anyway. The active-index `force_scan` path flips synchronously, so
/// this only bites the enable-first-scan path (async SMB especially).
const SCAN_DEPARTURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn execute_indexing(params: &Value) -> ToolResult {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'action' parameter"))?;
    let volume_id = params
        .get("volumeId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'volumeId' parameter"))?;

    match action {
        "enable" | "rescan" => {
            // Capture the pre-scan freshness BEFORE the backend call, so the
            // ordering-contract wait can detect the departure.
            let pre = crate::indexing::get_volume_index_status(volume_id).freshness;
            let outcome = if action == "rescan" {
                rescan_drive_index_via_handle(volume_id.to_string()).await
            } else {
                enable_drive_index_via_handle(volume_id.to_string()).await
            };

            match outcome {
                Ok(started) => {
                    // On macOS/Linux the outcome is a two-variant enum; an SMB
                    // volume that can't index yet is a typed refusal, surfaced
                    // as an honest error rather than a false OK.
                    #[cfg(any(target_os = "macos", target_os = "linux"))]
                    if let EnableIndexingOutcome::Refused { reason } = started {
                        return Err(ToolError::internal(format!("Can't index {volume_id}: {reason}")));
                    }
                    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
                    let _ = started; // `Started` is the only variant off macOS/Linux

                    wait_for_scan_departure(volume_id, pre).await;
                    let status = freshness_token(crate::indexing::get_volume_index_status(volume_id).freshness);
                    Ok(json!(format!(
                        "OK: {action} indexing for {volume_id}; status now {status}"
                    )))
                }
                Err(e) => Err(ToolError::internal(format!(
                    "Couldn't {action} indexing for {volume_id}: {e}"
                ))),
            }
        }
        "disable" => match disable_drive_index(volume_id.to_string()).await {
            Ok(()) => Ok(json!(format!("OK: Turned off indexing for {volume_id}"))),
            Err(e) => Err(ToolError::internal(format!(
                "Couldn't turn off indexing for {volume_id}: {e}"
            ))),
        },
        "forget" => match forget_drive_index(volume_id.to_string()).await {
            Ok(()) => Ok(json!(format!("OK: Forgot the index for {volume_id}"))),
            Err(e) => Err(ToolError::internal(format!(
                "Couldn't forget the index for {volume_id}: {e}"
            ))),
        },
        other => Err(ToolError::invalid_params(format!(
            "action must be 'enable', 'disable', 'rescan', or 'forget' (got '{other}')"
        ))),
    }
}

/// The ordering contract: don't return until the freshness has left its pre-scan
/// state. `force_scan` flips to Scanning synchronously for an active index (so
/// the first read already differs); the enable-first-scan path may lag, so we
/// poll briefly. A pre-state that's already Scanning needs no wait (a scan is
/// already reflected). Bounded and timeout-tolerant — the scan is genuinely
/// starting either way, and the `await` tool has its own deadline.
async fn wait_for_scan_departure(volume_id: &str, pre: Option<Freshness>) {
    if pre == Some(Freshness::Scanning) {
        return;
    }
    let deadline = tokio::time::Instant::now() + SCAN_DEPARTURE_TIMEOUT;
    loop {
        if crate::indexing::get_volume_index_status(volume_id).freshness != pre {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
