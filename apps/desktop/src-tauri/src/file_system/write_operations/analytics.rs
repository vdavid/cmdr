//! PII-free PostHog analytics for completed write operations.
//!
//! Called only by `TauriEventSink::emit_complete` (in `event_sinks.rs`), hence
//! `pub(super)`. Every emitted property is categorical (op kind, a coarse count
//! bucket, a bool); no names or paths ever cross.

use super::types::{WriteCompleteEvent, WriteOperationType};
use crate::analytics::item_count_bucket;

/// Emits the PII-free PostHog event for a completed write operation. Copy/Move → `file_transfer_completed`
/// (with `op`, an item-count bucket, and a `had_conflicts` bool); Delete/Trash → `delete_used` (with
/// a `trashed` bool and a count bucket). Every prop is categorical; no names or paths cross. The
/// `had_conflicts` proxy is `files_skipped > 0` (skips happen only via conflict resolution, see
/// `WriteCompleteEvent::files_skipped`).
pub(super) fn emit_completion_analytics(event: &WriteCompleteEvent) {
    use serde_json::json;
    let bucket = item_count_bucket(event.files_processed);
    match event.operation_type {
        WriteOperationType::Copy | WriteOperationType::Move => {
            let op = if event.operation_type == WriteOperationType::Copy {
                "copy"
            } else {
                "move"
            };
            crate::analytics::posthog::capture(
                "file_transfer_completed",
                json!({ "op": op, "item_count": bucket, "had_conflicts": event.files_skipped > 0 }),
            );
        }
        WriteOperationType::Delete | WriteOperationType::Trash => {
            let trashed = event.operation_type == WriteOperationType::Trash;
            crate::analytics::posthog::capture("delete_used", json!({ "trashed": trashed, "item_count": bucket }));
        }
        WriteOperationType::ArchiveEdit => {
            crate::analytics::posthog::capture("archive_edit_completed", json!({ "item_count": bucket }));
        }
        // Instant metadata ops (`manager::run_instant`) emit no completion
        // analytics — they're transient and don't produce a `WriteCompleteEvent`.
        // Explicit no-op arms (not a catch-all `_`) so a future op type can't
        // silently skip analytics without a compile error here.
        WriteOperationType::Rename | WriteOperationType::CreateFolder | WriteOperationType::CreateFile => {}
    }
}
