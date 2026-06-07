//! The `transfers:` section of `cmdr://state`: in-flight write operations
//! (copy / move / delete / trash) with live progress, speed, and ETA.
//!
//! Without this, a running 10 GB copy is invisible to agents — they can only
//! poll for the destination file's existence, which appears before the bytes
//! finish. The data comes from the write-operations status cache (the same
//! single-source the progress dialog feeds from); entries vanish when the
//! operation completes, cancels, or errors (guard `Drop` cleans the cache).
//!
//! Speed and ETA are whole-run averages (`bytes_done / elapsed`), not the
//! progress dialog's EWMA — good enough for an agent's "is it moving, roughly
//! when is it done" questions without growing the write-operations surface.

use crate::file_system::write_operations::{OperationStatus, get_operation_status, list_active_operations};
use crate::search::query::format_size;

/// Snapshot of every active operation, oldest first.
pub(crate) fn snapshot_transfers() -> Vec<OperationStatus> {
    let mut ops: Vec<OperationStatus> = list_active_operations()
        .iter()
        .filter_map(|summary| get_operation_status(&summary.operation_id))
        .collect();
    ops.sort_by_key(|op| op.started_at);
    ops
}

/// Pure YAML builder for the `transfers:` section. `now_ms` is injected so tests
/// don't depend on the wall clock.
pub(crate) fn build_transfers_yaml(ops: &[OperationStatus], now_ms: u64) -> String {
    if ops.is_empty() {
        return "transfers: []\n".to_string();
    }
    let mut yaml = String::from("transfers:\n");
    for op in ops {
        let kind = enum_str(serde_json::to_value(op.operation_type).ok());
        let phase = enum_str(serde_json::to_value(op.phase).ok());
        let elapsed_s = now_ms.saturating_sub(op.started_at) / 1000;

        yaml.push_str(&format!(
            "  - id: {}\n    type: {kind}\n    phase: {phase}\n",
            op.operation_id
        ));
        yaml.push_str(&format!("    progress: {}\n", progress_line(op)));
        if let Some(file) = &op.current_file {
            // Filenames can carry PII (device names, person names); same
            // redaction contract as `recentErrors` and `cmdr://logs`.
            yaml.push_str(&format!("    currentFile: {:?}\n", crate::redact::redact_line(file)));
        }
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
