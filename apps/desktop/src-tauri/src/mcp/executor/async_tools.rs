//! Async tool handlers: await, network, and settings.

use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime};

use super::{PaneStateStore, ToolError, ToolResult, mcp_round_trip};
use crate::file_system::write_operations::{LifecycleStatus, WriteOperationType};
use crate::indexing::freshness::Freshness;
use crate::mcp::resources::indexing::freshness_token;
use crate::mcp::terminal_ops::{self, TerminalStatus};

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

    // The volume-scoped and operation-scoped conditions take no pane; branch
    // before pane parsing.
    if condition == "index_status" {
        return await_index_status(params, timeout_s).await;
    }
    if condition == "operation_complete" {
        return await_operation_complete(params, timeout_s).await;
    }
    if condition == "operations_idle" {
        return await_operations_idle(timeout_s).await;
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

    // Fast-fail an unknown / never-registered volume id rather than polling the full
    // timeout only to keep reporting 'off': a volume with no registered index can never
    // reach fresh / scanning / stale. Mirrors `operation_complete`'s unknown-id honesty.
    // After `indexing enable`, the volume IS registered (the enable tool's ordering
    // contract returns only once freshness has left its pre-scan state), so a
    // legitimate enable → await chain still passes this gate.
    if !crate::indexing::all_registered_volume_ids()
        .iter()
        .any(|id| id == &volume_id)
    {
        return Err(ToolError::invalid_params(format!(
            "Unknown volume '{volume_id}': no index is registered for it, so index_status can't reach '{target}'. Enable indexing first (indexing enable), or see cmdr://state volumes for indexable ids."
        )));
    }

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

// ── Operation await conditions ───────────────────────────────────────

/// How an `await operation_complete` tick resolves against the two sources: the
/// terminal-ops ring (settled outcome) and the live registry (`list_operations`).
#[derive(Debug, PartialEq, Eq)]
enum CompleteResolution {
    /// The op settled; carries its terminal outcome.
    Settled(TerminalStatus),
    /// The op is still queued / running / paused — keep polling.
    StillRunning,
    /// The op is in neither source: an unknown id (or one that settled and aged
    /// off the bounded ring). Reported honestly instead of hanging.
    Unknown,
}

/// The lower-case wire token for an operation kind (`copy`, `archive_edit`, …),
/// matching the `type:` field in `cmdr://state operations`.
fn operation_type_token(kind: WriteOperationType) -> &'static str {
    match kind {
        WriteOperationType::Copy => "copy",
        WriteOperationType::Move => "move",
        WriteOperationType::Delete => "delete",
        WriteOperationType::Trash => "trash",
        WriteOperationType::Rename => "rename",
        WriteOperationType::CreateFolder => "create_folder",
        WriteOperationType::CreateFile => "create_file",
        WriteOperationType::ArchiveEdit => "archive_edit",
    }
}

/// Pure classifier for one `operation_complete` tick. Ring outcome wins (a
/// settled op leaves the live registry); otherwise a live membership means it's
/// still going; neither means unknown.
fn classify_operation_complete(terminal: Option<TerminalStatus>, is_live: bool) -> CompleteResolution {
    match (terminal, is_live) {
        (Some(status), _) => CompleteResolution::Settled(status),
        (None, true) => CompleteResolution::StillRunning,
        (None, false) => CompleteResolution::Unknown,
    }
}

/// Whether the queue is idle: no operation is running or queued. Paused ops don't
/// count — a paused op is parked indefinitely, so requiring it to drain would
/// hang `operations_idle` forever.
fn operations_are_idle(statuses: &[LifecycleStatus]) -> bool {
    !statuses
        .iter()
        .any(|s| matches!(s, LifecycleStatus::Running | LifecycleStatus::Queued))
}

/// Poll until a specific operation settles (completed / cancelled / failed).
/// Reads the terminal-ops ring and the live registry each tick; an id in neither
/// errors honestly rather than hanging.
async fn await_operation_complete(params: &Value, timeout_s: u64) -> ToolResult {
    let operation_id = params
        .get("value")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("operation_complete requires 'value' (the operationId)"))?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_s);
    let poll_interval = std::time::Duration::from_millis(250);

    loop {
        let terminal = terminal_ops::lookup(operation_id);
        let is_live = crate::file_system::list_operations()
            .iter()
            .any(|op| op.operation_id == operation_id);
        match classify_operation_complete(terminal.as_ref().map(|op| op.status), is_live) {
            CompleteResolution::Settled(status) => {
                let (kind, settled_at) = terminal
                    .map(|op| (operation_type_token(op.operation_type), op.settled_at_unix_ms))
                    .unwrap_or(("operation", 0));
                return Ok(json!(format!(
                    "OK: {kind} '{operation_id}' settled — {} (settledAtUnixMs={settled_at})",
                    status.as_token()
                )));
            }
            CompleteResolution::Unknown => {
                return Err(ToolError::invalid_params(format!(
                    "Unknown operationId '{operation_id}': it isn't a currently running operation and hasn't recently settled. See cmdr://state operations for live ops, or operations_list for completed history."
                )));
            }
            CompleteResolution::StillRunning => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(ToolError::internal(format!(
                        "Timed out after {timeout_s}s waiting for operation '{operation_id}' to complete (still running)"
                    )));
                }
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}

/// Poll until no operation is running or queued (paused ops excluded — see
/// `operations_are_idle`).
async fn await_operations_idle(timeout_s: u64) -> ToolResult {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_s);
    let poll_interval = std::time::Duration::from_millis(250);

    loop {
        let statuses: Vec<LifecycleStatus> = crate::file_system::list_operations()
            .iter()
            .map(|op| op.status)
            .collect();
        if operations_are_idle(&statuses) {
            return Ok(json!("OK: Condition met — no running or queued operations."));
        }
        if tokio::time::Instant::now() >= deadline {
            let running = statuses
                .iter()
                .filter(|s| matches!(s, LifecycleStatus::Running | LifecycleStatus::Queued))
                .count();
            return Err(ToolError::internal(format!(
                "Timed out after {timeout_s}s waiting for the queue to go idle ({running} still running or queued)"
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

#[cfg(test)]
mod operation_await_tests {
    use super::*;

    #[test]
    fn settled_op_reports_its_terminal_status_even_if_not_live() {
        assert_eq!(
            classify_operation_complete(Some(TerminalStatus::Completed), false),
            CompleteResolution::Settled(TerminalStatus::Completed)
        );
        // Ring outcome wins even over a (stale) live membership.
        assert_eq!(
            classify_operation_complete(Some(TerminalStatus::Failed), true),
            CompleteResolution::Settled(TerminalStatus::Failed)
        );
    }

    #[test]
    fn live_op_with_no_terminal_record_keeps_polling() {
        assert_eq!(
            classify_operation_complete(None, true),
            CompleteResolution::StillRunning
        );
    }

    #[test]
    fn id_in_neither_source_is_unknown_not_a_hang() {
        assert_eq!(classify_operation_complete(None, false), CompleteResolution::Unknown);
    }

    #[test]
    fn idle_iff_nothing_running_or_queued() {
        // Empty registry is idle.
        assert!(operations_are_idle(&[]));
        // A running or queued op blocks idle.
        assert!(!operations_are_idle(&[LifecycleStatus::Running]));
        assert!(!operations_are_idle(&[LifecycleStatus::Queued]));
        assert!(!operations_are_idle(&[
            LifecycleStatus::Paused,
            LifecycleStatus::Running
        ]));
        // A lone paused op is idle: it's parked, so requiring it to drain would hang forever.
        assert!(operations_are_idle(&[LifecycleStatus::Paused]));
    }
}
