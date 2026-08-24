//! PII-free PostHog analytics for completed write operations.
//!
//! Two families, because the ops arrive by two different roads:
//!
//! - **Progressed ops** (copy / move / delete / archive edit) settle through
//!   `TauriEventSink::emit_complete` (in `event_sinks.rs`) with a
//!   `WriteCompleteEvent`, and [`emit_completion_analytics`] reads it.
//! - **Instant metadata ops** (rename / new folder / new file) never produce a
//!   `WriteCompleteEvent` at all — `manager::run_instant` returns their `Result`
//!   inline — so they emit from their own driver's single exit via
//!   [`emit_rename_analytics`] / [`emit_create_analytics`].
//!
//! Every emitted property is categorical (op kind, a coarse count bucket, a bool);
//! no names or paths ever cross.

use super::types::{WriteCompleteEvent, WriteOperationType};
use crate::analytics::item_count_bucket;
use crate::operation_log::types::Initiator;

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
        // Instant metadata ops (`manager::run_instant`) never reach this function:
        // they return their `Result` inline and produce no `WriteCompleteEvent`.
        // They emit from their own drivers instead (`emit_rename_analytics` /
        // `emit_create_analytics`), so these arms stay explicit no-ops — a
        // catch-all `_` would let a future progressed op type silently skip
        // analytics instead of failing to compile here.
        WriteOperationType::Rename | WriteOperationType::CreateFolder | WriteOperationType::CreateFile => {}
    }
}

/// Where an instant metadata op landed. An `Archive` target is a `.zip` rewrite
/// routed to the managed archive-edit driver, so it costs orders of magnitude more
/// than the same gesture on a real filesystem and its `done` means "the edit
/// STARTED" (its completion rides `archive_edit_completed`). Folding the two into
/// one number would make the rename/create counts unreadable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstantTarget {
    Volume,
    Archive,
}

impl InstantTarget {
    fn as_token(self) -> &'static str {
        match self {
            InstantTarget::Volume => "volume",
            InstantTarget::Archive => "archive",
        }
    }
}

/// The outcome vocabulary of an instant metadata op: it either landed or it didn't.
/// There's no cancel and no conflict prompt on this path (`manager::run_instant`
/// inserts no `WriteOperationState`), so two tokens cover it completely.
///
/// The failure arm is the point of having an outcome at all: a rename that keeps
/// bouncing off a taken name or a read-only volume is a usability answer, and a
/// success-only count can't tell that from a feature nobody reaches for.
fn instant_outcome<T, E>(result: &Result<T, E>) -> &'static str {
    match result {
        Ok(_) => "done",
        Err(_) => "failed",
    }
}

/// Emits the PII-free event for a completed rename. Called from `rename_managed`'s
/// single exit (including the archive route), so no early return can skip it.
///
/// Props are the initiator (who asked: the person, the agent, or an AI client), where
/// it landed, and whether it worked. Never a name, a path, or the reason it failed.
pub(super) fn emit_rename_analytics<T, E>(initiator: Initiator, target: InstantTarget, result: &Result<T, E>) {
    crate::analytics::posthog::capture("rename_used", instant_props(initiator, target, result));
}

/// Emits the PII-free event for a completed new-folder / new-file. Same shape and
/// same call-site discipline as [`emit_rename_analytics`]; the two creates get
/// separate event names because "do people make folders?" and "do people make files?"
/// are different questions with very different expected answers.
pub(super) fn emit_create_analytics<T, E>(
    op: WriteOperationType,
    initiator: Initiator,
    target: InstantTarget,
    result: &Result<T, E>,
) {
    let props = instant_props(initiator, target, result);
    // The event name is a LITERAL in each arm, never a variable holding one: the
    // `analytics-event-catalog` check reads emitters by matching `capture("…")`, so
    // a name that arrives as a `&str` would be invisible to it and could drift from
    // the catalog forever. Worth the duplicated call.
    match op {
        WriteOperationType::CreateFolder => crate::analytics::posthog::capture("folder_created", props),
        WriteOperationType::CreateFile => crate::analytics::posthog::capture("file_created", props),
        // Unreachable by construction (both call sites pass one of the two above),
        // and a silent no-op beats inventing an event name for a fourth op type.
        _ => {}
    }
}

/// The props both instant-op events carry, built once so the two can't drift.
fn instant_props<T, E>(initiator: Initiator, target: InstantTarget, result: &Result<T, E>) -> serde_json::Value {
    serde_json::json!({
        "initiator": initiator.as_token(),
        "target": target.as_token(),
        "outcome": instant_outcome(result),
    })
}
