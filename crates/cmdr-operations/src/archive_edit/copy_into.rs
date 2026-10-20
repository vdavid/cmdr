//! Copy/move INTO a zip: the routing entry, source materialization (a remote
//! source is streamed into a scratch dir first, a local one is used in place),
//! and the managed-op driver that plans (`copy_into_plan.rs`) and applies inside
//! the op — against the real archive for a LOCAL parent, or the pulled-local
//! working copy for a REMOTE one.

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::OperationEventSink;
use super::super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::super::scratch_dir::ScratchDir;
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::transfer::left_in_source::LeftInSource;
use super::super::transfer::volume::pull_path_to_local;
use super::super::transfer::volume::{CarriedSource, stamp_source, sweep_carried_source};
use super::super::types::ReadOnlySide;
use super::super::types::{ConflictResolution, WriteOperationError, WriteOperationStartResult, WriteOperationType};
use super::copy_into_plan::{build_copy_into_changeset, build_copy_into_changeset_interactive};
use super::edit_error::EditError;
use super::engine::{EditLeftovers, MutatorHooks, emit_archive_terminal, run_managed_edit, to_write_error};
use super::routing::{ensure_zip_writable, normalize_inner_path, read_only_error};
use cmdr_archive::mutator::{self, MutationError};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{LaneKey, Volume, VolumeError};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::types::{ArchiveSubkind, ExecutionStatus, OperationId};

impl crate::Operations {
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
    #[cfg(any(test, feature = "testing"))]
    #[allow(
        clippy::too_many_arguments,
        reason = "the cross-volume→archive seam threads the source handle, paths, dest, parent id, and policy; a struct would just shuffle them"
    )]
    pub async fn route_archive_copy_into(
        &self,
        events: Arc<dyn OperationEventSink>,
        source_volume: Arc<dyn Volume>,
        source_paths: Vec<PathBuf>,
        dest_full_path: PathBuf,
        parent_volume_id: String,
        conflict: ConflictResolution,
        progress_interval_ms: u64,
        is_move: bool,
        compression_level: Option<i64>,
        preview_id: Option<String>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        // A copy/move INTO an existing archive is a user-initiated zip-inner edit —
        // journaled but not rollbackable in v1. An MCP-initiated into-archive edit
        // supplies its own provenance via [`Self::route_archive_copy_into_with_provenance`]
        // (as compress does); this convenience wrapper defaults to `user`.
        self.route_archive_copy_into_with_provenance(
            events,
            source_volume,
            source_paths,
            dest_full_path,
            parent_volume_id,
            conflict,
            progress_interval_ms,
            is_move,
            compression_level,
            preview_id,
            super::super::journal::ArchiveProvenance::edit(cmdr_operation_log::types::Initiator::User),
            None,
        )
        .await
    }

    /// Routes a transfer into an archive with explicit [`ArchiveProvenance`](super::super::journal::ArchiveProvenance), so
    /// the compress driver can supply `subkind = compress` + the net-new flag the
    /// journal can't derive.
    #[allow(
        clippy::too_many_arguments,
        reason = "same seam as route_archive_copy_into plus provenance"
    )]
    pub(crate) async fn route_archive_copy_into_with_provenance(
        &self,
        events: Arc<dyn OperationEventSink>,
        source_volume: Arc<dyn Volume>,
        source_paths: Vec<PathBuf>,
        dest_full_path: PathBuf,
        parent_volume_id: String,
        conflict: ConflictResolution,
        progress_interval_ms: u64,
        is_move: bool,
        compression_level: Option<i64>,
        preview_id: Option<String>,
        prov: super::super::journal::ArchiveProvenance,
        destination_name: Option<String>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        super::super::validation::validate_transfer_destination_name(
            &source_paths,
            &dest_full_path,
            destination_name.as_deref(),
        )?;
        // A LOCAL source volume's root — `Some` skips the pull (the changeset walks
        // real paths); `None` (a remote source) triggers the in-op pull-to-scratch.
        let src_local_root = source_volume.local_path();

        // Confirmation already happened at the routing site (`dest_resolved.is_archive`
        // from the async, parent-aware `resolve`), so a pure string split is enough
        // here — and it works for a REMOTE dest zip, where the `std::fs` confirm would
        // wrongly return `None`.
        let (archive_path, dest_inner) = cmdr_fs::archive_format::archive_boundary_candidate(&dest_full_path)
            .ok_or_else(|| read_only_error(&dest_full_path, ReadOnlySide::Destination))?;
        ensure_zip_writable(&archive_path, ReadOnlySide::Destination)?;
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
        self.archive_copy_into_start(
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
            preview_id,
            prov,
            destination_name,
        )
        .await
    }
}

/// The sources materialized as LOCAL paths the changeset walk and mutator read
/// with `std::fs`. A local source volume is already local (no pull, no scratch);
/// a remote one is streamed into a scratch dir whose guard lives for the whole
/// op. For a MOVE, `carried` records what each ORIGINAL looked like before it was
/// read, so the removal after the commit takes the real user files, and only
/// the ones this move carried.
struct MaterializedSources {
    /// Absolute LOCAL paths, one per top-level source, in the caller's order.
    /// What the changeset walks and the mutator reads via `AddSource::LocalPath`.
    absolute: Vec<PathBuf>,
    /// The volume holding the originals, local or remote.
    source_volume: Arc<dyn Volume>,
    /// Each original's volume-relative path and what the move carried out of it.
    /// Empty on a copy, which removes nothing.
    carried: Vec<(PathBuf, CarriedSource)>,
    /// Keeps the pulled copies alive for the op; `None` for a local source.
    _scratch: Option<ScratchDir>,
}

impl MaterializedSources {
    /// Removes the ORIGINAL sources after an into-archive MOVE's rewrite durably
    /// commits (the move invariant: never delete a source before its bytes are
    /// safe). Removes exactly what the move carried, from the ledger stamped
    /// before the read, ❌ never the tree as it stands now: a file saved over or
    /// added in the source meanwhile exists only there, so it stays and the
    /// answer counts it. Best-effort per source: a failure leaves an incomplete
    /// move (a copy in both places), never data loss.
    async fn delete_originals(&self, state: &Arc<WriteOperationState>) -> LeftInSource {
        let mut left = LeftInSource::default();
        let nothing_skipped = HashSet::new();
        for (path, carried) in &self.carried {
            match sweep_carried_source(
                state,
                &self.source_volume,
                path,
                carried,
                &nothing_skipped,
                &crate::locked_sources::LockedPlan::none(),
            )
            .await
            {
                Ok(kept) => left.note(
                    path,
                    matches!(carried, CarriedSource::Folder(_)),
                    kept.appeared,
                    kept.changed,
                ),
                // `e.path` is the item that actually refused (a leaf inside
                // `path` when the source is a tree).
                Err(e) => log::warn!(
                    target: "archive_edit",
                    "couldn't remove {} of moved source {}: {}",
                    LogPrivate::path(&e.path),
                    LogPrivate::path(path),
                    LogError(&e.error)
                ),
            }
        }
        left
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
    is_move: bool,
    state: &Arc<WriteOperationState>,
) -> Result<MaterializedSources, EditError> {
    // A move stamps its originals FIRST, before anything reads them, so a save
    // during the pull or the rewrite counts as a change too.
    let mut carried = Vec::new();
    if is_move {
        // A source that is or holds a mount point would have the drive's files
        // added to the zip and then deleted there. Refused before anything is
        // read, so the zip is never opened.
        crate::mount_boundary::refuse_moving_a_volume_mount(&**source_volume, source_paths, state)
            .await
            .map_err(EditError::Op)?;
        for src in source_paths {
            let stamped = stamp_source(source_volume, src).await.map_err(|e| {
                EditError::Op(WriteOperationError::ReadError {
                    path: e.path.display().to_string(),
                    message: e.error.to_string(),
                })
            })?;
            carried.push((src.clone(), stamped));
        }
    }

    if let Some(root) = src_local_root {
        return Ok(MaterializedSources {
            absolute: source_paths.iter().map(|p| root.join(p)).collect(),
            source_volume: Arc::clone(source_volume),
            carried,
            _scratch: None,
        });
    }

    let scratch = ScratchDir::new("cmdr-archive-source-pull").map_err(|e| {
        EditError::Op(WriteOperationError::IoError {
            path: String::new(),
            message: e.to_string(),
        })
    })?;
    let dest_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "archive-source-pull",
        scratch.path().to_path_buf(),
        state.scope().host().volume_host().clone(),
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
        source_volume: Arc::clone(source_volume),
        carried,
        _scratch: Some(scratch),
    })
}

/// Streams one remote source (a file or a whole subtree) into the local scratch
/// dir through the copy engine's `pull_path_to_local` seam, so the pull inherits
/// its streaming (never whole-file-buffered), nested-tree recursion, cancel, and
/// pause. A cancel surfaces as `EditError::Cancelled`; any other fault surfaces
/// typed. The pull is silent — the archive rewrite stage drives the progress bar.
async fn pull_one_source(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<(), EditError> {
    let is_directory = source_volume.is_directory(source_path).await.map_err(|e| {
        EditError::Op(WriteOperationError::ReadError {
            path: source_path.display().to_string(),
            message: e.to_string(),
        })
    })?;

    pull_path_to_local(source_volume, source_path, is_directory, dest_volume, dest_path, state)
        .await
        .map(|_bytes| ())
        .map_err(|e| match e {
            VolumeError::Cancelled(_) => EditError::Cancelled,
            other => EditError::Op(WriteOperationError::ReadError {
                path: source_path.display().to_string(),
                message: other.to_string(),
            }),
        })
}

impl crate::Operations {
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
        &self,
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
        preview_id: Option<String>,
        prov: super::super::journal::ArchiveProvenance,
        destination_name: Option<String>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        let operation_id = OperationId::from(cmdr_operation_log::new_operation_id());
        let operation_type = if prov.subkind == ArchiveSubkind::Compress {
            WriteOperationType::Compress
        } else {
            WriteOperationType::ArchiveEdit
        };
        let state = Arc::new(self.new_operation_state(Duration::from_millis(progress_interval_ms)));

        let lane = state
            .scope()
            .host()
            .volumes()
            .get(&parent_volume_id)
            .map(|v| v.lane_key())
            .unwrap_or_else(|| LaneKey::new(parent_volume_id.clone()));
        let summary_source = source_paths
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let descriptor = OperationDescriptor {
            operation_id: operation_id.clone(),
            operation_type,
            lanes: vec![lane],
            volume_ids: vec![parent_volume_id.clone()],
            summary: OperationSummaryText {
                source: summary_source,
                destination: None,
                items: None,
            },
            // A zip edit is a whole-archive temp+rename rewrite: it either lands or
            // it doesn't, so there's no half-written state to reverse.
            rollback_support: crate::types::RollbackSupport::ArchiveRewrite,
            preview_id,
            reverses: None,
        };

        let events_for_op = Arc::clone(&events);
        let op_id_outer = operation_id.clone();
        let state_for_op = Arc::clone(&state);
        let progress_interval = Duration::from_millis(progress_interval_ms);

        let deferred = move |ops: crate::Operations| -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async move {
                let events = events_for_op;
                let op_id = op_id_outer;
                let state = state_for_op;
                let task_guard = ManagedTaskGuard::new(ops.manager(), op_id.clone());
                let settle_volume = (parent_volume_id != "root").then(|| parent_volume_id.clone());
                let _settled =
                    WriteSettledGuard::new(Arc::clone(&events), op_id.to_string(), operation_type, settle_volume);

                // Wait out the confirming dialog's scan. The changeset planner
                // walks the sources itself, so this buys serialization rather than
                // reuse: without it, ⌥F5's sampling preview and this walk would run
                // down the same tree at once, which is what costs on MTP and SMB.
                if crate::scan_bridge::await_claimed_preview(&ops, &*events, &op_id, operation_type, &state)
                    .await
                    .stopped()
                {
                    task_guard.disarm();
                    ops.manager().on_settled(&op_id);
                    return;
                }

                // Open the journal row when the op actually starts. Archive edits
                // spawn directly (not through `start_write_operation`), so this is
                // their own open/finalize bracket, mirroring the generic one.
                super::super::journal::open_archive_op(
                    state.scope(),
                    op_id.as_str(),
                    prov.initiator,
                    &parent_volume_id,
                );

                let hooks = Arc::new(MutatorHooks::new(
                    Arc::clone(&state),
                    Arc::clone(&events),
                    op_id.to_string(),
                    operation_type,
                    progress_interval,
                ));

                // Materialize sources (pull if remote), then plan+apply against the
                // archive. One `Result<skipped_count, EditError>` funnels into a single
                // terminal emit below. A cancel/fault in the PULL returns before
                // `run_managed_edit` ever opens the zip, so the archive stays untouched.
                let outcome: Result<(usize, LeftInSource), EditError> = async {
                    let materialized =
                        materialize_sources(&source_volume, &source_paths, src_local_root, is_move, &state).await?;
                    let absolute_sources = materialized.absolute.clone();

                    let (should_delete_sources, skipped_count) = run_managed_edit(
                        &parent_volume_id,
                        archive_path.clone(),
                        Arc::clone(&state),
                        hooks.remote_progress_observer(),
                        {
                            let events_for_blocking = Arc::clone(&events);
                            let state_for_blocking = Arc::clone(&state);
                            let op_id_for_blocking = op_id.clone();
                            let hooks_for_blocking = Arc::clone(&hooks);
                            let dest_inner = dest_inner.clone();
                            move |working: &Path| -> Result<(bool, usize), EditError> {
                                // Stop → interactive per-file prompts; any pre-resolved
                                // policy → non-interactive. Both plan against `working`
                                // (the pulled-local copy for a remote parent), never the
                                // raw remote path.
                                let mut plan = if matches!(conflict, ConflictResolution::Stop) {
                                    build_copy_into_changeset_interactive(
                                        working,
                                        &absolute_sources,
                                        &dest_inner,
                                        destination_name.as_deref(),
                                        &*events_for_blocking,
                                        op_id_for_blocking.as_str(),
                                        &state_for_blocking,
                                    )?
                                } else {
                                    build_copy_into_changeset(
                                        working,
                                        &absolute_sources,
                                        &dest_inner,
                                        destination_name.as_deref(),
                                        conflict,
                                    )
                                    .map_err(EditError::Op)?
                                };
                                // The user's compression level governs every newly added
                                // entry in this edit (the mutator clamps it to 1..=9).
                                plan.changeset.compression_level = compression_level;
                                let should_delete = is_move && plan.skipped_count == 0;
                                mutator::apply(working, &plan.changeset, &*hooks_for_blocking).map_err(
                                    |e| match e {
                                        MutationError::Cancelled => EditError::Cancelled,
                                        other => EditError::Op(to_write_error(working, other)),
                                    },
                                )?;
                                Ok((should_delete, plan.skipped_count))
                            }
                        },
                    )
                    .await?;

                    let left = if should_delete_sources {
                        materialized.delete_originals(&state).await
                    } else {
                        LeftInSource::default()
                    };
                    Ok((skipped_count, left))
                }
                .await;

                let final_progress = hooks.latest_progress();
                // Capture the terminal status before the consuming `match outcome`
                // below (which moves the error out of `outcome`).
                let execution_status = match &outcome {
                    Ok(_) => ExecutionStatus::Done,
                    Err(EditError::Cancelled) => ExecutionStatus::Canceled,
                    Err(EditError::Op(_)) => ExecutionStatus::Failed,
                };
                let (skipped_count, appeared_during_move) = outcome
                    .as_ref()
                    .map(|(skipped, left)| (*skipped, left.appeared_during_move()))
                    .unwrap_or((0, None));
                emit_archive_terminal(
                    events.as_ref(),
                    op_id.as_str(),
                    operation_type,
                    outcome.map(|_| ()),
                    skipped_count,
                    EditLeftovers::of(&state, &parent_volume_id, appeared_during_move),
                    &final_progress,
                );

                // Finalize the journal row. Compress records the created archive as
                // its single `rollback_unit` item (the rollback deletes it if still
                // net-new and unchanged), then finalizes with the driver's subkind +
                // net-new flag so eligibility is computed from what the driver knows
                // A plain into-archive edit records no item — it's not
                // rollbackable in v1 — just the header's terminal state.
                if prov.subkind == ArchiveSubkind::Compress && execution_status == ExecutionStatus::Done {
                    // Snapshot the finished archive (size + mtime) for the rollback drift
                    // recheck; best-effort and local-only (a remote archive snapshots
                    // as `None`, so its rollback rechecks existence only).
                    let (size, mtime) = std::fs::symlink_metadata(&archive_path)
                        .map(|m| (Some(m.len() as i64), super::super::journal::mtime_secs(&m)))
                        .unwrap_or((None, None));
                    super::super::journal::record_compress_archive(
                        state.scope(),
                        &parent_volume_id,
                        &archive_path,
                        size,
                        mtime,
                        prov.net_new,
                    );
                }
                super::super::journal::finalize_archive_op(
                    state.scope(),
                    prov.subkind,
                    prov.net_new,
                    execution_status,
                    None,
                );

                task_guard.disarm();
                ops.manager().on_settled(&op_id);
            })
        };

        self.manager().spawn_managed(descriptor, state, Box::new(deferred));

        Ok(WriteOperationStartResult {
            operation_id: operation_id.to_string(),
            operation_type,
        })
    }
}
