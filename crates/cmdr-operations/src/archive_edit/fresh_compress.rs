//! Managed fresh-archive creation: plan source entries, stream one seedless ZIP
//! producer into a tracked destination stage, validate it, then publish it.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cmdr_archive::mutator::{MutationHooks, MutationProgress};

use super::super::OperationEventSink;
use super::super::Operations;
use super::super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::super::scratch_dir::ScratchDir;
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::summary::volume_items;
use super::super::transfer::StagedWrite;
use super::super::transfer::volume_outage::StartedOn;
use super::super::transfer_tree::{TreeLanding, TreeOrder, from_archive_plan};
use super::super::types::{ConflictResolution, WriteOperationError, WriteOperationStartResult, WriteOperationType};
use super::edit_error::EditError;
use super::engine::{EditLeftovers, MutatorHooks, emit_archive_terminal};
use super::fresh_pipeline::{ProducerRuns, continue_unless, operation_cancelled, pipeline_failure, volume_write_error};
use super::fresh_plan::{FreshPlan, PlanProgress, plan_sources, validate_aliases};
use super::fresh_publish::{Published, Target, publish_stage};
use super::fresh_rows::CompressRows;
use super::fresh_validate::{ExpectedIndex, check_entry_names, validate_stage};
use super::outage_wait::{Next, StepError, wait_out};
use cmdr_fs::volume::{
    BackendKind, LaneKey, MutationEvent, StreamLength, StreamWriteProgress, Volume, VolumeError, VolumeReadStream,
    WriteMode,
};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::types::{ArchiveSubkind, ExecutionStatus, Initiator, OperationId};

struct PausableSpoolStream {
    inner: Box<dyn VolumeReadStream>,
    state: Arc<WriteOperationState>,
}

impl VolumeReadStream for PausableSpoolStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.state.stop_or_park_async().await {
                return Some(Err(VolumeError::Cancelled("fresh ZIP upload cancelled".into())));
            }
            self.inner.next_chunk().await
        })
    }

    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }

    /// This operation's own Cancel and pause, for a destination that buffers
    /// ahead of this stream (S3).
    fn stop_signal(&self) -> cmdr_fs::volume::ScanStop {
        cmdr_fs::volume::ScanStop::new(Arc::clone(&self.state) as Arc<dyn cmdr_fs::volume::ScanStopSignal>)
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.inner.modified_at()
    }

    /// The source's own metadata, so a destination that stores it keeps it.
    fn metadata(&self, wanted: cmdr_fs::metadata::MetadataClasses) -> cmdr_fs::metadata::CarriedMetadata {
        self.inner.metadata(wanted)
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the managed-operation seam carries both endpoints, lifecycle settings, and journal provenance"
)]
pub(super) async fn start(
    engine: &Operations,
    events: Arc<dyn OperationEventSink>,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    archive_path: PathBuf,
    parent_volume_id: String,
    conflict: ConflictResolution,
    progress_interval_ms: u64,
    compression_level: Option<i64>,
    preview_id: Option<String>,
    net_new: bool,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Plain `get`: a new `.zip` lands as a plain file on its parent drive.
    let (dest_volume, dest_lane) = match engine.host().volumes().get(&parent_volume_id) {
        Some(volume) => {
            let lane = volume.lane_key();
            (volume, lane)
        }
        None => (
            Arc::new(LocalPosixVolume::local_folder(
                "local",
                PathBuf::from("/"),
                engine.host().volume_host().clone(),
            )) as Arc<dyn Volume>,
            LaneKey::new(parent_volume_id.clone()),
        ),
    };
    if !dest_volume.is_writable() {
        return Err(WriteOperationError::ReadOnlyDevice {
            path: archive_path.display().to_string(),
            device_name: Some(dest_volume.name().to_string()),
            side: super::super::types::ReadOnlySide::Destination,
        });
    }
    validate_aliases(
        &engine.host().runtime(),
        &source_volume,
        &source_paths,
        &dest_volume,
        &archive_path,
    )
    .await?;

    let operation_id = OperationId::from(cmdr_operation_log::new_operation_id());
    let registered_source = engine
        .host()
        .volumes()
        .find_by_root(source_volume.root())
        .filter(|(_, registered)| {
            registered.root() == source_volume.root() && registered.lane_key() == source_volume.lane_key()
        });
    let source_volume_id = registered_source
        .as_ref()
        .map_or_else(|| source_volume.lane_key().to_string(), |(id, _)| id.clone());
    let state = Arc::new(
        engine
            .new_operation_state(Duration::from_millis(progress_interval_ms))
            .with_journal_volumes(source_volume_id.clone(), parent_volume_id.clone()),
    );
    let mut lanes = vec![source_volume.lane_key(), dest_lane];
    lanes.dedup();
    let mut volume_ids = vec![parent_volume_id.clone()];
    if let Some((source_id, _)) = registered_source {
        volume_ids.push(source_id);
    }
    volume_ids.sort();
    volume_ids.dedup();
    let summary_source = source_paths
        .first()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Compress,
        lanes,
        volume_ids,
        summary: OperationSummaryText {
            source: summary_source,
            destination: Some(archive_path.display().to_string()),
            // The archive is where the items go, so the row reads "→ ~/x.zip".
            items: Some(volume_items(
                &*source_volume,
                &source_paths,
                None,
                Some((&*dest_volume, &archive_path)),
            )),
        },
        // A new archive is this op's own file, so a rollback removes it; compressing
        // over an existing one is the same all-or-nothing rewrite as any zip edit.
        rollback_support: if net_new {
            crate::types::RollbackSupport::Supported
        } else {
            crate::types::RollbackSupport::ArchiveRewrite
        },
        preview_id,
        reverses: None,
    };

    let op_id_for_task = operation_id.clone();
    let state_for_task = Arc::clone(&state);
    let events_for_task = Arc::clone(&events);
    let deferred = move |ops: Operations| {
        Box::pin(async move {
            let op_id = op_id_for_task;
            let state = state_for_task;
            let events = events_for_task;
            let task_guard = ManagedTaskGuard::new(ops.manager(), op_id.clone());
            let settle_volume = (parent_volume_id != "root").then(|| parent_volume_id.clone());
            let _settled = WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.to_string(),
                WriteOperationType::Compress,
                settle_volume,
            );

            if super::super::scan_bridge::await_claimed_preview(
                &ops,
                &*events,
                &op_id,
                WriteOperationType::Compress,
                &state,
            )
            .await
            .stopped()
            {
                task_guard.disarm();
                ops.manager().on_settled(&op_id);
                return;
            }

            super::super::journal::open_compress_op(
                state.scope(),
                op_id.as_str(),
                initiator,
                &source_volume_id,
                &parent_volume_id,
                source_paths.len() as u64,
            );
            let hooks = Arc::new(MutatorHooks::new(
                Arc::clone(&state),
                Arc::clone(&events),
                op_id.to_string(),
                WriteOperationType::Compress,
                Duration::from_millis(progress_interval_ms),
            ));
            let outcome = run(
                Arc::clone(&source_volume),
                source_paths,
                Arc::clone(&dest_volume),
                archive_path.clone(),
                !net_new,
                conflict,
                Arc::clone(&state),
                Arc::clone(&hooks),
                compression_level,
                events.as_ref(),
                op_id.as_str(),
            )
            .await;
            let execution_status = match &outcome {
                Ok(_) => ExecutionStatus::Done,
                Err(EditError::Cancelled) => ExecutionStatus::Canceled,
                Err(EditError::Op(_)) => ExecutionStatus::Failed,
            };
            let skipped = outcome.as_ref().map_or(0, |count| *count);
            let final_progress = hooks.latest_progress();
            if execution_status == ExecutionStatus::Done {
                if let (Some(parent), Some(name)) = (archive_path.parent(), archive_path.file_name()) {
                    dest_volume
                        .notify_mutation(
                            &parent_volume_id,
                            parent,
                            if net_new {
                                MutationEvent::Created(name.to_string_lossy().into_owned())
                            } else {
                                MutationEvent::Modified(name.to_string_lossy().into_owned())
                            },
                        )
                        .await;
                }
                let meta = dest_volume.get_metadata(&archive_path).await.ok();
                super::super::journal::record_compress_archive(
                    state.scope(),
                    &parent_volume_id,
                    &archive_path,
                    meta.as_ref()
                        .and_then(|entry| entry.size)
                        .and_then(|size| i64::try_from(size).ok()),
                    meta.as_ref()
                        .and_then(|entry| entry.modified_at)
                        .and_then(|time| i64::try_from(time).ok()),
                    net_new,
                );
            }
            super::super::journal::finalize_archive_op(
                state.scope(),
                ArchiveSubkind::Compress,
                net_new,
                execution_status,
                Some(super::super::journal::PackedTotals {
                    entries: final_progress.entries_total as u64,
                    entries_done: final_progress.entries_done as u64,
                    source_bytes: final_progress.bytes_total,
                }),
            );
            emit_archive_terminal(
                events.as_ref(),
                op_id.as_str(),
                WriteOperationType::Compress,
                outcome.map(|_| ()),
                skipped,
                EditLeftovers::of(&state, &parent_volume_id, None),
                &final_progress,
            );
            task_guard.disarm();
            ops.manager().on_settled(&op_id);
        }) as Pin<Box<dyn Future<Output = ()> + Send>>
    };
    engine.manager().spawn_managed(descriptor, state, Box::new(deferred));
    Ok(WriteOperationStartResult {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Compress,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the coordinator owns both endpoints, lifecycle/progress, conflict dispatch, and producer settings"
)]
async fn run(
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume: Arc<dyn Volume>,
    archive_path: PathBuf,
    replace_existing: bool,
    conflict: ConflictResolution,
    state: Arc<WriteOperationState>,
    hooks: Arc<MutatorHooks>,
    compression_level: Option<i64>,
    events: &dyn OperationEventSink,
    operation_id: &str,
) -> Result<usize, EditError> {
    let report_walk = |tick: PlanProgress<'_>| hooks.emit_scan_progress(tick, Some(tick.current_dir), false);
    let plan = plan_sources(
        &source_volume,
        &source_paths,
        conflict,
        &state,
        events,
        operation_id,
        &archive_path,
        &report_walk,
    )
    .await?;
    check_entry_names(&plan).map_err(EditError::Op)?;
    // The plan's entries are the compress's rows: entry names are already
    // relative to the selection's parent. Each entry's row pairs with the
    // entry index `produce` reports (`CompressRows`).
    // A tree bug builds none: no rows, and the compress runs as it would without them.
    let (tree, entry_rows) = from_archive_plan(
        plan.entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.is_directory, entry.size)),
        &TreeOrder::active(state.scope().host()),
    )
    .unzip();
    state.transfer_tree.install(tree, TreeLanding::nowhere());
    let rows = CompressRows::new(
        state.transfer_tree.reporter(),
        entry_rows
            .unwrap_or_default()
            .into_iter()
            .zip(plan.entries.iter().map(|entry| entry.is_directory))
            .collect(),
    );
    let planned_dirs = plan.entries.iter().filter(|entry| entry.is_directory).count();
    hooks.emit_scan_progress(
        PlanProgress {
            files: plan.entries.len() - planned_dirs,
            dirs: planned_dirs,
            bytes: plan.source_bytes,
            current_dir: &archive_path,
        },
        None,
        true,
    );
    let direct = matches!(
        dest_volume.backend_kind(),
        BackendKind::Local | BackendKind::Smb | BackendKind::Sftp | BackendKind::Adb
    ) && dest_volume.supports_unknown_length_writes();
    if !direct {
        hooks.number_steps_as_zip_then_upload();
    }
    let runs = ProducerRuns {
        state: Arc::clone(&state),
        hooks: Arc::clone(&hooks),
        rows: Arc::new(rows),
        total_entries: plan.entries.len(),
        total_bytes: plan.source_bytes,
        level: compression_level,
    };
    MutationHooks::on_progress(
        &*hooks,
        MutationProgress {
            entries_total: runs.total_entries,
            entries_changed: runs.total_entries,
            bytes_total: runs.total_bytes,
            ..Default::default()
        },
    );

    let target = Target {
        volume: dest_volume,
        archive_path,
        replace_existing,
    };
    if direct {
        produce_direct(&plan, &target, &state, &runs).await?;
    } else {
        produce_via_spool(&plan, &target, &state, &hooks, &runs).await?;
    }
    Ok(plan.skipped)
}

/// Generates the archive straight into a tracked stage on the destination,
/// validates it there, and publishes it.
///
/// A dropped share (the destination, or a remote source) is waited for: the
/// producer can't resume mid-stream, so once the share is back the stage goes
/// and the plan runs again into a fresh one. A publish the drop cut off settles
/// first ([`publish_stage`]), and runs the plan again only when nothing proves
/// it published.
async fn produce_direct(
    plan: &FreshPlan,
    target: &Target,
    state: &Arc<WriteOperationState>,
    runs: &ProducerRuns,
) -> Result<(), EditError> {
    let expected = ExpectedIndex::of_plan(plan);
    let dest_volume = &target.volume;
    let volumes = [&plan.source_volume, dest_volume];
    let mut outages = 0;
    loop {
        let started_on = StartedOn::now(&volumes);
        let stage = begin_stage(
            state,
            dest_volume.as_ref(),
            &target.archive_path,
            target.replace_existing,
        );
        let stage_path = stage.target().to_path_buf();
        let end = runs
            .produce_into(plan, Arc::clone(dest_volume), stage_path.clone())
            .await;
        let checked = match (&end.written, &end.produced) {
            (Ok(written), Ok(produced)) => {
                let written = *written;
                validate_stage(dest_volume, &stage_path, written, *produced, &expected)
                    .await
                    .map(|()| written)
            }
            _ => Err(pipeline_failure(end, &stage_path, state)),
        };
        let written = match checked {
            Ok(written) => written,
            Err(failed) => {
                let next = wait_out(state, failed, &volumes, &started_on, &mut outages, &target.archive_path).await;
                // The stage goes once the share can take the delete, so after
                // the wait; one it can't take stays recorded for the sweep.
                stage.abandon(dest_volume).await;
                match next {
                    Next::Again => continue,
                    Next::Stop(ended) => return Err(ended),
                }
            }
        };
        if state.stop_or_park_async().await {
            stage.abandon(dest_volume).await;
            return Err(EditError::Cancelled);
        }
        match publish_stage(stage, target, written, state, &mut outages).await {
            Published::Yes => return Ok(()),
            Published::RunAgain => continue,
            Published::Ended(ended) => return Err(ended),
        }
    }
}

/// Generates the archive into a private local spool, validates it there, then
/// uploads it to a tracked stage on the destination, validates that, and
/// publishes it.
///
/// A dropped remote source re-runs the plan into the spool. A dropped
/// destination uploads the spool again once it's back, ❌ without producing
/// the archive again; a publish the drop cut off settles first.
async fn produce_via_spool(
    plan: &FreshPlan,
    target: &Target,
    state: &Arc<WriteOperationState>,
    hooks: &Arc<MutatorHooks>,
    runs: &ProducerRuns,
) -> Result<(), EditError> {
    let expected = ExpectedIndex::of_plan(plan);
    let archive_path = &target.archive_path;
    let scratch = ScratchDir::new("cmdr-fresh-zip").map_err(|error| io_write_error(archive_path, error))?;
    let spool_path = PathBuf::from("archive.zip");
    let spool_full = scratch.path().join(&spool_path);
    let spool_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "fresh-zip-spool",
        scratch.path().to_path_buf(),
        state.scope().host().volume_host().clone(),
    ));
    let source_volumes = [&plan.source_volume];
    let mut outages = 0;
    let produced = loop {
        let started_on = StartedOn::now(&source_volumes);
        // A run after an outage starts on an empty spool: the producer writes
        // it `CreateNew`.
        if let Err(error) = std::fs::remove_file(&spool_full)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(io_write_error(&spool_full, error));
        }
        let end = runs
            .produce_into(plan, Arc::clone(&spool_volume), spool_path.clone())
            .await;
        let failed = match (&end.written, &end.produced) {
            (Ok(written), Ok(produced)) => {
                match validate_stage(&spool_volume, &spool_path, *written, *produced, &expected).await {
                    Ok(()) => break *produced,
                    Err(failed) => failed,
                }
            }
            _ => pipeline_failure(end, &spool_full, state),
        };
        match wait_out(state, failed, &source_volumes, &started_on, &mut outages, archive_path).await {
            Next::Again => {}
            Next::Stop(ended) => return Err(ended),
        }
    };

    let dest_volume = &target.volume;
    let dest_volumes = [dest_volume];
    loop {
        let started_on = StartedOn::now(&dest_volumes);
        let stage = begin_stage(state, dest_volume.as_ref(), archive_path, target.replace_existing);
        let stage_path = stage.target().to_path_buf();
        let checked = match upload_spool(
            &spool_volume,
            &spool_full,
            &spool_path,
            &stage,
            dest_volume,
            produced,
            state,
            hooks,
            runs,
        )
        .await
        {
            Ok(uploaded) => validate_stage(dest_volume, &stage_path, uploaded, produced, &expected)
                .await
                .map(|()| uploaded),
            Err(failed) => Err(failed),
        };
        let uploaded = match checked {
            Ok(uploaded) => uploaded,
            Err(failed) => {
                let next = wait_out(state, failed, &dest_volumes, &started_on, &mut outages, archive_path).await;
                stage.abandon(dest_volume).await;
                match next {
                    Next::Again => continue,
                    Next::Stop(ended) => return Err(ended),
                }
            }
        };
        if state.stop_or_park_async().await {
            stage.abandon(dest_volume).await;
            return Err(EditError::Cancelled);
        }
        match publish_stage(stage, target, uploaded, state, &mut outages).await {
            Published::Yes => return Ok(()),
            Published::RunAgain => continue,
            Published::Ended(ended) => return Err(ended),
        }
    }
}

/// Uploads the validated spool to the stage on its known-length axis.
#[allow(
    clippy::too_many_arguments,
    reason = "one upload of the spool: both ends, the stage, its size, and the progress it reports"
)]
async fn upload_spool(
    spool_volume: &Arc<dyn Volume>,
    spool_full: &Path,
    spool_path: &Path,
    stage: &StagedWrite,
    dest_volume: &Arc<dyn Volume>,
    produced: u64,
    state: &Arc<WriteOperationState>,
    hooks: &Arc<MutatorHooks>,
    runs: &ProducerRuns,
) -> Result<u64, StepError> {
    let stream = spool_volume
        .open_read_stream(spool_path)
        .await
        .map_err(|error| StepError::Ended(volume_read_error(spool_full, error)))?;
    let stream: Box<dyn VolumeReadStream> = Box::new(PausableSpoolStream {
        inner: stream,
        state: Arc::clone(state),
    });
    let cancellation = runs.cancellation();
    let transfer_observer = hooks.remote_progress_observer();
    transfer_observer(super::remote::RemoteCommitProgress::Transferring {
        bytes_done: 0,
        bytes_total: produced,
    });
    let callback = |tick: StreamWriteProgress| {
        if tick.bytes_written >= produced {
            transfer_observer(super::remote::RemoteCommitProgress::Finishing);
        } else {
            transfer_observer(super::remote::RemoteCommitProgress::Transferring {
                bytes_done: tick.bytes_written,
                bytes_total: produced,
            });
        }
        continue_unless(&cancellation)
    };
    let stage_path = stage.target();
    match dest_volume
        .write_from_stream(
            stage_path,
            WriteMode::CreateNew,
            StreamLength::Known(produced),
            stream,
            &callback,
        )
        .await
    {
        Ok(written) => {
            transfer_observer(super::remote::RemoteCommitProgress::Finishing);
            Ok(written.bytes)
        }
        // Classify without parking: a paused op whose upload failed reports
        // that failure now, not after someone presses Resume.
        Err(_) if operation_cancelled(state) => Err(StepError::Ended(EditError::Cancelled)),
        Err(error) => Err(StepError::fault(error.clone(), volume_write_error(stage_path, error))),
    }
}

/// Stages the archive on its destination, announcing the archive and its temp
/// before either exists.
fn begin_stage(
    state: &Arc<WriteOperationState>,
    dest_volume: &dyn Volume,
    archive_path: &Path,
    replace_existing: bool,
) -> StagedWrite {
    let archive = state.scope().host().announce_on(dest_volume, archive_path);
    StagedWrite::begin_generated(state, dest_volume, archive, replace_existing)
}

fn volume_read_error(path: &Path, error: VolumeError) -> EditError {
    EditError::Op(WriteOperationError::ReadError {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

fn io_write_error(path: &Path, error: std::io::Error) -> EditError {
    EditError::Op(super::super::error_classification::classify_io_error(
        &error,
        path.display().to_string(),
    ))
}

#[cfg(test)]
#[path = "fresh_compress_outage_tests.rs"]
mod outage_tests;
#[cfg(test)]
#[path = "fresh_compress_tests.rs"]
mod tests;
