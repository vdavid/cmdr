//! Conflict resolution for volume-to-volume copy operations.
//!
//! Handles what to do when a destination file already exists:
//! - Stop: Emit conflict event, wait for user input via oneshot channel
//! - Skip: Return None to skip this file
//! - Overwrite (file→file): safe-replace — write into a temp sibling, then
//!   delete the original and rename the temp in (`finalize_safe_replace`), so a
//!   mid-stream failure can't lose both the old and the new copy
//! - Overwrite (dir→dir): merge into the existing tree (no delete)
//! - Overwrite (cross-type): delete the dest first, then write
//! - Rename: Find unique name like "file (1).txt"

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::conflict::{ApplyToAll, apply_to_all_effective, apply_to_all_record};
use super::super::super::state::WriteOperationState;
use super::super::super::types::{
    ConflictResolution, OperationEventSink, VolumeCopyConfig, WriteConflictEvent, WriteConflictResolvedEvent,
    WriteOperationError,
};
use super::super::super::unique_name::{ClaimedNames, NameCandidates};
use super::super::dest_name_index::fold;
use super::transfer_error::map_volume_error;
use crate::file_system::volume::{Volume, VolumeError};

/// Outcome of resolving a volume conflict.
///
/// The caller writes streaming bytes to `write_path`. When `replace_after_write`
/// is `Some(orig)`, `write_path` is a temp sibling on the destination volume:
/// after the streaming write fully succeeds, the caller must call
/// [`finalize_safe_replace`] to delete `orig` (which survived the whole write)
/// and rename `write_path` → `orig`. When `replace_after_write` is `None`,
/// `write_path` is the final destination and the caller writes directly.
#[derive(Debug)]
pub(super) struct ResolvedConflict {
    /// Where the streaming writer should land bytes.
    pub write_path: PathBuf,
    /// `Some(orig)` ⇒ `write_path` is a temp sibling; after a successful write the
    /// caller must delete `orig` (it survived the full write) then rename
    /// `write_path` → `orig`. `None` ⇒ `write_path` is final, write directly.
    pub replace_after_write: Option<PathBuf>,
}

/// Resolves a file conflict for volume-to-volume copy.
/// Returns None if file should be skipped, or Some(path) with the resolved destination path.
#[allow(
    clippy::too_many_arguments,
    reason = "Conflict resolution requires many context parameters"
)]
pub(super) async fn resolve_volume_conflict(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    config: &VolumeCopyConfig,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut ApplyToAll,
    // Size hints for the conflict dialog. `Some` skips a `scan_for_copy` call
    // on that side. The copy path already has both: source size in
    // `source_hints` (from the cached preview scan), dest size in `dest_meta`
    // from the stat just done by the caller. Without these hints, an MTP
    // source means listing the parent directory of `source_path` to find one
    // entry's size — 18 s for /DCIM/Camera with 1046 photos when the listing
    // cache has lapsed. The move path doesn't have a scan phase, so it still
    // falls through to `scan_for_copy` for unknown hints.
    source_size_hint: Option<u64>,
    dest_size_hint: Option<u64>,
    // Whether the source is a directory, known from the caller's preflight
    // `source_hints`. `Some` skips a redundant `source_volume.is_directory`
    // round-trip; on MTP that's a parent-directory listing (device-lock
    // acquisition) on the conflict-emit critical path, paid per conflict.
    // `None` falls back to the trait call for callers without the hint.
    source_is_directory_hint: Option<bool>,
) -> Result<Option<ResolvedConflict>, WriteOperationError> {
    // Classify the clash up front so the two-bucket lookup and store stay
    // consistent. ❌ Neither probe may fall back to `false`: `is_file_to_folder`
    // below is `!source_is_directory && destination_is_directory`, so a guessed
    // `false` on the SOURCE flips the destructive cross-type latch on for a
    // folder, and Overwrite's cross-type arm then recursively deletes the user's
    // destination folder. A guessed `false` on the DESTINATION reaches the same
    // arm's bare `delete`. An unanswerable stat fails the item instead.
    let source_is_directory = match source_is_directory_hint {
        Some(is_dir) => is_dir,
        None => source_volume
            .is_directory(source_path)
            .await
            .map_err(|e| map_volume_error(&source_path.display().to_string(), e))?,
    };

    // The op's ledger of names already handed out, for every namer below.
    let claimed = &state.claimed_names;

    // A source that would land on ITSELF is a request to DUPLICATE it, never a
    // conflict: hand back a free ` (N)` name, and neither the policy nor the
    // person is consulted. Answered before the destination probe and before the
    // dir/dir short-circuit below, because both of those are wrong for this
    // shape — "merging" an item into itself walks its own tree shuffling leaves
    // aside, and every policy answer either destroys the original or refuses
    // what was asked. Nothing propagates the new name: `merge_level` joins each
    // child onto the destination it was handed, so a renamed root carries its
    // whole subtree. `../DETAILS.md` § "Self-collision (duplicating in place)".
    if is_the_same_item(source_volume, source_path, dest_volume, dest_path) {
        let unique_path = find_unique_volume_name(dest_volume, dest_path, source_is_directory, claimed).await;
        log::info!(
            "resolve_volume_conflict: {} is already in the destination, duplicating it as {}",
            source_path.display(),
            unique_path.display()
        );
        return Ok(Some(ResolvedConflict {
            write_path: unique_path,
            replace_after_write: None,
        }));
    }

    let destination_is_directory = resolve_dest_is_directory(dest_volume, dest_path).await?;
    let is_file_to_folder = !source_is_directory && destination_is_directory;

    // Dir-vs-dir is NOT a conflict — it's an unconditional merge. No policy
    // lookup, no `write-conflict` emit, no Stop prompt: a source folder landing
    // on an existing same-named dest folder always merges into it. The configured
    // file policy governs every clash INSIDE the merge (handled per-child by the
    // scan-as-you-merge walker in `volume/strategy.rs`), not the folder itself.
    // We hand back the dest path as the merge target with no safe-replace
    // finalize, exactly the same outcome `apply_volume_conflict_resolution`
    // produces for a same-type-dir Overwrite — but reached without consulting
    // `conflict_resolution`, so even Stop / Skip / Rename merge the folder.
    if source_is_directory && destination_is_directory {
        return Ok(Some(ResolvedConflict {
            write_path: dest_path.to_path_buf(),
            replace_after_write: None,
        }));
    }

    // Determine effective conflict resolution
    let resolution = if let Some(saved_resolution) = apply_to_all_effective(apply_to_all_resolution, is_file_to_folder)
    {
        // Use saved "apply to all" resolution
        saved_resolution
    } else {
        config.conflict_resolution
    };

    match resolution {
        ConflictResolution::Stop => {
            // Serialize the whole Stop-mode dispatch. There is exactly one human
            // and one `conflict_slot`, so two tasks both hitting a
            // Stop-mode clash at once (the concurrent volume-copy spawn loop, or
            // two parallel deep directory merges) must queue here rather than race
            // to emit a `write-conflict` and clobber each other's oneshot sender.
            // The guard is held for the latch re-check, the emit, and the await,
            // then dropped at the end of this step — NEVER across the subsequent
            // file write (the caller does that after we return). See
            // `WriteOperationState::conflict_dispatch_lock`.
            let _dispatch_guard = state.conflict_dispatch_lock.lock().await;

            // Cancel check, load-bearing: on cancel, dropping the oneshot sender
            // unblocks only the ONE task currently awaiting `rx`. A task parked on
            // the dispatch mutex would otherwise acquire it next and emit a fresh
            // `write-conflict` that no one will ever answer (the dialog is tearing
            // down) — a hang. Bail with `Cancelled` before emitting anything.
            if super::super::super::state::is_cancelled(&state.intent) {
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            // Re-check the latch under the lock. While this task waited on the
            // mutex, the task ahead of it may have answered with an "…all" choice
            // that resolves this clash too. If so, apply that resolution without
            // prompting — the queued prompt silently collapses.
            if let Some(saved) = apply_to_all_effective(apply_to_all_resolution, is_file_to_folder) {
                let effective = reduce_volume_conditional_resolution(
                    saved,
                    source_volume,
                    source_path,
                    dest_volume,
                    dest_path,
                    source_size_hint,
                    dest_size_hint,
                )
                .await;
                return apply_volume_conflict_resolution(
                    effective,
                    dest_volume,
                    dest_path,
                    source_is_directory,
                    claimed,
                )
                .await;
            }

            // Need to prompt user - gather metadata for the conflict event.
            // Source size: the pre-flight scan hint is authoritative for both
            // file and folder sources. Surface it opportunistically — `None`
            // ("unknown") when no hint reached us (the same-volume move fast
            // path runs no pre-flight scan), which the FE renders as
            // `(unknown)`, mirroring the destination side.
            let source_size: Option<u64> = source_size_hint;

            // Pull mtimes via `get_metadata` so the per-file conflict dialog
            // can render its "(newer)" / "(older)" annotations on volume copies
            // (MTP, SMB) the same way it does on local-FS. Both sides may
            // legitimately return `None` (SMB servers vary on `modified_at`);
            // we surface that as `None` and the FE simply omits the annotation.
            //
            // Fired only on the Stop path (user-prompted), so the extra two
            // round-trips never run for Skip / Overwrite / Rename / conditional
            // policies. Each is bounded by the time the user takes to click,
            // so the cost is invisible.
            let source_modified: Option<i64> = source_volume
                .get_metadata(source_path)
                .await
                .ok()
                .and_then(|m| m.modified_at)
                .map(|s| s as i64);
            let destination_meta = dest_volume.get_metadata(dest_path).await.ok();
            let destination_modified: Option<i64> =
                destination_meta.as_ref().and_then(|m| m.modified_at).map(|s| s as i64);

            // Destination size: the caller's hint when it has one, else the
            // stat just above (free — it's the same round-trip the mtime needs).
            // A folder destination has no meaningful size, so it stays `None`
            // and the FE renders "(unknown)", mirroring the source side.
            //
            // ❗ NEVER fabricate a `0` for a missing hint. A deep-merge child
            // carries no dest hint, so a fabricated `0` told the user "Existing:
            // 0 bytes" about a file with content — and, because the answer below
            // feeds `reduce_volume_conditional_resolution`, made every
            // destination look smaller than the incoming file, silently turning
            // "Overwrite all smaller" into an unconditional overwrite. `None` is
            // the honest unknown, and it reduces to Skip.
            let dest_size: Option<u64> = if destination_is_directory {
                None
            } else {
                dest_size_hint.or_else(|| destination_meta.as_ref().and_then(|m| m.size))
            };
            let destination_is_newer = matches!((source_modified, destination_modified), (Some(s), Some(d)) if d > s);
            // Collapse to `None` when either side is unknown.
            let size_difference = match (dest_size, source_size) {
                (Some(d), Some(s)) => Some(d as i64 - s as i64),
                _ => None,
            };

            // Arm the conflict slot BEFORE emitting the event. A responder (the
            // FE's `resolve_write_conflict`, or a test responder sink that
            // answers inside its `emit_conflict` callback) can only answer a
            // conflict it has observed; if the event reached it before the slot
            // was armed, its answer would land on nothing and the op's
            // `rx.await` below would hang. Arming first makes the sender
            // available the instant the event is in the responder's hands.
            //
            // Arming also mints this clash's id and builds the event around it,
            // so the question the slot holds and the one on the wire are the
            // same value: an answer has to name that id, and one meant for a
            // clash this operation has already left behind can't decide the
            // next one.
            let (tx, rx) = tokio::sync::oneshot::channel();
            let event = state.conflict_slot.arm(tx, |conflict_id| WriteConflictEvent {
                operation_id: operation_id.to_string(),
                conflict_id,
                source_path: source_path.display().to_string(),
                destination_path: dest_path.display().to_string(),
                source_size,
                destination_size: dest_size,
                source_modified,
                destination_modified,
                destination_is_newer,
                size_difference,
                source_is_directory,
                destination_is_directory,
            });

            // Say that the operation has parked on a person, before the prompt
            // goes out and while the slot is armed. A concurrent copy usually
            // has other tasks still emitting, but a serial one (MTP, a
            // single-file copy) goes as quiet as a local one does, and the same
            // frozen speed sits on screen. Local twin: `../../conflict.rs`.
            state.announce_human_wait(events);

            let event_conflict_id = event.conflict_id;
            events.emit_conflict(event);

            // Wait for user to call resolve_write_conflict.
            match rx.await {
                Ok(response) => {
                    // The wait is over, and this tick is what puts the speed back.
                    state.announce_human_wait(events);
                    // And this clash is over, for every surface showing it, not
                    // only the one whose own call returned. Local twin:
                    // `../../conflict.rs`.
                    events.emit_conflict_resolved(WriteConflictResolvedEvent {
                        operation_id: operation_id.to_string(),
                        conflict_id: event_conflict_id,
                    });
                    // Save the original (unreduced) variant under the right bucket so
                    // subsequent clashes re-evaluate the conditional variants against
                    // their own metadata. `apply_to_all_record` also flips the
                    // "first-clash" flag whether or not the user picked an apply-to-all
                    // option, so a later file→folder "* all" choice won't be considered
                    // "first" if a regular clash happened earlier in this op.
                    apply_to_all_record(
                        apply_to_all_resolution,
                        is_file_to_folder,
                        response.resolution,
                        response.apply_to_all,
                    );
                    let effective = reduce_volume_conditional_resolution(
                        response.resolution,
                        source_volume,
                        source_path,
                        dest_volume,
                        dest_path,
                        source_size,
                        dest_size,
                    )
                    .await;
                    apply_volume_conflict_resolution(effective, dest_volume, dest_path, source_is_directory, claimed)
                        .await
                }
                Err(_) => {
                    // Sender dropped = operation cancelled
                    Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".to_string(),
                    })
                }
            }
            // `_dispatch_guard` drops here, releasing the next queued task.
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            apply_volume_conflict_resolution(
                ConflictResolution::Overwrite,
                dest_volume,
                dest_path,
                source_is_directory,
                claimed,
            )
            .await
        }
        ConflictResolution::Rename => {
            apply_volume_conflict_resolution(
                ConflictResolution::Rename,
                dest_volume,
                dest_path,
                source_is_directory,
                claimed,
            )
            .await
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            let effective = reduce_volume_conditional_resolution(
                resolution,
                source_volume,
                source_path,
                dest_volume,
                dest_path,
                source_size_hint,
                dest_size_hint,
            )
            .await;
            apply_volume_conflict_resolution(effective, dest_volume, dest_path, source_is_directory, claimed).await
        }
    }
}

/// Volume-side counterpart of `reduce_conditional_resolution`. Maps the
/// conditional variants to `Overwrite` / `Skip` by comparing source vs dest
/// sizes (cheap: hints from the caller or one `get_metadata` round-trip each)
/// or `modified_at` timestamps (`get_metadata` on both sides).
///
/// Strict comparison: equal sizes / equal mtimes / unknown values all reduce
/// to `Skip`. Volume backends may not always populate `modified_at` (SMB
/// servers vary, MTP usually does); in that case `OverwriteOlder` skips,
/// which is the safe default.
async fn reduce_volume_conditional_resolution(
    resolution: ConflictResolution,
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    source_size_hint: Option<u64>,
    dest_size_hint: Option<u64>,
) -> ConflictResolution {
    match resolution {
        ConflictResolution::OverwriteSmaller => {
            let src_size = match source_size_hint {
                Some(s) => Some(s),
                None => source_volume.get_metadata(source_path).await.ok().and_then(|m| m.size),
            };
            let dst_size = match dest_size_hint {
                Some(s) => Some(s),
                None => dest_volume.get_metadata(dest_path).await.ok().and_then(|m| m.size),
            };
            match (src_size, dst_size) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(src), Some(dst)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller (volume): skipping {} — destination not strictly smaller (src={src}, dst={dst})",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller (volume): skipping {} — size unknown for source or destination (the volume backend may not surface it)",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
            }
        }
        ConflictResolution::OverwriteOlder => {
            let src_t = source_volume
                .get_metadata(source_path)
                .await
                .ok()
                .and_then(|m| m.modified_at);
            let dst_t = dest_volume
                .get_metadata(dest_path)
                .await
                .ok()
                .and_then(|m| m.modified_at);
            match (src_t, dst_t) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(_), Some(_)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder (volume): skipping {} — destination not strictly older than source",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder (volume): skipping {} — modified time unknown for source or destination (some SMB servers don't surface it)",
                        dest_path.display()
                    );
                    ConflictResolution::Skip
                }
            }
        }
        other => other,
    }
}

/// Applies a specific conflict resolution for volume copy.
/// Returns `None` for Skip, or `Some(ResolvedConflict)` describing where to
/// write and whether a post-write safe-replace finalize is needed.
/// Whether `path` on `dest_volume` is a directory, for the branches that decide
/// what to DELETE.
///
/// "It isn't there" is an answer, and the honest one: a destination that raced
/// away between conflict detection and resolution has nothing to protect, and
/// failing the item there would break a write that would simply have succeeded.
/// Every other error is a refusal to answer, and ❌ must not become `false`:
/// both callers route a `false` into an arm that deletes.
async fn resolve_dest_is_directory(dest_volume: &Arc<dyn Volume>, path: &Path) -> Result<bool, WriteOperationError> {
    match dest_volume.is_directory(path).await {
        Ok(is_dir) => Ok(is_dir),
        Err(VolumeError::NotFound(_)) => Ok(false),
        Err(e) => Err(map_volume_error(&path.display().to_string(), e)),
    }
}

async fn apply_volume_conflict_resolution(
    resolution: ConflictResolution,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    source_is_directory: bool,
    // The operation's ledger of names already handed out, for the `Rename` arm.
    claimed: &ClaimedNames,
) -> Result<Option<ResolvedConflict>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Cmdr's UX promise is "Overwrite means merge for dirs, replace for files":
            //
            // - For files (file→file): SAFE-REPLACE. Stream into a temp sibling on the dest volume
            //   and return `replace_after_write: Some(dest_path)`. The original survives the entire
            //   write; only after the temp is fully written does the caller delete the original and
            //   rename the temp into place (see `finalize_safe_replace`). A mid-stream failure
            //   (network drop, USB yank, cancel) leaves the original intact — we never lose both the
            //   old and the new copy. DO NOT delete the dest here.
            // - For directories (same type): SKIP the delete entirely. The recursive copy merges into
            //   the existing tree; same-named files inside get overwritten by the streaming writers,
            //   files in dest that aren't in source are preserved.
            // - For cross-type clashes (file→folder or folder→file): the dest type is wrong, so we
            //   must delete it before the source materializes. There's no volume-level temp+rename
            //   atomicity (cross-backend) for a type swap, so a recursive delete is the best we can
            //   do; backends that support it (LocalPosix, MTP, SMB) handle the delete safely under
            //   their own semantics. These are rare and lower-stakes (a type mismatch already means
            //   the dest content is being intentionally replaced wholesale).
            //
            // The same-type dir branch is enforced HERE rather than relying on `Volume::delete`'s
            // "file or empty directory" trait contract. That contract is real — a shared
            // conformance assertion every backend's suite runs enforces it
            // (`cmdr_fs::volume::conformance`) — but it's a promise a backend keeps, and
            // MTP once broke it for months with nothing to catch that. A backend with
            // recursive delete semantics, or a refactor that consolidates delete +
            // delete_recursive, would silently flip the UX from merge to wholesale replace,
            // deleting files unique to dest. That's a data-loss footgun. Stat-and-skip makes
            // the merge guarantee architectural rather than borrowed from a backend's good
            // behavior. See `dir_overwrite_must_merge_not_replace_even_with_recursive_delete`
            // in the test module; it pins this with a wrapper Volume that violates
            // the contract.
            let dest_is_dir = resolve_dest_is_directory(dest_volume, dest_path).await?;

            if !dest_is_dir && !source_is_directory {
                // file→file: safe-replace via a temp sibling. No delete here.
                let temp = temp_sibling_path(dest_path);
                return Ok(Some(ResolvedConflict {
                    write_path: temp,
                    replace_after_write: Some(dest_path.to_path_buf()),
                }));
            }

            let same_type_dir = dest_is_dir && source_is_directory;
            if !same_type_dir {
                // Cross-type (file→folder or folder→file): clear the dest first.
                if dest_is_dir {
                    // File→folder overwrite: recursively delete the dest folder.
                    // The one recursive delete this file is allowed, and it says
                    // so in the type: the user picked Overwrite on a clash whose
                    // types differ.
                    let nothing_to_spare = HashSet::new();
                    if let Err(e) = super::cleanup::remove_tree(
                        dest_volume,
                        dest_path,
                        &nothing_to_spare,
                        super::cleanup::TreeRemoval::UserChoseOverwriteAcrossTypes,
                    )
                    .await
                    {
                        log::warn!(
                            "apply_volume_conflict_resolution(Overwrite): recursive delete of folder {} stopped at {}: {}",
                            dest_path.display(),
                            e.path.display(),
                            e.error
                        );
                    }
                } else if let Err(e) = dest_volume.delete(dest_path).await {
                    log::warn!(
                        "apply_volume_conflict_resolution(Overwrite): delete of file {} failed: {}",
                        dest_path.display(),
                        e
                    );
                    // Continue: the streaming writer might still succeed if the failure
                    // was transient.
                }
            }
            Ok(Some(ResolvedConflict {
                write_path: dest_path.to_path_buf(),
                replace_after_write: None,
            }))
        }
        ConflictResolution::Rename => {
            // Find a unique name - we need to check what exists on the volume
            let unique_path = find_unique_volume_name(dest_volume, dest_path, source_is_directory, claimed).await;
            Ok(Some(ResolvedConflict {
                write_path: unique_path,
                replace_after_write: None,
            }))
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            // Reduced to Overwrite / Skip by `reduce_volume_conditional_resolution`
            // before reaching this function.
            unreachable!("conditional conflict resolutions must be reduced before apply_volume_conflict_resolution")
        }
    }
}

/// Builds a temp sibling path next to `dest_path` for a staged write.
///
/// Uses the recognizable `.cmdr-tmp-<uuid>` marker (matches the project's temp
/// convention, so a leftover after a crash is identifiable and cleanup helpers
/// recognize it). The temp lives in the same parent directory as the original
/// so the finalize step's `rename` stays within one directory (no cross-dir
/// rename, which some backends refuse).
///
/// Shared with `staged_write.rs`, which stages EVERY cross-volume file write on
/// one of these, not only the conflict-driven safe-replace.
pub(super) fn temp_sibling_path(dest_path: &Path) -> PathBuf {
    let parent = dest_path.parent().unwrap_or(Path::new(""));
    let filename = dest_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    parent.join(format!("{filename}.cmdr-tmp-{}", uuid::Uuid::new_v4()))
}

/// Lands a fully-written temp at its final name: deletes whatever is at `orig`
/// (which survived the entire streaming write) and renames the temp into its
/// place.
///
/// Two callers, one shape: the conflict layer's file→file safe-replace, and
/// `staged_write.rs`'s landing of an ordinary staged write (where `orig` usually
/// doesn't exist yet and the delete is a tolerated `NotFound`).
///
/// Order matters and is the whole point of safe-replace: the temp holds the
/// COMPLETE new data the moment this is called, and `orig` still holds the
/// complete old data. We delete `orig` first, then `rename(temp, orig, false)`
/// into the now-absent slot. We do NOT use `rename(force=true)` to replace:
/// MTP's `rename(force=true)` does NOT delete an existing destination (it can
/// create a duplicate), so an explicit delete-then-rename is the only shape
/// that's correct and uniform across Local / SMB / MTP / InMemory.
///
/// There is a tiny window between the delete and the rename where neither name
/// resolves to a file on disk — but the complete new data lives in `temp`
/// throughout, so a crash in that window leaves a recoverable `.cmdr-tmp-*`
/// sibling rather than data loss. We tolerate `NotFound` on the delete (the
/// original may have vanished out from under us). If the delete fails for any
/// other reason we return the error WITHOUT deleting the temp — the new data
/// must survive so the user (or a retry) can recover it.
///
/// CALLER CONTRACT: when this returns `Err` (either the delete failed, or — the
/// nastier case — the delete SUCCEEDED and the rename failed), `temp` holds the
/// only complete copy of the new data and the original may already be gone. The
/// caller MUST NOT delete `temp` on this error path: leaving it as a recoverable
/// `.cmdr-tmp-*` artifact is the safe outcome; cleaning it would be total data
/// loss. The three write sites enforce this by stopping their partial-cleanup
/// tracking from designating the temp the moment the streaming write succeeded,
/// before this function runs. See `transfer/CLAUDE.md` § "The post-write temp is
/// committed data" and the `*_preserves_new_data_on_finalize_failure` tests.
pub(super) async fn finalize_safe_replace(
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    orig: &Path,
) -> Result<(), VolumeError> {
    match dest_volume.delete(orig).await {
        Ok(()) => {}
        Err(VolumeError::NotFound(_)) => {
            // Already gone; the rename below will land the new data anyway.
        }
        Err(e) => {
            log::warn!(
                "finalize_safe_replace: failed to delete original {} before rename (temp {} holds the complete new data and is preserved): {}",
                orig.display(),
                temp.display(),
                e
            );
            return Err(e);
        }
    }
    dest_volume.rename(temp, orig, false).await
}

/// Whether `source_path` and `dest_path` name the same item: the question
/// `validation::is_same_file` settles with `dev+ino` on the local-FS side, asked
/// the only way a volume can answer it.
///
/// Same volume is `Arc::ptr_eq`, which is what every path in this directory
/// already means by it (the dest-inside-source guard included): the command
/// layer hands one `Arc` for a same-volume-id transfer.
///
/// `copy.rs` asks it too, to keep the sources it covers out of the pre-known-conflict
/// bulk skip, and so does `routing::transfer_would_land_on_its_source`, which
/// gives the pre-flight conflict scan the answer this engine will give.
pub(crate) fn is_the_same_item(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
) -> bool {
    Arc::ptr_eq(source_volume, dest_volume) && is_the_same_volume_path(source_path, dest_path)
}

/// Whether two paths on ONE volume name the same item.
///
/// Folded (NFC + lowercase), the same key `DestNameIndex` buckets destination
/// names under, so a case-differing route (SMB shares, macOS volumes) or an
/// NFC/NFD-differing one (macOS and SMB move paths between the two routinely)
/// counts. That fold IS this project's answer to "would this backend treat these
/// two as the same". A non-UTF-8 path can't be folded the way a backend would,
/// so there only a byte-exact match counts — the same stance
/// `DestNameIndex::lookup` takes.
///
/// The same-volume move drops its self-colliding sources with this before any
/// engine runs (`move_same.rs`), which is why it isn't private to the resolver.
pub(super) fn is_the_same_volume_path(source_path: &Path, dest_path: &Path) -> bool {
    match (source_path.to_str(), dest_path.to_str()) {
        (Some(source), Some(dest)) => fold(source) == fold(dest),
        _ => source_path == dest_path,
    }
}

/// Finds a unique filename on a volume by appending " (1)", " (2)", etc.
///
/// On a **local-FS-backed** destination volume (`local_path().is_some()`) the
/// chosen name is atomically RESERVED with an `O_CREAT|O_EXCL` placeholder, the
/// same TOCTOU guard `unique_name::find_unique_name` uses for the local-FS copy
/// path. Without it, a concurrent writer (a second Cmdr op, a cloud-sync agent,
/// a backup tool) could land a real file at `name (N)` between our non-atomic
/// `exists()` probe and the streaming writer's create+truncate, and the copy
/// would silently clobber it. The streaming write then lands ON the placeholder
/// (the write site opens the dest with create+truncate), exactly like the
/// local-FS path's `needs_safe_overwrite` flow. The returned path is the volume
/// path; the placeholder is created at the resolved local path.
///
/// On backends without exclusive-create semantics (MTP / SMB / InMemory,
/// `local_path()` is `None`) we can't reserve, so we fall back to the
/// `exists()` probe and re-check existence immediately before returning to keep
/// the residual window as narrow as the backend allows.
///
/// A **directory** takes that probe branch on every backend, local-FS dest
/// included. The placeholder is a FILE, and one sitting where the copy is about
/// to create a directory makes `merge.rs::merge_level`'s `create_directory`
/// report `AlreadyExists`, so the walk would try to merge into it and list it.
/// Letting the merge walker create the directory itself is also what records it
/// in `CreatedPaths`, which a pre-created one would miss and rollback would then
/// leave behind.
///
/// Both branches record the pick in the operation's `ClaimedNames` ledger and
/// walk past what's already there, which is what the probe alone can't do for a
/// directory (never reserved) or for the concurrent driver resolving several
/// top-level sources at once. Without it `photo.jpg` and `photo (1).jpg`
/// duplicated together both land on `photo (2).jpg`.
///
/// Naming itself is not this function's business: the candidates come from
/// `unique_name::NameCandidates`, the same sequence the local-FS namer walks, so a
/// volume dest numbers identically and `photo (1).jpg` continues to
/// `photo (2).jpg` here too. This function owns only the reservation.
async fn find_unique_volume_name(
    dest_volume: &Arc<dyn Volume>,
    path: &Path,
    is_directory: bool,
    claimed: &ClaimedNames,
) -> PathBuf {
    let local_root = dest_volume.local_path().filter(|_| !is_directory);
    let mut candidates = NameCandidates::for_path(path);

    loop {
        let new_path = candidates.current();

        if !claimed.claim(&new_path) {
            // Spoken for by another source of this same operation.
            candidates.advance();
            continue;
        }

        if let Some(root) = &local_root {
            // Local-FS dest: reserve the name with an O_CREAT|O_EXCL placeholder
            // so no concurrent writer can sneak a file in before our write lands.
            let local_path = resolve_local_path(root, &new_path);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&local_path)
            {
                Ok(_) => return new_path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    candidates.advance();
                }
                Err(_) => {
                    // Anything else (parent unwritable, ENOSPC, …) leaks back to
                    // the caller's write attempt, which has its own error path.
                    return new_path;
                }
            }
        } else {
            // Non-local backend: best-effort `exists()` probe. Re-check right
            // before returning to keep the residual window as narrow as we can.
            if !dest_volume.exists(&new_path).await {
                return new_path;
            }
            candidates.advance();
        }

        // Safety limit to prevent an infinite loop.
        if candidates.attempts() > 1000 {
            // Extremely unlikely to happen.
            return candidates.current();
        }
    }
}

/// Resolves a destination-volume path against a local-FS volume root, so the
/// O_EXCL reservation lands at the same local path the volume's streaming
/// writer will later resolve `new_path` to. The rule itself is
/// `cmdr_fs::volume::root_anchored`, which `LocalPosixVolume::resolve` uses too:
/// that shared rule IS the guarantee the two paths agree.
fn resolve_local_path(root: &Path, path: &Path) -> PathBuf {
    cmdr_fs::volume::root_anchored(root, path)
}

#[cfg(test)]
#[path = "conflict_tests.rs"]
mod tests;
