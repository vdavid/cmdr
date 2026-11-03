//! PII-free analytics for completed write operations.
//!
//! Two families, because the ops arrive by two different roads:
//!
//! - **Progressed ops** (copy / move / delete / archive edit) settle through
//!   `TauriEventSink::emit_complete` (via `terminal_effects::complete`) with a
//!   `WriteCompleteEvent`, and [`emit_completion_analytics`] reads it.
//! - **Instant metadata ops** (rename / new folder / new file) never produce a
//!   `WriteCompleteEvent` at all — the engine's `manager::run_instant` returns
//!   their `Result` inline — so their driver's single exit reports an
//!   `InstantMutationFact` through `EngineEvents`, and `events.rs` captures it.
//!
//! Every emitted property is categorical (op kind, a coarse count bucket, a bool);
//! no names or paths ever cross.

use crate::analytics::item_count_bucket;
use cmdr_operations::{WriteCompleteEvent, WriteOperationType};

/// Emits the PII-free analytics event for a completed write operation. Copy/Move → `file_transfer_completed`
/// (with `op`, an item-count bucket, and a `had_conflicts` bool); Delete/Trash → `delete_used` (with
/// a `trashed` bool and a count bucket). Every prop is categorical; no names or paths cross. The
/// `had_conflicts` proxy is `files_skipped > 0` (skips happen only via conflict resolution, see
/// `WriteCompleteEvent::files_skipped`).
pub(super) fn emit_completion_analytics(event: &WriteCompleteEvent) {
    use serde_json::json;
    let bucket = item_count_bucket(event.files_processed);
    // Each event name stays a LITERAL in its arm: the `analytics-event-catalog`
    // check reads emitters by matching `capture("…")`.
    match completion_category(event.operation_type) {
        Some(CompletionCategory::FileTransfer) => {
            let op = if event.operation_type == WriteOperationType::Copy {
                "copy"
            } else {
                "move"
            };
            crate::analytics::events::capture(
                "file_transfer_completed",
                json!({ "op": op, "item_count": bucket, "had_conflicts": event.files_skipped > 0 }),
            );
        }
        Some(CompletionCategory::Delete) => {
            let trashed = event.operation_type == WriteOperationType::Trash;
            crate::analytics::events::capture("delete_used", json!({ "trashed": trashed, "item_count": bucket }));
        }
        Some(CompletionCategory::ArchiveEdit) => {
            crate::analytics::events::capture("archive_edit_completed", json!({ "item_count": bucket }));
        }
        None => {}
    }
}

/// Which completion event a progressed op's `write-complete` counts under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompletionCategory {
    /// `file_transfer_completed`.
    FileTransfer,
    /// `delete_used`.
    Delete,
    /// `archive_edit_completed`.
    ArchiveEdit,
}

/// The completion category for `operation_type`, `None` for an instant op.
pub(super) fn completion_category(operation_type: WriteOperationType) -> Option<CompletionCategory> {
    match operation_type {
        WriteOperationType::Copy | WriteOperationType::Move => Some(CompletionCategory::FileTransfer),
        WriteOperationType::Delete | WriteOperationType::Trash => Some(CompletionCategory::Delete),
        WriteOperationType::ArchiveEdit | WriteOperationType::Compress => Some(CompletionCategory::ArchiveEdit),
        // Instant metadata ops (`manager::run_instant`) never reach this function:
        // they return their `Result` inline and produce no `WriteCompleteEvent`.
        // Their drivers report an `InstantMutationFact` instead (`create.rs`,
        // `rename.rs`), so these arms stay explicit — a catch-all `_`
        // would let a future progressed op type silently skip analytics instead of
        // failing to compile here.
        WriteOperationType::Rename | WriteOperationType::CreateFolder | WriteOperationType::CreateFile => None,
        // Only a tag write's reversal runs managed; the write reports its own facts.
        WriteOperationType::Tag => None,
    }
}
