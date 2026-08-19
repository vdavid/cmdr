//! Conflict resolution for write operations.
//!
//! The two-bucket `ApplyToAll` latch model, the Stop-mode oneshot wait, the
//! conditional-variant reduction (`OverwriteSmaller` / `OverwriteOlder`), and
//! the helpers that build conflict events / conflict info and sample conflicts
//! for the dialog.
//!
//! Policy only. The ` (N)` name a Rename resolution lands on comes from
//! `unique_name.rs`, which carries no conflict policy of its own.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::durability::lookup_indexed_size;
use super::overwrite::ResolvedDestination;
use super::state::WriteOperationState;
use super::types::{
    ConflictId, ConflictInfo, ConflictResolution, OperationEventSink, WriteConflictEvent, WriteConflictResolvedEvent,
    WriteOperationConfig, WriteOperationError,
};
use super::unique_name::find_unique_name;

// ============================================================================
// Apply-to-all state (two-bucket latches)
// ============================================================================

/// Per-operation "apply to all" latch state for conflict resolution.
///
/// Splits into two buckets so the destructive file-to-folder clash variant
/// (replacing a directory with a file) can be tracked separately from the
/// normal (file↔file / folder↔folder / folder↔file) variants. See
/// `apply_to_all_tests` for the full rule set; the short version:
///
/// - A choice latched on a *normal* clash applies to subsequent normal
///   clashes. Only Skip / Rename carry over to file-to-folder; Overwrite
///   variants don't.
/// - A choice latched on a *file-to-folder* clash applies to subsequent
///   file-to-folder clashes. If it was the **first** clash of the whole
///   operation, the latch spreads to the normal bucket too.
// DEFAULT-OK: nothing latched and no clash seen yet is precisely the state before the
// operation's first conflict.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ApplyToAll {
    normal: Option<ConflictResolution>,
    file_to_folder: Option<ConflictResolution>,
    /// `false` until the first clash (any kind) has been resolved. Used to
    /// decide whether a "* all" choice in a file-to-folder dialog should
    /// spread to the normal bucket — only if the file-to-folder clash was
    /// the very first one the user saw.
    has_seen_clash: bool,
}

/// Returns the latched resolution that applies to the next clash, or `None`
/// if there's nothing latched yet for the given clash type. Encodes the
/// Skip/Rename carry-over rule: when looking up a file-to-folder clash, fall
/// back to the normal bucket only when the latched value there is one of
/// the safe variants.
pub(super) fn apply_to_all_effective(state: &ApplyToAll, is_file_to_folder: bool) -> Option<ConflictResolution> {
    if is_file_to_folder {
        state.file_to_folder.or(match state.normal {
            Some(r @ (ConflictResolution::Skip | ConflictResolution::Rename)) => Some(r),
            _ => None,
        })
    } else {
        state.normal
    }
}

/// Records a user response. `apply_to_all == false` doesn't latch but still
/// flips `has_seen_clash`, so a later file-to-folder "* all" choice won't be
/// considered "first" and won't spread to the normal bucket.
pub(super) fn apply_to_all_record(
    state: &mut ApplyToAll,
    is_file_to_folder: bool,
    resolution: ConflictResolution,
    apply_to_all: bool,
) {
    let was_first_clash = !state.has_seen_clash;
    state.has_seen_clash = true;
    if !apply_to_all {
        return;
    }
    if is_file_to_folder {
        state.file_to_folder = Some(resolution);
        // File-to-folder clash + "* all" + first-ever clash → spread to
        // normal too. After this point both buckets agree.
        if was_first_clash {
            state.normal = Some(resolution);
        }
    } else {
        state.normal = Some(resolution);
    }
}

// ============================================================================
// Conflict handling helpers
// ============================================================================

/// Resolves a file conflict based on the configured resolution mode.
/// Returns the resolved destination info, or None if the file should be skipped.
/// Also returns whether the resolution should be applied to all future conflicts.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
pub(super) fn resolve_conflict(
    source: &Path,
    dest_path: &Path,
    config: &WriteOperationConfig,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut ApplyToAll,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    // Pre-fetch metadata once; reused for the conflict event, the "is file →
    // folder?" classification, and the conditional-variant reduction.
    let source_meta = fs::metadata(source).ok();
    let dest_meta = fs::metadata(dest_path).ok();
    let is_file_to_folder = matches!(
        (
            source_meta.as_ref().map(|m| m.is_dir()),
            dest_meta.as_ref().map(|m| m.is_dir())
        ),
        (Some(false), Some(true)),
    );

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
            // Emit conflict event for frontend to handle. Folder sizes come
            // from the drive index — we never walk the destination tree
            // synchronously to compute one. `None` is the legitimate
            // "(unknown)" rendering on the FE.
            let source_size_for_dir = if matches!(source_meta.as_ref().map(|m| m.is_dir()), Some(true)) {
                lookup_indexed_size(source)
            } else {
                None
            };
            let destination_size_for_dir = if matches!(dest_meta.as_ref().map(|m| m.is_dir()), Some(true)) {
                lookup_indexed_size(dest_path)
            } else {
                None
            };
            // Arm the conflict slot BEFORE emitting the event. A responder (the
            // FE's `resolve_write_conflict`, which answers through the slot) can
            // only answer a conflict it has observed; if the event reached it
            // before the slot was armed, the answer would land on nothing and
            // the `blocking_recv` below would hang. Arming first makes the
            // sender available the instant the event is in the responder's
            // hands. The slot's lock is released inside `arm` — never held
            // across the emit or the recv. Mirrors the volume-side Stop branch
            // in `transfer/volume/conflict.rs`.
            //
            // Arming also mints this clash's id and builds the event around it,
            // so the question the slot holds and the one on the wire are the
            // same value: an answer has to name that id, and one meant for a
            // clash this operation has already left behind can't decide the
            // next one.
            let (tx, rx) = tokio::sync::oneshot::channel();
            let event = state.conflict_slot.arm(tx, |conflict_id| {
                build_conflict_event(
                    operation_id,
                    conflict_id,
                    source,
                    dest_path,
                    source_meta.as_ref(),
                    dest_meta.as_ref(),
                    source_size_for_dir,
                    destination_size_for_dir,
                )
            });
            let event_conflict_id = event.conflict_id;

            // The operation has parked on a person, and from here it emits
            // nothing until they answer. Announced BEFORE the prompt goes out,
            // and while the slot is armed: a surface can answer synchronously
            // from inside `emit_conflict` (the FE effectively does), and this
            // window's own wait would be over before it was ever mentioned.
            state.announce_human_wait(events);

            events.emit_conflict(event);

            // Wait for user to call resolve_write_conflict.
            // The sender is dropped on cancel_write_operation, which unblocks the
            // receiver immediately. No timeout needed (the old 30s timeout was a
            // safety net; sender-drop is strictly better).
            // `blocking_recv` because this local-FS conflict path is synchronous
            // and runs inside `spawn_blocking`, so it blocks its blocking-pool
            // thread on the oneshot. The async volume path (`transfer/volume/conflict.rs`)
            // uses `rx.await` instead.
            match rx.blocking_recv() {
                Ok(response) => {
                    // The wait is over: the slot is spent, so this tick carries no
                    // wait at all and every window puts the speed back. The next
                    // file's own progress would eventually say the same thing, and
                    // on a Skip near the end of a copy there may not be one.
                    state.announce_human_wait(events);
                    // This clash is over. Said out loud, because only ONE surface
                    // learns it from its own call's return value: everyone else
                    // showing the same prompt (the queue window, the main
                    // window's host, anything watching after an agent answered
                    // over MCP) would keep asking a question with no answer left
                    // to give. Volume twin: `transfer/volume/conflict.rs`.
                    events.emit_conflict_resolved(WriteConflictResolvedEvent {
                        operation_id: operation_id.to_string(),
                        conflict_id: event_conflict_id,
                    });
                    // Save the original (unreduced) variant under the right bucket so
                    // subsequent conflicts re-evaluate the conditional variants against
                    // their own metadata, not the file that originally prompted.
                    apply_to_all_record(
                        apply_to_all_resolution,
                        is_file_to_folder,
                        response.resolution,
                        response.apply_to_all,
                    );
                    // Reduce conditional variants to Overwrite / Skip against this
                    // file's already-fetched metadata, then apply.
                    let effective =
                        reduce_conditional_resolution(response.resolution, source_meta.as_ref(), dest_meta.as_ref());
                    apply_resolution(effective, dest_path)
                }
                Err(_) => {
                    // Sender dropped = operation cancelled
                    Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".to_string(),
                    })
                }
            }
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => apply_resolution(ConflictResolution::Overwrite, dest_path),
        ConflictResolution::Rename => apply_resolution(ConflictResolution::Rename, dest_path),
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            let effective = reduce_conditional_resolution(resolution, source_meta.as_ref(), dest_meta.as_ref());
            apply_resolution(effective, dest_path)
        }
    }
}

/// Maps the conditional variants (`OverwriteSmaller` / `OverwriteOlder`) to a
/// concrete `Overwrite` or `Skip` for the file at hand, based on its source/dest
/// metadata. Non-conditional variants pass through unchanged. Comparisons are
/// strict: equal sizes / equal mtimes / missing metadata all reduce to `Skip`,
/// so a borderline file is never silently overwritten.
///
/// Logs the *reason* on Skip (kept vs missing-metadata vs equal) so users
/// running an SMB / MTP copy who pick "Overwrite all older" against a backend
/// that doesn't surface `modified_at` can see in the operation log why every
/// conflict was skipped, rather than wondering why nothing happened.
fn reduce_conditional_resolution(
    resolution: ConflictResolution,
    source_meta: Option<&fs::Metadata>,
    dest_meta: Option<&fs::Metadata>,
) -> ConflictResolution {
    match resolution {
        ConflictResolution::OverwriteSmaller => {
            match (source_meta.map(fs::Metadata::len), dest_meta.map(fs::Metadata::len)) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(src), Some(dst)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller: skipping — destination not strictly smaller (src={src}, dst={dst})"
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller: skipping — size unknown for source or destination"
                    );
                    ConflictResolution::Skip
                }
            }
        }
        ConflictResolution::OverwriteOlder => {
            let src_time = source_meta.and_then(|m| m.modified().ok());
            let dst_time = dest_meta.and_then(|m| m.modified().ok());
            match (src_time, dst_time) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(_), Some(_)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder: skipping — destination not strictly older than source"
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder: skipping — modified time unknown for source or destination"
                    );
                    ConflictResolution::Skip
                }
            }
        }
        other => other,
    }
}

/// Applies a specific conflict resolution to a destination path.
/// Returns None for Skip, or ResolvedDestination with path and overwrite flag.
fn apply_resolution(
    resolution: ConflictResolution,
    dest_path: &Path,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Don't delete here - the copy function will use safe overwrite pattern
            Ok(Some(ResolvedDestination {
                path: dest_path.to_path_buf(),
                needs_safe_overwrite: true,
            }))
        }
        ConflictResolution::Rename => {
            // Find a unique name by appending " (1)", " (2)", etc. `find_unique_name`
            // atomically RESERVES the chosen name by creating a 0-byte placeholder
            // file (TOCTOU guard, see its doc comment). The caller's write must
            // therefore land *on* that placeholder, overwriting it — so we flag
            // `needs_safe_overwrite`. Without it the same-APFS-volume copy path
            // (`copyfile(3)` with `COPYFILE_EXCL`) refuses to write over the
            // existing placeholder and fails with `DestinationExists`, losing the
            // incoming bytes. The overwrite path consumes the placeholder cleanly
            // and the reservation still closes the race window.
            let unique_path = find_unique_name(dest_path);
            Ok(Some(ResolvedDestination {
                path: unique_path,
                needs_safe_overwrite: true,
            }))
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            // Conditional variants are always reduced to Overwrite / Skip by
            // `reduce_conditional_resolution` before reaching this function.
            unreachable!("conditional conflict resolutions must be reduced before apply_resolution")
        }
    }
}

/// Builds a `WriteConflictEvent` from the source / destination metadata pair.
/// Extracted from `resolve_conflict` so the source/destination type-mismatch
/// flags can be unit-tested in isolation. Pre-fix the inline event omitted
/// `source_is_directory` / `destination_is_directory` entirely; the FE Stop
/// dialog couldn't tell the user "you're about to replace a folder with a
/// file" and silently took the user's "Overwrite" click as consent to drop
/// an entire directory tree.
#[allow(
    clippy::too_many_arguments,
    reason = "the event describes both sides of a clash from four sources (identity, paths, stat'd metadata, indexed folder sizes); bundling them would only move the same list one call up"
)]
fn build_conflict_event(
    operation_id: &str,
    conflict_id: ConflictId,
    source: &Path,
    dest_path: &Path,
    source_meta: Option<&fs::Metadata>,
    dest_meta: Option<&fs::Metadata>,
    // Recursive size of the *source* when it's a directory (from the
    // pre-flight scan's per-source-root total). Ignored when source is a
    // file — files use `metadata.len()` directly. Always `Some` for folder
    // sources after pre-flight; the rare MCP / skip-preflight path may pass
    // `None`, in which case source_size falls back to 0.
    source_size_for_dir: Option<u64>,
    // Recursive size of the *destination* when it's a directory. The caller
    // looks it up in the drive index; `None` means "the index doesn't cover
    // this path" (network mount, MTP, paths outside the index scope) and
    // surfaces to the FE as the `(unknown)` rendering. Files always use
    // `metadata.len()` and this override is ignored.
    destination_size_for_dir: Option<u64>,
) -> WriteConflictEvent {
    let destination_is_newer = match (source_meta, dest_meta) {
        (Some(s), Some(d)) => {
            let src_time = s.modified().ok();
            let dst_time = d.modified().ok();
            matches!((src_time, dst_time), (Some(src), Some(dst)) if dst > src)
        }
        _ => false,
    };

    let source_is_directory = source_meta.map(|m| m.is_dir()).unwrap_or(false);
    let destination_is_directory = dest_meta.map(|m| m.is_dir()).unwrap_or(false);

    // Files: use `metadata.len()` directly. Directories: use the caller-
    // supplied recursive total (the BE never walks a destination tree). On the
    // local-FS path the source is always stat-able, so a file source is always
    // `Some`; a folder source is `Some` post-preflight and `None` only on the
    // rare skip-preflight path.
    let source_size: Option<u64> = if source_is_directory {
        source_size_for_dir
    } else {
        source_meta.map(|m| m.len())
    };
    let destination_size = if destination_is_directory {
        destination_size_for_dir
    } else {
        dest_meta.map(|m| m.len())
    };
    // Collapse to `None` when either side is unknown — the FE can't render a
    // meaningful "(larger)" annotation without both numbers.
    let size_difference = match (destination_size, source_size) {
        (Some(d), Some(s)) => Some(d as i64 - s as i64),
        _ => None,
    };

    let unix_secs = |m: Option<&fs::Metadata>| -> Option<i64> {
        m?.modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    };

    WriteConflictEvent {
        operation_id: operation_id.to_string(),
        conflict_id,
        source_path: source.display().to_string(),
        destination_path: dest_path.display().to_string(),
        source_size,
        destination_size,
        source_modified: unix_secs(source_meta),
        destination_modified: unix_secs(dest_meta),
        destination_is_newer,
        size_difference,
        source_is_directory,
        destination_is_directory,
    }
}

// ============================================================================
// Conflict info helpers
// ============================================================================

/// Calculates destination path for a source file relative to source root.
pub(super) fn calculate_dest_path(
    path: &Path,
    source_root: &Path,
    dest_root: &Path,
) -> Result<PathBuf, WriteOperationError> {
    // If path is the source root itself, use the file name in dest_root
    if path == source_root {
        let file_name = path.file_name().ok_or_else(|| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Invalid source path".to_string(),
        })?;
        return Ok(dest_root.join(file_name));
    }

    // Otherwise, strip the source root's parent and join with dest_root
    let source_parent = source_root.parent().unwrap_or(source_root);
    let relative = path
        .strip_prefix(source_parent)
        .map_err(|_| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Failed to calculate relative path".to_string(),
        })?;

    Ok(dest_root.join(relative))
}

/// Creates ConflictInfo for a source/destination pair.
pub(super) fn create_conflict_info(
    source: &Path,
    dest: &Path,
    source_metadata: &fs::Metadata,
) -> Result<Option<ConflictInfo>, WriteOperationError> {
    let dest_metadata = match fs::symlink_metadata(dest) {
        Ok(m) => m,
        Err(_) => return Ok(None), // No conflict if dest doesn't exist
    };

    let source_modified = source_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let dest_modified = dest_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let destination_is_newer = match (source_modified, dest_modified) {
        (Some(s), Some(d)) => d > s,
        _ => false,
    };

    Ok(Some(ConflictInfo {
        source_path: source.display().to_string(),
        destination_path: dest.display().to_string(),
        source_size: source_metadata.len(),
        destination_size: dest_metadata.len(),
        source_modified,
        destination_modified: dest_modified,
        destination_is_newer,
        is_directory: source_metadata.is_dir(),
    }))
}

/// Samples conflicts if there are too many, using reservoir sampling.
pub(super) fn sample_conflicts(conflicts: Vec<ConflictInfo>, max_count: usize) -> (Vec<ConflictInfo>, bool) {
    if conflicts.len() <= max_count {
        return (conflicts, false);
    }

    // Use reservoir sampling for uniform random selection
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut sampled: Vec<ConflictInfo> = conflicts.iter().take(max_count).cloned().collect();

    for (i, conflict) in conflicts.iter().enumerate().skip(max_count) {
        // Deterministic "random" based on path hash for reproducibility
        let mut hasher = DefaultHasher::new();
        conflict.source_path.hash(&mut hasher);
        i.hash(&mut hasher);
        let hash = hasher.finish();
        let j = (hash as usize) % (i + 1);

        if j < max_count {
            sampled[j] = conflict.clone();
        }
    }

    (sampled, true)
}

// The tests are split by topic into `#[path]` children so this module stays
// readable; the ` (N)` naming suites live with their code in `unique_name.rs`.
#[cfg(test)]
#[path = "conflict_apply_to_all_tests.rs"]
mod apply_to_all_tests;
#[cfg(test)]
#[path = "conflict_event_tests.rs"]
mod build_conflict_event_tests;
#[cfg(test)]
#[path = "conflict_conditional_tests.rs"]
mod conditional_resolution_tests;
#[cfg(test)]
#[path = "conflict_stop_tests.rs"]
mod stop_branch_park_tests;
