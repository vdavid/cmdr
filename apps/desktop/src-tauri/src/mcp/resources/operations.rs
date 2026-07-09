//! The `operations:` section of `cmdr://state`: every queued, running, or paused
//! write operation (copy / move / delete / trash / compress / archive edit) with
//! live progress, speed, and ETA where it's running.
//!
//! This is the discovery surface for the `queue` tool — an agent reads it to find
//! operation ids and their lifecycle status before pausing / resuming / cancelling.
//!
//! **Two-source join.** Membership + lifecycle status (`running` / `paused` /
//! `queued`) come from the operation manager's registry (`list_operations`, whose
//! `OperationSnapshot` carries no progress by design), while progress / speed /
//! ETA come from the separate write-operations status cache
//! (`get_operation_status`, the same single-source the progress dialog feeds
//! from). Joined by operation id; a queued op simply has no progress fields, and
//! settled ops are already gone from both sources (see `terminal_ops` for the
//! ring that keeps their outcome for `await operation_complete`).
//!
//! Speed and ETA are whole-run averages (`bytes_done / elapsed`), not the
//! progress dialog's EWMA — good enough for an agent's "is it moving, roughly
//! when is it done" questions.

use crate::file_system::write_operations::{
    LifecycleStatus, OperationSnapshot, OperationStatus, get_operation_status, list_operations,
};
use crate::search::query::format_size;

/// One row of the `operations:` section: the registry's membership / status /
/// summary joined with the status cache's live progress (absent for queued ops).
pub(crate) struct OperationRow {
    pub snapshot: OperationSnapshot,
    pub progress: Option<OperationStatus>,
}

/// Snapshot every registered operation (queued / running / paused), joining the
/// registry membership with the live progress cache by id.
pub(crate) fn snapshot_operations() -> Vec<OperationRow> {
    list_operations()
        .into_iter()
        .map(|snapshot| {
            let progress = get_operation_status(&snapshot.operation_id);
            OperationRow { snapshot, progress }
        })
        .collect()
}

/// The lower-case lifecycle token. Only `queued` / `running` / `paused` ever
/// reach here — the manager removes an op on settle, so a terminal status never
/// appears in `list_operations`.
fn status_token(status: LifecycleStatus) -> &'static str {
    match status {
        LifecycleStatus::Queued => "queued",
        LifecycleStatus::Running => "running",
        LifecycleStatus::Paused => "paused",
        // Unreachable in production (settled ops leave the registry). Rendered
        // honestly rather than papered over if one ever slips through.
        LifecycleStatus::Done => "done",
        LifecycleStatus::Cancelled => "cancelled",
        LifecycleStatus::Failed => "failed",
    }
}

/// Pure YAML builder for the `operations:` section. `now_ms` is injected so tests
/// don't depend on the wall clock.
pub(crate) fn build_operations_yaml(rows: &[OperationRow], now_ms: u64) -> String {
    if rows.is_empty() {
        return "operations: []\n".to_string();
    }
    let mut yaml = String::from("operations:\n");
    for row in rows {
        let kind = enum_str(serde_json::to_value(row.snapshot.operation_type).ok());
        let status = status_token(row.snapshot.status);
        yaml.push_str(&format!(
            "  - operationId: {}\n    type: {kind}\n    status: {status}\n",
            row.snapshot.operation_id
        ));
        // Source / destination summaries help the agent tell operations apart.
        // Redacted like every other user path in `cmdr://state`.
        if let Some(source) = &row.snapshot.source {
            yaml.push_str(&format!("    source: {:?}\n", crate::redact::redact_line(source)));
        }
        if let Some(destination) = &row.snapshot.destination {
            yaml.push_str(&format!(
                "    destination: {:?}\n",
                crate::redact::redact_line(destination)
            ));
        }
        // Live progress: present for running / paused, absent for queued (the
        // status cache has no entry until the op is admitted and starts work).
        if let Some(op) = &row.progress {
            yaml.push_str(&format!("    progress: {}\n", progress_line(op)));
            if let Some(file) = &op.current_file {
                // Filenames can carry PII (device names, person names); same
                // redaction contract as `recentErrors` and `cmdr://logs`.
                yaml.push_str(&format!("    currentFile: {:?}\n", crate::redact::redact_line(file)));
            }
            let elapsed_s = now_ms.saturating_sub(op.started_at) / 1000;
            if elapsed_s > 0 && op.bytes_done > 0 {
                let rate = op.bytes_done / elapsed_s;
                yaml.push_str(&format!("    speed: {}/s\n", format_size(rate)));
                if op.bytes_total > op.bytes_done && rate > 0 {
                    let eta_s = (op.bytes_total - op.bytes_done) / rate;
                    yaml.push_str(&format!("    etaSeconds: {eta_s}\n"));
                }
            }
            yaml.push_str(&format!("    elapsedSeconds: {elapsed_s}\n"));
        }
    }
    yaml
}

/// One human line: bytes and file counters, with a percent when totals are known.
/// Totals are 0 while scanning, so each part only renders once it's meaningful.
fn progress_line(op: &OperationStatus) -> String {
    let mut parts: Vec<String> = Vec::new();
    if op.bytes_total > 0 {
        let percent = ((op.bytes_done as f64 / op.bytes_total as f64) * 100.0).min(100.0) as u8;
        parts.push(format!(
            "{} / {} ({percent}%)",
            format_size(op.bytes_done),
            format_size(op.bytes_total)
        ));
    } else if op.bytes_done > 0 {
        parts.push(format!("{} so far", format_size(op.bytes_done)));
    }
    if op.files_total > 0 {
        parts.push(format!("{}/{} files", op.files_done, op.files_total));
    } else if op.files_done > 0 {
        parts.push(format!("{} files so far", op.files_done));
    }
    if parts.is_empty() {
        return "scanning".to_string();
    }
    parts.join(", ")
}

/// Serde-rendered enum name (`copy`, `rolling_back`), lowercased as a fallback
/// for enums without a `rename_all` attribute.
fn enum_str(value: Option<serde_json::Value>) -> String {
    value
        .as_ref()
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}
