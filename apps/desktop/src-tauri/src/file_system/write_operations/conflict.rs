//! Conflict resolution for write operations.
//!
//! The two-bucket `ApplyToAll` latch model, the Stop-mode oneshot wait, the
//! conditional-variant reduction (`OverwriteSmaller` / `OverwriteOlder`),
//! unique-name reservation, and the helpers that build conflict events /
//! conflict info and sample conflicts for the dialog.

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
use super::validation::path_exists_or_is_symlink;

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

/// Builds the `counter`-th candidate name under the shared ` (N)` dedup
/// convention: `counter == 0` is the bare `stem[.ext]`, `1..` appends ` (N)`
/// before the extension. This is the ONE place the convention is written:
/// `find_unique_name`, `next_available_name`, the volume namer
/// (`transfer/volume/conflict.rs::find_unique_volume_name`), and the
/// clipboard-paste writer all go through it, so the numbering paths can't drift.
pub(super) fn numbered_name(stem: &str, ext: Option<&str>, counter: u32) -> String {
    match (ext, counter) {
        (Some(e), 0) => format!("{stem}.{e}"),
        (None, 0) => stem.to_string(),
        (Some(e), n) => format!("{stem} ({n}).{e}"),
        (None, n) => format!("{stem} ({n})"),
    }
}

/// Reads a trailing ` (N)` off a file stem so a search *continues* the series
/// rather than nesting inside it: duplicating `photo (1).jpg` gives
/// `photo (2).jpg`, never `photo (1) (1).jpg`. Returns the base to number from
/// and the first counter to try.
///
/// Pure, and the only rule for what a sequence IS. Searches reach it through
/// [`NameCandidates`] rather than calling it directly.
///
/// What counts as a sequence is deliberately narrow, because everything else is
/// somebody's filename:
/// - The separating space is required, and the digits must be ASCII, so
///   `Report (final).pdf`, `photo(1).jpg`, and `photo (+1).jpg` are all plain text.
/// - Zero padding isn't a format to preserve: `photo (007)` continues at `(8)`.
/// - A number with no `u32` successor (too large to parse, or `u32::MAX`) is
///   plain text too, which also keeps the returned counter always advanceable.
pub(super) fn split_sequence(stem: &str) -> (&str, u32) {
    if let Some(without_close) = stem.strip_suffix(')')
        && let Some((base, digits)) = without_close.rsplit_once(" (")
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && let Ok(current) = digits.parse::<u32>()
        && let Some(next) = current.checked_add(1)
    {
        return (base, next);
    }
    (stem, 1)
}

/// The ` (N)` candidate sequence for a path: where to put the result, the base
/// to number from (any trailing sequence already split off by
/// [`split_sequence`]), and the counter to try next.
///
/// This is the whole of what the ` (N)` searches share. Each walks the same
/// candidates and differs only in how it TESTS one: [`find_unique_name`]
/// reserves with `O_CREAT|O_EXCL` and has to keep advancing when it loses that
/// race, [`next_available_name`] only probes, and the volume namer
/// (`transfer/volume/conflict.rs::find_unique_volume_name`) does either
/// depending on the backend. A search loop can't be shared across that
/// difference, so this carries the candidates and the searches keep their loops.
pub(super) struct NameCandidates<'a> {
    parent: &'a Path,
    base: String,
    extension: Option<String>,
    counter: u32,
    /// Where `counter` started, so a search that bounds its own effort can count
    /// ATTEMPTS. A name that already ends in a high ` (N)` starts the sequence
    /// there, so the counter's absolute value says nothing about effort spent.
    start: u32,
}

impl<'a> NameCandidates<'a> {
    pub(super) fn for_path(path: &'a Path) -> Self {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let (base, counter) = split_sequence(&stem);
        Self {
            parent: path.parent().unwrap_or(Path::new("")),
            base: base.to_string(),
            extension: path.extension().map(|s| s.to_string_lossy().to_string()),
            counter,
            start: counter,
        }
    }

    /// The candidate to try right now.
    pub(super) fn current(&self) -> PathBuf {
        self.parent
            .join(numbered_name(&self.base, self.extension.as_deref(), self.counter))
    }

    /// Moves past a candidate the caller found taken.
    pub(super) fn advance(&mut self) {
        self.counter = self.counter.saturating_add(1);
    }

    /// How many candidates this search has already rejected.
    pub(super) fn attempts(&self) -> u32 {
        self.counter.saturating_sub(self.start)
    }
}

/// Finds a unique filename by appending " (1)", " (2)", etc., **atomically
/// reserving** the chosen name via `O_CREAT|O_EXCL` so a concurrent process
/// (backup tool, cloud-sync agent, second Cmdr op) can't land a file at the
/// same path between our pick and the caller's write.
///
/// Pre-fix this returned the first non-existing candidate after an
/// `if !new_path.exists()` check; the caller then copied or renamed to that
/// path, leaving a ~ms TOCTOU window during which a concurrent write could
/// land an unrelated file at the same name — silently clobbered the next time
/// our copy / rename hit the path. By creating an empty placeholder under the
/// reserved name and letting downstream operations (`fs::copy` truncates;
/// `fs::rename` atomically replaces; `copyfile(3)` / `copy_file_range(2)` open
/// the dest with create+truncate) overwrite it, the race window collapses to
/// microseconds. Callers never observe the placeholder.
///
/// On the rare loss-of-the-placeholder edge case (a third party deletes our
/// empty file before the caller writes), the caller's write still succeeds
/// (creating fresh).
pub(super) fn find_unique_name(path: &Path) -> PathBuf {
    let mut candidates = NameCandidates::for_path(path);

    loop {
        let new_path = candidates.current();

        match fs::OpenOptions::new().write(true).create_new(true).open(&new_path) {
            Ok(_) => return new_path,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                candidates.advance();
            }
            Err(_) => {
                // Anything else (parent unwritable, ENOSPC, …) leaks back to
                // the caller's write attempt, which has its own error path.
                // Mirror the pre-fix behaviour of returning the path so the
                // caller surfaces the real error against the right operation.
                return new_path;
            }
        }
    }
}

/// Picks the next free ` (N)` name **without reserving it** — same convention
/// and same sequence rule as [`find_unique_name`], but a probe and no create.
///
/// For callers that reserve the name themselves in a way an `O_CREAT|O_EXCL`
/// *file* placeholder would get in the way of: a directory claims its name with
/// `create_dir`, and a copy that lands through the ordinary non-overwrite write
/// path would otherwise find `find_unique_name`'s placeholder sitting at the
/// destination and raise a conflict against it.
///
/// Occupancy uses `path_exists_or_is_symlink`, so a dangling symlink counts as
/// taken; handing that name back would let the caller's write follow the
/// symlink to wherever it points.
pub(super) fn next_available_name(path: &Path) -> PathBuf {
    let mut candidates = NameCandidates::for_path(path);

    loop {
        let new_path = candidates.current();
        if !path_exists_or_is_symlink(&new_path) {
            return new_path;
        }
        candidates.advance();
    }
}

/// Claims the next free ` (N)` name as a DIRECTORY, creating it. The `create_dir`
/// loop IS the reservation: `mkdir(2)` fails `AlreadyExists` on a taken name, so
/// advancing on that error is the directory analogue of [`find_unique_name`]'s
/// `O_CREAT|O_EXCL` file placeholder.
///
/// Separate from [`find_unique_name`] because a placeholder FILE is precisely
/// what a directory destination can't have sitting at its name. The caller
/// records the returned path so rollback can remove it.
pub(super) fn create_unique_dir(path: &Path) -> std::io::Result<PathBuf> {
    let mut candidates = NameCandidates::for_path(path);

    loop {
        let new_path = candidates.current();
        match fs::create_dir(&new_path) {
            Ok(()) => return Ok(new_path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => candidates.advance(),
            Err(e) => return Err(e),
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

#[cfg(test)]
mod conditional_resolution_tests {
    //! Tests for `reduce_conditional_resolution` — the gate that decides whether
    //! the user's "Overwrite all smaller" / "Overwrite all older" choice
    //! actually overwrites this particular file, or skips it.
    //!
    //! **Data-safety contract pinned here**: a destination is overwritten ONLY
    //! when strictly smaller (by byte count) or strictly older (by mtime) than
    //! the source. Equal-or-bigger / equal-or-newer / unknown metadata always
    //! reduces to `Skip`. Bugs in this function would silently overwrite files
    //! the user expected to keep, so the tests below are exhaustive across the
    //! comparison axes.
    use super::*;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;
    use uuid::Uuid;

    fn temp_with_size_and_mtime(dir: &Path, name: &str, size: usize, mtime_secs: i64) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, vec![0u8; size]).unwrap();
        let mtime = filetime::FileTime::from_unix_time(mtime_secs, 0);
        filetime::set_file_mtime(&path, mtime).unwrap();
        path
    }

    /// Snapshot the metadata once. `reduce_conditional_resolution` borrows
    /// `&Metadata`, so the helper has to return owned values the caller
    /// stores in locals.
    fn meta(path: &Path) -> fs::Metadata {
        fs::metadata(path).unwrap()
    }

    fn unique_dir() -> TempDir {
        // Per-test unique dir keeps parallel runs from racing on identical
        // names. Default TempDir is already unique but the explicit prefix
        // makes failing-test debug output easier to interpret.
        tempfile::Builder::new()
            .prefix(&format!("cmdr-cond-resolve-{}", Uuid::new_v4()))
            .tempdir()
            .unwrap()
    }

    // ------------------------------------------------------------------
    // OverwriteSmaller — strict by byte count
    // ------------------------------------------------------------------

    #[test]
    fn smaller_overwrites_only_when_dest_strictly_smaller() {
        // Pin: dst (smaller) < src → Overwrite. Any other relation → Skip.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Overwrite,
            "Strictly smaller dst must be overwritten under OverwriteSmaller"
        );
    }

    #[test]
    fn smaller_skips_when_dest_equal_size() {
        // Pin: dst.len == src.len → Skip. Critical to data safety: a same-size
        // dst is NOT obviously stale; user picked "smaller" meaning STRICTLY.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 1000, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Equal-size dst must NOT be overwritten under OverwriteSmaller"
        );
    }

    #[test]
    fn smaller_skips_when_dest_strictly_larger() {
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 1000, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Larger dst must NOT be overwritten under OverwriteSmaller — would corrupt the user's keeper file"
        );
    }

    #[test]
    fn smaller_skips_on_zero_byte_dest_when_source_also_zero() {
        // Edge: 0 bytes vs 0 bytes is NOT strictly smaller; must skip.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 0, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 0, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(resolved, ConflictResolution::Skip);
    }

    #[test]
    fn smaller_skips_when_source_metadata_missing() {
        // Source meta missing means we can't prove dst is smaller; fail closed (Skip).
        let dir = unique_dir();
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, None, Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Unknown source size must fail closed to Skip; we cannot prove dst is smaller"
        );
    }

    #[test]
    fn smaller_skips_when_dest_metadata_missing() {
        // Dest meta missing: the dest exists (we're in conflict resolution
        // because of that) but we can't size it — skip rather than overwrite blindly.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let src_m = meta(&src);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), None);
        assert_eq!(resolved, ConflictResolution::Skip);
    }

    #[test]
    fn smaller_skips_when_both_metadata_missing() {
        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, None, None);
        assert_eq!(resolved, ConflictResolution::Skip);
    }

    // ------------------------------------------------------------------
    // OverwriteOlder — strict by mtime
    // ------------------------------------------------------------------

    #[test]
    fn older_overwrites_only_when_dest_strictly_older() {
        // Pin: dst mtime < src mtime → Overwrite. The user wants stale files replaced.
        let dir = unique_dir();
        // 2020 dst, 2024 src
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_700_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(resolved, ConflictResolution::Overwrite);
    }

    #[test]
    fn older_skips_when_dest_equal_mtime() {
        // Equal mtime is not strictly older — skip.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Equal-mtime dst must NOT be overwritten under OverwriteOlder"
        );
    }

    #[test]
    fn older_skips_when_dest_strictly_newer() {
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_700_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Newer dst must NOT be overwritten under OverwriteOlder — would clobber the user's fresher file"
        );
    }

    #[test]
    fn older_one_second_difference_still_counts_as_older() {
        // Subsecond paranoia: cross the second boundary so the comparison is unambiguous.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 100, 1_600_000_001);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 100, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(resolved, ConflictResolution::Overwrite);
    }

    #[test]
    fn older_skips_when_metadata_missing() {
        // Both sides missing: skip. Source missing: skip. Dest missing: skip.
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, None, None),
            ConflictResolution::Skip
        );
        let dir = unique_dir();
        let some = temp_with_size_and_mtime(dir.path(), "f", 100, 1_600_000_000);
        let some_m = meta(&some);
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, None, Some(&some_m)),
            ConflictResolution::Skip,
            "Missing source mtime → fail closed to Skip"
        );
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&some_m), None),
            ConflictResolution::Skip,
            "Missing dest mtime → fail closed to Skip"
        );
    }

    // ------------------------------------------------------------------
    // Pass-through variants
    // ------------------------------------------------------------------

    #[test]
    fn non_conditional_variants_pass_through_unchanged() {
        // The reduction must be a no-op for the four pre-existing variants;
        // callers depend on this when they pass `response.resolution` blindly.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        for v in [
            ConflictResolution::Stop,
            ConflictResolution::Skip,
            ConflictResolution::Overwrite,
            ConflictResolution::Rename,
        ] {
            assert_eq!(
                reduce_conditional_resolution(v, Some(&src_m), Some(&dst_m)),
                v,
                "Variant {v:?} must pass through unchanged"
            );
        }
    }

    // ------------------------------------------------------------------
    // Independence of axes
    // ------------------------------------------------------------------

    #[test]
    fn smaller_ignores_mtime() {
        // A dst that's smaller AND newer must still be overwritten under
        // OverwriteSmaller — the user opted in to size-only comparison.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 100, 1_700_000_000); // newer + smaller
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Overwrite
        );
    }

    #[test]
    fn older_ignores_size() {
        // A dst that's older AND larger must still be overwritten under
        // OverwriteOlder. (Common case: a stub file replaced by a fuller version
        // — the user wants the newer file regardless of size.)
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 100, 1_700_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 5000, 1_600_000_000); // older + larger
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Overwrite
        );
    }

    // ------------------------------------------------------------------
    // Realistic mtime span via SystemTime (in case filetime crate ever drifts).
    // ------------------------------------------------------------------

    #[test]
    fn older_works_with_systemtime_offsets_from_now() {
        // Belt-and-suspenders: don't depend on hardcoded epoch values.
        // Use "now" and "now - 1 hour" to prove the comparison works for
        // realistic timestamps the OS would produce live.
        let dir = unique_dir();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        fs::write(&src, b"src").unwrap();
        fs::write(&dst, b"dst").unwrap();

        let now = SystemTime::now();
        let an_hour_ago = now - Duration::from_secs(3600);
        filetime::set_file_mtime(&src, filetime::FileTime::from_system_time(now)).unwrap();
        filetime::set_file_mtime(&dst, filetime::FileTime::from_system_time(an_hour_ago)).unwrap();

        let src_m = meta(&src);
        let dst_m = meta(&dst);
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Overwrite,
            "dst from an hour ago must be older than src from now"
        );
        // And the inverse: dst newer than src → Skip.
        filetime::set_file_mtime(&src, filetime::FileTime::from_system_time(an_hour_ago)).unwrap();
        filetime::set_file_mtime(&dst, filetime::FileTime::from_system_time(now)).unwrap();
        let src_m = meta(&src);
        let dst_m = meta(&dst);
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Skip,
            "dst from now must NOT be overwritten by src from an hour ago"
        );
    }
}

#[cfg(test)]
mod build_conflict_event_tests {
    //! Regression for the low-severity audit finding: the Stop-mode
    //! conflict event used to carry no `is_directory` flags, so the FE
    //! dialog rendered a generic "file already exists" prompt even when
    //! the collision was a type mismatch (file → directory or vice versa).
    //! User clicked "Overwrite" thinking they were replacing a file, ended
    //! up dropping a whole directory tree without warning.
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn file_over_directory_marks_destination_is_directory() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("notes.txt");
        let dest = temp.path().join("conflicting");
        fs::write(&source, b"a file").unwrap();
        fs::create_dir(&dest).unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op-1",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            None,
            Some(12345),
        );

        assert!(!event.source_is_directory, "source is a file");
        assert!(event.destination_is_directory, "destination is a directory");
    }

    #[test]
    fn directory_over_file_marks_source_is_directory() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("conflicting");
        let dest = temp.path().join("notes.txt");
        fs::create_dir(&source).unwrap();
        fs::write(&dest, b"a file").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op-2",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            Some(67890),
            None,
        );

        assert!(event.source_is_directory, "source is a directory");
        assert!(!event.destination_is_directory, "destination is a file");
    }

    #[test]
    fn file_over_file_flags_both_false() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let dest = temp.path().join("b.txt");
        fs::write(&source, b"a").unwrap();
        fs::write(&dest, b"b").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op-3",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            None,
            None,
        );

        assert!(!event.source_is_directory);
        assert!(!event.destination_is_directory);
    }

    #[test]
    fn file_dest_uses_metadata_len_ignoring_override() {
        // Files always have a known size via metadata. The override exists
        // only for directories (where metadata.len() is the inode entry
        // size, not the recursive content size).
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let dest = temp.path().join("b.txt");
        fs::write(&source, b"hello").unwrap();
        fs::write(&dest, b"world!").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            Some(99999),
            Some(99999),
        );

        assert_eq!(event.source_size, Some(5));
        assert_eq!(event.destination_size, Some(6));
        assert_eq!(event.size_difference, Some(1));
    }

    #[test]
    fn folder_dest_uses_override_size() {
        // For dir destinations the recursive size lives in the drive index;
        // the caller fetches it (or `None` when the index doesn't cover the
        // path) and hands it to us.
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("notes.txt");
        let dest = temp.path().join("conflicting");
        fs::write(&source, b"a").unwrap();
        fs::create_dir(&dest).unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            None,
            Some(4_096_000),
        );

        assert_eq!(event.source_size, Some(1));
        assert_eq!(event.destination_size, Some(4_096_000));
        assert_eq!(event.size_difference, Some(4_095_999));
    }

    #[test]
    fn folder_dest_with_unknown_size_surfaces_none() {
        // The index doesn't cover the destination (network mount, MTP, …).
        // Report `(unknown)` on the wire as `None`; size_difference also
        // collapses to `None` since one side is unknown.
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("notes.txt");
        let dest = temp.path().join("conflicting");
        fs::write(&source, b"a").unwrap();
        fs::create_dir(&dest).unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            None,
            None,
        );

        assert_eq!(event.source_size, Some(1));
        assert_eq!(event.destination_size, None);
        assert_eq!(event.size_difference, None);
    }

    #[test]
    fn folder_source_uses_override_size() {
        // Folder-source sizes come from the pre-flight scan's per-source-root
        // total. The override is always Some for source-folder clashes.
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("payload");
        let dest = temp.path().join("notes.txt");
        fs::create_dir(&source).unwrap();
        fs::write(&dest, b"hi").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            Some(123_456),
            None,
        );

        assert_eq!(event.source_size, Some(123_456));
        assert_eq!(event.destination_size, Some(2));
        assert_eq!(event.size_difference, Some(2 - 123_456));
    }

    #[test]
    fn folder_source_with_unknown_size_surfaces_none() {
        // A folder source with no pre-flight scan total (the skip-preflight /
        // fast-path case) surfaces `source_size: None`, and `size_difference`
        // collapses to `None` just as it does when the destination is unknown.
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("payload");
        let dest = temp.path().join("notes.txt");
        fs::create_dir(&source).unwrap();
        fs::write(&dest, b"hi").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            ConflictId(1),
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            None,
            None,
        );

        assert_eq!(event.source_size, None);
        assert_eq!(event.destination_size, Some(2));
        assert_eq!(event.size_difference, None);
    }
}

#[cfg(test)]
mod find_unique_name_tests {
    //! Regression for the low-severity audit finding: pre-fix
    //! `find_unique_name` picked a name by exists()-checking each candidate
    //! and returning the first miss. Between the check and the caller's
    //! write, a concurrent process (backup tool, cloud-sync agent, second
    //! Cmdr op) could land a file at the same name and the next copy /
    //! rename would silently clobber it. The fix atomically reserves the
    //! chosen name via O_EXCL.
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn reserves_the_chosen_name_on_disk() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("notes.txt");
        fs::write(&target, b"original").unwrap();

        let unique = find_unique_name(&target);

        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
        // O_EXCL placeholder must already exist after the call.
        assert!(unique.exists(), "reservation must create the placeholder");
        // Second call goes to (2), proving the first reservation persisted.
        let next = find_unique_name(&target);
        assert_eq!(next.file_name().unwrap().to_string_lossy(), "notes (2).txt");
    }

    #[test]
    fn keeps_extension_in_the_right_place() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("report.pdf");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "report (1).pdf");
    }

    #[test]
    fn handles_extensionless_filenames() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("README");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "README (1)");
    }

    #[test]
    fn continues_a_trailing_sequence_instead_of_nesting() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("photo (1).jpg");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (2).jpg");
    }

    #[test]
    fn a_sequence_skips_past_every_taken_number() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("photo (1).jpg");
        fs::write(&target, b"x").unwrap();
        fs::write(temp.path().join("photo (2).jpg"), b"x").unwrap();
        fs::write(temp.path().join("photo (3).jpg"), b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (4).jpg");
    }

    #[test]
    fn a_non_numeric_parenthetical_is_not_a_sequence() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("Report (final).pdf");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "Report (final) (1).pdf");
    }

    #[test]
    fn a_number_too_big_for_u32_is_literal_text() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("photo (99999999999).jpg");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(
            unique.file_name().unwrap().to_string_lossy(),
            "photo (99999999999) (1).jpg"
        );
    }

    #[test]
    fn zero_padding_is_not_preserved() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("photo (007).jpg");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (8).jpg");
    }

    #[test]
    fn a_zero_sequence_advances_to_one() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("photo (0).jpg");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "photo (1).jpg");
    }
}

#[cfg(test)]
mod split_sequence_tests {
    //! The pure half of the ` (N)` convention: what counts as a trailing
    //! sequence and what is ordinary text that happens to hold parentheses.
    use super::*;

    #[test]
    fn a_bare_stem_starts_at_one() {
        assert_eq!(split_sequence("photo"), ("photo", 1));
    }

    #[test]
    fn a_trailing_number_continues_from_itself() {
        assert_eq!(split_sequence("photo (4)"), ("photo", 5));
        assert_eq!(split_sequence("photo (0)"), ("photo", 1));
        assert_eq!(split_sequence("photo (007)"), ("photo", 8));
    }

    #[test]
    fn only_the_last_parenthetical_counts() {
        assert_eq!(split_sequence("photo (2) (3)"), ("photo (2)", 4));
        assert_eq!(split_sequence("photo (2) (draft)"), ("photo (2) (draft)", 1));
    }

    #[test]
    fn text_in_parentheses_is_not_a_sequence() {
        assert_eq!(split_sequence("Report (final)"), ("Report (final)", 1));
        assert_eq!(split_sequence("photo ()"), ("photo ()", 1));
        assert_eq!(split_sequence("photo (12a)"), ("photo (12a)", 1));
        // `u32::from_str` accepts a leading `+`; the convention never writes one.
        assert_eq!(split_sequence("photo (+1)"), ("photo (+1)", 1));
        // Non-ASCII digits parse in no locale we generate.
        assert_eq!(split_sequence("photo (١)"), ("photo (١)", 1));
    }

    #[test]
    fn the_separating_space_is_required() {
        assert_eq!(split_sequence("photo(1)"), ("photo(1)", 1));
        assert_eq!(split_sequence("(1)"), ("(1)", 1));
    }

    #[test]
    fn a_number_that_cannot_advance_is_literal_text() {
        // Doesn't fit `u32` at all.
        assert_eq!(split_sequence("photo (99999999999)"), ("photo (99999999999)", 1));
        // Fits, but has no successor, so continuing the sequence is impossible.
        let at_max = format!("photo ({})", u32::MAX);
        assert_eq!(split_sequence(&at_max), (at_max.as_str(), 1));
    }
}

#[cfg(test)]
mod next_available_name_tests {
    //! The non-reserving sibling of `find_unique_name`: same convention, same
    //! sequence rule, but it only probes and never creates. Callers that reserve
    //! the name themselves (a directory claiming its own name with `create_dir`)
    //! need the probe without the placeholder a file reservation leaves behind.
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn picks_the_next_free_name_without_creating_anything() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("notes.txt");
        fs::write(&target, b"original").unwrap();

        let picked = next_available_name(&target);

        assert_eq!(picked.file_name().unwrap().to_string_lossy(), "notes (1).txt");
        assert!(!picked.exists(), "the probe must not reserve the name");
        // Nothing was reserved, so a second call answers the same.
        assert_eq!(next_available_name(&target), picked);
    }

    #[test]
    fn skips_names_already_taken_and_continues_a_sequence() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("photo (1).jpg");
        fs::write(&target, b"x").unwrap();
        fs::write(temp.path().join("photo (2).jpg"), b"x").unwrap();

        let picked = next_available_name(&target);
        assert_eq!(picked.file_name().unwrap().to_string_lossy(), "photo (3).jpg");
    }

    #[test]
    fn works_for_a_directory_source() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("docs");
        fs::create_dir(&target).unwrap();

        let picked = next_available_name(&target);
        assert_eq!(picked.file_name().unwrap().to_string_lossy(), "docs (1)");
        assert!(!picked.exists());
    }

    #[test]
    fn a_dangling_symlink_counts_as_taken() {
        // `Path::exists()` follows symlinks and reports `false` for a broken
        // one; handing that name back would let the caller's write follow the
        // symlink to wherever it points.
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("notes.txt");
        fs::write(&target, b"x").unwrap();
        std::os::unix::fs::symlink(temp.path().join("gone"), temp.path().join("notes (1).txt")).unwrap();

        let picked = next_available_name(&target);
        assert_eq!(picked.file_name().unwrap().to_string_lossy(), "notes (2).txt");
    }
}

#[cfg(test)]
mod apply_to_all_tests {
    //! Pure-state tests for the two-bucket `ApplyToAll` latch model.
    //!
    //! Rules (per UX spec):
    //!   1. Normal clash → choice lands in the `normal` bucket only.
    //!   2. File-to-folder clash → choice lands in the `file_to_folder` bucket
    //!      only.
    //!   3. Special case: if the FIRST clash of the operation is a
    //!      file-to-folder one, a "* all" choice spreads to both buckets.
    //!   4. Carry-over: Skip/Rename in the `normal` bucket apply to subsequent
    //!      file-to-folder clashes too (these are universally safe). Overwrite
    //!      variants never carry over from normal → file-to-folder.
    use super::*;

    fn fresh() -> ApplyToAll {
        ApplyToAll::default()
    }

    #[test]
    fn default_state_is_empty() {
        let state = fresh();
        assert!(apply_to_all_effective(&state, false).is_none());
        assert!(apply_to_all_effective(&state, true).is_none());
    }

    #[test]
    fn normal_overwrite_all_stays_in_normal_bucket() {
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, false),
            Some(ConflictResolution::Overwrite)
        );
        // Does NOT spread to file-to-folder — user has to be re-prompted.
        assert_eq!(apply_to_all_effective(&state, true), None);
    }

    #[test]
    fn normal_skip_all_carries_over_to_file_to_folder() {
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Skip, true);
        assert_eq!(apply_to_all_effective(&state, false), Some(ConflictResolution::Skip));
        // Safe action: skip the file-to-folder one too without re-prompting.
        assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Skip));
    }

    #[test]
    fn normal_rename_all_carries_over_to_file_to_folder() {
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Rename, true);
        assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Rename));
    }

    #[test]
    fn normal_conditional_variants_do_not_carry_over() {
        // OverwriteSmaller / OverwriteOlder are destructive — same rule as
        // Overwrite. They never reach file-to-folder without an explicit prompt.
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::OverwriteSmaller, true);
        assert_eq!(apply_to_all_effective(&state, true), None);

        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::OverwriteOlder, true);
        assert_eq!(apply_to_all_effective(&state, true), None);
    }

    #[test]
    fn file_to_folder_first_overwrite_all_spreads_to_normal() {
        // Spec: "if a file-to-folder clash is the first one, then any '* all'
        // choices should apply to ALL types of clashes."
        let mut state = fresh();
        apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, true),
            Some(ConflictResolution::Overwrite)
        );
        assert_eq!(
            apply_to_all_effective(&state, false),
            Some(ConflictResolution::Overwrite)
        );
    }

    #[test]
    fn file_to_folder_later_overwrite_all_does_not_spread() {
        // Spec example: user picks Overwrite all on a normal clash; later a
        // file-to-folder clash comes up and the user picks Skip all in it —
        // that Skip all applies to file-to-folder only.
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Overwrite, true);
        // Now a file-to-folder clash arrives. Even though a normal "Overwrite all"
        // is set, file-to-folder is destructive enough to re-prompt → user picks
        // Skip all in the file-to-folder dialog.
        apply_to_all_record(&mut state, true, ConflictResolution::Skip, true);

        // Normal bucket keeps the original Overwrite — the new Skip is
        // file-to-folder-only.
        assert_eq!(
            apply_to_all_effective(&state, false),
            Some(ConflictResolution::Overwrite)
        );
        assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Skip));
    }

    #[test]
    fn single_choice_does_not_set_apply_to_all_but_still_seeds_first_clash_flag() {
        // A non-"apply to all" choice doesn't latch, but it DOES mean the next
        // file-to-folder clash isn't "the first" any more, so its "* all"
        // choice shouldn't spread to normal.
        let mut state = fresh();
        apply_to_all_record(
            &mut state,
            false,
            ConflictResolution::Overwrite,
            /* apply_to_all */ false,
        );

        // Nothing latched yet.
        assert_eq!(apply_to_all_effective(&state, false), None);
        assert_eq!(apply_to_all_effective(&state, true), None);

        // Now a file-to-folder clash; user picks Overwrite all. Because a
        // normal clash already happened, this is NOT the first clash any more
        // → don't spread.
        apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, true),
            Some(ConflictResolution::Overwrite)
        );
        assert_eq!(apply_to_all_effective(&state, false), None);
    }

    #[test]
    fn file_to_folder_latch_wins_over_normal_carry_over() {
        // If both buckets have a value, the directly-set file-to-folder one
        // wins (don't fall back to the normal-bucket Skip/Rename carry-over).
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Skip, true);
        apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, true),
            Some(ConflictResolution::Overwrite)
        );
    }
}

#[cfg(test)]
mod stop_branch_park_tests {
    //! What the local-FS Stop branch of `resolve_conflict` must do when it parks
    //! on a person: arm the slot before the prompt goes out, and say out loud
    //! that it has parked.
    //!
    //! The ordering pin: the oneshot sender must be armed in
    //! `state.conflict_slot` BEFORE the `write-conflict` event is
    //! emitted, so a responder that observes the event and answers it
    //! synchronously (the FE's `resolve_write_conflict`, modeled here by a sink
    //! that answers inside `emit_conflict`) finds the sender already present.

    use super::*;
    use crate::file_system::write_operations::state::{ConflictResolutionResponse, WriteOperationState};
    use crate::file_system::write_operations::types::{
        CollectorEventSink, ConflictInfo, ConflictResolution, DryRunResult, OperationEventSink, ScanProgressEvent,
        TransferWaitReason, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteErrorEvent,
        WriteOperationConfig, WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
    };
    use crate::ignore_poison::IgnorePoison;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;

    /// A sink that answers a Stop-mode `write-conflict` synchronously the moment
    /// it observes it, through the same conflict slot the FE's
    /// `resolve_write_conflict` answers through, but driven from inside
    /// `emit_conflict`. Forwards every event to an inner `CollectorEventSink`
    /// for inspection.
    ///
    /// This only resolves the conflict if the slot is ALREADY armed when the
    /// event arrives. If the production code emitted before arming, the answer
    /// would land on nothing, and `resolve_conflict`'s
    /// `rx.blocking_recv()` would deadlock — turning the ordering bug into a hang
    /// instead of a wrong value. The store-before-emit fix is what keeps this
    /// test from hanging.
    struct AnswerOnConflictSink {
        inner: CollectorEventSink,
        state: Arc<WriteOperationState>,
        resolution: ConflictResolution,
    }

    impl OperationEventSink for AnswerOnConflictSink {
        fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
            self.inner.emit_settled(e);
        }
        fn emit_conflict(&self, e: WriteConflictEvent) {
            // Answer the clash this event names, exactly as a surface would.
            let clash = e.conflict_id;
            self.inner.emit_conflict(e);
            let _ = self.state.conflict_slot.answer(
                clash,
                ConflictResolutionResponse {
                    resolution: self.resolution,
                    apply_to_all: false,
                },
            );
        }
        fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
            self.inner.emit_conflict_resolved(e);
        }
        fn emit_progress(&self, e: WriteProgressEvent) {
            self.inner.emit_progress(e);
        }
        fn emit_complete(&self, e: WriteCompleteEvent) {
            self.inner.emit_complete(e);
        }
        fn emit_cancelled(&self, e: WriteCancelledEvent) {
            self.inner.emit_cancelled(e);
        }
        fn emit_error(&self, e: WriteErrorEvent) {
            self.inner.emit_error(e);
        }
        fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _c: ConflictInfo) {}
        fn emit_dry_run_complete(&self, _r: DryRunResult) {}
    }

    /// THE PIN: with the sender stored before the emit, a responder answering
    /// synchronously inside `emit_conflict` resolves the local-FS Stop clash and
    /// `resolve_conflict` returns the scripted resolution. Run WITHOUT a Tokio
    /// runtime so the function's `rx.blocking_recv()` is legal; the answer is
    /// already buffered in the oneshot by the time `blocking_recv` runs, so it
    /// returns immediately. Against the pre-fix emit-then-store ordering the take
    /// inside `emit_conflict` would miss and this test would deadlock.
    #[test]
    fn stop_clash_answered_from_within_emit_resolves_without_hanging() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        fs::write(&src, b"SRC").unwrap();
        fs::write(&dst, b"DEST").unwrap();

        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        let events = AnswerOnConflictSink {
            inner: CollectorEventSink::new(),
            state: Arc::clone(&state),
            resolution: ConflictResolution::Skip,
        };
        let config = WriteOperationConfig::default(); // Stop, overwrite=false
        let mut latch = ApplyToAll::default();

        let result = resolve_conflict(&src, &dst, &config, &events, "op-local-stop-pin", &state, &mut latch);

        // Skip resolves to "skip this file" (None), proving the responder's
        // answer reached the op — which is only possible if the sender was
        // stored before the event was emitted.
        assert!(
            matches!(result, Ok(None)),
            "Skip answer should resolve the clash to None, got {result:?}"
        );
        // Exactly one conflict event was recorded, and it's a file-vs-file clash.
        let conflicts = events.inner.conflicts.lock_ignore_poison();
        assert_eq!(conflicts.len(), 1, "exactly one Stop prompt for the file clash");
        assert!(
            !conflicts[0].source_is_directory && !conflicts[0].destination_is_directory,
            "the clash is file-vs-file"
        );
    }

    /// A clash that gets answered is announced as over, naming itself.
    ///
    /// The prompt went out to every webview, and only the surface whose own call
    /// returned learns what became of it. Without this, the queue window's copy
    /// of the prompt — or the main window's, after an AGENT answered over MCP —
    /// keeps asking a question with no answer left to give, and blocks anything
    /// new from starting behind it.
    #[test]
    fn an_answered_clash_is_announced_as_over_by_id() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        fs::write(&src, b"SRC").unwrap();
        fs::write(&dst, b"DEST").unwrap();

        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        let events = AnswerOnConflictSink {
            inner: CollectorEventSink::new(),
            state: Arc::clone(&state),
            resolution: ConflictResolution::Skip,
        };
        let config = WriteOperationConfig::default();
        let mut latch = ApplyToAll::default();

        let _ = resolve_conflict(&src, &dst, &config, &events, "op-local-resolved", &state, &mut latch);

        let raised = events.inner.conflicts.lock_ignore_poison();
        let resolved = events.inner.conflicts_resolved.lock_ignore_poison();
        assert_eq!(resolved.len(), 1, "the answered clash is announced exactly once");
        assert_eq!(
            resolved[0].conflict_id, raised[0].conflict_id,
            "it names the clash that was answered, ❌ never 'whatever is on screen'"
        );
        assert_eq!(resolved[0].operation_id, "op-local-resolved");
    }

    /// A local copy keeps no in-flight table, so nothing speaks for it while it
    /// stands still: it emits no progress at all with a prompt up, and the tick
    /// it emitted just before the clash says it was moving at whatever it was
    /// doing then. Every window would keep that speed on screen for the whole
    /// answer. So the park announces itself, and so does its end.
    #[test]
    fn a_local_clash_announces_the_wait_and_its_end() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("f001");
        let dst = dir.path().join("dst-f001");
        fs::write(&src, b"SRC").unwrap();
        fs::write(&dst, b"DEST").unwrap();

        let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
        let events = AnswerOnConflictSink {
            inner: CollectorEventSink::new(),
            state: Arc::clone(&state),
            resolution: ConflictResolution::Skip,
        };

        // The copy was moving, and said so, right up to the clash.
        state.emit_progress_via_sink(
            &events,
            WriteProgressEvent::new(
                "op-local-park".to_owned(),
                WriteOperationType::Copy,
                WriteOperationPhase::Copying,
                Some("f000".to_owned()),
                3,
                40,
                300,
                4_000,
            ),
        );

        let config = WriteOperationConfig::default(); // Stop, overwrite=false
        let mut latch = ApplyToAll::default();
        let result = resolve_conflict(&src, &dst, &config, &events, "op-local-park", &state, &mut latch);
        assert!(matches!(result, Ok(None)), "the scripted Skip resolves the clash");

        let progress = events.inner.progress.lock_ignore_poison();
        assert_eq!(progress.len(), 3, "the copy's own tick, then one per edge of the wait");
        assert_eq!(progress[0].activity, None, "nothing was waiting on anybody yet");

        let parked = progress[1].activity.expect("the park has to reach the windows");
        assert_eq!(parked.waiting_on, TransferWaitReason::You);
        assert_eq!(
            (progress[1].files_done, progress[1].bytes_done),
            (3, 300),
            "the counters are the ones from before the clash: nothing moved, and saying otherwise would be a lie",
        );

        assert_eq!(
            progress[2].activity, None,
            "the answer is in, so the copy is nobody's to wait on any more",
        );
    }
}
