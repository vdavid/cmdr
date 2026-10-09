//! Managed fresh-archive creation: plan source entries, stream one seedless ZIP
//! producer into a tracked destination stage, validate it, then publish it.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cmdr_archive::mutator::{MutationHooks, MutationProgress};

use super::super::OperationEventSink;
use super::super::manager::{self, ManagedTaskGuard, OperationDescriptor, OperationPaths, OperationSummaryText};
use super::super::scratch_dir::ScratchDir;
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::transfer::StagedWrite;
use super::super::types::{ConflictResolution, WriteOperationError, WriteOperationStartResult, WriteOperationType};
use super::edit_error::EditError;
use super::engine::{MutatorHooks, emit_archive_terminal};
use super::fresh_plan::{FreshPlan, PlanProgress, RemoteFeed, plan_sources, validate_aliases};
use super::fresh_validate::{ExpectedIndex, check_entry_names, validate_stage};
use super::fresh_zip::{FreshZipCancellation, FreshZipError, FreshZipProgressObserver, spawn_fresh_zip_with_progress};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{
    BackendKind, LaneKey, LocalPosixVolume, MutationEvent, StreamLength, StreamWriteProgress, Volume, VolumeError,
    VolumeReadStream, WriteMode,
};
use crate::operation_log::types::{ArchiveSubkind, ExecutionStatus, Initiator};

struct PausableSpoolStream {
    inner: Box<dyn VolumeReadStream>,
    state: Arc<WriteOperationState>,
}

impl VolumeReadStream for PausableSpoolStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            if self.state.stop_or_park_async().await {
                return Some(Err(VolumeError::Cancelled("fresh ZIP upload cancelled".to_string())));
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
    fn stop_signal(&self) -> crate::file_system::volume::ScanStop {
        crate::file_system::volume::ScanStop::new(
            Arc::clone(&self.state) as Arc<dyn crate::file_system::volume::ScanStopSignal>
        )
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.inner.modified_at()
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the managed-operation seam carries both endpoints, lifecycle settings, and journal provenance"
)]
pub(super) async fn start(
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
    let (dest_volume, dest_lane) = match get_volume_manager().get(&parent_volume_id) {
        Some(volume) => {
            let lane = volume.lane_key();
            (volume, lane)
        }
        None => (
            Arc::new(LocalPosixVolume::local_folder("local", PathBuf::from("/"))) as Arc<dyn Volume>,
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
    validate_aliases(&source_volume, &source_paths, &dest_volume, &archive_path).await?;

    let operation_id = crate::operation_log::new_operation_id();
    let registered_source = get_volume_manager()
        .find_by_root(source_volume.root())
        .filter(|(_, registered)| {
            registered.root() == source_volume.root() && registered.lane_key() == source_volume.lane_key()
        });
    let source_volume_id = registered_source
        .as_ref()
        .map_or_else(|| source_volume.lane_key().to_string(), |(id, _)| id.clone());
    let state = Arc::new(
        WriteOperationState::new(Duration::from_millis(progress_interval_ms))
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
            paths: OperationPaths::from_paths(&source_paths, Some(archive_path.as_path())).on_volumes(
                OperationPaths::volume_label(source_volume.as_ref()),
                OperationPaths::volume_label(dest_volume.as_ref()),
            ),
        },
        supports_rollback: net_new,
        preview_id,
        reverses: None,
    };

    let op_id_for_task = operation_id.clone();
    let state_for_task = Arc::clone(&state);
    let events_for_task = Arc::clone(&events);
    let deferred = move || {
        Box::pin(async move {
            let op_id = op_id_for_task;
            let state = state_for_task;
            let events = events_for_task;
            let task_guard = ManagedTaskGuard::new(op_id.clone());
            let settle_volume = (parent_volume_id != "root").then(|| parent_volume_id.clone());
            let _settled = WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::Compress,
                settle_volume,
            );

            if super::super::scan_bridge::await_claimed_preview(&*events, &op_id, WriteOperationType::Compress, &state)
                .await
                .stopped()
            {
                task_guard.disarm();
                manager::manager().on_settled(&op_id);
                return;
            }

            super::super::journal::open_compress_op(
                &op_id,
                initiator,
                &source_volume_id,
                &parent_volume_id,
                source_paths.len() as u64,
            );
            let hooks = Arc::new(MutatorHooks::new(
                Arc::clone(&state),
                Arc::clone(&events),
                op_id.clone(),
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
                &op_id,
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
                    &op_id,
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
                &op_id,
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
                &op_id,
                WriteOperationType::Compress,
                outcome.map(|_| ()),
                skipped,
                None,
                &final_progress,
            );
            task_guard.disarm();
            manager::manager().on_settled(&op_id);
        }) as Pin<Box<dyn Future<Output = ()> + Send>>
    };
    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));
    Ok(WriteOperationStartResult {
        operation_id,
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
    let total_entries = plan.entries.len();
    let total_bytes = plan.source_bytes;
    let direct = matches!(
        dest_volume.backend_kind(),
        BackendKind::Local | BackendKind::Smb | BackendKind::Sftp | BackendKind::Adb
    ) && dest_volume.supports_unknown_length_writes();
    if !direct {
        hooks.number_steps_as_zip_then_upload();
    }
    MutationHooks::on_progress(
        &*hooks,
        MutationProgress {
            entries_total: total_entries,
            entries_changed: total_entries,
            bytes_total: total_bytes,
            ..Default::default()
        },
    );
    let hooks_for_progress = Arc::clone(&hooks);
    let state_for_progress = Arc::clone(&state);
    let state_for_wake = Arc::clone(&state);
    // One source for every stop: the op's tier-1 cancel plus the pipeline's own
    // internal requests. ❌ Never race a write against it; it reaches the backend
    // through the write callback, so the backend removes its own partial.
    let cancellation = FreshZipCancellation::for_operation(
        &state.backend_cancel,
        Some(Arc::new(move || state_for_wake.pause_gate.wake())),
    );
    let cancellation_for_progress = cancellation.clone();
    let progress: FreshZipProgressObserver = Arc::new(move |tick| {
        MutationHooks::on_progress(
            &*hooks_for_progress,
            MutationProgress {
                entries_done: tick.entries_done,
                entries_total: total_entries,
                entries_changed: total_entries,
                bytes_done: tick.source_bytes_done,
                bytes_total: total_bytes,
            },
        );
        state_for_progress
            .pause_gate
            .wait_while_paused_sync_until(&|| cancellation_for_progress.is_requested());
        cancellation_for_progress.is_requested()
    });

    let skipped = plan.skipped;
    if direct {
        produce_direct(
            plan,
            dest_volume,
            archive_path,
            replace_existing,
            state,
            compression_level,
            progress,
            cancellation,
        )
        .await?;
    } else {
        produce_via_spool(
            plan,
            dest_volume,
            archive_path,
            replace_existing,
            state,
            hooks,
            compression_level,
            progress,
            cancellation,
        )
        .await?;
    }
    Ok(skipped)
}

#[allow(
    clippy::too_many_arguments,
    reason = "direct generation joins one producer to one staged destination and carries its lifecycle controls"
)]
async fn produce_direct(
    plan: FreshPlan,
    dest_volume: Arc<dyn Volume>,
    archive_path: PathBuf,
    replace_existing: bool,
    state: Arc<WriteOperationState>,
    level: Option<i64>,
    progress: FreshZipProgressObserver,
    cancellation: FreshZipCancellation,
) -> Result<(), EditError> {
    let expected = ExpectedIndex::of_plan(&plan);
    let stage = StagedWrite::begin_generated(&state, &archive_path, replace_existing);
    let stage_path = stage.target().to_path_buf();
    let (written, produced) = produce_into(
        plan,
        Arc::clone(&dest_volume),
        stage_path.clone(),
        level,
        progress,
        cancellation,
    )
    .await;
    let (written, produced) = match (written, produced) {
        (Ok(written), Ok(produced)) => (written, produced),
        (write, producer) => {
            stage.abandon(&dest_volume).await;
            return Err(prefer_pipeline_error(
                write,
                producer,
                &stage_path,
                operation_cancelled(&state),
            ));
        }
    };
    if let Err(error) = validate_stage(&dest_volume, &stage_path, written, produced, &expected).await {
        stage.abandon(&dest_volume).await;
        return Err(error);
    }
    if state.stop_or_park_async().await {
        stage.abandon(&dest_volume).await;
        return Err(EditError::Cancelled);
    }
    publish_stage(stage, &dest_volume, &archive_path, replace_existing).await?;
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "fallback shares the direct producer controls and additionally reports its staged upload axis"
)]
async fn produce_via_spool(
    plan: FreshPlan,
    dest_volume: Arc<dyn Volume>,
    archive_path: PathBuf,
    replace_existing: bool,
    state: Arc<WriteOperationState>,
    hooks: Arc<MutatorHooks>,
    level: Option<i64>,
    progress: FreshZipProgressObserver,
    cancellation: FreshZipCancellation,
) -> Result<(), EditError> {
    let expected = ExpectedIndex::of_plan(&plan);
    let scratch = ScratchDir::new("cmdr-fresh-zip").map_err(|error| io_write_error(&archive_path, error))?;
    let spool_path = PathBuf::from("archive.zip");
    let spool_full = scratch.path().join(&spool_path);
    let spool_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "fresh-zip-spool",
        scratch.path().to_path_buf(),
    ));
    let (written, produced) = produce_into(
        plan,
        Arc::clone(&spool_volume),
        spool_path.clone(),
        level,
        progress,
        cancellation.clone(),
    )
    .await;
    let (written, produced) = match (written, produced) {
        (Ok(written), Ok(produced)) => (written, produced),
        (write, producer) => {
            return Err(prefer_pipeline_error(
                write,
                producer,
                &spool_full,
                operation_cancelled(&state),
            ));
        }
    };
    validate_stage(&spool_volume, &spool_path, written, produced, &expected).await?;

    let stage = StagedWrite::begin_generated(&state, &archive_path, replace_existing);
    let stage_path = stage.target().to_path_buf();
    let stream = spool_volume
        .open_read_stream(&spool_path)
        .await
        .map_err(|error| volume_read_error(&spool_full, error))?;
    let stream: Box<dyn VolumeReadStream> = Box::new(PausableSpoolStream {
        inner: stream,
        state: Arc::clone(&state),
    });
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
    let uploaded = dest_volume
        .write_from_stream(
            &stage_path,
            WriteMode::CreateNew,
            StreamLength::Known(produced),
            stream,
            &callback,
        )
        .await;
    let uploaded = match uploaded {
        Ok(bytes) => bytes,
        Err(error) => {
            stage.abandon(&dest_volume).await;
            // Classify without parking: a paused op whose upload failed reports
            // that failure now, not after someone presses Resume.
            if operation_cancelled(&state) {
                return Err(EditError::Cancelled);
            }
            return Err(volume_write_error(&stage_path, error));
        }
    };
    transfer_observer(super::remote::RemoteCommitProgress::Finishing);
    if let Err(error) = validate_stage(&dest_volume, &stage_path, uploaded, produced, &expected).await {
        stage.abandon(&dest_volume).await;
        return Err(error);
    }
    if state.stop_or_park_async().await {
        stage.abandon(&dest_volume).await;
        return Err(EditError::Cancelled);
    }
    publish_stage(stage, &dest_volume, &archive_path, replace_existing).await?;
    Ok(())
}

async fn publish_stage(
    stage: StagedWrite,
    dest_volume: &Arc<dyn Volume>,
    archive_path: &Path,
    replace_existing: bool,
) -> Result<(), EditError> {
    if !replace_existing {
        stage.commit(dest_volume).await.map_err(|failure| {
            EditError::Op(super::super::transfer::volume::map_finalize_failure(
                archive_path,
                failure,
            ))
        })?;
    } else if dest_volume.supports_atomic_replace_rename() {
        stage
            .commit_atomic_replace(dest_volume)
            .await
            .map_err(|error| volume_write_error(archive_path, error))?;
    } else {
        stage
            .commit_with_displaced_original(dest_volume)
            .await
            .map_err(EditError::Op)?;
    }
    Ok(())
}

/// Breaks a destination write at its next acknowledgement once the pipeline's
/// one cancellation source fires; that callback is how SFTP, SMB, and ADB hear a
/// cancel mid-write.
fn continue_unless(cancellation: &FreshZipCancellation) -> ControlFlow<()> {
    if cancellation.is_requested() {
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
}

/// Whether a PERSON (or the quit gate) stopped this op, as opposed to the
/// pipeline stopping itself. Only this makes a failure read as a cancel.
fn operation_cancelled(state: &WriteOperationState) -> bool {
    super::super::state::is_cancelled(&state.intent)
}

/// Runs one producer into one unknown-length destination write and joins every
/// participant: the writer, the producer thread, and the remote feed task.
async fn produce_into(
    plan: FreshPlan,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    level: Option<i64>,
    progress: FreshZipProgressObserver,
    cancellation: FreshZipCancellation,
) -> (Result<u64, VolumeError>, Result<u64, FreshZipError>) {
    let source_volume = Arc::clone(&plan.source_volume);
    let output = match spawn_fresh_zip_with_progress(plan.entries, level, Some(progress), cancellation.clone()) {
        Ok(output) => output,
        Err(error) => return (Err(VolumeError::NotSupported), Err(error)),
    };
    let (stream, completion) = output.into_parts();
    let feed_task = tokio::spawn(feed_remote(plan.remote_feeds, source_volume, cancellation.clone()));
    let callback = |_tick: StreamWriteProgress| continue_unless(&cancellation);
    let written = dest_volume
        .write_from_stream(
            &dest_path,
            WriteMode::CreateNew,
            StreamLength::Unknown,
            Box::new(stream),
            &callback,
        )
        .await;
    let produced = completion.finish().await;
    if produced.is_err() {
        // A producer that ended early leaves the feed nothing to feed; stop a
        // source read that may never answer instead of awaiting it.
        cancellation.request();
    }
    let feed_result = feed_task
        .await
        .map_err(|_| FreshZipError::ProducerPanicked)
        .and_then(|result| result);
    let produced = match (produced, feed_result) {
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    (written, produced)
}

/// Feeds each remote entry's bytes through its bridge, one entry at a time.
///
/// Every source read races the pipeline's cancellation: a `ChannelReadStream`
/// read is a queue wait, and dropping the stream is the backend's documented
/// stop. Opening a stream is one protocol round trip and is allowed to finish,
/// as the copy engine does. Returning drops every remaining feeder, which is
/// what wakes a producer parked on the bridge.
async fn feed_remote(
    feeds: Vec<RemoteFeed>,
    source_volume: Arc<dyn Volume>,
    cancellation: FreshZipCancellation,
) -> Result<(), FreshZipError> {
    for feed in feeds {
        if cancellation.is_requested() {
            return Err(FreshZipError::Cancelled);
        }
        let mut stream = match source_volume.open_read_stream(&feed.path).await {
            Ok(stream) => stream,
            Err(error) => {
                let source_error = FreshZipError::Source {
                    entry: feed.entry_name.clone(),
                    message: error.to_string(),
                };
                feed.feeder.send(Err(source_error.clone()), &cancellation).await?;
                return Err(source_error);
            }
        };
        loop {
            let chunk = tokio::select! {
                biased;
                () = cancellation.cancelled() => return Err(FreshZipError::Cancelled),
                chunk = stream.next_chunk() => chunk,
            };
            let Some(chunk) = chunk else { break };
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(error) => {
                    let source_error = FreshZipError::Source {
                        entry: feed.entry_name.clone(),
                        message: error.to_string(),
                    };
                    feed.feeder.send(Err(source_error.clone()), &cancellation).await?;
                    return Err(source_error);
                }
            };
            feed.feeder.send(Ok(bytes), &cancellation).await?;
        }
        feed.feeder.finish(&cancellation).await?;
    }
    Ok(())
}

/// Names the CAUSAL failure of a pipeline that didn't finish. A genuine producer
/// failure (source read, ZIP, count drift) wins, then a genuine destination
/// failure. What's left is derivative: one side stopped because the other did.
/// That reads as a cancel only when the operation itself was cancelled; a writer
/// that quit early on its own is a write failure, never a silent user cancel.
fn prefer_pipeline_error(
    writer: Result<u64, VolumeError>,
    producer: Result<u64, FreshZipError>,
    path: &Path,
    operation_cancelled: bool,
) -> EditError {
    let derivative = |error: &FreshZipError| matches!(error, FreshZipError::Cancelled | FreshZipError::OutputClosed);
    match (writer, producer) {
        (_, Err(error)) if !derivative(&error) => fresh_error(path, error),
        (Err(error), _) if !matches!(error, VolumeError::Cancelled(_)) => volume_write_error(path, error),
        _ if operation_cancelled => EditError::Cancelled,
        _ => EditError::Op(WriteOperationError::WriteError {
            path: path.display().to_string(),
            message: "the ZIP pipeline stopped before the archive was complete".to_string(),
        }),
    }
}

fn fresh_error(path: &Path, error: FreshZipError) -> EditError {
    match error {
        FreshZipError::Source { entry, message } => {
            EditError::Op(WriteOperationError::ReadError { path: entry, message })
        }
        other => EditError::Op(WriteOperationError::WriteError {
            path: path.display().to_string(),
            message: format!("{other:?}"),
        }),
    }
}

fn volume_read_error(path: &Path, error: VolumeError) -> EditError {
    EditError::Op(WriteOperationError::ReadError {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

fn volume_write_error(path: &Path, error: VolumeError) -> EditError {
    EditError::Op(WriteOperationError::WriteError {
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
#[path = "fresh_compress_tests.rs"]
mod tests;
