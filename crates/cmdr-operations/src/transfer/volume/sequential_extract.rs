//! One-pass bulk extraction for SEQUENTIAL archive sources (compressed tar /
//! solid 7z), where a per-entry `open_read_stream` would re-decode the whole
//! prefix and make a subtree extract O(n²).
//!
//! `copy_single_path`'s directory branch routes a source whose
//! `Volume::extraction_is_sequential` is `true` here (a compressed tar or solid
//! 7z); every other directory source keeps the per-entry
//! `copy_directory_streaming` walk, so this is zero-regression for random-access
//! sources. See `volume/DETAILS.md` § "One-pass sequential extract" for the
//! full mechanism.
//!
//! ❗ The one place a transfer doesn't start files in tree order: the plan pass
//! walks the tree sorted like every merge, but the data pass lands members in
//! the ARCHIVE's order, because the stream decodes once and decides. A progress
//! row only ever claims its own state, so the tree stays honest; the rows below
//! the active one just aren't necessarily next. `volume/DETAILS.md` §
//! "Processing order".

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::super::super::state::WriteOperationState;
use super::super::super::transfer_tree::NodeId;
use super::super::retry::after_a_cut_off_landing_wait;
use super::super::staged_write::{LandingError, Resumed, StagedWrite};
use super::super::transfer_driver::SourceProgress;
use super::super::volume_outage::{self, OutageEnd, OutageFailure, StartedOn, lost_connection, wait_after_failure};
use super::folder_metadata::{FolderMetadata, SourceFolderDates};
use super::merge::copy_directory_streaming;
use super::merge_ctx::{CreatedPaths, MergeCtx};
use super::strategy::{LandingName, Replaces, resolve_staging, staging_for};
use super::transfer_error::{AtPath, PathedVolumeError};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{Volume, VolumeError};

/// One resolved file write the planning pass records: where the bytes land, and
/// what that does to a file at the name (for a file→file Overwrite
/// safe-replace, the original to swap the temp over once written).
pub(super) struct PlannedWrite {
    pub(super) dest_path: PathBuf,
    pub(super) replaces: Replaces,
    /// Whether a conflict resolution picked `dest_path`, which is what the
    /// landing needs to tell its own placeholder from the user's file
    /// (`staged_write.rs::LandingName`).
    pub(super) landing: LandingName,
    /// The member's row in the transfer tree, found by the plan pass's walk.
    pub(super) row: Option<NodeId>,
}

/// The plan the one-pass extractor builds in its first pass and consumes in its
/// second: source file path → resolved destination write. A file the conflict
/// resolver decided to SKIP is simply absent (the data pass drains and drops it).
/// Keyed by the full source path (`archive_path/inner`), matching what the
/// extractor reports per member, so the data pass looks each member up directly.
/// Interior-mutable (like [`CreatedPaths`]) because `copy_directory_streaming`
/// threads it behind a shared `&`.
// DEFAULT-OK: an empty plan is a first pass that hasn't planned a write yet, and an
// absent entry already carries the meaning "skipped".
#[derive(Default)]
pub(super) struct ExtractPlan {
    writes: Mutex<HashMap<PathBuf, PlannedWrite>>,
    /// The folders the planning pass created, stamped only once the data pass
    /// has landed every member in them.
    folder_metadata: Mutex<FolderMetadata>,
}

impl ExtractPlan {
    pub(super) fn record(&self, source_path: PathBuf, write: PlannedWrite) {
        self.writes.lock_ignore_poison().insert(source_path, write);
    }

    /// Hands over the folders the planning pass created, for the data pass to
    /// stamp when it's done.
    pub(super) fn hold_folder_metadata(&self, folders: FolderMetadata) {
        *self.folder_metadata.lock_ignore_poison() = folders;
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
/// same `write_from_stream` + `StagedWrite` landing as `stream_pipe_file`, so
/// the partial-cleanup, safe-replace, and rollback contracts all carry over.
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors copy_single_path's argument list; the sequential path needs the same source/dest volumes, paths, state, rollback ledger, progress accounting, and merge context."
)]
pub(super) async fn extract_sequential_subtree(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    progress: &Arc<SourceProgress>,
    merge: Option<&MergeCtx<'_>>,
    // The source folder's own dates, for `copy_directory_streaming`.
    source_dates: SourceFolderDates,
) -> Result<u64, PathedVolumeError> {
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
        progress,
        merge,
        Some(&plan),
        source_dates,
    ))
    .await?;

    // Phase 2: one decode pass over the subtree's files.
    let mut extractor = source_volume
        .open_sequential_extract(source_path)
        .await
        .at_source(source_path)?;
    let mut total_bytes = 0u64;
    while let Some(file) = extractor.next_file().await.at_source(source_path)? {
        // The cooperative boundary, between members. One decode pass can't be
        // re-entered, so this is the only place the extract can stop, and one
        // iteration covers the remainder of the subtree whenever skipped members
        // drain through it.
        if state.stop_or_park_async().await {
            return Err(VolumeError::Cancelled("Operation cancelled by user".into())).at_source(source_path);
        }
        // Not in the plan ⇒ the file was skipped by conflict resolution (or isn't
        // wanted). Drop it; the next `next_file` drains its bytes.
        let Some(planned) = plan.take(&file.source_path) else {
            continue;
        };

        // Stage the write on a `.cmdr-tmp-*` sibling unless the destination
        // lands this write in one shot (`resolve_staging`), so a killed extract
        // leaves nothing at a final name; an overwrite's lands delete-first
        // (`Replaces::Swap`). Same contract as `stream_pipe_file`; unlike it, a sequential
        // source can't be re-read, so a destination that can't land a staged write
        // fails the extract instead of falling back.
        let length = cmdr_fs::volume::StreamLength::Known(file.size);
        let lands_whole = dest_volume.write_is_single_shot(length).await || dest_volume.publishes_writes_whole();
        let staging = resolve_staging(staging_for(&planned.replaces, planned.landing), lands_whole);
        // Register the destination before the write, exactly as `stream_pipe_file`
        // does (covers a Downloads-landing local dest; a no-op for MTP/SMB).
        let announced_dest = state.scope().host().announce_on(&**dest_volume, &planned.dest_path);
        let staged = StagedWrite::begin(state, &**dest_volume, announced_dest, staging);
        // The member reports its own mode off the archive's index; an extract
        // adds the archive's quarantine to its xattrs (`extract_quarantine.rs`).
        // A member that fails to decode comes back as the WRITE's error; the
        // watch is what still names the archive for it.
        let (stream, source_reads) = super::source_reads::watch(extractor.current_stream(), &file.source_path, length);
        let stream = super::extract_quarantine::quarantine_stream(state, source_volume, stream);
        let lost = super::super::super::metadata_losses::file_losses(&**source_volume, &**dest_volume, &*stream);
        // One decode pass means one member at a time, but this member still
        // takes a leaf handle of its own: it is what carries its bytes on the
        // bar while it writes, and what takes them back off if the write fails.
        let leaf = progress.begin_leaf(planned.row);
        let on_chunk = |progress: cmdr_fs::volume::StreamWriteProgress| leaf.on_chunk(progress.bytes_written);
        let written = dest_volume
            .write_from_stream(staged.target(), staged.write_mode(), length, stream, &on_chunk)
            .await;
        let written = match written {
            Ok(written) => written,
            Err(e) => {
                staged.abandon(dest_volume).await;
                return Err(source_reads.label(e, &file.source_path, &planned.dest_path));
            }
        };
        land_member(state, dest_volume, staged, written.bytes, &planned.dest_path).await?;
        // Landed, so the ` (N)` placeholder the plan pass reserved here is filled.
        // Every one this pass never reaches (a failure, a cancel between members)
        // stays on the op's ledger for the post-loop to take back
        // (`naming.rs::take_back_unfilled_reservations`).
        state.claimed_names.release_placeholder(&planned.dest_path);
        // What the destination refused on the way joins what its caps couldn't
        // hold.
        state.metadata_losses.note_file(lost.union(written.refused()));
        let bytes = written.bytes;

        // A landed overwrite is recorded the way `merge.rs::copy_leaf` records
        // it: an op that overwrote isn't rollbackable, and the journal can only
        // say so if it's told.
        if planned.replaces.overwrites() {
            created.record_overwrite();
        }
        created.record_file(planned.dest_path, bytes);
        total_bytes += bytes;
        leaf.complete(bytes);
    }

    // Every member landed, so the folders the planning pass created take their
    // source dates now; a failed or stopped pass returned above and dates none.
    let folders = std::mem::take(&mut *plan.folder_metadata.lock_ignore_poison());
    folders.stamp(source_volume, dest_volume, state).await;

    Ok(total_bytes)
}

/// Gives one member's written temp its name, riding out an outage the landing
/// meets the way `stream_pipe_file` does: a step the transport cut off waits
/// for the destination (or, with nothing to wait for, takes the per-file retry's
/// backoff), then settles what it did and lands what's left
/// (`staged_write::CutOffLanding`).
///
/// ❗ A one-pass decode can't run the member again, so whatever would run the
/// file again fails it here instead, and with no source to compare a gone temp
/// against, a cleared-way landing whose temp is gone ❌ never counts as landed.
async fn land_member(
    state: &Arc<WriteOperationState>,
    dest_volume: &Arc<dyn Volume>,
    staged: StagedWrite,
    bytes: u64,
    dest_path: &Path,
) -> Result<(), PathedVolumeError> {
    let volumes = [dest_volume];
    let mut started_on = StartedOn::now(&volumes);
    let mut cut_off = match staged.commit(dest_volume, bytes).await {
        Ok(()) => return Ok(()),
        Err(LandingError::Failed(failure)) => return Err(PathedVolumeError::from_landing(failure, dest_path)),
        Err(LandingError::CutOff(cut_off)) => cut_off,
    };
    let (mut outages, mut attempt) = (0, 1);
    loop {
        let ended = wait_after_failure(state, cut_off.error(), &volumes, &started_on, &mut outages).await;
        let ended = after_a_cut_off_landing_wait(ended, cut_off.error(), &mut attempt, state).await;
        if ended != OutageEnd::Back {
            let lost = lost_connection(ended, cut_off.error(), &volumes);
            return match cut_off.give_up(dest_volume).await {
                Ok(()) => Ok(()),
                Err(_) if ended == OutageEnd::Cancelled => Err(volume_outage::cancelled()).at_destination(dest_path),
                Err(failure) => Err(PathedVolumeError::from_landing(failure, dest_path).with_lost_connection(lost)),
            };
        }
        started_on = StartedOn::now(&volumes);
        match cut_off.resume(dest_volume, None).await {
            Resumed::Landed => return Ok(()),
            Resumed::RunAgain(error) => return Err(error).at_destination(dest_path),
            Resumed::Failed(failure) => return Err(PathedVolumeError::from_landing(failure, dest_path)),
            Resumed::Stopped => return Err(volume_outage::cancelled()).at_destination(dest_path),
            Resumed::CutOff(again) => cut_off = again,
        }
    }
}
