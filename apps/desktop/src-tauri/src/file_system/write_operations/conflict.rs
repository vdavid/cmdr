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
use super::event_sinks::OperationEventSink;
use super::overwrite::ResolvedDestination;
use super::state::WriteOperationState;
use super::types::{
    ConflictId, ConflictInfo, ConflictResolution, WriteConflictEvent, WriteConflictResolvedEvent, WriteOperationConfig,
    WriteOperationError,
};
use super::unique_name::find_unique_name;

// ============================================================================
// Apply-to-all state (per-shape latches)
// ============================================================================

/// Which shape a clash is, as far as the apply-to-all latches care.
///
/// The two cross-type directions get their own buckets because the dialog puts
/// a DIFFERENT question for each: replacing a folder with a file throws a tree
/// away, replacing a file with a folder throws the file away, and the prompt
/// names both kinds (`build_conflict_event`'s `source_is_directory` /
/// `destination_is_directory`). An answer to one of those questions is consent
/// to that act and to nothing else, so it may not leak into the other shapes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ClashKind {
    /// Both sides the same kind of entry: file↔file, folder↔folder, link↔link.
    SameKind,
    /// A leaf landing on a real directory.
    FileOverFolder,
    /// A directory landing on a leaf.
    FolderOverFile,
}

impl ClashKind {
    /// Classifies a clash from what is ARRIVING and whether the destination is
    /// a real directory.
    ///
    /// `destination_is_real_dir: None` is a destination that wouldn't stat, and
    /// reads as `SameKind` on purpose: an unanswerable type is never grounds
    /// for a destructive cross-type act, and the write below refuses an
    /// occupied name of its own accord.
    pub(super) fn of(incoming: IncomingItem, destination_is_real_dir: Option<bool>) -> Self {
        match (incoming, destination_is_real_dir) {
            (IncomingItem::Leaf, Some(true)) => Self::FileOverFolder,
            (IncomingItem::Directory, Some(false)) => Self::FolderOverFile,
            _ => Self::SameKind,
        }
    }

    /// Whether enacting a resolution here would replace one kind of entry with
    /// another.
    pub(super) const fn is_cross_type(self) -> bool {
        !matches!(self, Self::SameKind)
    }
}

/// Per-operation "apply to all" latch state for conflict resolution.
///
/// One bucket per [`ClashKind`], so a choice only ever carries to clashes that
/// put the same question. See `apply_to_all_tests` for the full rule set; the
/// short version:
///
/// - A choice latched on a clash applies to subsequent clashes of that same
///   shape.
/// - From the same-kind bucket, only Skip / Rename reach a cross-type clash.
///   An Overwrite there answered a question about two files.
/// - A cross-type choice that was the **first** clash of the whole operation
///   spreads to the same-kind bucket too (the person had seen nothing else, so
///   they were answering for the operation). It never spreads to the OTHER
///   cross-type bucket: that is a different destructive act.
// DEFAULT-OK: nothing latched and no clash seen yet is precisely the state before the
// operation's first conflict.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ApplyToAll {
    same_kind: Option<ConflictResolution>,
    file_over_folder: Option<ConflictResolution>,
    folder_over_file: Option<ConflictResolution>,
    /// `false` until the first clash (any kind) has been resolved. Used to
    /// decide whether a "* all" choice in a cross-type dialog should spread to
    /// the same-kind bucket — only if that clash was the very first one the
    /// user saw.
    has_seen_clash: bool,
}

/// A resolution the latches answered with, plus whether a person answered it
/// on a prompt for THIS clash shape.
#[derive(Clone, Copy, Debug)]
pub(super) struct LatchedResolution {
    resolution: ConflictResolution,
    /// `true` only when the value came from the bucket for THIS cross-type
    /// shape. Nothing but an answered prompt naming both kinds fills one, which
    /// is what can make the carry consent where a blanket policy isn't.
    answered_for_this_shape: bool,
}

impl LatchedResolution {
    /// The answer a person just gave on a Stop prompt for this very clash. Same
    /// standing as a carry latched on this shape: they saw both kinds named.
    const fn answered_now(resolution: ConflictResolution) -> Self {
        Self {
            resolution,
            answered_for_this_shape: true,
        }
    }

    /// This answer, held to what it may enact at `kind`. For the sites that
    /// already have a latch and so never need the configured fallback.
    pub(super) fn at(self, kind: ClashKind, destination: &dyn std::fmt::Display) -> ConflictResolution {
        resolution_for_clash(Some(self), self.resolution, kind, destination)
    }
}

/// Returns the latched resolution that applies to the next clash of `kind`, or
/// `None` if nothing is latched for it yet.
pub(super) fn apply_to_all_effective(state: &ApplyToAll, kind: ClashKind) -> Option<LatchedResolution> {
    let blanket = |resolution| LatchedResolution {
        resolution,
        answered_for_this_shape: false,
    };
    // Only the non-destructive same-kind carries reach across types.
    let same_kind_carry = || match state.same_kind {
        Some(r @ (ConflictResolution::Skip | ConflictResolution::Rename)) => Some(blanket(r)),
        _ => None,
    };
    match kind {
        ClashKind::SameKind => state.same_kind.map(blanket),
        ClashKind::FileOverFolder => state
            .file_over_folder
            .map(LatchedResolution::answered_now)
            .or_else(same_kind_carry),
        ClashKind::FolderOverFile => state
            .folder_over_file
            .map(LatchedResolution::answered_now)
            .or_else(same_kind_carry),
    }
}

/// Records a user response. `apply_to_all == false` doesn't latch but still
/// flips `has_seen_clash`, so a later cross-type "* all" choice won't be
/// considered "first" and won't spread to the same-kind bucket.
pub(super) fn apply_to_all_record(
    state: &mut ApplyToAll,
    kind: ClashKind,
    resolution: ConflictResolution,
    apply_to_all: bool,
) {
    let was_first_clash = !state.has_seen_clash;
    state.has_seen_clash = true;
    if !apply_to_all {
        return;
    }
    let bucket = match kind {
        ClashKind::SameKind => &mut state.same_kind,
        ClashKind::FileOverFolder => &mut state.file_over_folder,
        ClashKind::FolderOverFile => &mut state.folder_over_file,
    };
    *bucket = Some(resolution);
    // A cross-type answer that was the operation's first clash spreads to the
    // same-kind bucket: nothing else had been shown, so it was an answer for
    // the operation. The other cross-type bucket is left alone.
    if kind.is_cross_type() && was_first_clash {
        state.same_kind = Some(resolution);
    }
}

// ============================================================================
// Cross-type clashes
// ============================================================================

/// What is ARRIVING at a destination, told by the caller that knows.
///
/// A stat of the source path answers this everywhere but one place: the
/// folder→file branch in `transfer/copy/single_item.rs` resolves the clash
/// against the BLOCKING FILE as both source and destination (so the prompt
/// describes the entry that's in the way), while what's really arriving is a
/// directory. Asking the caller keeps that site honest instead of quietly
/// classifying a folder landing as file-on-file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum IncomingItem {
    /// A file, or a symlink — a leaf whatever a link points at.
    Leaf,
    /// A directory, with its subtree behind it.
    Directory,
}

impl IncomingItem {
    /// What the entry at a local source path is, read with `symlink_metadata`
    /// so a link stays a leaf (`validation::is_real_directory` holds the why).
    pub(super) fn of_local_source(source: &Path) -> Self {
        if super::validation::is_real_directory(source) {
            Self::Directory
        } else {
            Self::Leaf
        }
    }

    /// The same answer from a caller that already resolved the question, like
    /// the cross-volume engine's `resolve_source_is_directory`.
    pub(super) const fn of_directory_flag(is_directory: bool) -> Self {
        if is_directory { Self::Directory } else { Self::Leaf }
    }
}

/// The resolution that decides this clash: the latch's answer if there is one,
/// otherwise the configured policy, reduced to `Skip` when it would replace one
/// kind of entry with another without anyone having consented to that.
///
/// `Overwrite`, `OverwriteSmaller`, and `OverwriteOlder` are all answers about
/// two FILES — "the one at the destination is stale, put the source there". The
/// act behind them across types is a different thing: replacing a folder with a
/// file throws a whole tree away, and replacing a file with a folder throws the
/// file away, and nobody picking a policy in the transfer dialog was shown
/// either. The conditional variants can't even ask their own question there — a
/// directory's `len()` is its inode's own size, so every ordinary file looks
/// "bigger" and "Overwrite all smaller" deleted destination folders wholesale.
///
/// **Consent is what separates the two, and it is per SHAPE, not per item.** A
/// person answering a cross-type prompt saw both kinds named in it, so a plain
/// `Overwrite` from them replaces — including the "* all" carry it latched,
/// which the dialog labels for exactly that act ("Overwrite folders with
/// files"). Refusing the carry would make the checkbox a trap: the first item
/// replaces and every one after it is skipped, with nothing on screen saying
/// so. What has no consent is a policy nobody looked at per item: the
/// configured `conflict_resolution`, and a same-kind carry reaching across
/// types.
///
/// ❗ **The conditional variants are refused across types no matter who asked.**
/// The dialog offers "Overwrite all smaller" / "Overwrite all older" on a
/// cross-type prompt too, but neither label names the cross-type act, and
/// neither can ask its own question here: a directory's `len()` is its inode's
/// own size, so every ordinary file looks "bigger" and `OverwriteSmaller` would
/// clear destination folders wholesale. Only a plain `Overwrite` is consent.
///
/// All three engines (local, cross-volume, in-archive) call this, so the rule
/// is one rule.
pub(super) fn resolution_for_clash(
    latched: Option<LatchedResolution>,
    configured: ConflictResolution,
    kind: ClashKind,
    destination: &dyn std::fmt::Display,
) -> ConflictResolution {
    let answered_here = latched.is_some_and(|l| l.answered_for_this_shape);
    let resolution = latched.map_or(configured, |l| l.resolution);
    if !kind.is_cross_type() {
        return resolution;
    }
    match resolution {
        // The one button that names the act, clicked on a prompt for this shape.
        ConflictResolution::Overwrite if answered_here => ConflictResolution::Overwrite,
        ConflictResolution::Overwrite | ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            log::info!(
                target: "conflict_resolution",
                "{resolution:?}: skipping {destination}, because a folder and a file can only replace each other on an explicit Overwrite answered for that clash"
            );
            ConflictResolution::Skip
        }
        other => other,
    }
}

/// [`resolution_for_clash`] for the answer a person has just given on a Stop
/// prompt for this very clash.
///
/// The Stop arm can't skip this step: the dialog's "Overwrite all smaller" /
/// "Overwrite all older" buttons reach it directly, and the conditional
/// reduction behind them compares an incoming file against a DIRECTORY INODE's
/// size. Every ordinary file wins that comparison, so the first cross-type item
/// a person answered that way lost its destination folder.
pub(super) fn answered_resolution_for_clash(
    resolution: ConflictResolution,
    kind: ClashKind,
    destination: &dyn std::fmt::Display,
) -> ConflictResolution {
    LatchedResolution::answered_now(resolution).at(kind, destination)
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
    incoming: IncomingItem,
    config: &WriteOperationConfig,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut ApplyToAll,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    // Pre-fetch metadata once; reused for the conflict event and the
    // conditional-variant reduction.
    let source_meta = fs::metadata(source).ok();
    let dest_meta = fs::metadata(dest_path).ok();
    // Whether the destination is a folder is asked of the ENTRY, not of what it
    // points at: a symlink is a leaf whatever its target is, so a link facing a
    // real directory is a cross-type clash. `dest_meta` follows links and would
    // call that pair folder-on-folder, which is the one answer that walks into
    // the link. `validation::is_real_directory` holds the why.
    let destination_is_real_dir = fs::symlink_metadata(dest_path).map(|m| m.is_dir()).ok();
    let kind = ClashKind::of(incoming, destination_is_real_dir);

    // Determine effective conflict resolution. A policy nobody looked at per
    // item — the config's, or a same-kind "* all" reaching across types — never
    // replaces a folder with a file or a file with a folder; a carry latched on
    // this very shape does, because a person answered for it.
    let latched = apply_to_all_effective(apply_to_all_resolution, kind);
    let resolution = resolution_for_clash(latched, config.conflict_resolution, kind, &dest_path.display());

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
                        kind,
                        response.resolution,
                        response.apply_to_all,
                    );
                    // Hold the answer to what it can consent to at this shape,
                    // then reduce conditional variants to Overwrite / Skip
                    // against this file's already-fetched metadata, and apply.
                    let answered = answered_resolution_for_clash(response.resolution, kind, &dest_path.display());
                    let effective = reduce_conditional_resolution(answered, source_meta.as_ref(), dest_meta.as_ref());
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
/// It compares two files and nothing else. A clash whose sides are different
/// KINDS never gets here under a blanket policy —
/// [`resolution_for_clash`] has already turned it into a `Skip` —
/// which is why a folder's `len()` (its own inode's size, not its contents')
/// can't decide anything.
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
