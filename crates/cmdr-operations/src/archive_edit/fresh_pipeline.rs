//! One fresh-ZIP pipeline run: the producer streaming into one unknown-length
//! destination write, the remote feed beside them, and the join that names how
//! each participant ended. Plus what every run of one compress shares
//! ([`ProducerRuns`]), and how a run that didn't finish is named.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_archive::mutator::{MutationHooks, MutationProgress};

use super::super::state::WriteOperationState;
use super::super::types::WriteOperationError;
use super::edit_error::EditError;
use super::engine::MutatorHooks;
use super::fresh_plan::{FreshPlan, RemoteFeed};
use super::fresh_rows::CompressRows;
use super::fresh_zip::{FreshZipCancellation, FreshZipError, FreshZipProgressObserver, spawn_fresh_zip_with_progress};
use super::outage_wait::StepError;
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, Volume, VolumeError, WriteMode};
use tokio::runtime::Handle;

/// How one pipeline run ended, participant by participant.
pub(super) struct PipelineEnd {
    /// The destination write: the bytes it took.
    pub(super) written: Result<u64, VolumeError>,
    /// The producer, joined with the remote feed: the bytes it made.
    pub(super) produced: Result<u64, FreshZipError>,
    /// What the source VOLUME said when a remote feed stopped on its error,
    /// typed, so the compress can tell a dropped share it can wait for from
    /// any other source failure (the producer's error only words it).
    pub(super) source_fault: Option<VolumeError>,
}

/// Runs one producer over `plan` into one unknown-length destination write and
/// joins every participant: the writer, the producer thread, and the remote
/// feed task. The feed and the producer's join run on `runtime` (the
/// engine's). The plan stays the caller's, for another run.
pub(super) async fn produce_into(
    runtime: &Handle,
    plan: &FreshPlan,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    level: Option<i64>,
    progress: FreshZipProgressObserver,
    cancellation: FreshZipCancellation,
) -> PipelineEnd {
    let source_volume = Arc::clone(&plan.source_volume);
    let (entries, remote_feeds) = plan.bridged();
    let output = match spawn_fresh_zip_with_progress(entries, level, Some(progress), cancellation.clone()) {
        Ok(output) => output,
        Err(error) => {
            return PipelineEnd {
                written: Err(VolumeError::NotSupported),
                produced: Err(error),
                source_fault: None,
            };
        }
    };
    let (stream, completion) = output.into_parts();
    let feed_task = runtime.spawn(feed_remote(remote_feeds, source_volume, cancellation.clone()));
    let callback = |_tick: StreamWriteProgress| continue_unless(&cancellation);
    let written = dest_volume
        .write_from_stream(
            &dest_path,
            WriteMode::CreateNew,
            StreamLength::Unknown,
            Box::new(stream),
            &callback,
        )
        .await
        .map(|written| written.bytes);
    let produced = completion.finish(runtime).await;
    if produced.is_err() {
        // A producer that ended early leaves the feed nothing to feed; stop a
        // source read that may never answer instead of awaiting it.
        cancellation.request();
    }
    let (feed_result, source_fault) = match feed_task.await {
        Ok(Ok(())) => (Ok(()), None),
        Ok(Err(stop)) => (Err(stop.error), stop.source_fault),
        Err(_) => (Err(FreshZipError::ProducerPanicked), None),
    };
    let produced = match (produced, feed_result) {
        (Ok(bytes), Ok(())) => Ok(bytes),
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    PipelineEnd {
        written,
        produced,
        source_fault,
    }
}

/// Why the remote feed stopped: the producer's error, and the source volume's
/// own when that's what stopped it.
struct FeedStop {
    error: FreshZipError,
    source_fault: Option<VolumeError>,
}

impl From<FreshZipError> for FeedStop {
    fn from(error: FreshZipError) -> Self {
        Self {
            error,
            source_fault: None,
        }
    }
}

/// Breaks a destination write at its next acknowledgement once the pipeline's
/// one cancellation source fires; that callback is how SFTP, SMB, and ADB hear a
/// cancel mid-write.
pub(super) fn continue_unless(cancellation: &FreshZipCancellation) -> ControlFlow<()> {
    if cancellation.is_requested() {
        ControlFlow::Break(())
    } else {
        ControlFlow::Continue(())
    }
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
) -> Result<(), FeedStop> {
    for feed in feeds {
        if cancellation.is_requested() {
            return Err(FreshZipError::Cancelled.into());
        }
        let mut stream = match source_volume.open_read_stream(&feed.path).await {
            Ok(stream) => stream,
            Err(error) => return Err(source_failed(&feed, error, &cancellation).await),
        };
        loop {
            let chunk = tokio::select! {
                biased;
                () = cancellation.cancelled() => return Err(FreshZipError::Cancelled.into()),
                chunk = stream.next_chunk() => chunk,
            };
            let Some(chunk) = chunk else { break };
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(error) => return Err(source_failed(&feed, error, &cancellation).await),
            };
            feed.feeder.send(Ok(bytes), &cancellation).await?;
        }
        feed.feeder.finish(&cancellation).await?;
    }
    Ok(())
}

/// Tells the producer its remote source failed, and keeps the volume's own
/// error for the compress.
async fn source_failed(feed: &RemoteFeed, error: VolumeError, cancellation: &FreshZipCancellation) -> FeedStop {
    let source_error = FreshZipError::Source {
        entry: feed.entry_name.clone(),
        message: error.to_string(),
    };
    if let Err(stopped) = feed.feeder.send(Err(source_error.clone()), cancellation).await {
        return stopped.into();
    }
    FeedStop {
        error: source_error,
        source_fault: Some(error),
    }
}

/// What every producer run of one compress shares. A run that a dropped share
/// cut off runs again from the plan, so each run gets its own cancellation
/// source (a pipeline that stopped requests its own) and the progress
/// observer that watches it ([`controls`](Self::controls)).
pub(super) struct ProducerRuns {
    pub(super) state: Arc<WriteOperationState>,
    pub(super) hooks: Arc<MutatorHooks>,
    pub(super) rows: Arc<CompressRows>,
    pub(super) total_entries: usize,
    pub(super) total_bytes: u64,
    pub(super) level: Option<i64>,
}

impl ProducerRuns {
    /// One run's stop signal, and the progress observer that reports its ticks.
    ///
    /// One source for every stop: the op's tier-1 cancel plus the pipeline's
    /// own internal requests. ❌ Never race a write against it; it reaches the
    /// backend through the write callback, so the backend removes its own
    /// partial.
    fn controls(&self) -> (FreshZipCancellation, FreshZipProgressObserver) {
        let cancellation = self.cancellation();
        let hooks = Arc::clone(&self.hooks);
        let state = Arc::clone(&self.state);
        let rows = Arc::clone(&self.rows);
        let (total_entries, total_bytes) = (self.total_entries, self.total_bytes);
        let watched = cancellation.clone();
        let progress: FreshZipProgressObserver = Arc::new(move |tick| {
            rows.on_tick(tick);
            MutationHooks::on_progress(
                &*hooks,
                MutationProgress {
                    entries_done: tick.entries_done,
                    entries_total: total_entries,
                    entries_changed: total_entries,
                    bytes_done: tick.source_bytes_done,
                    bytes_total: total_bytes,
                },
            );
            state
                .pause_gate
                .wait_while_paused_sync_until(&|| watched.is_requested());
            watched.is_requested()
        });
        (cancellation, progress)
    }

    pub(super) fn cancellation(&self) -> FreshZipCancellation {
        let state = Arc::clone(&self.state);
        FreshZipCancellation::for_operation(
            &self.state.backend_cancel,
            Some(Arc::new(move || state.pause_gate.wake())),
        )
    }

    /// One producer run over `plan` into `path` on `volume`.
    pub(super) async fn produce_into(&self, plan: &FreshPlan, volume: Arc<dyn Volume>, path: PathBuf) -> PipelineEnd {
        let (cancellation, progress) = self.controls();
        produce_into(
            &self.state.scope().host().runtime(),
            plan,
            volume,
            path,
            self.level,
            progress,
            cancellation,
        )
        .await
    }
}

/// A pipeline run that didn't finish, as a step's failure: its causal failure
/// (`prefer_pipeline_error`), carrying the volume error behind it when there
/// is one, the remote source's own or the destination write's, for the
/// compress's outage wait.
pub(super) fn pipeline_failure(end: PipelineEnd, path: &Path, state: &WriteOperationState) -> StepError {
    let PipelineEnd {
        written,
        produced,
        source_fault,
    } = end;
    let fault = match (&produced, &written) {
        (Err(error), _) if !is_derivative(error) => source_fault,
        (_, Err(error)) if !matches!(error, VolumeError::Cancelled(_)) => Some(error.clone()),
        _ => None,
    };
    let otherwise = prefer_pipeline_error(written, produced, path, operation_cancelled(state));
    match fault {
        Some(error) => StepError::fault(error, otherwise),
        None => StepError::Ended(otherwise),
    }
}

/// Whether a PERSON (or the quit gate) stopped this op, as opposed to the
/// pipeline stopping itself. Only this makes a failure read as a cancel.
pub(super) fn operation_cancelled(state: &WriteOperationState) -> bool {
    state.intent().is_cancelled()
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
    match (writer, producer) {
        (_, Err(error)) if !is_derivative(&error) => fresh_error(path, error),
        (Err(error), _) if !matches!(error, VolumeError::Cancelled(_)) => volume_write_error(path, error),
        _ if operation_cancelled => EditError::Cancelled,
        _ => EditError::Op(WriteOperationError::WriteError {
            path: path.display().to_string(),
            message: "the ZIP pipeline stopped before the archive was complete".into(),
        }),
    }
}

/// A producer failure that only says the other side stopped first.
fn is_derivative(error: &FreshZipError) -> bool {
    matches!(error, FreshZipError::Cancelled | FreshZipError::OutputClosed)
}

fn fresh_error(path: &Path, error: FreshZipError) -> EditError {
    match error {
        FreshZipError::Source { entry, message } => EditError::Op(WriteOperationError::ReadError {
            path: entry,
            message: message.into(),
        }),
        other => EditError::Op(WriteOperationError::WriteError {
            path: path.display().to_string(),
            message: format!("{other:?}").into(),
        }),
    }
}

pub(super) fn volume_write_error(path: &Path, error: VolumeError) -> EditError {
    EditError::Op(WriteOperationError::WriteError {
        path: path.display().to_string(),
        message: cmdr_fs::error_detail::ErrorDetail::of(error),
    })
}
