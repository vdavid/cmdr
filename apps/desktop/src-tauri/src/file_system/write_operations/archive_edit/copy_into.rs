//! Copy/move INTO a zip: the routing entry, the changeset planning (walk the
//! local sources, resolve each collision via [`super::conflicts`], build one
//! `{ add + mkdir + delete }` batch), and the managed-op driver that plans and
//! applies inside the op — LOCAL in place, or against the pulled-local working
//! copy for a REMOTE parent.

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;
use walkdir::WalkDir;

use super::super::OperationEventSink;
use super::super::conflict::ApplyToAll;
use super::super::manager::{self, ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::types::{
    ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError,
    WriteOperationStartResult, WriteOperationType,
};
use super::conflicts::{ConflictMode, conditional_overwrites, find_unique_inner, resolve_effective};
use super::engine::{MutatorHooks, PlanError, delete_move_sources, run_managed_edit, to_write_error};
use super::routing::{ensure_zip_writable, normalize_inner_path, read_only_error};
use crate::file_system::get_volume_manager;
use crate::file_system::volume::backends::archive;
use crate::file_system::volume::backends::archive::mutator::{self, AddEntry, AddSource, Changeset, MutationError};
use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveIndex, LocalFileSource};
use crate::file_system::volume::{LaneKey, Volume};

/// Routes a copy/move INTO a zip (a local-FS source dropped onto an archive
/// destination) to the managed edit driver as a SINGLE `{ add + mkdir }`
/// changeset — the whole transfer becomes one archive rewrite, not one per file.
///
/// Conflicts (an added inner path that already exists) are resolved
/// non-interactively from `conflict` against the archive's index (Skip drops the
/// add; Overwrite deletes the existing entry then adds; Rename picks a unique
/// name; Stop fails with `DestinationExists`; the conditional policies compare
/// size/mtime). `source_paths` are relative to the source volume root, as
/// `copy_between_volumes` passes them. For a MOVE, the top-level sources are
/// deleted after the commit — but only when nothing was skipped, so a partial
/// (conflict-skipped) move never deletes a source whose bytes didn't land.
///
/// v1 handles LOCAL sources only; a non-local source (MTP/SMB → zip) is refused
/// (the mutator streams adds from a local path).
#[allow(
    clippy::too_many_arguments,
    reason = "the cross-volume→archive seam threads the source handle, paths, dest, parent id, and policy; a struct would just shuffle them"
)]
pub(crate) async fn route_archive_copy_into(
    events: Arc<dyn OperationEventSink>,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_full_path: PathBuf,
    parent_volume_id: String,
    conflict: ConflictResolution,
    progress_interval_ms: u64,
    is_move: bool,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let src_root = source_volume.local_path().ok_or_else(|| WriteOperationError::IoError {
        path: String::new(),
        message: "copying into an archive from this source isn't supported yet".to_string(),
    })?;
    let absolute_sources: Vec<PathBuf> = source_paths.iter().map(|p| src_root.join(p)).collect();

    // Confirmation already happened at the routing site (`dest_resolved.is_archive`
    // from the async, parent-aware `resolve`), so a pure string split is enough
    // here — and it works for a REMOTE dest zip, where the `std::fs` confirm would
    // wrongly return `None`.
    let (archive_path, dest_inner) =
        archive::archive_boundary_candidate(&dest_full_path).ok_or_else(|| read_only_error(&dest_full_path))?;
    ensure_zip_writable(&archive_path)?;
    let dest_inner = normalize_inner_path(&dest_inner);

    // Plan AND apply inside the managed op, against the working copy — the real
    // archive for a LOCAL parent, or the pulled-local copy for a REMOTE one. The
    // changeset must NOT be planned up front against `archive_path`: for a REMOTE
    // parent that path has no local file, so `LocalFileSource::open` fails (MTP) or
    // opens the OS mount the design routes around (direct SMB, hang risk). Both
    // policies plan inside the op's closure via `run_managed_edit`: a pre-resolved
    // policy resolves non-interactively; Stop prompts per file (the op is
    // registered, so `resolve_write_conflict(op_id)` reaches the oneshot).
    archive_copy_into_start(
        events,
        absolute_sources,
        archive_path,
        dest_inner,
        parent_volume_id,
        conflict,
        is_move,
        progress_interval_ms,
    )
    .await
}

/// The planned changeset for a copy/move-into, plus how many source entries were
/// skipped (a conflict resolved to Skip, or a symlink / special file a zip can't
/// hold). A non-zero count suppresses the source deletion on a move and surfaces
/// as `files_skipped` on the terminal event.
struct CopyIntoPlan {
    changeset: Changeset,
    skipped_count: usize,
}

/// Walks the local sources and builds the `{ add + delete + mkdir }` changeset
/// under a pre-resolved policy (no prompting). Blocking — call from `spawn_blocking`.
fn build_copy_into_changeset(
    archive_path: &Path,
    absolute_sources: &[PathBuf],
    dest_inner: &str,
    conflict: ConflictResolution,
) -> Result<CopyIntoPlan, WriteOperationError> {
    let mut mode = ConflictMode::Policy(conflict);
    build_copy_into_changeset_inner(archive_path, absolute_sources, dest_inner, &mut mode).map_err(|e| match e {
        PlanError::Op(w) => w,
        // A pre-resolved policy never prompts, so it can't be cancelled here.
        PlanError::Cancelled => WriteOperationError::Cancelled {
            message: "the archive copy was cancelled".to_string(),
        },
    })
}

/// Walks the local sources and builds the changeset with INTERACTIVE per-file
/// conflict prompts (the Stop UX). Blocking (parses the index, walks the tree, and
/// blocks on each prompt's oneshot) — call from `spawn_blocking` inside the managed
/// op so `resolve_write_conflict(operation_id)` can answer.
fn build_copy_into_changeset_interactive(
    archive_path: &Path,
    absolute_sources: &[PathBuf],
    dest_inner: &str,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
) -> Result<CopyIntoPlan, PlanError> {
    let mut latch = ApplyToAll::default();
    let mut mode = ConflictMode::Interactive {
        events,
        operation_id,
        state,
        apply_to_all: &mut latch,
    };
    build_copy_into_changeset_inner(archive_path, absolute_sources, dest_inner, &mut mode)
}

/// The shared copy-into walk. Resolves each FILE collision via `mode`; directory
/// collisions merge silently (an existing archive dir is never re-added and never
/// prompts — the app-wide dir-vs-dir rule).
fn build_copy_into_changeset_inner(
    archive_path: &Path,
    absolute_sources: &[PathBuf],
    dest_inner: &str,
    mode: &mut ConflictMode<'_>,
) -> Result<CopyIntoPlan, PlanError> {
    let source = LocalFileSource::open(archive_path).map_err(|e| {
        PlanError::Op(WriteOperationError::WriteError {
            path: archive_path.display().to_string(),
            message: e.to_string(),
        })
    })?;
    let index = ArchiveIndex::parse(Arc::new(source), ArchiveFormat::Zip).map_err(|e| {
        PlanError::Op(WriteOperationError::WriteError {
            path: archive_path.display().to_string(),
            message: e.to_string(),
        })
    })?;

    let mut adds: Vec<AddEntry> = Vec::new();
    let mut mkdirs: Vec<String> = Vec::new();
    let mut deletes: Vec<String> = Vec::new();
    // Inner paths this changeset already claims (dedup + Rename uniqueness).
    let mut planned: HashSet<String> = HashSet::new();
    let mut skipped_count: usize = 0;

    for src in absolute_sources {
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let base_inner = join_inner_str(dest_inner, name);
        let meta = std::fs::symlink_metadata(src).map_err(|e| {
            PlanError::Op(WriteOperationError::ReadError {
                path: src.display().to_string(),
                message: e.to_string(),
            })
        })?;

        if meta.is_dir() {
            for entry in WalkDir::new(src).into_iter().filter_map(Result::ok) {
                let rel = entry.path().strip_prefix(src).unwrap_or_else(|_| entry.path());
                let inner = if rel.as_os_str().is_empty() {
                    base_inner.clone()
                } else {
                    join_inner_str(&base_inner, &rel.to_string_lossy().replace('\\', "/"))
                };
                let file_type = entry.file_type();
                if file_type.is_dir() {
                    if !index.exists(&inner) && planned.insert(inner.clone()) {
                        mkdirs.push(inner);
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
            deletes,
            renames: Vec::new(),
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
) -> Result<(), PlanError> {
    let in_index = index.exists(&inner);
    let collides = in_index || planned.contains(&inner);

    let target = if collides {
        // The clashing side is a file (this is the file-add path); classify the
        // destructive file→folder variant only when an existing ARCHIVE entry at
        // this name is a directory (a planned add is always a file).
        let is_file_to_folder = index.is_directory(&inner) == Some(true);
        let resolution = resolve_effective(mode, &inner, src_path, archive_path, index, is_file_to_folder)?;
        match resolution {
            ConflictResolution::Skip => {
                *skipped_count += 1;
                return Ok(());
            }
            ConflictResolution::Stop => {
                // Only reachable under a pre-resolved `Policy(Stop)` (the
                // interactive path never returns Stop). Treat as a hard collision.
                return Err(PlanError::Op(WriteOperationError::DestinationExists { path: inner }));
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

/// Starts a copy/move INTO a zip as a managed op, planning AND applying inside the
/// op's deferred (on the blocking pool) against the working copy `run_managed_edit`
/// hands the closure — the real archive for a LOCAL parent, the pulled-local copy
/// for a REMOTE one. Planning inside the op is what lets a remote edit plan against
/// the pulled bytes (never the unopenable remote path) and lets a Stop collision
/// emit a `write-conflict` and block on the op's registered oneshot. A pre-resolved
/// policy resolves each collision without prompting; Stop prompts per file.
/// Dir-vs-dir collisions merge silently under both.
#[allow(
    clippy::too_many_arguments,
    reason = "the copy-into seam threads the sources, archive path, dest, parent id, policy, and move flag; a struct would just shuffle them"
)]
async fn archive_copy_into_start(
    events: Arc<dyn OperationEventSink>,
    absolute_sources: Vec<PathBuf>,
    archive_path: PathBuf,
    dest_inner: String,
    parent_volume_id: String,
    conflict: ConflictResolution,
    is_move: bool,
    progress_interval_ms: u64,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    let lane = get_volume_manager()
        .get(&parent_volume_id)
        .map(|v| v.lane_key())
        .unwrap_or_else(|| LaneKey::new(parent_volume_id.clone()));
    let summary_source = absolute_sources
        .first()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::ArchiveEdit,
        lanes: vec![lane],
        volume_ids: vec![parent_volume_id.clone()],
        summary: OperationSummaryText {
            source: summary_source,
            destination: None,
        },
    };

    let events_for_op = Arc::clone(&events);
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let progress_interval = Duration::from_millis(progress_interval_ms);

    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let events = events_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = ManagedTaskGuard::new(op_id.clone());
            let settle_volume = (parent_volume_id != "root").then(|| parent_volume_id.clone());
            let _settled = WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::ArchiveEdit,
                settle_volume,
            );

            let hooks = Arc::new(MutatorHooks::new(
                Arc::clone(&state),
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::ArchiveEdit,
                progress_interval,
            ));

            // Plan (with prompting) then apply against the archive — LOCAL in place,
            // or pulled-local then uploaded+swapped for a REMOTE parent. Returns
            // the move sources to delete after a committed MOVE (empty otherwise).
            let hooks_for_blocking = Arc::clone(&hooks);
            let events_for_blocking = Arc::clone(&events);
            let state_for_blocking = Arc::clone(&state);
            let op_id_for_blocking = op_id.clone();
            let result = run_managed_edit(
                &parent_volume_id,
                archive_path.clone(),
                Arc::clone(&state),
                move |working: &Path| -> Result<(Vec<PathBuf>, usize), PlanError> {
                    // Stop → interactive per-file prompts; any pre-resolved policy →
                    // non-interactive. Both plan against `working` (the pulled-local
                    // copy for a remote parent), never the raw remote path.
                    let plan = if matches!(conflict, ConflictResolution::Stop) {
                        build_copy_into_changeset_interactive(
                            working,
                            &absolute_sources,
                            &dest_inner,
                            &*events_for_blocking,
                            &op_id_for_blocking,
                            &state_for_blocking,
                        )?
                    } else {
                        build_copy_into_changeset(working, &absolute_sources, &dest_inner, conflict)
                            .map_err(PlanError::Op)?
                    };
                    let move_sources = if is_move && plan.skipped_count == 0 {
                        absolute_sources.clone()
                    } else {
                        Vec::new()
                    };
                    mutator::apply(working, &plan.changeset, &*hooks_for_blocking).map_err(|e| match e {
                        MutationError::Cancelled => PlanError::Cancelled,
                        other => PlanError::Op(to_write_error(working, other)),
                    })?;
                    Ok((move_sources, plan.skipped_count))
                },
            )
            .await;

            let final_progress = hooks.latest_progress();
            match result {
                Ok((move_sources, skipped_count)) => {
                    // Move invariant: delete the local sources only now that the
                    // archive rewrite is durably committed.
                    delete_move_sources(&move_sources).await;
                    events.emit_complete(WriteCompleteEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::ArchiveEdit,
                        files_processed: final_progress.entries_changed,
                        files_skipped: skipped_count,
                        bytes_processed: final_progress.bytes_total,
                    });
                }
                Err(PlanError::Cancelled) => {
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::ArchiveEdit,
                        files_processed: final_progress.entries_done,
                        rolled_back: false,
                    });
                }
                Err(PlanError::Op(err)) => {
                    events.emit_error(WriteErrorEvent::new(
                        op_id.clone(),
                        WriteOperationType::ArchiveEdit,
                        err,
                    ));
                }
            }

            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));

    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::ArchiveEdit,
    })
}
