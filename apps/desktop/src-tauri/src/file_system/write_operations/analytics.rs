//! PII-free PostHog analytics for completed write operations.
//!
//! Called only by `TauriEventSink::emit_complete` (in `event_sinks.rs`), hence
//! `pub(super)`. Every emitted property is categorical (op kind, a coarse count
//! bucket, a bool); no names or paths ever cross.

use super::types::{WriteCompleteEvent, WriteOperationType};

/// Buckets an item count into a coarse, PII-free range string for analytics. A raw count is fine to
/// ship (it's not PII), but a bucket keeps the dashboard's cardinality low and the signal readable.
pub(super) fn item_count_bucket(count: usize) -> &'static str {
    match count {
        0 => "0",
        1 => "1",
        2..=10 => "2-10",
        11..=100 => "11-100",
        101..=1000 => "101-1000",
        _ => "1000+",
    }
}

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
    }
}

#[cfg(test)]
mod analytics_bucket_tests {
    use super::*;

    #[test]
    fn item_count_buckets_map_to_coarse_ranges() {
        assert_eq!(item_count_bucket(0), "0");
        assert_eq!(item_count_bucket(1), "1");
        assert_eq!(item_count_bucket(2), "2-10");
        assert_eq!(item_count_bucket(10), "2-10");
        assert_eq!(item_count_bucket(11), "11-100");
        assert_eq!(item_count_bucket(100), "11-100");
        assert_eq!(item_count_bucket(101), "101-1000");
        assert_eq!(item_count_bucket(1000), "101-1000");
        assert_eq!(item_count_bucket(1001), "1000+");
        assert_eq!(item_count_bucket(50_000), "1000+");
    }
}
