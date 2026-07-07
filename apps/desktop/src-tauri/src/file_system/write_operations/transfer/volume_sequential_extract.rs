//! One-pass bulk extraction for SEQUENTIAL archive sources (compressed tar /
//! solid 7z), where a per-entry `open_read_stream` would re-decode the whole
//! prefix and make a subtree extract O(n²).
//!
//! `copy_single_path`'s directory branch routes a source whose
//! `Volume::extraction_is_sequential` is `true` here (a compressed tar or solid
//! 7z); every other directory source keeps the per-entry
//! `copy_directory_streaming` walk, so this is zero-regression for random-access
//! sources. See `write_operations/transfer/DETAILS.md` § "One-pass sequential
//! extract" for the full mechanism.

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::super::state::WriteOperationState;
use super::volume_strategy::{CreatedPaths, MergeCtx, copy_directory_streaming, note_pending_for_local_dest};
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// One resolved file write the planning pass records: where the bytes land, and
/// (for a file→file Overwrite safe-replace) the original to swap the temp over
/// once written.
pub(super) struct PlannedWrite {
    pub(super) dest_path: PathBuf,
    pub(super) replace_after_write: Option<PathBuf>,
}

/// The plan the one-pass extractor builds in its first pass and consumes in its
/// second: source file path → resolved destination write. A file the conflict
/// resolver decided to SKIP is simply absent (the data pass drains and drops it).
/// Keyed by the full source path (`archive_path/inner`), matching what the
/// extractor reports per member, so the data pass looks each member up directly.
/// Interior-mutable (like [`CreatedPaths`]) because `copy_directory_streaming`
/// threads it behind a shared `&`.
#[derive(Default)]
pub(super) struct ExtractPlan {
    writes: Mutex<HashMap<PathBuf, PlannedWrite>>,
}

impl ExtractPlan {
    pub(super) fn record(&self, source_path: PathBuf, write: PlannedWrite) {
        self.writes.lock_ignore_poison().insert(source_path, write);
    }

    /// Removes and returns the planned write for `source_path`, or `None` if the
    /// file was skipped / not part of the subtree. `remove` (not `get`) so a
    /// duplicate archive member with the same path is served once.
    fn take(&self, source_path: &Path) -> Option<PlannedWrite> {
        self.writes.lock_ignore_poison().remove(source_path)
    }
}

/// Extracts a subtree from a SEQUENTIAL source in ONE decode pass, instead of the
/// O(n²) per-file `open_read_stream` re-decode the tree walk would do. Two phases:
///
/// 1. **Plan** — run [`copy_directory_streaming`] in plan mode. This reuses that
///    function's entire merge machinery: it creates the whole destination directory
///    structure (empty dirs included, since it walks the tree), resolves every file
///    conflict (policy, Stop-prompt, apply-to-all latch, type mismatches,
///    safe-replace, Rename reservation), records newly-created dirs in `created`
///    for rollback, and records each surviving file's resolved destination in the
///    plan. It streams no bytes.
/// 2. **Data** — open the one-pass extractor and stream each file the plan kept,
///    in ARCHIVE order, through the destination's `write_from_stream` (safe
///    overwrite, downloads-watcher registration, fsync — all inherited unchanged).
///    Files the plan skipped are drained. Cancellation is checked between members.
///
/// Data-safety parity with the per-entry path: dirs and conflicts are resolved by
/// the same `copy_directory_streaming` code, and each byte write goes through the
/// same `write_from_stream` + `finalize_safe_replace` as `stream_pipe_file`, so
/// the partial-cleanup, safe-replace, and rollback contracts all carry over.
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors copy_single_path's argument list; the sequential path needs the same source/dest volumes, paths, state, rollback ledger, progress callbacks, and merge context."
)]
pub(super) async fn extract_sequential_subtree(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn(u64) + Sync),
    merge: Option<&MergeCtx<'_>>,
) -> Result<u64, VolumeError> {
    // Phase 1: build the directory structure + resolve conflicts, recording each
    // file's resolved destination in the plan (no bytes streamed).
    let plan = ExtractPlan::default();
    Box::pin(copy_directory_streaming(
        source_volume,
        source_path,
        dest_volume,
        dest_path,
        state,
        created,
        on_file_progress,
        on_file_complete,
        merge,
        Some(&plan),
    ))
    .await?;

    // Phase 2: one decode pass over the subtree's files.
    let mut extractor = source_volume.open_sequential_extract(source_path).await?;
    let mut total_bytes = 0u64;
    while let Some(file) = extractor.next_file().await? {
        if super::super::state::is_cancelled(&state.intent) {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
        }
        // Not in the plan ⇒ the file was skipped by conflict resolution (or isn't
        // wanted). Drop it; the next `next_file` drains its bytes.
        let Some(planned) = plan.take(&file.source_path) else {
            continue;
        };

        // Register the destination before the write, exactly as `stream_pipe_file`
        // does (covers a Downloads-landing local dest; a no-op for MTP/SMB).
        note_pending_for_local_dest(dest_volume, &planned.dest_path);
        let stream = extractor.current_stream();
        let bytes = dest_volume
            .write_from_stream(&planned.dest_path, file.size, stream, on_file_progress)
            .await?;

        // Safe-replace finalize for a file→file Overwrite (same as the per-entry
        // path): the temp holds the complete new bytes; swap it over the original.
        let recorded = match planned.replace_after_write {
            Some(orig) => {
                super::volume_conflict::finalize_safe_replace(dest_volume, &planned.dest_path, &orig).await?;
                orig
            }
            None => planned.dest_path,
        };
        created.record_file(recorded);
        total_bytes += bytes;
        on_file_complete(bytes);
    }

    Ok(total_bytes)
}
