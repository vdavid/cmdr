//! Volume cleanup / rollback helpers for the volume-aware copy and move paths.
//! Shared by `volume::copy` and `volume::r#move`, so they live here rather than
//! inside either operation module.
//!
//! **Three deletes, split by capability, and only [`remove_tree`] recurses.**
//! Rollback reaches for `delete_clearing_blockers` on one `NodeKind::File` (one
//! node, no listing) and [`prune_created_dir_if_empty`] (empty-only, and it establishes the
//! emptiness itself); a tree removal has to name its authorization in the type
//! ([`TreeRemoval`]). That's what keeps a wrong "is this a directory?" belief
//! from reaching a recursive delete: on the cleanup path there isn't one in
//! scope. `DETAILS.md` § "Three ways to delete, and who may use each".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::super::super::event_sinks::OperationEventSink;
use super::super::super::in_flight_temps;
use super::super::super::ledger::WrittenFile;
use super::super::super::reversal::{PresentRecheck, ReversalTally, drained, recheck_volume};
use super::super::super::state::{WindDown, WriteOperationState};
use super::super::super::types::{
    StagedLeftoverFate, StagedLeftovers, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent,
};
use super::super::super::unblocked_removal::{NodeKind, RemovalRefused, delete_clearing_blockers};
use super::super::leftover_cleanup::{LeftBehind, finish_in_background};
use super::super::recovered_name::KeptNewData;
use super::transfer_error::{PathRole, PathedVolumeError};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{ConnectionState, EntryKind, Volume, VolumeError};
use cmdr_operation_log::rollback::ItemResult;
use cmdr_operation_log::types::SkipReason;
use tokio::runtime::Handle;

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
/// Neither loop can recurse: they remove one `NodeKind::File` node each and call
/// [`prune_created_dir_if_empty`], so a directory that reaches this ledger by mistake costs
/// the user a leftover, never a file.
///
/// Every entry is rechecked against the size it landed with immediately before the delete;
/// one the backend now reports differently is left alone and counted, never removed. The
/// bar drains over the LEDGER's own length, so it reaches zero whether an entry was removed,
/// left alone, or refused to go. What got left, and why, comes back in the returned
/// [`ReversalTally`].
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
    wind_down: &WindDown,
    files_at_cancel: usize,
    bytes_at_cancel: u64,
    files_total: usize,
    bytes_total: u64,
) -> ReversalTally {
    let paths_to_process = copied_paths.len();
    let runtime = wind_down.runtime();
    let mut tally = ReversalTally::default();
    let mut last_progress_time = Instant::now();

    let emit = |current_file: Option<String>, files_left: usize, bytes_left: u64| {
        wind_down.report(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Copy,
                WriteOperationPhase::RollingBack,
                current_file,
                files_left,
                files_total,
                bytes_left,
                bytes_total,
            ),
        );
    };
    emit(None, files_at_cancel, bytes_at_cancel);

    // Delete newest first, draining the ledger as they go. The intent is read
    // BEFORE the pop: an entry taken off the ledger and then left standing would
    // be a file on the volume nothing claims any more.
    loop {
        // Check if the user cancelled the rollback itself (see `WindDown`).
        if wind_down.stop_requested() {
            log::info!(
                "volume_rollback_with_progress: rollback cancelled at {}/{} paths, keeping remaining",
                tally.processed(),
                paths_to_process,
            );
            tally.mark_canceled();
            return tally;
        }

        let Some(entry) = copied_paths.pop() else {
            break;
        };
        // Rechecked here, one item before the act — ❌ never in a batch, where a
        // verification would age while other items were processed.
        let result = reverse_written_file(&runtime, volume, &entry).await;
        tally.record(result, &entry.path);

        // Throttled progress events with decreasing values. The counters advance
        // for every entry the reversal walked past, removed or not.
        if last_progress_time.elapsed() >= wind_down.progress_interval() {
            let (files_left, bytes_left) =
                drained(files_at_cancel, bytes_at_cancel, tally.processed(), paths_to_process);
            let current_file_name = entry
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            emit(Some(current_file_name), files_left, bytes_left);
            last_progress_time = Instant::now();
        }
    }

    // Prune the directories this operation newly created, deepest-first.
    // `created_dirs` is in creation order (shallowest first), so iterating in
    // reverse empties leaves before their parents are tried.
    for dir in created_dirs.iter().rev() {
        if wind_down.stop_requested() {
            tally.mark_canceled();
            return tally;
        }
        tally.record(prune_created_dir_if_empty(&runtime, volume, dir).await, dir);
    }

    // The frame that lands on zero, so a run whose last items fell inside the
    // throttle window still ends where it ended.
    emit(None, 0, 0);
    tally
}

/// Remove ONE destination file this operation wrote, if the backend still reports
/// the file this operation wrote.
async fn reverse_written_file(runtime: &Handle, volume: &Arc<dyn Volume>, entry: &WrittenFile) -> ItemResult {
    let live_size = match volume.get_metadata(&entry.path).await {
        Ok(live) => live.size,
        // Already gone ⇒ the end state we wanted already holds.
        Err(VolumeError::NotFound(_)) => return ItemResult::Skipped(SkipReason::AlreadyGone),
        Err(e) => {
            log::warn!(
                "volume_rollback_with_progress: couldn't recheck {}: {e}",
                LogPrivate::file_path(&entry.path),
                e = LogError(&e)
            );
            return ItemResult::Skipped(SkipReason::UnverifiablePrecondition);
        }
    };
    match recheck_volume(entry, live_size) {
        PresentRecheck::Act => {}
        PresentRecheck::Skip(reason) => {
            log::info!(
                "volume_rollback_with_progress: leaving {} alone ({reason:?})",
                LogPrivate::file_path(&entry.path)
            );
            return ItemResult::Skipped(reason);
        }
    }
    // One node each: these are the FILES this operation wrote, verified just
    // above, so a Locked one is ours to unlock (invariant 9).
    removal_result(
        &entry.path,
        delete_clearing_blockers(runtime, volume, &entry.path, NodeKind::File).await,
    )
}

/// What one verified removal came to, as a rollback item.
fn removal_result(path: &Path, removed: Result<(), RemovalRefused>) -> ItemResult {
    match removed {
        Ok(()) | Err(RemovalRefused::Volume(VolumeError::NotFound(_))) => ItemResult::Reversed,
        Err(RemovalRefused::Locked) => {
            log::warn!(
                "volume_rollback_with_progress: {} wears a lock this process can't clear",
                LogPrivate::file_path(path)
            );
            ItemResult::Skipped(SkipReason::Locked)
        }
        Err(RemovalRefused::Volume(e)) => {
            log::warn!(
                "volume_rollback_with_progress: couldn't delete {}: {e}",
                LogPrivate::file_path(path),
                e = LogError(&e)
            );
            ItemResult::Skipped(SkipReason::Failed)
        }
    }
}

/// Removes the staged `.cmdr-tmp-*` partials this operation's stopped tasks
/// couldn't take with them, as far as a LIVE destination takes them right now,
/// and answers what's left for the stop's summary.
///
/// A task that stops on its own deletes its temp (`StagedWrite::abandon`, or
/// its backend's tier-1 cleanup). What `state.in_flight_temps` still lists is
/// what those couldn't: a task the drain's deadline dropped mid-write, or an
/// abandon whose delete the destination turned down. A landing about to clear
/// the way re-records its temp as an `ItemKind::Landing` and takes it out of
/// this set first, so a temp that may hold committed data is never in it and is
/// never touched here; a landing the stop left recorded comes back as
/// [`StagedLeftoverFate::LandsWhenBack`], named by the file it's on its way to,
/// and one that saved its bytes under a ` (recovered)` name as
/// [`StagedLeftoverFate::Rescued`], named by that name (`KeptNewData`), all but
/// the one the operation's own `failure` already names. A failure carries
/// those two beside its cause (`WriteOperationError::with_new_files_kept`).
///
/// ❗ **Never waits on a share that's out**, so the terminal event goes out the
/// moment this returns (`transfer/DETAILS.md` § "File writes are staged"):
///
/// - A destination that reads out takes no inline delete at all.
/// - The inline deletes are capped, in all, by the cleanup policy's
///   `inline_budget`, for a destination that reads live and dies under them.
/// - A temp it couldn't take goes to the background pass
///   (`transfer/leftover_cleanup.rs`): one reconnect attempt and a short wait
///   for one that was out, a few retries for one that refused, then the ledger's
///   reconnect sweep for what's still there. The summary names each kind
///   ([`StagedLeftoverFate::RemovedWhenBack`], [`StagedLeftoverFate::Refused`]):
///   a user who chose Rollback asked for the destination to be cleared, and
///   multi-gigabyte scratch staying behind is news they have to be told.
///
/// **Skipped entirely once tier 2 has fired** (`state.backend_abort`). Every
/// delete here is a round trip through the destination, and the reason the abort
/// fired is that the destination stopped answering — so this pass would hold the
/// quit deadline for a second time, right after the streaming write stopped
/// holding it. The entries stay in both ledgers instead, and
/// `cmdr_operations::in_flight_temps`'s startup sweep clears them at the next
/// launch, with no age gate, off the launch thread. Nothing is reported in that
/// case: the app is on its way out and there is no toast left to read it.
pub(super) async fn clean_abandoned_staged_writes(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    failure: Option<&WriteOperationError>,
) -> Vec<StagedLeftovers> {
    let kept = KeptNewData::take(state);
    let kept = match failure {
        Some(failure) => kept.beside(failure),
        None => kept,
    };
    if state.backend_abort.is_cancelled() {
        log::info!(
            target: "copy",
            "clean_abandoned_staged_writes: the app is shutting down, leaving the staged partial(s) to the startup sweep"
        );
        return Vec::new();
    }
    let temps: Vec<PathBuf> = std::mem::take(&mut *state.in_flight_temps.lock_ignore_poison());
    let left = remove_what_a_live_destination_takes(volume, state, temps).await;
    let leftovers = [
        (StagedLeftoverFate::Refused, &left.refused),
        (StagedLeftoverFate::RemovedWhenBack, &left.out),
    ]
    .into_iter()
    .filter_map(|(fate, paths)| StagedLeftovers::of(fate, paths, volume.name()))
    .chain(kept.groups(volume.name()))
    .collect();
    finish_in_background(volume, state, in_flight_temps::dest_home(state), left);
    leftovers
}

/// The inline half of [`clean_abandoned_staged_writes`]: deletes what the
/// destination takes now, within the policy's budget, and sorts the rest by
/// why it stayed.
async fn remove_what_a_live_destination_takes(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    temps: Vec<PathBuf>,
) -> LeftBehind {
    let mut left = LeftBehind::default();
    if temps.is_empty() {
        return left;
    }
    log::info!(
        target: "copy",
        "clean_abandoned_staged_writes: removing {} staged partial(s) the stopped tasks left",
        temps.len()
    );
    let deadline = tokio::time::Instant::now() + state.cleanup_policy.inline_budget;
    for temp in temps {
        if !volume.connection_state().is_none_or(ConnectionState::is_live) {
            left.out.push(temp);
            continue;
        }
        match tokio::time::timeout_at(deadline, volume.delete(&temp)).await {
            // Already gone counts as removed: the end state this wants already
            // holds, and reporting it would send the user hunting for a file
            // that isn't there.
            Ok(Ok(()) | Err(VolumeError::NotFound(_))) => {
                in_flight_temps::retire_temp(state, in_flight_temps::dest_home(state), &temp);
            }
            Ok(Err(error)) if error.transport_fault().is_some() => left.out.push(temp),
            Ok(Err(error)) => {
                log::warn!(
                    target: "copy",
                    "clean_abandoned_staged_writes: {} was refused ({error}); trying again in the background",
                    LogPrivate::file_path(&temp),
                    error = LogError(&error),
                );
                left.refused.push(temp);
            }
            // The budget ran out on a destination that may be dying under us.
            Err(_elapsed) => left.out.push(temp),
        }
    }
    left
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
/// Guards, mirroring `archive_edit::remote::reap_remote_temps`:
/// - **One round trip.** A single `list_directory`, then a `delete` per match.
/// - **Age-gated.** Only leftovers older than [`STALE_TEMP_MIN_AGE`] go, so a
///   temp another instance is streaming into right now is never removed. An entry
///   with no reported mtime is treated as fresh and spared.
/// - **Files only**, and only names carrying the `.cmdr-tmp-` marker.
/// - ❗ **Never a path the engine's ledger records** on this volume, under any
///   kind, in flight or waiting for the volume (`in_flight_temps::is_recorded_on`).
///   Every staged temp wears the SOURCE's dates (`stamping.rs`), so the age
///   gate alone can't tell an abandoned one from a live write's or from a
///   landing's (`ItemKind::Landing`), which may hold the only copy.
///
/// ❗ **Nothing unrecorded may leave committed data under a temp name.** This
/// sweep reaps by name and age whatever the ledger doesn't name, so a
/// `.cmdr-tmp-*` that holds the only copy of something must be recorded or
/// renamed: a staged landing records its temp as a landing before it clears
/// the way, and rescues it to a ` (recovered)` name when its rename then fails.
///
/// Best-effort throughout: a listing or delete failure is logged at debug and
/// never fails or delays the user's transfer.
///
/// **Returns the listing it already paid for**, minus the temps it reaped, so
/// the copy driver can answer its top-level conflict pre-check from it instead
/// of one `get_metadata` round trip per source (`volume/copy.rs`, and
/// `DETAILS.md` § "Answering the pre-check from one listing"). `None` means the
/// listing itself failed — ❌ never read that as "the destination is empty".
pub(super) async fn reap_stale_transfer_temps(
    state: &WriteOperationState,
    volume: &Arc<dyn Volume>,
    dir: &Path,
) -> Option<Vec<FileEntry>> {
    let entries = match volume.list_directory(dir, None).await {
        Ok(entries) => entries,
        Err(e) => {
            log::debug!(target: "copy", "skipping stale-temp reap of {}: {e}", LogPrivate::dir_path(dir), e = LogError(&e));
            return None;
        }
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let min_age_secs = STALE_TEMP_MIN_AGE.as_secs();
    // An operation that never named its destination volume can't ask: nothing
    // of its own was persisted, and its live temps are in its own state.
    let recorded_by_the_ledger = |path: &Path| {
        state
            .dest_volume_id()
            .is_some_and(|volume_id| in_flight_temps::is_recorded_on(state, volume_id, volume.root(), path))
    };

    let mut survivors = Vec::with_capacity(entries.len());
    for entry in entries {
        let is_stale_temp = !entry.is_directory
            && entry.name.contains(TEMP_INFIX)
            && entry
                .modified_at
                .is_some_and(|modified| now_secs.saturating_sub(modified) >= min_age_secs);
        let temp_path = dir.join(&entry.name);
        if !is_stale_temp || recorded_by_the_ledger(&temp_path) {
            survivors.push(entry);
            continue;
        }
        log::info!(
            target: "copy",
            "reap_stale_transfer_temps: removing a transfer partial left by an earlier run: {}",
            LogPrivate::file_path(&temp_path)
        );
        if let Err(e) = volume.delete(&temp_path).await {
            log::debug!(target: "copy", "couldn't reap stale temp {}: {e}", LogPrivate::file_path(&temp_path), e = LogError(&e));
        }
    }
    Some(survivors)
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
async fn prune_created_dir_if_empty(runtime: &Handle, volume: &Arc<dyn Volume>, dir: &Path) -> ItemResult {
    match volume.list_directory(dir, None).await {
        // Verified empty and ours, so a Locked one is ours to unlock (invariant 9).
        Ok(entries) if entries.is_empty() => {
            match delete_clearing_blockers(runtime, volume, dir, NodeKind::Directory).await {
                Ok(()) => ItemResult::Reversed,
                Err(RemovalRefused::Volume(VolumeError::NotFound(_))) => ItemResult::Skipped(SkipReason::AlreadyGone),
                Err(RemovalRefused::Locked) => ItemResult::Skipped(SkipReason::Locked),
                Err(RemovalRefused::Volume(e)) => {
                    log::debug!(
                        "prune_created_dir_if_empty: couldn't remove empty created dir {}: {}",
                        LogPrivate::dir_path(dir),
                        LogError(&e)
                    );
                    ItemResult::Skipped(SkipReason::Failed)
                }
            }
        }
        Ok(entries) => {
            log::debug!(
                "prune_created_dir_if_empty: keeping created dir {}, it still holds {} entr{}",
                LogPrivate::dir_path(dir),
                entries.len(),
                if entries.len() == 1 { "y" } else { "ies" }
            );
            ItemResult::Skipped(SkipReason::DirNotEmpty)
        }
        Err(e) => {
            log::debug!(
                "prune_created_dir_if_empty: keeping created dir {}, couldn't list it: {}",
                LogPrivate::dir_path(dir),
                LogError(&e)
            );
            ItemResult::Skipped(SkipReason::Failed)
        }
    }
}

/// Why a caller is allowed to take a whole tree down.
///
/// Fieldless on purpose, with **no `Default` and no `From<bool>`**: a recursive
/// delete is the one thing in this directory that can remove data the user
/// never named, so every call site writes down which authorization it holds.
/// The variants are the complete list; a new sweep has to justify itself by
/// adding one. A move's SOURCE is never one of them, into a zip included: it goes
/// from the ledger its copy kept (`source_sweep.rs`, and `move_op/source_sweep.rs`
/// locally), because a tree removal acts on what is on disk NOW.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TreeRemoval {
    /// A cross-type clash (a file landing on a folder) the user resolved with
    /// Overwrite: the folder was set aside to free the name, and goes once what
    /// replaced it has landed. `displaced_destination.rs::DisplacedDestination::discard`.
    UserChoseOverwriteAcrossTypes,
}

impl TreeRemoval {
    /// Which side's tree this removes, so a refusal is reported as that side's.
    fn side(self) -> PathRole {
        match self {
            Self::UserChoseOverwriteAcrossTypes => PathRole::Destination,
        }
    }
}

/// Recursively removes a file or directory tree, reporting the path that
/// actually refused to go.
///
/// **The only recursive delete in this directory.** `why` names the
/// authorization and rides into the log, so a tree removal says who asked for
/// it; ❌ don't add a call site without adding the variant that describes it.
///
/// A child that FAILS to delete keeps its parent: attempting the parent would
/// only add a misleading `ENOTEMPTY` on top of the real leaf error.
///
/// For directories: lists contents, deletes children (recursing into subdirs),
/// then deletes the directory itself. The sweep keeps going after a child fails
/// so it clears everything it can, and it remembers the FIRST child failure
/// with that child's own path. What comes out of a directory that couldn't be
/// emptied is that remembered child failure, ❌ never the directory's own
/// `ENOTEMPTY`: the surviving child is the diagnosis and the parent's refusal is
/// only its symptom, named after the folder the user selected. A directory that
/// DID go leaves nothing behind to tell anyone about, so a child failure that
/// raced with another deleter stays `Ok` rather than turning a finished removal
/// into a reported failure.
///
/// **A symlink is a leaf, and the link goes, ❌ never its target's contents.**
/// Every "is this a directory?" here asks `Volume::entry_kind`:
/// `Volume::is_directory` and a listing's `is_directory` both answer yes for a
/// link to a folder on some backends, and recursing through one deletes a
/// folder the user never selected.
pub(super) async fn remove_tree(
    volume: &Arc<dyn Volume>,
    path: &Path,
    why: TreeRemoval,
) -> Result<(), PathedVolumeError> {
    log::debug!(target: "delete", "remove_tree: {} ({why:?})", LogPrivate::path(path));
    remove_tree_inner(volume, path, why.side()).await.map(|_| ())
}

/// Recursion body. `Ok(true)` means "content remains under here", so the caller
/// must keep this directory. `side` is the volume's role in the transfer.
async fn remove_tree_inner(volume: &Arc<dyn Volume>, path: &Path, side: PathRole) -> Result<bool, PathedVolumeError> {
    let refused = |at: &Path, error| PathedVolumeError::new(side, at, error);
    let kind = match volume.entry_kind(path).await {
        Ok(kind) => kind,
        Err(_) => {
            // Path may not exist (already deleted or never fully created). Nothing to do.
            return Ok(false);
        }
    };

    if kind != EntryKind::Directory {
        volume.delete(path).await.map_err(|e| refused(path, e))?;
        return Ok(false);
    }

    // List directory contents and delete children first
    let children = volume.list_directory(path, None).await.map_err(|e| refused(path, e))?;

    let mut first_child_failure: Option<PathedVolumeError> = None;
    let mut content_remains = false;
    for child in &children {
        let child_path = PathBuf::from(&child.path);
        // Labelled with `child_path` at the frame that knows the child: one level
        // up and every leaf failure would answer with this directory's name instead.
        let outcome = if child.is_directory || child.is_symlink {
            // A link re-asks `entry_kind` one frame down, which deletes it as the
            // leaf it is.
            Box::pin(remove_tree_inner(volume, &child_path, side)).await
        } else {
            volume
                .delete(&child_path)
                .await
                .map_err(|e| refused(&child_path, e))
                .map(|()| false)
        };
        match outcome {
            Ok(remains) => content_remains |= remains,
            Err(e) => {
                log::warn!(
                    target: "delete",
                    "remove_tree: couldn't delete {}: {}",
                    LogPrivate::path(&e.path),
                    LogError(&e.error)
                );
                // The child is still there, so this directory isn't empty either.
                content_remains = true;
                first_child_failure.get_or_insert(e);
            }
        }
    }

    // Something under here refused to go: keep this directory, and report the
    // leaf that refused.
    if content_remains {
        return match first_child_failure {
            Some(child) => Err(child),
            None => Ok(true),
        };
    }

    // Delete the now-empty directory, as a directory (`Volume::delete_directory`)
    match volume.delete_directory(path, None).await {
        Ok(()) => Ok(false),
        Err(e) => Err(refused(path, e)),
    }
}

#[cfg(test)]
#[path = "cleanup_tests.rs"]
mod tests;
