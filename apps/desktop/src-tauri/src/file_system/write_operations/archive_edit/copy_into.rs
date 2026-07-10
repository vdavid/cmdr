//! Copy/move INTO a zip: the routing entry, source materialization (a remote
//! source is streamed into a scratch dir first, a local one is used in place),
//! the changeset planning (walk the local sources, resolve each collision via
//! [`super::conflicts`], build one `{ add + mkdir + delete }` batch), and the
//! managed-op driver that plans and applies inside the op — against the real
//! archive for a LOCAL parent, or the pulled-local working copy for a REMOTE one.

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
use super::super::scratch_dir::ScratchDir;
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::transfer::volume_copy::delete_volume_path_recursive;
use super::super::transfer::volume_strategy::pull_path_to_local;
use super::super::types::{
    ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError,
    WriteOperationStartResult, WriteOperationType,
};
use super::conflicts::{ConflictMode, conditional_overwrites, find_unique_inner, resolve_effective};
use super::engine::{MutatorHooks, PlanError, delete_move_sources, run_managed_edit, to_write_error};
use super::routing::{ensure_zip_writable, normalize_inner_path, read_only_error};
use crate::file_system::get_volume_manager;
use crate::operation_log::types::{ArchiveSubkind, ExecutionStatus};
use crate::file_system::volume::backends::archive;
use crate::file_system::volume::backends::archive::mutator::{self, AddEntry, AddSource, Changeset, MutationError};
use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveIndex, LocalFileSource};
use crate::file_system::volume::{LaneKey, LocalPosixVolume, Volume, VolumeError};

/// Routes a copy/move INTO a zip (a source dropped onto an archive destination)
/// to the managed edit driver as a SINGLE `{ add + mkdir }` changeset — the whole
/// transfer becomes one archive rewrite, not one per file.
///
/// The SOURCE may be LOCAL or REMOTE. A local-FS source gives real filesystem
/// paths the changeset walks directly. A REMOTE source (MTP / SMB) has no local
/// path, so its subtree is streamed into a scratch dir inside the op first, and
/// the ordinary local ingest then runs against the pulled bytes (see
/// [`materialize_sources`]). The archive PARENT is a separate axis: local or
/// remote, `run_managed_edit` handles it independently, so all four
/// source×parent combinations work.
///
/// Conflicts (an added inner path that already exists) are resolved
/// non-interactively from `conflict` against the archive's index (Skip drops the
/// add; Overwrite deletes the existing entry then adds; Rename picks a unique
/// name; Stop fails with `DestinationExists`; the conditional policies compare
/// size/mtime). `source_paths` are relative to the source volume root, as
/// `copy_between_volumes` passes them. For a MOVE, the top-level sources are
/// deleted after the commit — but only when nothing was skipped, so a partial
/// (conflict-skipped) move never deletes a source whose bytes didn't land.
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
    compression_level: Option<i64>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // A copy/move INTO an existing archive is a zip-inner edit — journaled but not
    // rollbackable in v1. Compress overrides this via
    // [`route_archive_copy_into_with_provenance`].
    route_archive_copy_into_with_provenance(
        events,
        source_volume,
        source_paths,
        dest_full_path,
        parent_volume_id,
        conflict,
        progress_interval_ms,
        is_move,
        compression_level,
        super::super::journal::ArchiveProvenance::edit(crate::operation_log::types::Initiator::User),
    )
    .await
}

/// Like [`route_archive_copy_into`] but with an explicit [`ArchiveProvenance`], so
/// the compress driver can supply `subkind = compress` + the net-new flag the
/// journal can't derive (Finding 3).
#[allow(clippy::too_many_arguments, reason = "same seam as route_archive_copy_into plus provenance")]
pub(crate) async fn route_archive_copy_into_with_provenance(
    events: Arc<dyn OperationEventSink>,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_full_path: PathBuf,
    parent_volume_id: String,
    conflict: ConflictResolution,
    progress_interval_ms: u64,
    is_move: bool,
    compression_level: Option<i64>,
    prov: super::super::journal::ArchiveProvenance,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // A LOCAL source volume's root — `Some` skips the pull (the changeset walks
    // real paths); `None` (a remote source) triggers the in-op pull-to-scratch.
    let src_local_root = source_volume.local_path();

    // Confirmation already happened at the routing site (`dest_resolved.is_archive`
    // from the async, parent-aware `resolve`), so a pure string split is enough
    // here — and it works for a REMOTE dest zip, where the `std::fs` confirm would
    // wrongly return `None`.
    let (archive_path, dest_inner) =
        archive::archive_boundary_candidate(&dest_full_path).ok_or_else(|| read_only_error(&dest_full_path))?;
    ensure_zip_writable(&archive_path)?;
    let dest_inner = normalize_inner_path(&dest_inner);

    // Materialize sources AND plan+apply inside the managed op. The pull (remote
    // source) streams into a scratch dir; then the changeset is planned against
    // the working copy — the real archive for a LOCAL parent, or the pulled-local
    // copy for a REMOTE one. Planning must NOT run up front against `archive_path`:
    // for a REMOTE parent that path has no local file, so `LocalFileSource::open`
    // fails (MTP) or opens the OS mount the design routes around (direct SMB, hang
    // risk). A pre-resolved policy resolves non-interactively; Stop prompts per
    // file (the op is registered, so `resolve_write_conflict(op_id)` reaches the
    // oneshot).
    archive_copy_into_start(
        events,
        source_volume,
        source_paths,
        src_local_root,
        archive_path,
        dest_inner,
        parent_volume_id,
        conflict,
        is_move,
        progress_interval_ms,
        compression_level,
        prov,
    )
    .await
}

/// The sources materialized as LOCAL paths the changeset walk and mutator read
/// with `std::fs`. A local source volume is already local (no pull, no scratch);
/// a remote one is streamed into a scratch dir whose guard lives for the whole
/// op. `origin` records where the ORIGINAL sources live so an into-archive MOVE
/// deletes the real user files, not the local scratch copies.
struct MaterializedSources {
    /// Absolute LOCAL paths, one per top-level source, in the caller's order.
    /// What the changeset walks and the mutator reads via `AddSource::LocalPath`.
    absolute: Vec<PathBuf>,
    origin: SourceOrigin,
    /// Keeps the pulled copies alive for the op; `None` for a local source.
    _scratch: Option<ScratchDir>,
}

/// Where an into-archive MOVE finds the ORIGINAL sources to delete after the
/// rewrite commits — the absolute local paths for a local source, or the remote
/// volume + volume-relative paths for a pulled remote one.
enum SourceOrigin {
    Local,
    Remote {
        volume: Arc<dyn Volume>,
        paths: Vec<PathBuf>,
    },
}

impl MaterializedSources {
    /// Deletes the ORIGINAL sources after an into-archive MOVE's rewrite durably
    /// commits (the move invariant — never delete a source before its bytes are
    /// safe). Local originals go straight off the FS; remote originals go through
    /// the source volume (recursive for trees). Best-effort per source: a failure
    /// leaves an incomplete move (a copy in both places), never data loss.
    async fn delete_originals(&self) {
        match &self.origin {
            SourceOrigin::Local => delete_move_sources(&self.absolute).await,
            SourceOrigin::Remote { volume, paths } => {
                for path in paths {
                    if let Err(e) = delete_volume_path_recursive(volume, path).await {
                        log::warn!(target: "archive_edit", "couldn't remove moved remote source {}: {e}", path.display());
                    }
                }
            }
        }
    }
}

/// Makes the copy's sources available as LOCAL paths. A local source volume needs
/// no work (the absolute paths are `root.join(rel)`). A REMOTE source is streamed
/// into a fresh scratch dir through the shared copy engine's `copy_single_path`,
/// so the pull inherits its streaming (never whole-file-buffered), nested-tree
/// recursion, cancel, and pause — then the changeset walks real bytes with
/// `std::fs`. The pull emits no progress (this is the pull stage; the archive
/// rewrite drives the progress bar, matching the remote-PARENT flow).
async fn materialize_sources(
    source_volume: &Arc<dyn Volume>,
    source_paths: &[PathBuf],
    src_local_root: Option<PathBuf>,
    state: &Arc<WriteOperationState>,
) -> Result<MaterializedSources, PlanError> {
    if let Some(root) = src_local_root {
        return Ok(MaterializedSources {
            absolute: source_paths.iter().map(|p| root.join(p)).collect(),
            origin: SourceOrigin::Local,
            _scratch: None,
        });
    }

    let scratch = ScratchDir::new("cmdr-archive-source-pull").map_err(|e| {
        PlanError::Op(WriteOperationError::IoError {
            path: String::new(),
            message: e.to_string(),
        })
    })?;
    let dest_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new(
        "archive-source-pull",
        scratch.path().to_path_buf(),
    ));

    let mut absolute = Vec::with_capacity(source_paths.len());
    for src in source_paths {
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let dest_path = scratch.path().join(name);
        pull_one_source(source_volume, src, &dest_volume, &dest_path, state).await?;
        absolute.push(dest_path);
    }

    Ok(MaterializedSources {
        absolute,
        origin: SourceOrigin::Remote {
            volume: Arc::clone(source_volume),
            paths: source_paths.to_vec(),
        },
        _scratch: Some(scratch),
    })
}

/// Streams one remote source (a file or a whole subtree) into the local scratch
/// dir through the copy engine's `pull_path_to_local` seam, so the pull inherits
/// its streaming (never whole-file-buffered), nested-tree recursion, cancel, and
/// pause. A cancel surfaces as `PlanError::Cancelled`; any other fault surfaces
/// typed. The pull is silent — the archive rewrite stage drives the progress bar.
async fn pull_one_source(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<(), PlanError> {
    let is_directory = source_volume.is_directory(source_path).await.map_err(|e| {
        PlanError::Op(WriteOperationError::ReadError {
            path: source_path.display().to_string(),
            message: e.to_string(),
        })
    })?;

    pull_path_to_local(source_volume, source_path, is_directory, dest_volume, dest_path, state)
        .await
        .map(|_bytes| ())
        .map_err(|e| match e {
            VolumeError::Cancelled(_) => PlanError::Cancelled,
            other => PlanError::Op(WriteOperationError::ReadError {
                path: source_path.display().to_string(),
                message: other.to_string(),
            }),
        })
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
    let index = ArchiveIndex::parse(Arc::new(source), ArchiveFormat::Zip, None).map_err(|e| {
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

/// Starts a copy/move INTO a zip as a managed op. Inside the op's deferred (so
/// the dialog opened the instant we returned the id), two stages run: (1)
/// materialize the sources locally — a no-op for a local source, a streamed pull
/// into a scratch dir for a remote one; (2) plan AND apply the changeset against
/// the working copy `run_managed_edit` hands the closure — the real archive for a
/// LOCAL parent, the pulled-local copy for a REMOTE one. Planning inside the op is
/// what lets a remote edit plan against the pulled bytes (never the unopenable
/// remote path) and lets a Stop collision emit a `write-conflict` and block on the
/// op's registered oneshot. A pre-resolved policy resolves each collision without
/// prompting; Stop prompts per file. Dir-vs-dir collisions merge silently.
#[allow(
    clippy::too_many_arguments,
    reason = "the copy-into seam threads the source volume, paths, locality, dest, parent id, policy, and move flag; a struct would just shuffle them"
)]
async fn archive_copy_into_start(
    events: Arc<dyn OperationEventSink>,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    src_local_root: Option<PathBuf>,
    archive_path: PathBuf,
    dest_inner: String,
    parent_volume_id: String,
    conflict: ConflictResolution,
    is_move: bool,
    progress_interval_ms: u64,
    compression_level: Option<i64>,
    prov: super::super::journal::ArchiveProvenance,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    let lane = get_volume_manager()
        .get(&parent_volume_id)
        .map(|v| v.lane_key())
        .unwrap_or_else(|| LaneKey::new(parent_volume_id.clone()));
    let summary_source = source_paths
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

            // Open the journal row when the op actually starts. Archive edits
            // spawn directly (not through `start_write_operation`), so this is
            // their own open/finalize bracket, mirroring the generic one.
            super::super::journal::open_archive_op(&op_id, prov.initiator, &parent_volume_id);

            let hooks = Arc::new(MutatorHooks::new(
                Arc::clone(&state),
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::ArchiveEdit,
                progress_interval,
            ));

            // Materialize sources (pull if remote), then plan+apply against the
            // archive. One `Result<skipped_count, PlanError>` funnels into a single
            // terminal emit below. A cancel/fault in the PULL returns before
            // `run_managed_edit` ever opens the zip, so the archive stays untouched.
            let outcome: Result<usize, PlanError> = async {
                let materialized = materialize_sources(&source_volume, &source_paths, src_local_root, &state).await?;
                let absolute_sources = materialized.absolute.clone();

                let (should_delete_sources, skipped_count) =
                    run_managed_edit(&parent_volume_id, archive_path.clone(), Arc::clone(&state), {
                        let events_for_blocking = Arc::clone(&events);
                        let state_for_blocking = Arc::clone(&state);
                        let op_id_for_blocking = op_id.clone();
                        let hooks_for_blocking = Arc::clone(&hooks);
                        let dest_inner = dest_inner.clone();
                        move |working: &Path| -> Result<(bool, usize), PlanError> {
                            // Stop → interactive per-file prompts; any pre-resolved
                            // policy → non-interactive. Both plan against `working`
                            // (the pulled-local copy for a remote parent), never the
                            // raw remote path.
                            let mut plan = if matches!(conflict, ConflictResolution::Stop) {
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
                            // The user's compression level governs every newly added
                            // entry in this edit (the mutator clamps it to 1..=9).
                            plan.changeset.compression_level = compression_level;
                            let should_delete = is_move && plan.skipped_count == 0;
                            mutator::apply(working, &plan.changeset, &*hooks_for_blocking).map_err(|e| match e {
                                MutationError::Cancelled => PlanError::Cancelled,
                                other => PlanError::Op(to_write_error(working, other)),
                            })?;
                            Ok((should_delete, plan.skipped_count))
                        }
                    })
                    .await?;

                if should_delete_sources {
                    materialized.delete_originals().await;
                }
                Ok(skipped_count)
            }
            .await;

            let final_progress = hooks.latest_progress();
            // Capture the terminal status before the consuming `match outcome`
            // below (which moves the error out of `outcome`).
            let execution_status = match &outcome {
                Ok(_) => ExecutionStatus::Done,
                Err(PlanError::Cancelled) => ExecutionStatus::Canceled,
                Err(PlanError::Op(_)) => ExecutionStatus::Failed,
            };
            match outcome {
                Ok(skipped_count) => {
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

            // Finalize the journal row. Compress records the created archive as
            // its single `rollback_unit` item (the M3 rollback deletes it if still
            // net-new and unchanged), then finalizes with the driver's subkind +
            // net-new flag so eligibility is computed from what the driver knows
            // (Finding 3). A plain into-archive edit records no item — it's not
            // rollbackable in v1 — just the header's terminal state.
            if prov.subkind == ArchiveSubkind::Compress && execution_status == ExecutionStatus::Done {
                // Snapshot the finished archive (size + mtime) for the M3 drift
                // recheck; best-effort and local-only (a remote archive snapshots
                // as `None`, so its rollback rechecks existence only).
                let (size, mtime) = std::fs::symlink_metadata(&archive_path)
                    .map(|m| (Some(m.len() as i64), super::super::journal::mtime_secs(&m)))
                    .unwrap_or((None, None));
                super::super::journal::record_compress_archive(
                    &op_id,
                    &parent_volume_id,
                    &archive_path,
                    size,
                    mtime,
                    prov.net_new,
                );
            }
            super::super::journal::finalize_archive_op(&op_id, prov.subkind, prov.net_new, execution_status);

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
