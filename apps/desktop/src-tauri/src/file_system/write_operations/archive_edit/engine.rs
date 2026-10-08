//! The shared apply engine: the single chokepoint that runs a plan+apply closure
//! against an archive (LOCAL in place, or REMOTE pull-apply-upload-swap), the
//! mutator control-seam [`MutatorHooks`] (cancel/pause/progress/downloads-ignore,
//! plus E2E pacing), the mutator-error mapping, and the post-commit source
//! deletion for an into-archive move. The cancel-vs-fault split every stage
//! returns is [`EditError`], in its own leaf so `remote` can name it too.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::OperationEventSink;
use super::super::operation_intent::is_cancelled;
use super::super::state::{WriteOperationState, update_operation_status};
use super::super::types::{
    AppearedDuringMove, CancelRollback, ProgressStep, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent,
    WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent,
};
use super::edit_error::EditError;
use super::remote::{RemoteCommitProgress, RemoteProgressObserver, pull_apply_upload_swap};
use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;
use cmdr_archive::mutator::{MutationError, MutationHooks, MutationProgress};

/// Runs a plan+apply closure against an archive, transparently LOCAL or REMOTE.
///
/// The closure is exactly the blocking plan+apply the local path always ran (it
/// plans against, and mutates, the path it's handed). For a LOCAL parent this is
/// byte-identical to before — the closure runs on the real archive file via
/// `spawn_blocking`, and the mutator's own temp+rename commits the edit. For a
/// REMOTE parent (direct SMB / MTP) it routes through [`pull_apply_upload_swap`]:
/// pull the `.zip` to a local temp, run the closure there, upload the result
/// under a remote temp name, and swap. The remote original is untouched until that final swap; a
/// cancel or fault anywhere before it leaves the original intact.
///
/// `parent_volume_id` is the drive holding the `.zip` (`"root"` for a local disk);
/// an unregistered id falls back to the local path (a plain-file edit that will
/// surface its own not-found).
pub(super) async fn run_managed_edit<T, F>(
    parent_volume_id: &str,
    archive_path: PathBuf,
    state: Arc<WriteOperationState>,
    remote_progress: RemoteProgressObserver,
    plan_and_apply: F,
) -> Result<T, EditError>
where
    F: FnOnce(&Path) -> Result<T, EditError> + Send + 'static,
    T: Send + 'static,
{
    let parent = get_volume_manager().get(parent_volume_id);
    let is_remote = parent.as_ref().is_some_and(|p| !p.supports_local_fs_access());

    if !is_remote {
        // LOCAL: run plan+apply on the real archive file (mutator temp+rename).
        let path = archive_path.clone();
        return match tokio::task::spawn_blocking(move || plan_and_apply(&path)).await {
            Ok(result) => result,
            Err(join) => Err(EditError::Op(WriteOperationError::IoError {
                path: archive_path.display().to_string(),
                message: format!("archive edit task failed: {join}"),
            })),
        };
    }

    let parent = parent.expect("is_remote is only true when the parent is registered");
    pull_apply_upload_swap(parent, archive_path, state, Some(remote_progress), plan_and_apply).await
}

/// Maps a mutator failure onto the typed `WriteOperationError` the FE renders.
/// `Cancelled` never reaches here (it's handled as a `write-cancelled`).
pub(super) fn to_write_error(archive_path: &Path, err: MutationError) -> WriteOperationError {
    let path = archive_path.display().to_string();
    match err {
        MutationError::Cancelled => WriteOperationError::Cancelled {
            message: "the archive edit was cancelled".to_string(),
        },
        MutationError::OpenOriginal(io) | MutationError::Io(io) => {
            super::super::error_classification::classify_io_error(&io, path)
        }
        MutationError::Zip(zip_err) => WriteOperationError::WriteError {
            path,
            message: zip_err.to_string(),
        },
        MutationError::EncryptedEntryRetained { name } => WriteOperationError::WriteError {
            path,
            message: format!("the archive contains an encrypted entry ('{name}') and can't be edited"),
        },
        MutationError::ReadSource { inner_path, source } => WriteOperationError::ReadError {
            path: inner_path,
            message: source.to_string(),
        },
    }
}

/// Emits the ONE terminal event an archive edit's outcome calls for, and
/// nothing else.
///
/// Both managed routes (`copy_into.rs`'s into-archive edit and `driver.rs`'s
/// general one) end the same three ways, and each owns only what it must do
/// BEFORE the emit — a move's source delete, a compress route's journal row. So
/// the three-arm match itself lives here rather than being written twice and
/// drifting: it was already a 26-line clone the duplication gauge tracked.
///
/// `skipped_count` is meaningful only on the `Ok` path; the other two arms
/// carry `entries_done` instead, since a run that stopped has no final tally.
///
/// ❗ An archive edit tracks no top-level SELECTION, so `top_level_skipped` is
/// `None`: its `skipped_count` is unrepresentable entries and in-zip clashes,
/// not items the user picked in a pane. The FE words the summary from
/// `files_skipped` alone for these.
pub(super) fn emit_archive_terminal(
    events: &dyn OperationEventSink,
    op_id: &str,
    operation_type: WriteOperationType,
    outcome: Result<(), EditError>,
    skipped_count: usize,
    // What a move INTO an archive left in its source (`copy_into.rs`); `None`
    // for every other edit.
    appeared_during_move: Option<AppearedDuringMove>,
    final_progress: &MutationProgress,
) {
    match outcome {
        Ok(()) => events.emit_complete(WriteCompleteEvent {
            operation_id: op_id.to_string(),
            operation_type,
            files_processed: final_progress.entries_changed,
            files_skipped: skipped_count,
            bytes_processed: final_progress.bytes_total,
            appeared_during_move,
            top_level_skipped: None,
            refused: None,
        }),
        Err(EditError::Cancelled) => events.emit_cancelled(WriteCancelledEvent {
            operation_id: op_id.to_string(),
            operation_type,
            files_processed: final_progress.entries_done,
            rollback: CancelRollback::none(),
        }),
        Err(EditError::Op(err)) => {
            events.emit_error(WriteErrorEvent::new(op_id.to_string(), operation_type, err));
        }
    }
}

/// How often a paced sleep re-checks cancel, so E2E pacing never holds a click
/// back longer than this.
const PACE_CANCEL_SLICE: Duration = Duration::from_millis(10);

/// Sleeps for `total`, re-checking `cancelled` every [`PACE_CANCEL_SLICE`] and
/// returning as soon as it answers `true`.
fn sleep_unless_cancelled(total: Duration, cancelled: impl Fn() -> bool) {
    let deadline = Instant::now() + total;
    while !cancelled() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        std::thread::sleep(remaining.min(PACE_CANCEL_SLICE));
    }
}

/// Bridges the mutator's control seam to the operation's live state: cancel from
/// `OperationIntent`, pause from the `PauseGate`, throttled progress events, the
/// downloads-watcher ignore registration for the temp + final paths, and the E2E
/// per-entry pacing.
pub(super) struct MutatorHooks {
    state: Arc<WriteOperationState>,
    events: Arc<dyn OperationEventSink>,
    operation_id: String,
    /// The op type the mutator runs under: `ArchiveEdit` for a plain zip edit, or
    /// `Move` when the mutator is the delete phase of an out-of-zip move (so
    /// progress/cancel ride under the move op the FE is tracking).
    operation_type: WriteOperationType,
    progress_interval: Duration,
    /// Last time a `write-progress` was emitted, for throttling.
    last_emit: Mutex<Option<Instant>>,
    /// Last transfer-byte event emitted after a remote compression rewrite.
    /// Kept separate because transfer starts a new byte axis and must emit its
    /// zero tick even when compression emitted a moment earlier.
    last_remote_emit: Mutex<Option<Instant>>,
    /// Last scanning tick a fresh compress's planning walk emitted; its own
    /// clock, so a throttled walk never swallows the first compression tick.
    last_scan_emit: Mutex<Option<Instant>>,
    /// Latest progress snapshot, read by the driver for the terminal event's totals.
    latest: Mutex<MutationProgress>,
    /// The last `entries_done` the E2E pacing slept for, so it sleeps once per entry
    /// rather than once per chunk. `usize::MAX` until the first tick.
    paced_entries: AtomicUsize,
    /// Set when a fresh compress takes the spool route (zip here, then upload),
    /// so its phases say which of the two steps they belong to.
    zip_then_upload: AtomicBool,
}

impl MutatorHooks {
    /// Builds the hooks for one archive apply. `last_emit` / `latest` start empty;
    /// the driver reads the final snapshot back via [`MutatorHooks::latest_progress`].
    pub(super) fn new(
        state: Arc<WriteOperationState>,
        events: Arc<dyn OperationEventSink>,
        operation_id: String,
        operation_type: WriteOperationType,
        progress_interval: Duration,
    ) -> Self {
        Self {
            state,
            events,
            operation_id,
            operation_type,
            progress_interval,
            last_emit: Mutex::new(None),
            last_remote_emit: Mutex::new(None),
            last_scan_emit: Mutex::new(None),
            latest: Mutex::new(MutationProgress::default()),
            paced_entries: AtomicUsize::new(usize::MAX),
            zip_then_upload: AtomicBool::new(false),
        }
    }

    /// Reports a planning walk as the indeterminate `Scanning` phase: what it has
    /// found so far and where it is. Throttled to the op's progress interval
    /// unless `force` (the walk's final tally).
    pub(super) fn emit_scan_progress(
        &self,
        tally: super::fresh_plan::PlanProgress<'_>,
        current_dir: Option<&Path>,
        force: bool,
    ) {
        {
            let mut last = self.last_scan_emit.lock_ignore_poison();
            let now = Instant::now();
            if !force && last.is_some_and(|then| now.duration_since(then) < self.progress_interval) {
                return;
            }
            *last = Some(now);
        }
        let event = WriteProgressEvent::new(
            self.operation_id.clone(),
            self.operation_type,
            WriteOperationPhase::Scanning,
            None,
            tally.files,
            0,
            tally.bytes,
            0,
        )
        .with_scan_meta(current_dir.map(|dir| dir.display().to_string()), tally.dirs, None);
        self.emit(event);
    }

    /// Emits one `write-progress` AND mirrors it into the status cache, the pair
    /// the transfer driver's `emit_progress_and_status` keeps. Every emit here
    /// goes through it: the cache is all a query API sees (the MCP `cmdr://state`
    /// resource), and an op that only emits reads there as `Scanning` with no
    /// bytes from start to finish.
    fn emit(&self, event: WriteProgressEvent) {
        update_operation_status(
            &self.operation_id,
            event.phase,
            event.current_file.clone(),
            event.files_done,
            event.files_total,
            event.bytes_done,
            event.bytes_total,
        );
        self.state.emit_progress_via_sink(&*self.events, event);
    }

    /// Numbers this op's phases as two steps: compressing locally, then
    /// transferring the archive. Call before the first progress tick.
    pub(super) fn number_steps_as_zip_then_upload(&self) {
        self.zip_then_upload.store(true, Ordering::Relaxed);
    }

    fn step_for(&self, phase: WriteOperationPhase) -> Option<ProgressStep> {
        if !self.zip_then_upload.load(Ordering::Relaxed) {
            return None;
        }
        match phase {
            WriteOperationPhase::Compressing | WriteOperationPhase::FinishingCompression => {
                Some(ProgressStep { number: 1, total: 2 })
            }
            WriteOperationPhase::Transferring | WriteOperationPhase::FinishingTransfer => {
                Some(ProgressStep { number: 2, total: 2 })
            }
            _ => None,
        }
    }

    /// The latest progress snapshot, for the terminal event's totals.
    pub(super) fn latest_progress(&self) -> MutationProgress {
        *self.latest.lock_ignore_poison()
    }

    /// Reports an upload of a finished archive (a remote edit's rewritten copy,
    /// or a fresh ZIP's spool) as its own byte axis, then indeterminate
    /// finishing work, under this op's type.
    pub(super) fn remote_progress_observer(self: &Arc<Self>) -> RemoteProgressObserver {
        let hooks = Arc::clone(self);
        Arc::new(move |progress| hooks.emit_remote_progress(progress))
    }

    fn emit_remote_progress(&self, progress: RemoteCommitProgress) {
        if let RemoteCommitProgress::Transferring { bytes_done, .. } = progress {
            let mut last = self.last_remote_emit.lock_ignore_poison();
            let now = Instant::now();
            let due = bytes_done == 0 || last.is_none_or(|then| now.duration_since(then) >= self.progress_interval);
            if !due {
                return;
            }
            *last = Some(now);
        }
        let (phase, bytes_done, bytes_total) = match progress {
            RemoteCommitProgress::Transferring {
                bytes_done,
                bytes_total,
            } => (WriteOperationPhase::Transferring, bytes_done, bytes_total),
            RemoteCommitProgress::Finishing => (WriteOperationPhase::FinishingTransfer, 0, 0),
        };
        self.emit(
            WriteProgressEvent::new(
                self.operation_id.clone(),
                self.operation_type,
                phase,
                None,
                0,
                0,
                bytes_done,
                bytes_total,
            )
            .with_step(self.step_for(phase)),
        );
    }

    /// Emits `write-progress`, throttled to the op's progress interval, but always
    /// lets the final tick (all entries done) through so the bar reaches 100%.
    fn emit_progress_if_due(&self, progress: MutationProgress) {
        let is_final = progress.entries_done == progress.entries_total;
        {
            let mut last = self.last_emit.lock_ignore_poison();
            let now = Instant::now();
            let due = last.is_none_or(|t| now.duration_since(t) >= self.progress_interval);
            if !due && !is_final {
                return;
            }
            *last = Some(now);
        }

        let compression_finished = self.operation_type == WriteOperationType::Compress
            && (is_final || (progress.bytes_total > 0 && progress.bytes_done >= progress.bytes_total));
        let (phase, files_done, files_total, bytes_done, bytes_total) = if compression_finished {
            (WriteOperationPhase::FinishingCompression, 0, 0, 0, 0)
        } else {
            let phase = if self.operation_type == WriteOperationType::Compress {
                WriteOperationPhase::Compressing
            } else {
                WriteOperationPhase::Copying
            };
            (
                phase,
                progress.entries_done,
                progress.entries_total,
                progress.bytes_done,
                progress.bytes_total,
            )
        };
        let event = WriteProgressEvent::new(
            self.operation_id.clone(),
            self.operation_type,
            phase,
            None,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        )
        .with_step(self.step_for(phase));
        self.emit(event);
    }

    /// E2E-only per-entry pacing, the archive twin of the copy loop's per-file
    /// throttle (`transfer/copy/mod.rs`), so a spec can press Cancel mid-rewrite.
    /// `effective_copy_throttle_ms()` is `None` in production, so this is one atomic
    /// load. It sleeps only while an entry remains: the mutator checks cancel before
    /// every entry, so a click during the sleep always stops the rewrite, whereas
    /// after the last entry the commit follows with no check left to honor it.
    fn pace_entry_for_e2e(&self, progress: MutationProgress) {
        let Some(ms) = crate::test_mode::effective_copy_throttle_ms().filter(|ms| *ms > 0) else {
            return;
        };
        if progress.entries_done >= progress.entries_total
            || self.paced_entries.swap(progress.entries_done, Ordering::Relaxed) == progress.entries_done
        {
            return;
        }
        sleep_unless_cancelled(Duration::from_millis(ms), || is_cancelled(&self.state.intent));
    }
}

impl MutationHooks for MutatorHooks {
    fn is_cancelled(&self) -> bool {
        is_cancelled(&self.state.intent)
    }

    fn wait_if_paused(&self) {
        // Sync park — the mutator runs on the blocking pool, so parking its
        // thread is the correct shape (matches the local-FS drivers). Cancel
        // wins: the gate returns the instant the op leaves `Running`.
        self.state.pause_gate.wait_while_paused_sync(&self.state.intent);
    }

    fn on_progress(&self, progress: MutationProgress) {
        *self.latest.lock_ignore_poison() = progress;
        self.emit_progress_if_due(progress);
        self.pace_entry_for_e2e(progress);
    }

    fn note_pending(&self, path: &Path) {
        crate::downloads::note_pending_write_for_cmdr(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::event_sinks::CollectorEventSink;

    #[test]
    fn paced_sleep_gives_way_to_a_cancel_within_a_slice() {
        // The cancel lands after the first check, so a 10 s pace must return at once.
        let checks = AtomicUsize::new(0);
        let started = Instant::now();
        sleep_unless_cancelled(Duration::from_secs(10), || checks.fetch_add(1, Ordering::Relaxed) >= 1);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "slept {:?} past a cancel",
            started.elapsed()
        );
    }

    #[test]
    fn paced_sleep_runs_its_full_length_without_a_cancel() {
        let started = Instant::now();
        sleep_unless_cancelled(Duration::from_millis(30), || false);
        assert!(started.elapsed() >= Duration::from_millis(30));
    }

    /// What a query API (the MCP `cmdr://state` resource) reads for this op.
    fn cached(id: &str) -> (WriteOperationPhase, usize, usize, u64, u64) {
        let status = super::super::super::state::get_operation_status(id).expect("the op is registered");
        (
            status.phase,
            status.files_done,
            status.files_total,
            status.bytes_done,
            status.bytes_total,
        )
    }

    #[test]
    fn every_emitted_phase_of_a_compress_reaches_the_status_cache() {
        use super::super::super::state::{register_operation_status, unregister_operation_status};

        let id = "compress-status-mirror";
        register_operation_status(id, WriteOperationType::Compress, vec![]);
        let hooks = MutatorHooks::new(
            Arc::new(WriteOperationState::new(Duration::ZERO)),
            Arc::new(CollectorEventSink::new()) as Arc<dyn OperationEventSink>,
            id.to_string(),
            WriteOperationType::Compress,
            Duration::ZERO,
        );

        hooks.emit_scan_progress(
            super::super::fresh_plan::PlanProgress {
                files: 2,
                dirs: 1,
                bytes: 100,
                current_dir: Path::new("/src"),
            },
            None,
            true,
        );
        assert_eq!(cached(id), (WriteOperationPhase::Scanning, 2, 0, 100, 0));

        // Pre-fix the cache stayed at `Scanning` from here to the terminal event.
        let tick = |entries_done, bytes_done| MutationProgress {
            entries_done,
            entries_total: 3,
            entries_changed: 3,
            bytes_done,
            bytes_total: 100,
        };
        MutationHooks::on_progress(&hooks, tick(1, 40));
        assert_eq!(cached(id), (WriteOperationPhase::Compressing, 1, 3, 40, 100));

        MutationHooks::on_progress(&hooks, tick(3, 100));
        assert_eq!(cached(id), (WriteOperationPhase::FinishingCompression, 0, 0, 0, 0));

        hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
            bytes_done: 0,
            bytes_total: 60,
        });
        assert_eq!(cached(id), (WriteOperationPhase::Transferring, 0, 0, 0, 60));

        hooks.emit_remote_progress(RemoteCommitProgress::Finishing);
        assert_eq!(cached(id), (WriteOperationPhase::FinishingTransfer, 0, 0, 0, 0));
        unregister_operation_status(id);
    }

    #[test]
    fn remote_transfer_ticks_keep_the_configured_cadence_but_phase_boundaries_emit() {
        let events = Arc::new(CollectorEventSink::new());
        let hooks = MutatorHooks::new(
            Arc::new(WriteOperationState::new(Duration::from_secs(60))),
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            "compress-cadence".to_string(),
            WriteOperationType::Compress,
            Duration::from_secs(60),
        );

        hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
            bytes_done: 0,
            bytes_total: 100,
        });
        hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
            bytes_done: 10,
            bytes_total: 100,
        });
        hooks.emit_remote_progress(RemoteCommitProgress::Transferring {
            bytes_done: 20,
            bytes_total: 100,
        });
        hooks.emit_remote_progress(RemoteCommitProgress::Finishing);

        let progress = events.progress.lock_ignore_poison();
        assert_eq!(progress.len(), 2, "intermediate upload chunks must be throttled");
        assert_eq!(progress[0].phase, WriteOperationPhase::Transferring);
        assert_eq!(progress[0].bytes_done, 0);
        assert_eq!(progress[1].phase, WriteOperationPhase::FinishingTransfer);
    }
}
