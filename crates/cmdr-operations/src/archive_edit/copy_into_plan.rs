//! Planning a copy/move INTO a zip: walk the local sources, resolve each
//! collision with an archive entry through [`super::conflicts`] (a pre-resolved
//! policy, or the interactive Stop prompt), and build the one
//! `{ add + mkdir + delete }` changeset the mutator applies. Blocking, run inside
//! the managed op against the working copy `copy_into.rs` hands it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use walkdir::WalkDir;

use super::super::OperationEventSink;
use super::super::conflict::{ApplyToAll, IncomingItem};
use super::super::state::WriteOperationState;
use super::super::types::{ConflictResolution, WriteOperationError};
use super::conflicts::{ArchiveClash, ConflictMode, conditional_overwrites, find_unique_inner, resolve_effective};
use super::edit_error::EditError;
use cmdr_archive::mutator::{self, AddEntry, AddSource, Changeset};
use cmdr_archive::{ArchiveFormat, ArchiveIndex, LocalFileSource};

/// The planned changeset for a copy/move-into, plus how many source entries were
/// skipped (a conflict resolved to Skip, or a symlink / special file a zip can't
/// hold). A non-zero count suppresses the source deletion on a move and surfaces
/// as `files_skipped` on the terminal event.
pub(super) struct CopyIntoPlan {
    pub(super) changeset: Changeset,
    pub(super) skipped_count: usize,
}

/// Walks the local sources and builds the `{ add + delete + mkdir }` changeset
/// under a pre-resolved policy (no prompting). Blocking — call from `spawn_blocking`.
pub(super) fn build_copy_into_changeset(
    archive_path: &Path,
    absolute_sources: &[PathBuf],
    dest_inner: &str,
    destination_name: Option<&str>,
    conflict: ConflictResolution,
) -> Result<CopyIntoPlan, WriteOperationError> {
    let mut mode = ConflictMode::Policy(conflict);
    build_copy_into_changeset_inner(archive_path, absolute_sources, dest_inner, destination_name, &mut mode).map_err(
        |e| match e {
            EditError::Op(w) => w,
            // A pre-resolved policy never prompts, so it can't be cancelled here.
            EditError::Cancelled => WriteOperationError::Cancelled {
                message: "the archive copy was cancelled".into(),
            },
        },
    )
}

/// Walks the local sources and builds the changeset with INTERACTIVE per-file
/// conflict prompts (the Stop UX). Blocking (parses the index, walks the tree, and
/// blocks on each prompt's oneshot) — call from `spawn_blocking` inside the managed
/// op so `resolve_write_conflict(operation_id)` can answer.
pub(super) fn build_copy_into_changeset_interactive(
    archive_path: &Path,
    absolute_sources: &[PathBuf],
    dest_inner: &str,
    destination_name: Option<&str>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
) -> Result<CopyIntoPlan, EditError> {
    let mut latch = ApplyToAll::default();
    let mut mode = ConflictMode::Interactive {
        events,
        operation_id,
        state,
        apply_to_all: &mut latch,
    };
    build_copy_into_changeset_inner(archive_path, absolute_sources, dest_inner, destination_name, &mut mode)
}

/// The shared copy-into walk. Resolves each FILE collision, and each folder
/// meeting an archive FILE, via `mode`; folder-over-folder merges silently (an
/// existing archive dir is never re-added and never prompts, the app-wide rule).
fn build_copy_into_changeset_inner(
    archive_path: &Path,
    absolute_sources: &[PathBuf],
    dest_inner: &str,
    destination_name: Option<&str>,
    mode: &mut ConflictMode<'_>,
) -> Result<CopyIntoPlan, EditError> {
    let source = LocalFileSource::open(archive_path).map_err(|e| {
        EditError::Op(WriteOperationError::WriteError {
            path: archive_path.display().to_string(),
            message: cmdr_fs::error_detail::ErrorDetail::from_io(&e),
        })
    })?;
    let index = ArchiveIndex::parse(Arc::new(source), ArchiveFormat::Zip, None).map_err(|e| {
        EditError::Op(WriteOperationError::WriteError {
            path: archive_path.display().to_string(),
            message: cmdr_fs::error_detail::ErrorDetail::of(e),
        })
    })?;

    let mut adds: Vec<AddEntry> = Vec::new();
    let mut mkdirs: Vec<String> = Vec::new();
    // The local folder each mkdir comes from, whose xattrs ride its sidecar.
    let mut mkdir_sources: HashMap<String, PathBuf> = HashMap::new();
    let mut deletes: Vec<String> = Vec::new();
    // Inner paths this changeset already claims (dedup + Rename uniqueness).
    let mut planned: HashSet<String> = HashSet::new();
    let mut skipped_count: usize = 0;

    for src in absolute_sources {
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let base_inner = join_inner_str(dest_inner, destination_name.unwrap_or(name));
        // The root `__MACOSX` is where the zip's sidecars live, hidden from every
        // listing: an item landing there could never be seen again, and an
        // extract would read it as metadata. Skipped and counted, like a symlink.
        if mutator::is_in_sidecar_folder(&base_inner) {
            skipped_count += 1;
            continue;
        }
        let meta = std::fs::symlink_metadata(src).map_err(|e| {
            EditError::Op(WriteOperationError::ReadError {
                path: src.display().to_string(),
                message: cmdr_fs::error_detail::ErrorDetail::from_io(&e),
            })
        })?;

        if meta.is_dir() {
            // Where each walked folder landed, so its children follow a folder
            // that Rename moved to a free name.
            let mut landed_dirs: HashMap<PathBuf, String> = HashMap::new();
            let mut walk = WalkDir::new(src).into_iter();
            while let Some(entry) = walk.next() {
                let Ok(entry) = entry else {
                    // An unreadable folder or entry never reaches the zip, so
                    // it counts as skipped: a move then keeps its source.
                    skipped_count += 1;
                    continue;
                };
                let inner = if entry.depth() == 0 {
                    base_inner.clone()
                } else {
                    // A skipped folder's children are never walked, so every
                    // parent has landed somewhere.
                    let Some(parent_inner) = entry.path().parent().and_then(|parent| landed_dirs.get(parent)) else {
                        skipped_count += 1;
                        continue;
                    };
                    join_inner_str(parent_inner, &entry.file_name().to_string_lossy())
                };
                let file_type = entry.file_type();
                if file_type.is_dir() {
                    let landed = plan_dir(
                        inner,
                        entry.path(),
                        archive_path,
                        &index,
                        mode,
                        &mut mkdirs,
                        &mut mkdir_sources,
                        &mut deletes,
                        &mut planned,
                    )?;
                    match landed {
                        Some(landed) => {
                            landed_dirs.insert(entry.path().to_path_buf(), landed);
                        }
                        None => {
                            skipped_count += 1;
                            walk.skip_current_dir();
                        }
                    }
                } else if file_type.is_file() {
                    plan_file_add(
                        inner,
                        entry.path(),
                        archive_path,
                        &index,
                        mode,
                        &mut adds,
                        &mut deletes,
                        &mut planned,
                        &mut skipped_count,
                    )?;
                } else {
                    // A symlink or special file (fifo/socket/device) inside the
                    // tree: a zip can't represent it, so it can't be added. Count
                    // it as skipped — that suppresses the source deletion on a
                    // MOVE (data safety: the whole subtree stays put rather than
                    // being deleted with the symlink unarchived) and surfaces the
                    // skip to the user.
                    skipped_count += 1;
                }
            }
        } else if meta.is_file() {
            plan_file_add(
                base_inner,
                src,
                archive_path,
                &index,
                mode,
                &mut adds,
                &mut deletes,
                &mut planned,
                &mut skipped_count,
            )?;
        } else {
            // A top-level symlink or special file, same reasoning as above: it
            // can't be archived, so it's skipped (never silently deleted on a
            // move by falling through with an empty changeset).
            skipped_count += 1;
        }
    }

    Ok(CopyIntoPlan {
        changeset: Changeset {
            adds,
            mkdirs,
            mkdir_sources,
            deletes,
            // An overwrite replaces what's at the name, hidden entries included.
            deletes_spare_hidden: false,
            renames: Vec::new(),
            // The per-edit level is set on this changeset later, in
            // `archive_copy_into_start`, from the operation config.
            compression_level: None,
        },
        skipped_count,
    })
}

/// Resolves one file's conflict (via `mode`) against the index + already-planned
/// paths and appends the resulting add (and a delete, for an overwrite). Mutates
/// the accumulators in place.
#[allow(
    clippy::too_many_arguments,
    reason = "shared accumulators for one pass; bundling them into a struct adds ceremony without clarity"
)]
fn plan_file_add(
    inner: String,
    src_path: &Path,
    archive_path: &Path,
    index: &ArchiveIndex,
    mode: &mut ConflictMode<'_>,
    adds: &mut Vec<AddEntry>,
    deletes: &mut Vec<String>,
    planned: &mut HashSet<String>,
    skipped_count: &mut usize,
) -> Result<(), EditError> {
    let in_index = index.exists(&inner);
    let collides = in_index || planned.contains(&inner);

    let target = if collides {
        // The clashing side is a file (this is the file-add path); classify the
        // destructive file→folder variant only when an existing ARCHIVE entry at
        // this name is a directory (a planned add is always a file).
        let clash = ArchiveClash {
            inner: &inner,
            src_path,
            incoming: IncomingItem::Leaf,
            destination_is_directory: index.is_directory(&inner) == Some(true),
        };
        let resolution = resolve_effective(mode, clash, archive_path, index)?;
        match resolution {
            ConflictResolution::Skip => {
                *skipped_count += 1;
                return Ok(());
            }
            ConflictResolution::Stop => {
                // Only reachable under a pre-resolved `Policy(Stop)` (the
                // interactive path never returns Stop). Treat as a hard collision.
                return Err(EditError::Op(WriteOperationError::DestinationExists { path: inner }));
            }
            ConflictResolution::Overwrite => {
                if in_index {
                    deletes.push(inner.clone());
                }
                inner
            }
            ConflictResolution::Rename => find_unique_inner(&inner, index, planned),
            ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
                if in_index && conditional_overwrites(resolution, index, &inner, src_path) {
                    deletes.push(inner.clone());
                    inner
                } else {
                    *skipped_count += 1;
                    return Ok(());
                }
            }
        }
    } else {
        inner
    };

    planned.insert(target.clone());
    adds.push(AddEntry {
        inner_path: target,
        source: AddSource::LocalPath(src_path.to_path_buf()),
    });
    Ok(())
}

/// Plans one walked source folder and answers the inner path it lands at, or
/// `None` when it's skipped (the caller counts it and skips its subtree).
///
/// An archive folder at that name merges silently. An archive FILE there is a
/// type-crossing clash resolved through `mode` like any other: the zip can't
/// hold the file and a folder of that name both (every reader drops an entry
/// whose parent is a file), so only an Overwrite a person answered for this
/// shape replaces the file, Rename moves the folder to a free name, and
/// everything else skips it.
#[allow(
    clippy::too_many_arguments,
    reason = "shared accumulators for one pass, same as `plan_file_add`"
)]
fn plan_dir(
    inner: String,
    src_path: &Path,
    archive_path: &Path,
    index: &ArchiveIndex,
    mode: &mut ConflictMode<'_>,
    mkdirs: &mut Vec<String>,
    mkdir_sources: &mut HashMap<String, PathBuf>,
    deletes: &mut Vec<String>,
    planned: &mut HashSet<String>,
) -> Result<Option<String>, EditError> {
    let target = if index.is_directory(&inner) == Some(false) {
        let clash = ArchiveClash {
            inner: &inner,
            src_path,
            incoming: IncomingItem::Directory,
            destination_is_directory: false,
        };
        match resolve_effective(mode, clash, archive_path, index)? {
            ConflictResolution::Overwrite => {
                deletes.push(inner.clone());
                inner
            }
            ConflictResolution::Rename => find_unique_inner(&inner, index, planned),
            ConflictResolution::Stop => {
                // Only under a pre-resolved `Policy(Stop)`, as in `plan_file_add`.
                return Err(EditError::Op(WriteOperationError::DestinationExists { path: inner }));
            }
            // `resolution_for_clash` already reduced a conditional variant to
            // Skip across types; nothing here can compare a folder to a file.
            ConflictResolution::Skip | ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
                return Ok(None);
            }
        }
    } else if index.exists(&inner) {
        // An archive folder: merge into it.
        return Ok(Some(inner));
    } else {
        inner
    };
    if planned.insert(target.clone()) {
        mkdir_sources.insert(target.clone(), src_path.to_path_buf());
        mkdirs.push(target.clone());
    }
    Ok(Some(target))
}

/// Joins an archive-inner parent (possibly empty) and a child name into one
/// `/`-separated inner path.
fn join_inner_str(parent: &str, child: &str) -> String {
    let parent = parent.trim_matches('/');
    let child = child.trim_matches('/');
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
    }
}
