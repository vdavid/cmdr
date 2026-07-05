//! How a copy/move-into collision with an existing archive entry resolves: either
//! a pre-resolved policy applied non-interactively, or an interactive Stop-mode
//! prompt (storing the oneshot sender BEFORE the emit, honoring the shared
//! `ApplyToAll` latch). Also the conflict-policy comparison helpers the planner
//! enacts a resolution with (strict conditional overwrite, unique-name generation).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use uuid::Uuid;

use super::super::OperationEventSink;
use super::super::conflict::{ApplyToAll, apply_to_all_effective, apply_to_all_record};
use super::super::state::{ConflictResolutionResponse, WriteOperationState};
use super::super::types::{ConflictResolution, WriteConflictEvent};
use super::engine::PlanError;
use crate::file_system::volume::backends::archive::ArchiveIndex;
use crate::ignore_poison::IgnorePoison;

/// How a copy/move-into resolves collisions with existing archive entries.
pub(super) enum ConflictMode<'a> {
    /// A pre-resolved policy applied to every collision, no prompt. `Stop` in this
    /// mode is a hard `DestinationExists` (the interactive path handles real Stop).
    Policy(ConflictResolution),
    /// Interactive per-file prompting (the FE's Stop UX): each file collision emits
    /// a `write-conflict` and blocks on the user's answer, honoring the shared
    /// `ApplyToAll` latch. Dir collisions never reach here (they merge silently).
    Interactive {
        events: &'a dyn OperationEventSink,
        operation_id: &'a str,
        state: &'a Arc<WriteOperationState>,
        apply_to_all: &'a mut ApplyToAll,
    },
}

/// Produces the concrete resolution for a collision. `Policy` returns its fixed
/// choice; `Interactive` consults the `ApplyToAll` latch and otherwise prompts the
/// user (storing the oneshot sender BEFORE emitting `write-conflict`, then blocking
/// on the answer — the Stop-mode ordering must-know).
pub(super) fn resolve_effective(
    mode: &mut ConflictMode<'_>,
    inner: &str,
    src_path: &Path,
    archive_path: &Path,
    index: &ArchiveIndex,
    is_file_to_folder: bool,
) -> Result<ConflictResolution, PlanError> {
    match mode {
        ConflictMode::Policy(c) => Ok(*c),
        ConflictMode::Interactive {
            events,
            operation_id,
            state,
            apply_to_all,
        } => {
            if let Some(saved) = apply_to_all_effective(apply_to_all, is_file_to_folder) {
                return Ok(saved);
            }
            let response = prompt_archive_conflict(
                *events,
                operation_id,
                state,
                index,
                inner,
                src_path,
                archive_path,
                is_file_to_folder,
            )?;
            apply_to_all_record(
                apply_to_all,
                is_file_to_folder,
                response.resolution,
                response.apply_to_all,
            );
            Ok(response.resolution)
        }
    }
}

/// Emits a `write-conflict` for an in-archive file collision and blocks on the
/// user's answer. Stores the oneshot sender BEFORE the emit (a responder can only
/// answer a conflict it has observed; emit-first races the take and hangs the
/// recv). A dropped sender (cancel) surfaces as `PlanError::Cancelled`.
#[allow(
    clippy::too_many_arguments,
    reason = "the prompt gathers both sides' metadata from distinct sources (local file + archive index); bundling adds ceremony"
)]
fn prompt_archive_conflict(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    index: &ArchiveIndex,
    inner: &str,
    src_path: &Path,
    archive_path: &Path,
    is_file_to_folder: bool,
) -> Result<ConflictResolutionResponse, PlanError> {
    let node = index.get(inner);
    let dest_size = node.as_ref().and_then(|n| n.size);
    let dest_modified = node.as_ref().and_then(|n| n.modified);
    let src_meta = std::fs::metadata(src_path).ok();
    let source_size = src_meta.as_ref().map(std::fs::Metadata::len);
    let source_modified = src_meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let destination_is_newer = matches!((source_modified, dest_modified), (Some(s), Some(d)) if d > s);
    let size_difference = match (dest_size, source_size) {
        (Some(d), Some(s)) => Some(d as i64 - s as i64),
        _ => None,
    };

    // Store the sender BEFORE the emit (see doc comment); released as the
    // statement ends, never held across the emit or the blocking recv.
    let (tx, rx) = tokio::sync::oneshot::channel();
    *state.conflict_resolution_tx.lock_ignore_poison() = Some(tx);

    events.emit_conflict(WriteConflictEvent {
        operation_id: operation_id.to_string(),
        source_path: src_path.display().to_string(),
        destination_path: archive_path.join(inner).display().to_string(),
        source_size,
        destination_size: dest_size,
        source_modified,
        destination_modified: dest_modified,
        destination_is_newer,
        size_difference,
        source_is_directory: false,
        destination_is_directory: is_file_to_folder,
    });

    // Blocking recv: the planner runs on the blocking pool (like the local-FS Stop
    // path), so parking this thread on the oneshot is correct. A dropped sender
    // (cancel) returns `Err` → `Cancelled`.
    rx.blocking_recv().map_err(|_| PlanError::Cancelled)
}

/// Whether a conditional policy overwrites the existing entry: `OverwriteSmaller`
/// only when the destination is strictly smaller than the source, `OverwriteOlder`
/// only when the destination is strictly older. Missing metadata never overwrites
/// (strict comparison, matching the local-FS conflict reducer).
pub(super) fn conditional_overwrites(
    conflict: ConflictResolution,
    index: &ArchiveIndex,
    inner: &str,
    src_path: &Path,
) -> bool {
    let Some(node) = index.get(inner) else {
        return false;
    };
    let Ok(src_meta) = std::fs::metadata(src_path) else {
        return false;
    };
    match conflict {
        ConflictResolution::OverwriteSmaller => node.size.is_some_and(|dest_size| dest_size < src_meta.len()),
        ConflictResolution::OverwriteOlder => {
            let (Some(dest_mtime), Ok(src_mtime)) = (node.modified, src_meta.modified()) else {
                return false;
            };
            let Ok(src_secs) = src_mtime.duration_since(std::time::UNIX_EPOCH) else {
                return false;
            };
            dest_mtime < src_secs.as_secs() as i64
        }
        _ => false,
    }
}

/// Finds a unique inner path by appending ` (1)`, ` (2)`, … before the extension,
/// avoiding both existing archive entries and already-planned paths.
pub(super) fn find_unique_inner(inner: &str, index: &ArchiveIndex, planned: &HashSet<String>) -> String {
    let (stem, ext) = match inner.rsplit_once('.') {
        // Keep an extension only when there's a stem before the dot (not a dotfile).
        Some((stem, ext)) if !stem.rsplit('/').next().unwrap_or(stem).is_empty() => {
            (stem.to_string(), format!(".{ext}"))
        }
        _ => (inner.to_string(), String::new()),
    };
    for n in 1..=9999 {
        let candidate = format!("{stem} ({n}){ext}");
        if !index.exists(&candidate) && !planned.contains(&candidate) {
            return candidate;
        }
    }
    // Astronomically unlikely; fall back to a uuid suffix so we never loop forever.
    format!("{stem} ({}){ext}", Uuid::new_v4())
}
