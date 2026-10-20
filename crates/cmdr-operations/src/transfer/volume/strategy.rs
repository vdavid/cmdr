//! Copy strategy routing for volume-to-volume operations.
//!
//! Every cross-volume copy either (a) uses the APFS clonefile fast path when
//! both sides are `LocalPosixVolume` on the same APFS volume, or (b) pipes bytes
//! through `open_read_stream` + `write_from_stream`.
//!
//! Directories are walked (recursively, in `merge.rs`) so the user can cancel
//! between files. Per-file transfers go through `stream_pipe.rs`.

use std::path::Path;
use std::sync::Arc;

use super::super::super::state::WriteOperationState;
// Re-exported so the sibling test modules (and any future caller reached through
// this module's API) name the staging choice without a second import path.
pub(super) use super::super::staged_write::{LandingName, Replaces, WriteStaging, resolve_staging, staging_for};
use super::super::transfer_driver::{LeafProgressLedger, SourceProgress};
use super::merge::copy_directory_streaming;
use super::merge_ctx::{CreatedPaths, MergeCtx};
use super::preflight::SourceFileFacts;
use super::stream_pipe::stream_pipe_file;
use super::transfer_error::{AtPath, PathedVolumeError};
use cmdr_fs::volume::{Volume, VolumeError};

/// Answers "is this top-level source a directory?" from the preflight hint,
/// probing the source volume only when there is no hint.
///
/// **A missing hint means UNKNOWN, never "file".** A completed scan preview can
/// carry no per-source data at all, and defaulting to `false` there streams a
/// directory as a file AND tells both drivers the destination path is a
/// sweepable partial — which, for a directory merged into the user's own dest
/// folder, means a recursive delete of their data on any failure.
///
/// ❌ Don't probe when a hint IS present: a hinted 15k-source MTP copy would pay
/// 15k parent listings (~2 minutes of stalled dialog) for an answer the scan
/// already has.
///
/// `Err` only when the source can't be stat'd at all (it's gone, or unreadable),
/// which the caller surfaces as that source's failure rather than guessing.
pub(super) async fn resolve_source_is_directory(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    hint: Option<bool>,
) -> Result<bool, VolumeError> {
    match hint {
        Some(known) => Ok(known),
        None => source_volume.is_directory(source_path).await,
    }
}

/// Copies a single path from source volume to destination volume.
///
/// Dispatches on two cases:
/// - Both volumes are `LocalPosixVolume` and the source/destination are on the same APFS volume →
///   delegate to the native `copy_files_start` path upstream (handled in `copy_between_volumes`;
///   this function isn't called for that case).
/// - Otherwise → generic streaming pipe via `open_read_stream` + `write_from_stream`, walking
///   directories recursively so the user can cancel between files.
///
/// `source_is_directory` is `None` when the caller has no preflight hint; see
/// [`resolve_source_is_directory`] for why that must not collapse to `false`.
#[allow(
    clippy::too_many_arguments,
    reason = "Cross-volume copy needs source/dest volumes, paths, the source type hint, the size hint, shared state, the rollback ledger, and the source's progress accounting. Bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: Option<bool>,
    source_facts: SourceFileFacts,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    // This source's view of the operation's leaf accounting. Every file that
    // streams below mints its own [`LeafProgress`] from it, so leaves that
    // overlap each other each hold their own share of the in-flight total.
    progress: &Arc<SourceProgress>,
    // `Some` ⇒ deep clashes inside a merged directory honor the user's file
    // policy (Stop-wait, latch, conditional reduce, type mismatches). `None` ⇒
    // no per-child conflict resolution (the cross-volume move's copy phase,
    // where the dest is a fresh staging area, and tests that don't merge).
    merge: Option<&MergeCtx<'_>>,
    // How the write to `dest_path` (always the file's final name) is staged:
    // whose name it is (`Stage`, `StageOntoClaimedName`), an original a swap
    // replaces (`StageOverOriginal`), or the final name an in-place replace
    // writes over. Every call site derives it the same way:
    // `staging_for(&replaces, landing)`. Only the FILE branch reads it — a
    // directory source's children each get their own staging decision inside
    // the merge walker — and a directory conflict never yields a swap, so
    // passing the same expression everywhere stays correct.
    staging: WriteStaging,
) -> Result<u64, PathedVolumeError> {
    // Check cancellation up front.
    if state.work_stopped() {
        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string())).at_source(source_path);
    }

    let source_is_directory = resolve_source_is_directory(source_volume, source_path, source_is_directory)
        .await
        .at_source(source_path)?;

    if source_is_directory {
        // A sequential source (compressed tar / solid 7z) would re-decode the
        // whole prefix on every per-file `open_read_stream`, making a subtree
        // extract O(n²). Route it to the one-pass extractor instead, which decodes
        // the stream once. Random-access sources (a folder on any real FS, a plain
        // `.tar`, a zip) keep the per-entry walk below — zero regression.
        if source_volume.extraction_is_sequential(source_path) {
            return Box::pin(super::sequential_extract::extract_sequential_subtree(
                source_volume,
                source_path,
                dest_volume,
                dest_path,
                state,
                created,
                progress,
                merge,
                source_facts.folder_dates,
            ))
            .await;
        }
        Box::pin(copy_directory_streaming(
            source_volume,
            source_path,
            dest_volume,
            dest_path,
            state,
            created,
            progress,
            merge,
            None,
            source_facts.folder_dates,
        ))
        .await
    } else {
        // A top-level FILE source records nothing into `created` here: the
        // caller owns that path's rollback bookkeeping (it records the landed
        // name with the overwrite verdict it holds). `created` is for the
        // directory-merge case, where the
        // recursive copy below is the only place that knows which files and
        // newly-created subdirs landed inside a (possibly pre-existing) dest
        // directory.
        // A top-level FILE source is a leaf like any other: it holds its own
        // share of the in-flight total while it streams, so a sibling task's
        // file finishing can't take this one's progress off the bar. Its row
        // is the source's own.
        let leaf = progress.begin_leaf(progress.node());
        let on_chunk = |file_bytes_done: u64, _file_bytes_total: u64| leaf.on_chunk(file_bytes_done);
        let bytes = stream_pipe_file(
            source_volume,
            source_path,
            source_facts,
            dest_volume,
            dest_path,
            state,
            &on_chunk,
            staging,
        )
        .await?;
        leaf.complete(bytes);
        Ok(bytes)
    }
}

/// Pulls one source path (a file or a whole subtree) from `source_volume` into
/// `dest_volume` at `dest_path` with NO conflict resolution — the destination is
/// assumed empty (a fresh scratch dir), so nothing is merged or overwritten.
/// Cancel and pause ride the op's `state` (checked per chunk); the transfer is
/// otherwise silent (no progress events). This is the seam the archive copy-into
/// flow uses to materialize a REMOTE source locally before ingesting it into a
/// zip, so it reuses the exact streaming, recursion, and cancel of the copy
/// engine without exposing the conflict machinery. Returns bytes transferred.
pub(crate) async fn pull_path_to_local(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: bool,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<u64, VolumeError> {
    // A throwaway rollback ledger: on any failure the caller discards the whole
    // scratch dir, so per-file rollback bookkeeping is moot.
    let created = CreatedPaths::default();
    // Nobody is watching this pull, so the accounting reports to no one; what it
    // still does is stop the stream the moment the op is cancelled.
    let progress = LeafProgressLedger::silent_source(Arc::clone(state));
    copy_single_path(
        source_volume,
        source_path,
        // The caller probed the source itself, so this is a known answer.
        Some(source_is_directory),
        // Nothing known about the file: the stream reports the REAL length, so a
        // source whose listed metadata size lies still pulls its true bytes, and
        // the scratch dir this lands in is repackaged rather than handed to
        // anyone, so its modes carry nothing.
        SourceFileFacts::default(),
        dest_volume,
        dest_path,
        state,
        &created,
        &progress,
        None,
        // Fresh scratch destination, no conflicts: nothing is pre-staged.
        WriteStaging::Stage,
    )
    .await
    // The scratch dir is discarded wholesale on any failure and this seam
    // reports no per-item path, so the originating path has no reader here.
    .map_err(|e| e.error)
}

#[cfg(test)]
#[path = "strategy_abort_tests.rs"]
mod abort_tests;
#[cfg(test)]
#[path = "strategy_copy_tests.rs"]
mod copy_tests;
#[cfg(test)]
#[path = "strategy_dest_yield_tests.rs"]
mod dest_yield_tests;
#[cfg(test)]
#[path = "strategy_landing_retry_tests.rs"]
mod landing_retry_tests;
#[cfg(test)]
#[path = "strategy_pause_tests.rs"]
mod pause_tests;
#[cfg(test)]
#[path = "strategy_retry_tests.rs"]
mod retry_tests;
#[cfg(test)]
#[path = "strategy_sequential_tests.rs"]
mod sequential_tests;
#[cfg(test)]
#[path = "strategy_server_side_copy_tests.rs"]
mod server_side_copy_tests;
#[cfg(test)]
#[path = "strategy_single_shot_tests.rs"]
mod single_shot_tests;
#[cfg(test)]
#[path = "strategy_stale_handle_tests.rs"]
mod stale_handle_tests;
// `pub(super)` so sibling test modules under `transfer` (notably
// `volume_move_failure_tests`) reuse the same doubles instead of hand-rolling
// their own.
#[cfg(test)]
#[path = "strategy_test_support.rs"]
pub(super) mod test_support;

#[cfg(test)]
#[path = "strategy_dest_yield_test_support.rs"]
pub(super) mod dest_yield_test_support;

#[cfg(test)]
#[path = "strategy_yield_tests.rs"]
mod yield_tests;
