//! Recursive rename-merge for same-volume moves.
//!
//! A same-volume move is a server-side `rename` — instant, transfers no bytes.
//! When a top-level source folder lands on an existing same-named dest folder,
//! a flat `rename(source, dest, force=false)` fails with `AlreadyExists`, so the
//! move walks the source folder level by level and renames each child into the
//! merged destination instead. Folders always merge (no prompt, no policy for
//! the folder itself); only file / cross-type clashes consult the file policy
//! through the shared `resolve_volume_conflict` resolver.
//!
//! ## Why renames, not byte streams
//!
//! Each child rides along on a single `rename` — a whole subtree moves with one
//! server-side call, never descended unless its own name clashes with a
//! same-named dest directory (a dir-vs-dir merge). This is what keeps a
//! same-volume move feeling instant: a non-conflicting move of a 10k-file folder
//! is one rename, and even a conflicting merge only descends levels that actually
//! collide.
//!
//! ## Source-dir cleanup, inside-out
//!
//! After a level's children are processed, we attempt `volume.delete(source_dir)`.
//! `Volume::delete`'s contract is "file or EMPTY directory only", so a non-empty
//! source (a skipped child, an errored child, content left behind by a clash)
//! fails benignly and the directory — and all its ancestors — survive. An
//! all-moved level empties and is deleted; the natural recursion unwind then
//! deletes the spine deepest-first. We NEVER delete a source directory while any
//! content remains.
//!
//! ## Case-insensitive backends and TOCTOU
//!
//! The dest name map is exact-match, but SMB servers and APFS are typically
//! case-insensitive: `Foo.txt` vs `foo.txt` collides at the backend with no map
//! hit. An unexpected `AlreadyExists` from a child rename is therefore treated as
//! a late-detected conflict and routed through the resolver — never a hard error.
//! Per-level decisions already made via the map are tracked in a
//! `name → MergeChildResolution` map so a late collision on an already-resolved
//! child finalizes its stored decision instead of re-prompting.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use super::super::conflict::ApplyToAll;
use super::super::state::{WriteOperationState, is_cancelled};
use super::super::types::{OperationEventSink, VolumeCopyConfig, WriteOperationError};
use super::volume_conflict::{ResolvedConflict, resolve_volume_conflict};
use super::volume_copy::map_volume_error;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// Context threaded through the recursive rename-merge so each level can resolve
/// its clashing children through the same file-policy machinery the top-level
/// move uses (Stop-wait, the op-wide apply-to-all latch, conditional reduce,
/// type mismatches).
pub(super) struct RenameMergeCtx<'a> {
    pub volume: &'a Arc<dyn Volume>,
    pub events: &'a dyn OperationEventSink,
    pub operation_id: &'a str,
    pub config: &'a VolumeCopyConfig,
    /// Carries the cancel `intent`, the `conflict_resolution_tx` oneshot slot,
    /// and the `conflict_dispatch_lock` the resolver uses to serialize the human.
    pub state: &'a Arc<WriteOperationState>,
    /// Op-wide apply-to-all latch, shared between the top-level dispatch and
    /// every deep merge level so a "…all" choice applies everywhere.
    pub apply_to_all: &'a Mutex<ApplyToAll>,
}

/// The decision recorded for a child whose name hit the dest map, so a
/// late-detected (case-folded / TOCTOU) `AlreadyExists` on the SAME child can
/// finalize that decision instead of re-prompting.
#[derive(Clone)]
enum MergeChildResolution {
    /// The resolver said Skip — leave both sides untouched.
    Skip,
    /// The resolver said Proceed. `replace` is `Some(orig)` for a file→file
    /// safe-replace (delete the original, then rename onto it).
    Proceed {
        write_path: PathBuf,
        replace: Option<PathBuf>,
    },
}

/// Recursively merges `source_dir` into the existing `dest_dir` on the same
/// volume using server-side renames. `dest_dir` is assumed to already exist as a
/// directory (the caller entered this path BECAUSE of a dir-vs-dir collision).
///
/// On a successfully-emptied source level the source directory is deleted; a
/// level holding any skipped / errored / unmoved child survives, and so do its
/// ancestors (the parent's own `delete` fails benignly while the child remains).
///
/// `note_both_halves` is invoked with `(source, dest)` for every child rename so
/// the downloads watcher's ignore set suppresses both halves (a same-volume move
/// into `~/Downloads` must not toast "Downloaded …" per deep child).
pub(super) async fn rename_merge_directory(
    ctx: &RenameMergeCtx<'_>,
    source_dir: &Path,
    dest_dir: &Path,
    note_both_halves: &(dyn Fn(&Path, &Path) + Sync),
) -> Result<(), WriteOperationError> {
    if is_cancelled(&ctx.state.intent) {
        return Err(WriteOperationError::Cancelled {
            message: "Operation cancelled by user".to_string(),
        });
    }

    // List both levels once. The dest level pre-exists (we're merging into it),
    // so build a `name → entry` map for exact-match collision detection.
    let dest_by_name: HashMap<String, FileEntry> = volume_list(ctx.volume, dest_dir)
        .await?
        .into_iter()
        .map(|e| (e.name.clone(), e))
        .collect();
    let source_entries = volume_list(ctx.volume, source_dir).await?;

    // Per-level decisions for children whose name hit the dest map. A
    // late-detected `AlreadyExists` (case-fold / TOCTOU) for one of these
    // finalizes the stored decision rather than re-prompting. Orthogonal to the
    // op-wide apply-to-all latch: the latch answers "how to handle future
    // clashes," this map answers "which children of THIS level we already
    // handled."
    let mut resolved_children: HashMap<String, MergeChildResolution> = HashMap::new();

    for entry in &source_entries {
        if is_cancelled(&ctx.state.intent) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        let child_source = PathBuf::from(&entry.path);
        let child_dest = dest_dir.join(&entry.name);
        let dest_hit = dest_by_name.get(&entry.name);

        if entry.is_directory && dest_hit.is_some_and(|d| d.is_directory) {
            // Dir-vs-dir: always merge, never prompt. Recurse.
            Box::pin(rename_merge_directory(
                ctx,
                &child_source,
                &child_dest,
                note_both_halves,
            ))
            .await?;
            continue;
        }

        if let Some(_hit) = dest_hit {
            // File or cross-type clash: consult the file policy.
            let decision = resolve_child(ctx, &child_source, entry, &child_dest).await?;
            resolved_children.insert(entry.name.clone(), decision.clone());
            apply_child_decision(ctx, entry, &child_source, decision, note_both_halves).await?;
            continue;
        }

        // No exact-match hit: try a plain rename. A whole subtree (or symlink, or
        // file) rides along on this one server-side call — never descended.
        note_both_halves(&child_source, &child_dest);
        match ctx.volume.rename(&child_source, &child_dest, false).await {
            Ok(()) => {}
            Err(VolumeError::AlreadyExists(_)) => {
                // The exact-match map missed it, but the backend rejects the
                // name — a case-folded (SMB/APFS) or TOCTOU collision. Treat it
                // as a late-detected conflict; the backend is the authority.
                late_detected_collision(
                    ctx,
                    &child_source,
                    entry,
                    &child_dest,
                    &mut resolved_children,
                    note_both_halves,
                )
                .await?;
            }
            Err(e) => return Err(map_rename_error(&child_source, e)),
        }
    }

    // Source-dir cleanup, inside-out: empty-only delete. A level still holding a
    // skipped / errored / unmoved child fails benignly and survives, and so do
    // its ancestors. Never deletes a source dir while content remains.
    match ctx.volume.delete(source_dir).await {
        Ok(()) | Err(VolumeError::NotFound(_)) => {}
        Err(_non_empty_or_other) => {
            // Non-empty (children left behind) or a transient backend error:
            // leave the directory in place. This is the intended outcome for any
            // skip/error path — the source folder survives with its unmoved
            // content.
        }
    }

    Ok(())
}

/// Handles an `AlreadyExists` that the exact-match dest map didn't predict
/// (case-insensitive backend or a TOCTOU appearance). Branches on whether THIS
/// child was already resolved at this level:
/// - already resolved (Overwrite/Proceed) → finalize the stored decision.
/// - already resolved Skip → nothing to do.
/// - not yet resolved → a fresh case-folded collision: re-list the dest level to
///   find the case-folded match, recurse for dir-vs-dir, else resolve as a file
///   / cross-type clash.
async fn late_detected_collision(
    ctx: &RenameMergeCtx<'_>,
    child_source: &Path,
    entry: &FileEntry,
    child_dest: &Path,
    resolved_children: &mut HashMap<String, MergeChildResolution>,
    note_both_halves: &(dyn Fn(&Path, &Path) + Sync),
) -> Result<(), WriteOperationError> {
    if let Some(stored) = resolved_children.get(&entry.name).cloned() {
        // We already prompted/decided for this child; its rename collided on the
        // case-folded name. Finalize the stored decision — NEVER re-prompt.
        return apply_child_decision(ctx, entry, child_source, stored, note_both_halves).await;
    }

    // A genuinely new collision the exact-match map missed. Re-list the dest to
    // locate the case-folded sibling and learn its type.
    let dest_parent = child_dest.parent().unwrap_or(Path::new(""));
    let target_name = entry.name.to_lowercase();
    let dest_entry = volume_list(ctx.volume, dest_parent)
        .await?
        .into_iter()
        .find(|e| e.name.to_lowercase() == target_name);

    if entry.is_directory && dest_entry.as_ref().is_some_and(|d| d.is_directory) {
        // Case-folded dir-vs-dir: enter the merge recursion like any other
        // dir-dir, targeting the dest's actual (case-folded) path so renames land
        // in the directory the backend already has.
        let actual_dest = dest_entry
            .as_ref()
            .map(|d| dest_parent.join(&d.name))
            .unwrap_or_else(|| child_dest.to_path_buf());
        return Box::pin(rename_merge_directory(
            ctx,
            child_source,
            &actual_dest,
            note_both_halves,
        ))
        .await;
    }

    // File or cross-type case-folded clash: resolve against the dest's actual
    // path so an Overwrite/Rename acts on the entry the backend really holds.
    let actual_dest = dest_entry
        .as_ref()
        .map(|d| dest_parent.join(&d.name))
        .unwrap_or_else(|| child_dest.to_path_buf());
    let decision = resolve_child(ctx, child_source, entry, &actual_dest).await?;
    resolved_children.insert(entry.name.clone(), decision.clone());
    apply_child_decision(ctx, entry, child_source, decision, note_both_halves).await
}

/// Runs the shared conflict resolver for one clashing child and maps its outcome
/// onto a `MergeChildResolution`. Dir-vs-dir never reaches here — the caller
/// recurses for that.
async fn resolve_child(
    ctx: &RenameMergeCtx<'_>,
    child_source: &Path,
    entry: &FileEntry,
    child_dest: &Path,
) -> Result<MergeChildResolution, WriteOperationError> {
    // The source listing entry already tells us the type and size, saving the
    // resolver a redundant `is_directory` probe. Deep children aren't top-level
    // sources, so there's no preflight hint to reuse.
    let source_is_directory_hint = Some(entry.is_directory);
    let source_size_hint = if entry.is_directory { None } else { entry.size };

    let mut latched = *ctx.apply_to_all.lock_ignore_poison();
    let resolved = resolve_volume_conflict(
        ctx.volume,
        child_source,
        ctx.volume,
        child_dest,
        ctx.config,
        ctx.events,
        ctx.operation_id,
        ctx.state,
        &mut latched,
        source_size_hint,
        None, // dest size hint: the resolver stats only on the Stop path
        source_is_directory_hint,
    )
    .await;
    *ctx.apply_to_all.lock_ignore_poison() = latched;

    match resolved? {
        None => Ok(MergeChildResolution::Skip),
        Some(ResolvedConflict {
            write_path,
            replace_after_write,
        }) => Ok(MergeChildResolution::Proceed {
            write_path,
            replace: replace_after_write,
        }),
    }
}

/// Applies a resolved child decision via renames (the same-volume move never
/// streams bytes): Skip leaves both sides; Proceed renames the source onto the
/// resolved path. A file→file safe-replace collapses to delete-then-rename —
/// `rename(force=false)` can't replace an existing dest, and MTP's
/// `rename(force=true)` wouldn't delete it either, so we delete the original
/// first and rename straight onto the now-absent slot, mirroring the top-level
/// same-volume overwrite shape.
async fn apply_child_decision(
    ctx: &RenameMergeCtx<'_>,
    entry: &FileEntry,
    child_source: &Path,
    decision: MergeChildResolution,
    note_both_halves: &(dyn Fn(&Path, &Path) + Sync),
) -> Result<(), WriteOperationError> {
    let (write_path, replace) = match decision {
        MergeChildResolution::Skip => return Ok(()),
        MergeChildResolution::Proceed { write_path, replace } => (write_path, replace),
    };

    if entry.is_directory {
        // A directory child that resolved to Proceed is a cross-type
        // Overwrite/Rename (dir source vs a same-named dest FILE). The resolver
        // already cleared/relocated the dest entry. If `write_path` is now a
        // same-named dest directory (shouldn't happen — dir-vs-dir recurses
        // earlier), merge into it; otherwise rename the subtree across, clearing
        // a reserved placeholder FILE first (a directory can't `rename` over a
        // file).
        if ctx.volume.is_directory(&write_path).await.unwrap_or(false) {
            return Box::pin(rename_merge_directory(ctx, child_source, &write_path, note_both_halves)).await;
        }
        if ctx.volume.exists(&write_path).await {
            match ctx.volume.delete(&write_path).await {
                Ok(()) | Err(VolumeError::NotFound(_)) => {}
                Err(e) => return Err(map_rename_error(&write_path, e)),
            }
        }
        note_both_halves(child_source, &write_path);
        return match ctx.volume.rename(child_source, &write_path, false).await {
            Ok(()) => Ok(()),
            Err(e) => Err(map_rename_error(child_source, e)),
        };
    }

    // File child. For a file→file safe-replace (Overwrite), delete the original
    // first then rename onto it (atomic-ish; the rename is the commit). For
    // Rename / a fresh path, rename straight across — clearing a reserved
    // placeholder first.
    let target = match replace {
        // Overwrite: `orig` is the existing dest. Delete it, then rename the
        // source straight onto the now-absent slot. `rename(force=false)` can't
        // replace, and MTP's `rename(force=true)` wouldn't delete it either, so
        // an explicit delete-then-rename is the only shape correct across all
        // backends — the same legacy delete-first shape the top-level
        // same-volume overwrite uses.
        Some(orig) => orig,
        // Rename / fresh: `write_path` is the resolved (unique) name. On a
        // local-FS dest the resolver RESERVED it with an O_CREAT|O_EXCL
        // placeholder (the TOCTOU guard the streaming writer would truncate),
        // so `rename(force=false)` would collide with our own placeholder.
        // Clear it first; the name stays reserved logically and the rename lands
        // the source there. On non-local dests no placeholder exists and the
        // delete is a benign `NotFound`.
        None => write_path,
    };
    if ctx.volume.exists(&target).await {
        match ctx.volume.delete(&target).await {
            Ok(()) | Err(VolumeError::NotFound(_)) => {}
            Err(e) => return Err(map_rename_error(&target, e)),
        }
    }
    note_both_halves(child_source, &target);
    match ctx.volume.rename(child_source, &target, false).await {
        Ok(()) => Ok(()),
        Err(e) => Err(map_rename_error(child_source, e)),
    }
}

/// Lists a directory on the volume, mapping the error through the shared
/// `map_volume_error` (which preserves the typed `Cancelled` variant so the
/// post-loop reclassifies cancellation rather than reporting a transport error).
async fn volume_list(volume: &Arc<dyn Volume>, path: &Path) -> Result<Vec<FileEntry>, WriteOperationError> {
    volume
        .list_directory(path, None)
        .await
        .map_err(|e| map_rename_error(path, e))
}

/// Maps a rename / list `VolumeError` into a `WriteOperationError` via the shared
/// `map_volume_error`, which keeps `VolumeError::Cancelled` as the typed
/// `WriteOperationError::Cancelled`.
fn map_rename_error(path: &Path, e: VolumeError) -> WriteOperationError {
    map_volume_error(&path.display().to_string(), e)
}
