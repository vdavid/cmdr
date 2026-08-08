//! Volume cleanup / rollback helpers for the volume-aware copy and move paths.
//!
//! `volume_rollback_with_progress` reverses copied files (with reverse-progress
//! events) on cancel/failure, and `delete_volume_path_recursive` clears a file
//! or directory tree off a volume. Both are shared by `volume::copy` and
//! `volume::r#move`, so they live here rather than inside either operation module.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::super::super::state::{OperationIntent, WriteOperationState, load_intent, update_operation_status};
use super::super::super::types::{OperationEventSink, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::transfer_error::{AtPath, PathedVolumeError};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::Volume;
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
/// directory root). After deleting them, `created_dirs` — the directories this operation
/// NEWLY created — are removed deepest-first with a non-recursive, empty-only delete. A
/// directory that still holds a pre-existing sibling (a dest-only file the user already had,
/// or a kept-partial under cancel) is left in place, so rollback never destroys data this
/// operation didn't write.
///
/// Returns `true` if rollback completed fully, `false` if the user cancelled it.
#[allow(
    clippy::too_many_arguments,
    reason = "Needs the full progress state at cancellation time to emit reverse progress"
)]
pub(super) async fn volume_rollback_with_progress(
    volume: &Arc<dyn Volume>,
    copied_paths: &[PathBuf],
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

    // Delete in reverse order (newest first)
    for path in copied_paths.iter().rev() {
        // Check if user cancelled the rollback (RollingBack → Stopped)
        if load_intent(&state.intent) == OperationIntent::Stopped {
            log::info!(
                "volume_rollback_with_progress: rollback cancelled at {}/{} paths, keeping remaining",
                paths_deleted,
                paths_to_delete,
            );
            return false;
        }

        // Each copied path may be a file or a directory tree, so delete recursively
        if let Err(e) = delete_volume_path_recursive(volume, path).await {
            log::warn!(
                "volume_rollback_with_progress: couldn't delete {} (under {}): {:?}",
                e.path.display(),
                path.display(),
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

    // Prune the directories this operation newly created, deepest-first, with a
    // non-recursive empty-only delete. `created_dirs` is in creation order
    // (shallowest first), so iterating in reverse hits leaves before their
    // parents. A directory that still holds a pre-existing sibling (a dest-only
    // file the user already had) won't be empty, so its `delete` fails with
    // NotFound/IoError on real backends and we leave it standing — exactly the
    // protection that keeps rollback from destroying untouched user data. We
    // deliberately do NOT use `delete_volume_path_recursive` here: that would
    // recurse into and delete those pre-existing siblings.
    for dir in created_dirs.iter().rev() {
        if load_intent(&state.intent) == OperationIntent::Stopped {
            return false;
        }
        if let Err(e) = volume.delete(dir).await {
            log::debug!(
                "volume_rollback_with_progress: not removing created dir {} (likely non-empty, kept): {:?}",
                dir.display(),
                e
            );
        }
    }

    true
}

/// Removes the destination partials a Stopped or errored copy left behind: the
/// serial driver's single `last_dest_path` plus the concurrent driver's
/// one-per-in-flight-task set, already net of anything that finished.
///
/// Best-effort per path: a partial that refuses to go is logged and the sweep
/// moves on.
pub(super) async fn clean_partial_writes(volume: &Arc<dyn Volume>, partials: &[PathBuf], operation_id: &str) {
    for partial_path in partials {
        log::debug!(
            "copy_volumes_with_progress: cleaning up partial file {} for op={}",
            partial_path.display(),
            operation_id,
        );
        if let Err(e) = delete_volume_path_recursive(volume, partial_path).await {
            log::warn!(
                "copy_volumes_with_progress: couldn't clean up {} of partial {}: {:?}",
                e.path.display(),
                partial_path.display(),
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
pub(super) async fn clean_abandoned_staged_writes(volume: &Arc<dyn Volume>, state: &Arc<WriteOperationState>) {
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

/// Recursively deletes a file or directory on a volume, reporting the path that
/// actually refused to go.
///
/// For files: calls `volume.delete()` directly.
/// For directories: lists contents, deletes children (recursing into subdirs),
/// then deletes the directory itself. The sweep keeps going after a child fails
/// so it clears everything it can, and it remembers the FIRST child failure with
/// that child's own path.
///
/// What comes out of a directory that couldn't be emptied is that remembered
/// child failure, ❌ never the directory's own `ENOTEMPTY`: the surviving child
/// is the diagnosis and the parent's refusal is only its symptom, named after
/// the folder the user selected. A directory that DID go leaves nothing behind
/// to tell anyone about, so a child failure that raced with another deleter
/// stays `Ok` rather than turning a finished move into a reported failure.
pub(in crate::file_system::write_operations) async fn delete_volume_path_recursive(
    volume: &Arc<dyn Volume>,
    path: &Path,
) -> Result<(), PathedVolumeError> {
    delete_volume_path_recursive_preserving(volume, path, &HashSet::new()).await
}

/// [`delete_volume_path_recursive`], except every path in `preserve` — and every
/// ancestor directory still holding one — survives.
///
/// This is what a MOVE sweeps its source folder with. A merge can resolve some
/// deep children to Skip (the user chose Skip, or a conditional policy reduced
/// to it), and a skipped child never landed at the destination: the source copy
/// is the ONLY copy. An unconditional recursive sweep of the source folder
/// therefore destroys exactly the data the user declined to move. Pinned by
/// `volume/move_merge_tests.rs::move_folder_merge_never_loses_a_byte_under_every_policy`.
///
/// A directory is deleted only once its whole subtree is gone, so preserving one
/// leaf keeps its entire ancestor spine. A child that FAILS to delete counts as
/// preserved too — its parent still holds content, so attempting the parent
/// would only add a misleading `ENOTEMPTY` on top of the real leaf error.
pub(in crate::file_system::write_operations) async fn delete_volume_path_recursive_preserving(
    volume: &Arc<dyn Volume>,
    path: &Path,
    preserve: &HashSet<PathBuf>,
) -> Result<(), PathedVolumeError> {
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
                    "delete_volume_path_recursive: couldn't delete {}: {:?}",
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
