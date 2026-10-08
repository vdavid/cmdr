//! Conflict resolution for volume-to-volume copy operations.
//!
//! Handles what to do when a destination file already exists:
//! - Stop: Emit conflict event, wait for user input via oneshot channel
//! - Skip: Return None to skip this file
//! - Overwrite (file→file): safe-replace — write into a temp sibling, then
//!   delete the original and rename the temp in (`finalize_safe_replace`), so a
//!   mid-stream failure can't lose both the old and the new copy. On a
//!   destination that publishes every write whole (an object store), the write
//!   replaces the original in place instead: the protocol keeps it readable
//!   until the new bytes are complete, which is the same guarantee for one
//!   request instead of a copy plus a delete
//! - Overwrite (dir→dir): merge into the existing tree (no delete)
//! - Overwrite (cross-type): only ever from a plain Overwrite a person answered
//!   on a prompt for that SHAPE, carry included — rename the dest ASIDE, then
//!   write; the caller's ledger drops or restores it when the operation ends
//!   (`displaced_destination.rs`). A policy nobody looked at per item (the config's, a same-kind
//!   apply-to-all carry) refuses across types and Skips, and so do the
//!   conditional variants whoever asked for them;
//!   `../../conflict.rs::resolution_for_clash` holds the rule.
//! - Rename: Find unique name like "file (1).txt"

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::conflict::{
    ApplyToAll, ClashKind, IncomingItem, answered_resolution_for_clash, apply_to_all_effective, apply_to_all_record,
    resolution_for_clash,
};
use super::super::super::event_sinks::OperationEventSink;
use super::super::super::look_alike::is_look_alike_clash;
use super::super::super::state::WriteOperationState;
use super::super::super::types::{
    ConflictResolution, VolumeCopyConfig, WriteConflictEvent, WriteConflictResolvedEvent, WriteOperationError,
};
use super::super::staged_write::Replaces;
use super::displaced_destination::{DisplacedDestination, displace_destination};
use super::finalize::temp_sibling_path;
use super::item_identity::is_the_same_item;
use super::naming::find_unique_volume_name;
use super::transfer_error::{PathRole, map_volume_error};
use crate::file_system::volume::{EntryKind, Volume, VolumeError};

/// Outcome of resolving a volume conflict.
///
/// The caller writes streaming bytes to `write_path`. `replaces` says what that
/// does to a file already at the name: under [`Replaces::ViaTemp`]`(orig)`,
/// `write_path` is a temp sibling on the destination volume, and after the
/// streaming write fully succeeds the caller must call
/// [`super::finalize::finalize_safe_replace`] to delete `orig` (which survived
/// the whole write) and rename `write_path` → `orig`. Otherwise `write_path` is
/// the final destination and the caller writes directly.
#[derive(Debug)]
pub(super) struct ResolvedConflict {
    /// Where the streaming writer should land bytes.
    pub write_path: PathBuf,
    /// `true` ⇒ resolving this clash RESERVED `write_path` with a zero-byte
    /// `O_EXCL` placeholder (a `Rename` pick on a local-FS destination). A caller
    /// whose write then never happens owes taking it back: on disk that
    /// placeholder is indistinguishable from an empty file the copy produced.
    /// `naming.rs::ClaimedName` is where the answer comes from.
    pub reserved_placeholder: bool,
    /// What the write does to a file at the destination name
    /// ([`Replaces`]). Drives the finalize, the failed-write cleanup, and the
    /// journal's "this overwrote" at every write site.
    pub replaces: Replaces,
    /// `Some` ⇒ a cross-type Overwrite renamed the entry at `write_path` aside
    /// to free the name. The caller owns it from here: dropped once what
    /// replaces it has landed, put back when it doesn't
    /// (`displaced_destination.rs`). ❌ Never let it fall on the floor.
    pub displaced: Option<DisplacedDestination>,
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
    // Classify the clash up front so the per-shape latch lookup and store stay
    // consistent. ❌ Neither probe may fall back to `false`: `ClashKind::of`
    // below reads both sides, so a guessed `false` on the SOURCE files a folder
    // under the wrong shape's latch, and Overwrite's cross-type arm then
    // recursively deletes the user's destination folder. A guessed `false` on
    // the DESTINATION reaches the same arm's bare `delete`. An unanswerable
    // stat fails the item instead.
    let source_is_directory = match source_is_directory_hint {
        Some(is_dir) => is_dir,
        None => source_volume
            .is_directory(source_path)
            .await
            .map_err(|e| map_volume_error(&source_path.display().to_string(), PathRole::Source, e))?,
    };

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
        let unique = find_unique_volume_name(dest_volume, dest_path, source_is_directory, &state.claimed_names).await;
        log::info!(
            "resolve_volume_conflict: {} is already in the destination, duplicating it as {}",
            source_path.display(),
            unique.path.display()
        );
        return Ok(Some(ResolvedConflict {
            write_path: unique.path,
            reserved_placeholder: unique.reserved_on_disk,
            replaces: Replaces::Nothing,
            displaced: None,
        }));
    }

    let destination_is_directory = resolve_dest_is_directory(dest_volume, dest_path).await?;

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
            reserved_placeholder: false,
            replaces: Replaces::Nothing,
            displaced: None,
        }));
    }

    // Dir-vs-dir left above and self-collision left before it, so the two sides
    // differing means one is a folder and the other a leaf.
    let kind = ClashKind::of(
        IncomingItem::of_directory_flag(source_is_directory),
        Some(destination_is_directory),
    );

    // Determine effective conflict resolution. A policy nobody looked at per
    // item — the config's, or a same-kind "* all" reaching across types — never
    // replaces a folder with a file or a file with a folder; a plain Overwrite
    // answered for this very shape does. `resolution_for_clash` holds the why.
    let latched = apply_to_all_effective(apply_to_all_resolution, kind);
    let resolution = resolution_for_clash(latched, config.conflict_resolution, kind, &dest_path.display());

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
            if let Some(saved) = apply_to_all_effective(apply_to_all_resolution, kind) {
                // A carry stops exactly where it would have on the up-front
                // lookup: honoured across types when it was answered for this
                // shape, refused when it is a same-kind policy reaching over.
                // `Skip` needs no reduction, but running it through keeps the
                // one path.
                let saved = saved.at(kind, &dest_path.display());
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
                return apply_volume_conflict_resolution(effective, dest_volume, dest_path, source_is_directory, state)
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
                destination_is_look_alike: is_look_alike_clash(source_path, dest_path),
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
                        kind,
                        response.resolution,
                        response.apply_to_all,
                    );
                    // Hold the answer to what it can consent to at this shape
                    // before the conditional reduction, which across types
                    // would be comparing against a directory. Local twin:
                    // `../../conflict.rs`.
                    let answered = answered_resolution_for_clash(response.resolution, kind, &dest_path.display());
                    let effective = reduce_volume_conditional_resolution(
                        answered,
                        source_volume,
                        source_path,
                        dest_volume,
                        dest_path,
                        source_size,
                        dest_size,
                    )
                    .await;
                    apply_volume_conflict_resolution(effective, dest_volume, dest_path, source_is_directory, state)
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
                state,
            )
            .await
        }
        ConflictResolution::Rename => {
            apply_volume_conflict_resolution(
                ConflictResolution::Rename,
                dest_volume,
                dest_path,
                source_is_directory,
                state,
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
            apply_volume_conflict_resolution(effective, dest_volume, dest_path, source_is_directory, state).await
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

/// Whether `path` on `dest_volume` is a directory, for the branches that decide
/// what to clear out of the way.
///
/// "It isn't there" is an answer, and the honest one: a destination that raced
/// away between conflict detection and resolution has nothing to protect, and
/// failing the item there would break a write that would simply have succeeded.
/// Every other error is a refusal to answer, and ❌ must not become `false`:
/// both callers route a `false` into an arm that clears the name.
///
/// A LINK is not a directory here, whatever it points at: a merge into one
/// would land the source's files in the link's target, outside the folder the
/// user picked. It meets a folder as the type clash it is.
async fn resolve_dest_is_directory(dest_volume: &Arc<dyn Volume>, path: &Path) -> Result<bool, WriteOperationError> {
    match dest_volume.entry_kind(path).await {
        Ok(kind) => Ok(kind == EntryKind::Directory),
        Err(VolumeError::NotFound(_)) => Ok(false),
        Err(e) => Err(map_volume_error(&path.display().to_string(), PathRole::Destination, e)),
    }
}

/// Applies a specific conflict resolution for volume copy.
/// Returns `None` for Skip, or `Some(ResolvedConflict)` describing where to
/// write, whether a post-write safe-replace finalize is needed, and what was set
/// aside to free the name.
async fn apply_volume_conflict_resolution(
    resolution: ConflictResolution,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    source_is_directory: bool,
    // The operation's state: its ledger of names already handed out (the
    // `Rename` arm), and the in-flight ledger an aside is recorded in (the
    // cross-type Overwrite arm).
    state: &Arc<WriteOperationState>,
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
            //   and return `Replaces::ViaTemp(dest_path)`. The original survives the entire
            //   write; only after the temp is fully written does the caller delete the original and
            //   rename the temp into place (see `finalize_safe_replace`). A mid-stream failure
            //   (network drop, USB yank, cancel) leaves the original intact — we never lose both the
            //   old and the new copy. DO NOT delete the dest here. A destination that
            //   publishes every write whole (`Volume::publishes_writes_whole`, an
            //   object store) gives that guarantee by protocol, so the write goes to
            //   the original's own name (`Replaces::InPlace`): a temp there would be
            //   a server-side copy plus a delete to land.
            // - For directories (same type): SKIP the delete entirely. The recursive copy merges into
            //   the existing tree; same-named files inside get overwritten by the streaming writers,
            //   files in dest that aren't in source are preserved.
            // - For cross-type clashes (file→folder or folder→file): the dest type is wrong, so the
            //   name has to be freed before the source materializes. It's freed by renaming the dest
            //   ASIDE, never by deleting it: a folder replacing a file lands leaf by leaf, and a
            //   failure or cancel halfway would otherwise leave neither the old entry nor a complete
            //   new one. The caller holds the aside until the operation ends
            //   (`displaced_destination.rs::DisplacedLedger`). This arm is reachable ONLY from an
            //   Overwrite a person picked on a Stop prompt naming both types:
            //   `resolve_volume_conflict` turns every BLANKET Overwrite across types into a Skip
            //   before it gets here.
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
                // file→file: in place where the destination publishes whole,
                // else safe-replace via a temp sibling. No delete here.
                if dest_volume.publishes_writes_whole() {
                    return Ok(Some(ResolvedConflict {
                        write_path: dest_path.to_path_buf(),
                        reserved_placeholder: false,
                        replaces: Replaces::InPlace,
                        displaced: None,
                    }));
                }
                let temp = temp_sibling_path(dest_path);
                return Ok(Some(ResolvedConflict {
                    write_path: temp,
                    reserved_placeholder: false,
                    replaces: Replaces::ViaTemp(dest_path.to_path_buf()),
                    displaced: None,
                }));
            }

            // Cross-type (file→folder or folder→file): set the dest aside. A
            // refused rename fails the item: the name is still taken, and a
            // write onto it would fail or, for a folder, merge into a file.
            let displaced = if dest_is_dir == source_is_directory {
                None
            } else {
                displace_destination(state, dest_volume, dest_path, dest_is_dir).await?
            };
            Ok(Some(ResolvedConflict {
                write_path: dest_path.to_path_buf(),
                reserved_placeholder: false,
                replaces: Replaces::Nothing,
                displaced,
            }))
        }
        ConflictResolution::Rename => {
            // Find a unique name - we need to check what exists on the volume
            let unique =
                find_unique_volume_name(dest_volume, dest_path, source_is_directory, &state.claimed_names).await;
            Ok(Some(ResolvedConflict {
                write_path: unique.path,
                reserved_placeholder: unique.reserved_on_disk,
                replaces: Replaces::Nothing,
                displaced: None,
            }))
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            // Reduced to Overwrite / Skip by `reduce_volume_conditional_resolution`
            // before reaching this function.
            unreachable!("conditional conflict resolutions must be reduced before apply_volume_conflict_resolution")
        }
    }
}

/// Merge safety, the overwrite ordering, and the finalize swap.
#[cfg(test)]
#[path = "conflict_tests.rs"]
mod tests;

/// `OverwriteSmaller` / `OverwriteOlder`, on hints and on `get_metadata`.
#[cfg(test)]
#[path = "conflict_conditional_tests.rs"]
mod conditional_tests;

/// A blanket policy never replaces one KIND of entry with another.
#[cfg(test)]
#[path = "conflict_cross_type_tests.rs"]
mod cross_type_tests;
