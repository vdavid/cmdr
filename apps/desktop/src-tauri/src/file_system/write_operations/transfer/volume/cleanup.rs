//! Volume cleanup / rollback helpers for the volume-aware copy and move paths.
//! Shared by `volume::copy` and `volume::r#move`, so they live here rather than
//! inside either operation module.
//!
//! **Three deletes, split by capability, and only [`remove_tree`] recurses.**
//! Cleanup and rollback reach for [`delete_written_file`] (one node, no listing)
//! and [`prune_created_dir_if_empty`] (empty-only, and it establishes the
//! emptiness itself); a tree removal has to name its authorization in the type
//! ([`TreeRemoval`]). That's what keeps a wrong "is this a directory?" belief
//! from reaching a recursive delete: on the cleanup path there isn't one in
//! scope. `DETAILS.md` § "Three ways to delete, and who may use each".

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::super::super::event_sinks::OperationEventSink;
use super::super::super::ledger::WrittenFile;
use super::super::super::state::{StopMeans, WriteOperationState, update_operation_status};
use super::super::super::types::{WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::transfer_error::{AtPath, PathedVolumeError};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

use cmdr_fs::staging::STAGING_TEMP_MARKER as TEMP_INFIX;

/// How old a `.cmdr-tmp-*` leftover must be before a starting transfer reaps it.
///
/// Deliberately generous: the age gate is what keeps this from deleting a temp
/// another Cmdr instance (or another operation on the same share) is actively
/// writing. A live staged write touches its temp on every chunk, so its mtime
/// never gets near an hour old — even one parked on a destination-side
/// foreground yield, which is hard-capped at a second.
const STALE_TEMP_MIN_AGE: Duration = Duration::from_secs(60 * 60);

/// Rolls back copied files on a volume with progress events, matching the local copy's
/// `rollback_with_progress` pattern. Deletes paths in reverse order so that files inside
/// directories are removed before the directories themselves.
///
/// `copied_paths` are the individual destination FILES the operation wrote (never a merged
/// directory root), each with the identity it landed with. It's a **stack**: every entry is
/// popped as it's reversed, so a reversal the user stops halfway leaves a ledger that
/// claims exactly what's still on the volume. After the files, `created_dirs` — the
/// directories this operation NEWLY created — are pruned deepest-first, empty-only. A
/// directory that still holds a pre-existing sibling (a dest-only file the user already
/// had, or a kept-partial under cancel) is left in place, so rollback never destroys data
/// this operation didn't write.
///
/// Neither loop can recurse: they call [`delete_written_file`] and
/// [`prune_created_dir_if_empty`], so a directory that reaches this ledger by mistake costs
/// the user a leftover, never a file.
///
/// Returns `true` if rollback completed fully, `false` if the user cancelled it.
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
pub(super) async fn volume_rollback_with_progress(
    volume: &Arc<dyn Volume>,
    copied_paths: &mut Vec<WrittenFile>,
    created_dirs: &[PathBuf],
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> bool {
    let paths_to_delete = copied_paths.len();
    let mut paths_deleted = 0usize;
    let mut last_progress_time = Instant::now();

    // Emit initial rollback phase event
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Copy,
            WriteOperationPhase::RollingBack,
            None,
            files_at_cancel,
            files_total,
            bytes_at_cancel,
            bytes_total,
        ),
    );
    update_operation_status(
        operation_id,
        WriteOperationPhase::RollingBack,
        None,
        files_at_cancel,
        files_total,
        bytes_at_cancel,
        bytes_total,
    );

    // Delete newest first, draining the ledger as they go. The intent is read
    // BEFORE the pop: an entry taken off the ledger and then left standing would
    // be a file on the volume nothing claims any more.
    loop {
        // Check if the user cancelled the rollback itself. This runs UNDER an
        // operation whose intent already reads `RollingBack`, so only `Stopped`
        // means stop (see `StopMeans`).
        if StopMeans::IntentReachesStopped.requested(&state.intent) {
            log::info!(
                "volume_rollback_with_progress: rollback cancelled at {}/{} paths, keeping remaining",
                paths_deleted,
                paths_to_delete,
            );
            return false;
        }

        let Some(entry) = copied_paths.pop() else {
            break;
        };
        let path = &entry.path;

        // One node each: these are the FILES this operation wrote.
        if let Err(e) = delete_written_file(volume, path).await {
            log::warn!(
                "volume_rollback_with_progress: couldn't delete {}: {:?}",
                e.path.display(),
                e.error
            );
        }
        paths_deleted += 1;

        // Throttled progress events with decreasing values
        if last_progress_time.elapsed() >= state.progress_interval {
            let remaining_files = files_at_cancel.saturating_sub(paths_deleted);
            let remaining_bytes = if paths_to_delete > 0 {
                bytes_at_cancel - (bytes_at_cancel as f64 * paths_deleted as f64 / paths_to_delete as f64) as u64
            } else {
                0
            };

            let current_file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Copy,
                    WriteOperationPhase::RollingBack,
                    Some(current_file_name.clone()),
                    remaining_files,
                    files_total,
                    remaining_bytes,
                    bytes_total,
                ),
            );
            update_operation_status(
                operation_id,
                WriteOperationPhase::RollingBack,
                Some(current_file_name),
                remaining_files,
                files_total,
                remaining_bytes,
                bytes_total,
            );
            last_progress_time = Instant::now();
        }
    }

    // Prune the directories this operation newly created, deepest-first.
    // `created_dirs` is in creation order (shallowest first), so iterating in
    // reverse empties leaves before their parents are tried.
    for dir in created_dirs.iter().rev() {
        if StopMeans::IntentReachesStopped.requested(&state.intent) {
            return false;
        }
        prune_created_dir_if_empty(volume, dir).await;
    }

    true
}

/// Folds the writes that were still in flight into the rollback ledger, marked as
/// this operation's OWN partials: the serial driver's single `last_dest_path`
/// plus the concurrent driver's one-per-in-flight-task set.
///
/// **They go in as `WrittenIdentity::OwnPartial`, never as files whose identity
/// is unknown.** A partial has no size and no complete file to recognize, by
/// construction — so a reversal that treated "can't verify it" as "leave it
/// alone" would strand a truncated file at the destination, which is the exact
/// outcome cancelling mid-file exists to avoid. Nothing but this operation can
/// plausibly own a destination path that never held a complete file, so these are
/// removed on sight. Pinned by the E2E "cancelling inside one large file leaves
/// no partial behind".
///
/// A path the ledger already carries stays as it is: the completed write it
/// describes is the better record of the two, and pushing a second entry would
/// make the reversal walk the same path twice.
pub(super) fn append_own_partials(
    ledger: &mut Vec<WrittenFile>,
    last_dest_path: Option<PathBuf>,
    in_flight_partials: &[PathBuf],
) {
    for partial in last_dest_path.into_iter().chain(in_flight_partials.iter().cloned()) {
        if ledger.iter().any(|entry| entry.path == partial) {
            continue;
        }
        ledger.push(WrittenFile::own_partial(partial));
    }
}

/// Removes the destination partials a Stopped or errored copy left behind: the
/// serial driver's single `last_dest_path` plus the concurrent driver's
/// one-per-in-flight-task set, already net of anything that finished.
///
/// Best-effort per path: a partial that refuses to go is logged and the sweep
/// moves on. One node each, via [`delete_written_file`] — a directory source's
/// dest ROOT that reached this list through a driver bug is a merged folder
/// holding the user's own files, and it must survive.
pub(super) async fn clean_partial_writes(volume: &Arc<dyn Volume>, partials: &[PathBuf], operation_id: &str) {
    for partial_path in partials {
        log::debug!(
            "copy_volumes_with_progress: cleaning up partial file {} for op={}",
            partial_path.display(),
            operation_id,
        );
        if let Err(e) = delete_written_file(volume, partial_path).await {
            log::warn!(
                "copy_volumes_with_progress: couldn't clean up partial {}: {:?}",
                e.path.display(),
                e.error
            );
        }
    }
}

/// Removes the staged `.cmdr-tmp-*` partials this operation was still writing
/// when its tasks were abandoned.
///
/// The driver drops in-flight copy tasks on cancel and on the first failure, so
/// their futures never reach their own cleanup. What they left behind is exactly
/// what `state.in_flight_temps` still lists: a write that SUCCEEDED removes its
/// entry before landing, so a temp holding committed data (a landing that failed
/// after the bytes were complete) is never in this set and is never touched here.
///
/// Best-effort: a temp whose task is wedged with an open handle may refuse to
/// delete, which is why it wears a recognizable name.
///
/// **Skipped entirely once tier 2 has fired** (`state.backend_abort`). Every
/// delete here is a round trip through the destination, and the reason the abort
/// fired is that the destination stopped answering — so this pass would hold the
/// quit deadline for a second time, right after the streaming write stopped
/// holding it. The entries stay in both ledgers instead, and
/// `write_operations::in_flight_temps`'s startup sweep clears them at the next
/// launch, with no age gate, off the launch thread.
pub(super) async fn clean_abandoned_staged_writes(volume: &Arc<dyn Volume>, state: &Arc<WriteOperationState>) {
    if state.backend_abort.is_cancelled() {
        log::info!(
            target: "copy",
            "clean_abandoned_staged_writes: the app is shutting down, leaving the staged partial(s) to the startup sweep"
        );
        return;
    }
    let temps: Vec<PathBuf> = std::mem::take(&mut *state.in_flight_temps.lock_ignore_poison());
    if temps.is_empty() {
        return;
    }
    log::info!(
        target: "copy",
        "clean_abandoned_staged_writes: removing {} staged partial(s) left by abandoned tasks",
        temps.len()
    );
    for temp in temps {
        if let Err(e) = volume.delete(&temp).await {
            log::warn!(
                target: "copy",
                "clean_abandoned_staged_writes: couldn't remove {}: {e}",
                temp.display()
            );
        }
    }
}

/// Reaps `.cmdr-tmp-*` leftovers a crash or force-quit left in the destination
/// directory, at the start of the next transfer into it.
///
/// This is the crash-recovery half of the staged-write invariant: staging means
/// an interrupted transfer leaves a recognizable temp instead of a truncated file
/// at a real name, and this is what eventually clears those temps. It only sees
/// the operation's own destination directory — a leftover deeper inside a copied
/// subtree waits for a transfer into THAT directory — which is where the
/// 2026-07-31 incident's partials were.
///
/// Guards, mirroring `archive_remote_edit::reap_remote_temps`:
/// - **One round trip.** A single `list_directory`, then a `delete` per match.
/// - **Age-gated.** Only leftovers older than [`STALE_TEMP_MIN_AGE`] go, so a
///   temp another instance is streaming into right now is never removed. An entry
///   with no reported mtime is treated as fresh and spared.
/// - **Files only**, and only names carrying the `.cmdr-tmp-` marker.
///
/// Best-effort throughout: a listing or delete failure is logged at debug and
/// never fails or delays the user's transfer.
///
/// **Returns the listing it already paid for**, minus the temps it reaped, so
/// the copy driver can answer its top-level conflict pre-check from it instead
/// of one `get_metadata` round trip per source (`volume/copy.rs`, and
/// `DETAILS.md` § "Answering the pre-check from one listing"). `None` means the
/// listing itself failed — ❌ never read that as "the destination is empty".
pub(super) async fn reap_stale_transfer_temps(volume: &Arc<dyn Volume>, dir: &Path) -> Option<Vec<FileEntry>> {
    let entries = match volume.list_directory(dir, None).await {
        Ok(entries) => entries,
        Err(e) => {
            log::debug!(target: "copy", "skipping stale-temp reap of {}: {e}", dir.display());
            return None;
        }
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let min_age_secs = STALE_TEMP_MIN_AGE.as_secs();

    let mut survivors = Vec::with_capacity(entries.len());
    for entry in entries {
        let is_stale_temp = !entry.is_directory
            && entry.name.contains(TEMP_INFIX)
            && entry
                .modified_at
                .is_some_and(|modified| now_secs.saturating_sub(modified) >= min_age_secs);
        if !is_stale_temp {
            survivors.push(entry);
            continue;
        }
        let temp_path = dir.join(&entry.name);
        log::info!(
            target: "copy",
            "reap_stale_transfer_temps: removing a transfer partial left by an earlier run: {}",
            temp_path.display()
        );
        if let Err(e) = volume.delete(&temp_path).await {
            log::debug!(target: "copy", "couldn't reap stale temp {}: {e}", temp_path.display());
        }
    }
    Some(survivors)
}

/// Removes ONE destination file this operation wrote. ❌ Never recurses, never
/// lists.
///
/// The only delete the rollback loop over `copied_paths` and the post-loop
/// [`clean_partial_writes`] sweep may call. Both take their paths from a ledger
/// of individual files, and a directory reaching either of them is a bug in the
/// bookkeeping — one that used to cost the user a merged folder's worth of
/// pre-existing files and now costs a leftover directory and a warn.
///
/// A path that's already gone is a success: the job is "make sure this isn't
/// there", and a partial that never landed isn't worth a warn. Anything else
/// (the `ENOTEMPTY` of a non-empty directory, a permission refusal) comes back
/// carrying the path.
async fn delete_written_file(volume: &Arc<dyn Volume>, path: &Path) -> Result<(), PathedVolumeError> {
    match volume.delete(path).await {
        Ok(()) | Err(VolumeError::NotFound(_)) => Ok(()),
        Err(error) => Err(PathedVolumeError {
            path: path.to_path_buf(),
            error,
        }),
    }
}

/// Removes a directory this operation created, but only once it's empty.
///
/// **Establishes the emptiness itself**, with a `list_directory`, rather than
/// inferring it from `Volume::delete`'s no-recursion contract. Every shipping
/// backend honors that contract and a conformance assertion holds them to it,
/// but "the user's untouched files survive a rollback" shouldn't rest on a
/// promise a future backend has to remember to keep. A listing that FAILS
/// leaves the directory standing: unknown is not empty.
///
/// Best-effort and quiet — a directory kept because it still holds something is
/// the normal outcome, not a failure.
async fn prune_created_dir_if_empty(volume: &Arc<dyn Volume>, dir: &Path) {
    match volume.list_directory(dir, None).await {
        Ok(entries) if entries.is_empty() => {
            if let Err(e) = volume.delete(dir).await {
                log::debug!(
                    "prune_created_dir_if_empty: couldn't remove empty created dir {}: {:?}",
                    dir.display(),
                    e
                );
            }
        }
        Ok(entries) => log::debug!(
            "prune_created_dir_if_empty: keeping created dir {}, it still holds {} entr{}",
            dir.display(),
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" }
        ),
        Err(e) => log::debug!(
            "prune_created_dir_if_empty: keeping created dir {}, couldn't list it: {:?}",
            dir.display(),
            e
        ),
    }
}

/// Why a caller is allowed to take a whole tree down.
///
/// Fieldless on purpose, with **no `Default` and no `From<bool>`**: a recursive
/// delete is the one thing in this directory that can remove data the user
/// never named, so every call site writes down which authorization it holds.
/// The three variants are the complete list; a fourth sweep has to justify
/// itself by adding one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::file_system::write_operations) enum TreeRemoval {
    /// A cross-type clash (a file landing on a folder) the user resolved with
    /// Overwrite: the destination's type is wrong, so it goes before the source
    /// materializes. `conflict.rs::apply_volume_conflict_resolution`.
    UserChoseOverwriteAcrossTypes,
    /// A cross-volume move sweeping its source, after
    /// `flush_created_destinations` established that the destination landed.
    /// Carries the merge's skipped children in `preserve`. `move.rs`.
    MoveSourceAfterDestinationLanded,
    /// An into-archive move removing the remote originals it pulled, after the
    /// rewrite durably commits. `archive_edit/copy_into.rs`.
    ArchiveMoveSourceAfterCommit,
}

/// Recursively removes a file or directory tree, sparing every path in
/// `preserve` and every ancestor directory still holding one, and reporting the
/// path that actually refused to go.
///
/// **The only recursive delete in this directory.** `why` names the
/// authorization and rides into the log, so a tree removal says who asked for
/// it; ❌ don't add a call site without adding the variant that describes it.
///
/// The `preserve` set is what a MOVE's source sweep rests on. A merge can
/// resolve deep children to Skip (the user chose it, or a conditional policy
/// reduced to it), and a skipped child never landed at the destination: the
/// source copy is the ONLY copy, so an unconditional sweep destroys exactly the
/// data the user declined to move. Pinned by
/// `volume/move_merge_tests.rs::move_folder_merge_never_loses_a_byte_under_every_policy`.
/// A directory goes only once its whole subtree is gone, so preserving one leaf
/// keeps its entire ancestor spine. A child that FAILS to delete counts as
/// preserved too — its parent still holds content, so attempting the parent
/// would only add a misleading `ENOTEMPTY` on top of the real leaf error.
///
/// For directories: lists contents, deletes children (recursing into subdirs),
/// then deletes the directory itself. The sweep keeps going after a child fails
/// so it clears everything it can, and it remembers the FIRST child failure
/// with that child's own path. What comes out of a directory that couldn't be
/// emptied is that remembered child failure, ❌ never the directory's own
/// `ENOTEMPTY`: the surviving child is the diagnosis and the parent's refusal is
/// only its symptom, named after the folder the user selected. A directory that
/// DID go leaves nothing behind to tell anyone about, so a child failure that
/// raced with another deleter stays `Ok` rather than turning a finished move
/// into a reported failure.
pub(in crate::file_system::write_operations) async fn remove_tree(
    volume: &Arc<dyn Volume>,
    path: &Path,
    preserve: &HashSet<PathBuf>,
    why: TreeRemoval,
) -> Result<(), PathedVolumeError> {
    log::debug!(target: "delete", "remove_tree: {} ({why:?})", path.display());
    delete_preserving_inner(volume, path, preserve).await.map(|_| ())
}

/// Recursion body. `Ok(true)` means "content remains under here", so the caller
/// must keep this directory.
async fn delete_preserving_inner(
    volume: &Arc<dyn Volume>,
    path: &Path,
    preserve: &HashSet<PathBuf>,
) -> Result<bool, PathedVolumeError> {
    if preserve.contains(path) {
        return Ok(true);
    }

    let is_dir = match volume.is_directory(path).await {
        Ok(true) => true,
        Ok(false) => false,
        Err(_) => {
            // Path may not exist (already deleted or never fully created). Nothing to do.
            return Ok(false);
        }
    };

    if !is_dir {
        volume.delete(path).await.at(path)?;
        return Ok(false);
    }

    // List directory contents and delete children first
    let children = volume.list_directory(path, None).await.at(path)?;

    let mut first_child_failure: Option<PathedVolumeError> = None;
    let mut content_remains = false;
    for child in &children {
        let child_path = PathBuf::from(&child.path);
        // `.at(&child_path)` at the frame that knows the child: one level up and
        // every leaf failure would answer with this directory's name instead.
        let outcome = if child.is_directory {
            Box::pin(delete_preserving_inner(volume, &child_path, preserve)).await
        } else if preserve.contains(&child_path) {
            Ok(true)
        } else {
            volume.delete(&child_path).await.at(&child_path).map(|()| false)
        };
        match outcome {
            Ok(remains) => content_remains |= remains,
            Err(e) => {
                log::warn!(
                    target: "delete",
                    "remove_tree: couldn't delete {}: {:?}",
                    e.path.display(),
                    e.error
                );
                // The child is still there, so this directory isn't empty either.
                content_remains = true;
                first_child_failure.get_or_insert(e);
            }
        }
    }

    // Something under here survives on purpose (or refused to go): keep this
    // directory, and report the leaf that refused if there was one.
    if content_remains {
        return match first_child_failure {
            Some(child) => Err(child),
            None => Ok(true),
        };
    }

    // Delete the now-empty directory
    match volume.delete(path).await {
        Ok(()) => Ok(false),
        Err(e) => Err(PathedVolumeError {
            path: path.to_path_buf(),
            error: e,
        }),
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
