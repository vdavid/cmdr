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
use crate::commands::indexing::{disable_drive_index, enable_drive_index, forget_drive_index, rescan_drive_index};
use crate::tools::resources::indexing::freshness_token;
use cmdr_index::Freshness;

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
            let pre = crate::index_host::index().volume_status(volume_id).freshness;
            let outcome = if action == "rescan" {
                rescan_drive_index(volume_id.to_string()).await
            } else {
                enable_drive_index(volume_id.to_string()).await
            };

            match outcome {
                Ok(started) => {
                    if let Some(reply) = reply_without_a_walk(action, volume_id, &started) {
                        return reply;
                    }
                    let departed = wait_for_scan_departure(volume_id, pre).await;
                    let status = freshness_token(crate::index_host::index().volume_status(volume_id).freshness);
                    walk_reply(action, volume_id, status, departed)
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

/// The answer for an outcome that started no walk, or `None` for the one that did.
///
/// Exhaustive on purpose, so a new outcome has to say what it tells an agent: the
/// master switch used to fall through to the freshness wait and answer OK with
/// nothing walking.
fn reply_without_a_walk(action: &str, volume_id: &str, outcome: &EnableIndexingOutcome) -> Option<ToolResult> {
    match outcome {
        EnableIndexingOutcome::Started => None,
        // An SMB volume that can't index yet is a typed refusal, surfaced as an
        // honest error rather than a false OK.
        EnableIndexingOutcome::Refused { reason } => {
            Some(Err(ToolError::internal(format!("Can't index {volume_id}: {reason}"))))
        }
        EnableIndexingOutcome::IndexingDisabled => Some(Err(ToolError::internal(format!(
            "Can't {action} indexing for {volume_id}: drive indexing is off in settings, so nothing walked"
        )))),
        // Something else holds the drive, so the walk we asked for is REMEMBERED
        // rather than running: the index starts it when that holder ends. Say so
        // and return, since the freshness the wait watches isn't going to move yet.
        EnableIndexingOutcome::DeferredUntilSearchEnds => Some(Ok(json!(format!(
            "OK: {action} indexing for {volume_id} is waiting for the search walking it; \
             it starts on its own when that walk ends"
        )))),
        EnableIndexingOutcome::DeferredUntilScanEnds => Some(Ok(json!(format!(
            "OK: {action} indexing for {volume_id} is queued behind the walk already \
             running on it; it starts on its own when that one ends"
        )))),
        // An unmount of the drive was under way, so no start ran.
        EnableIndexingOutcome::DriveLeaving => Some(Err(ToolError::internal(format!(
            "Can't {action} indexing for {volume_id}: an unmount of the drive was under way"
        )))),
    }
}

/// The answer once the index said it started.
///
/// ❗ A rescan whose freshness never moved can't claim a walk: that freshness is what
/// a follow-up `await index_status` reads. Not a failure either, since a tiny volume
/// can finish between two polls. An `enable` that stays put is fine: a finished index
/// on a phone or a share resumes into Stale and stays browsable.
fn walk_reply(action: &str, volume_id: &str, status: &str, departed: bool) -> ToolResult {
    if action == "rescan" && !departed {
        return Err(ToolError::outcome_unknown(format!(
            "rescan indexing for {volume_id} started no visible walk: status is still {status} after {} seconds, \
             so the outcome is unknown. Check `cmdr://indexing` for its last scan",
            SCAN_DEPARTURE_TIMEOUT.as_secs()
        )));
    }
    Ok(json!(format!(
        "OK: {action} indexing for {volume_id}; status now {status}"
    )))
}

/// The ordering contract: don't return until the freshness has left its pre-scan
/// state. `force_scan` flips to Scanning synchronously for an active index (so
/// the first read already differs); the enable-first-scan path may lag, so we
/// poll briefly. A pre-state that's already Scanning needs no wait (a scan is
/// already reflected). Bounded and timeout-tolerant: the answer is whether it left
/// in time, and the `await` tool has its own deadline.
async fn wait_for_scan_departure(volume_id: &str, pre: Option<Freshness>) -> bool {
    if pre == Some(Freshness::Scanning) {
        return true;
    }
    let deadline = tokio::time::Instant::now() + SCAN_DEPARTURE_TIMEOUT;
    loop {
        if crate::index_host::index().volume_status(volume_id).freshness != pre {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The master switch used to fall through to the freshness wait and answer
    /// "OK ... status now stale" with nothing walking.
    #[test]
    fn a_rescan_with_drive_indexing_off_says_it_did_not_walk() {
        let reply = reply_without_a_walk("rescan", "mtp-1:65537", &EnableIndexingOutcome::IndexingDisabled);
        assert!(matches!(reply, Some(Err(_))), "got {reply:?}");
    }

    #[test]
    fn only_a_start_goes_on_to_wait_for_the_walk() {
        assert!(reply_without_a_walk("rescan", "root", &EnableIndexingOutcome::Started).is_none());
    }

    #[test]
    fn a_rescan_whose_status_never_moved_doesnt_claim_a_walk() {
        let err = walk_reply("rescan", "mtp-1:65537", "stale", false).expect_err("no visible walk isn't an OK");
        assert!(err.outcome_is_unknown(), "got {err:?}");
    }

    #[test]
    fn an_enable_that_resumed_into_stale_is_still_an_ok() {
        assert_eq!(
            walk_reply("enable", "mtp-1:65537", "stale", false).expect("a resume is what enable does"),
            json!("OK: enable indexing for mtp-1:65537; status now stale")
        );
    }

    #[test]
    fn a_rescan_that_moved_reports_the_new_status() {
        assert_eq!(
            walk_reply("rescan", "root", "scanning", true).expect("the walk showed up"),
            json!("OK: rescan indexing for root; status now scanning")
        );
    }
}
