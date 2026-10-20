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
use super::super::host::EngineHost;
use super::super::in_flight_temps::{self, RecordHome};
use super::super::state::WriteOperationState;
use super::super::transfer::leftover_cleanup::{LeftBehind, finish_in_background};
use super::super::transfer::recovered_name::KeptNewData;
use super::super::types::{
    AppearedDuringMove, CancelRollback, ProgressStep, StagedLeftoverFate, StagedLeftovers, WriteCancelledEvent,
    WriteCompleteEvent, WriteErrorEvent, WriteOperationError, WriteOperationPhase, WriteOperationType,
    WriteProgressEvent,
};
use super::edit_error::EditError;
use super::remote::{RemoteCommitProgress, RemoteProgressObserver, pull_apply_upload_swap};
use cmdr_archive::mutator::{MutationError, MutationHooks, MutationProgress};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{ConnectionState, Volume};

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
///
/// A committed edit then refreshes every pane open inside the archive, before
/// the caller emits its terminal event: see [`show_the_edit`].
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
    let scope = state.shared_scope();
    let result = apply_local_or_remote(
        parent_volume_id,
        archive_path.clone(),
        state,
        remote_progress,
        plan_and_apply,
    )
    .await;
    if result.is_ok() {
        show_the_edit(scope.host(), parent_volume_id, &archive_path).await;
    }
    result
}

/// Re-reads every listing open at or inside the archive, so a pane browsing the
/// zip shows the edit the moment the op completes.
///
/// The edit has to do it itself. The FE's post-op `refresh_listing` skips an
/// archive listing (its volume isn't local and claims a watch), and nothing else
/// is reliable: the local content watch rides FSEvents, which delivered the
/// app's own swap many seconds late or not at all (a pane stayed stale until
/// re-entered, vdavid/cmdr-reports#55), and a remote parent has no watch at all.
/// The parsed index goes first, since the cache key can miss the change.
async fn show_the_edit(host: &EngineHost, parent_volume_id: &str, archive_path: &Path) {
    host.volumes().forget_archive_index(archive_path);
    host.listings()
        .refresh_archive_listings(parent_volume_id, archive_path)
        .await;
}

/// [`run_managed_edit`]'s dispatch: the closure on the real file for a local
/// parent, or pull-apply-upload-swap for a remote one.
async fn apply_local_or_remote<T, F>(
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
    // Plain `get`: the `.zip` file itself is a plain file on its parent drive.
    let parent = state.scope().host().volumes().get(parent_volume_id);
    let is_remote = parent.as_ref().is_some_and(|p| !p.supports_local_fs_access());

    if !is_remote {
        // LOCAL: run plan+apply on the real archive file (mutator temp+rename).
        let path = archive_path.clone();
        let applying = state
            .scope()
            .host()
            .runtime()
            .spawn_blocking(move || plan_and_apply(&path));
        return match applying.await {
            Ok(result) => result,
            Err(join) => Err(EditError::Op(WriteOperationError::IoError {
                path: archive_path.display().to_string(),
                message: format!("archive edit task failed: {join}").into(),
            })),
        };
    }

    let parent = parent.expect("is_remote is only true when the parent is registered");
    pull_apply_upload_swap(
        parent,
        parent_volume_id,
        archive_path,
        state,
        Some(remote_progress),
        plan_and_apply,
    )
    .await
}

/// Maps a mutator failure onto the typed `WriteOperationError` the FE renders.
/// `Cancelled` never reaches here (it's handled as a `write-cancelled`).
pub(super) fn to_write_error(archive_path: &Path, err: MutationError) -> WriteOperationError {
    let path = archive_path.display().to_string();
    match err {
        MutationError::Cancelled => WriteOperationError::Cancelled {
            message: "the archive edit was cancelled".into(),
        },
        MutationError::OpenOriginal(io) | MutationError::Io(io) => {
            super::super::error_classification::classify_io_error(&io, path)
        }
        MutationError::Zip(zip_err) => WriteOperationError::WriteError {
            path,
            message: zip_err.to_string().into(),
        },
        MutationError::EncryptedEntryRetained { name } => WriteOperationError::ArchiveEncryptedEntry { path, name },
        MutationError::DuplicateEntryName { name } => WriteOperationError::ArchiveDuplicateEntryName { path, name },
        MutationError::EntryUnderFile { entry, file } => {
            WriteOperationError::ArchiveEntryUnderFile { path, entry, file }
        }
        MutationError::ReadSource { inner_path, source } => WriteOperationError::ReadError {
            path: inner_path,
            message: cmdr_fs::error_detail::ErrorDetail::from_io(&source),
        },
    }
}

/// What an archive edit left behind for its terminal event to name, besides
/// its outcome.
pub(super) struct EditLeftovers {
    /// What a move INTO an archive left in its source (`copy_into.rs`); `None`
    /// for every other edit.
    appeared_during_move: Option<AppearedDuringMove>,
    /// Complete archives a remote edit's swap, or a compress's publish, kept
    /// when it stopped after the way was cleared: left recorded for the drive's
    /// next arrival, or saved under a ` (recovered)` name (`KeptNewData`).
    /// `write-cancelled` names them, as a copy's does, and so does a failure,
    /// beside its cause (`WriteOperationError::NewFilesKept`).
    kept: KeptNewData,
    /// Upload temps the edit couldn't delete as it stopped, by fate: handed to
    /// the background cleanup ([`hand_off_temps`]), and named on
    /// `write-cancelled` as a copy's are.
    scratch: Vec<StagedLeftovers>,
    /// The drive's name, for the line that says where they are.
    volume_name: String,
}

impl EditLeftovers {
    /// Takes the new archives `state`'s stop kept, on the drive
    /// `parent_volume_id` names, beside what a move into an archive left in
    /// its source, and hands the temps it couldn't delete to the background
    /// cleanup on that drive ([`hand_off_temps`]).
    pub(super) fn of(
        state: &Arc<WriteOperationState>,
        parent_volume_id: &str,
        appeared_during_move: Option<AppearedDuringMove>,
    ) -> Self {
        let kept = KeptNewData::take(state);
        let volume = state.scope().host().volumes().get(parent_volume_id);
        let volume_name = volume
            .as_ref()
            .map_or(parent_volume_id, |volume| volume.name())
            .to_owned();
        let scratch = hand_off_temps(state, parent_volume_id, volume.as_ref(), &volume_name);
        Self {
            appeared_during_move,
            kept,
            scratch,
            volume_name,
        }
    }
}

/// Hands the upload temps an edit couldn't delete to the background cleanup on
/// the parent's drive (`transfer/leftover_cleanup.rs`), as a copy's stop does,
/// and answers the cancel summary's lines for them. A remote zip edit's or a
/// compress's `StagedWrite::abandon` leaves a temp listed when the share can't
/// take the delete, and both name-and-age reaps skip a recorded temp, so
/// nothing else reaches it before the next launch (#574).
///
/// Their records live in the parent's path space: a remote edit's state names
/// no destination (`StagedWrite::begin_over_original` records under the
/// parent), and a compress's destination is the parent. Skipped once tier 2
/// fired, as a copy's cleanup is: the records stay for the startup sweep.
fn hand_off_temps(
    state: &Arc<WriteOperationState>,
    parent_volume_id: &str,
    volume: Option<&Arc<dyn Volume>>,
    volume_name: &str,
) -> Vec<StagedLeftovers> {
    if state.backend_abort.is_cancelled() {
        return Vec::new();
    }
    let temps = std::mem::take(&mut *state.in_flight_temps.lock_ignore_poison());
    if temps.is_empty() {
        return Vec::new();
    }
    let home = RecordHome::volume_space(parent_volume_id);
    let Some(volume) = volume else {
        // The drive left: its next arrival's sweep removes them.
        for temp in &temps {
            in_flight_temps::keep_temp_for_arrival(state, Some(home.clone()), temp);
        }
        return StagedLeftovers::of(StagedLeftoverFate::RemovedWhenBack, &temps, volume_name)
            .into_iter()
            .collect();
    };
    let left = if volume.connection_state().is_none_or(ConnectionState::is_live) {
        LeftBehind {
            out: Vec::new(),
            refused: temps,
        }
    } else {
        LeftBehind {
            out: temps,
            refused: Vec::new(),
        }
    };
    let scratch = [
        (StagedLeftoverFate::Refused, &left.refused),
        (StagedLeftoverFate::RemovedWhenBack, &left.out),
    ]
    .into_iter()
    .filter_map(|(fate, paths)| StagedLeftovers::of(fate, paths, volume_name))
    .collect();
    finish_in_background(volume, state, Some(home), left);
    scratch
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
/// `left` rides on the arm it belongs to: what a move left in its source on
/// `write-complete`, the new archives a stop kept on `write-cancelled`, and on
/// `write-error` beside its cause.
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
    left: EditLeftovers,
    final_progress: &MutationProgress,
) {
    let EditLeftovers {
        appeared_during_move,
        kept,
        scratch,
        volume_name,
    } = left;
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
            metadata_left_behind: None,
            durability_unconfirmed: None,
            locked_kept: None,
        }),
        Err(EditError::Cancelled) => events.emit_cancelled(WriteCancelledEvent {
            operation_id: op_id.to_string(),
            operation_type,
            files_processed: final_progress.entries_done,
            rollback: CancelRollback::none()
                .with_staged_leftovers(scratch.into_iter().chain(kept.groups(&volume_name)).collect()),
        }),
        Err(EditError::Op(err)) => {
            let kept: Vec<StagedLeftovers> = kept.beside(&err).groups(&volume_name).collect();
            let err = err.with_new_files_kept(&kept);
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
        self.state.scope().status().update(
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
    /// The host's `Pacing::copy_throttle` is `None` in production, so this is one
    /// atomic load. It sleeps only while an entry remains: the mutator checks cancel before
    /// every entry, so a click during the sleep always stops the rewrite, whereas
    /// after the last entry the commit follows with no check left to honor it.
    fn pace_entry_for_e2e(&self, progress: MutationProgress) {
        let Some(pause) = self.state.scope().host().pacing().copy_throttle() else {
            return;
        };
        if progress.entries_done >= progress.entries_total
            || self.paced_entries.swap(progress.entries_done, Ordering::Relaxed) == progress.entries_done
        {
            return;
        }
        sleep_unless_cancelled(pause, || self.state.intent().is_cancelled());
    }
}

impl MutationHooks for MutatorHooks {
    fn is_cancelled(&self) -> bool {
        self.state.intent().is_cancelled()
    }

    fn wait_if_paused(&self) {
        // Sync park — the mutator runs on the blocking pool, so parking its
        // thread is the correct shape (matches the local-FS drivers). Cancel
        // wins: the gate returns the instant the op leaves `Running`.
        self.state.pause_gate.wait_while_paused_sync(self.state.intent());
    }

    fn on_progress(&self, progress: MutationProgress) {
        *self.latest.lock_ignore_poison() = progress;
        self.emit_progress_if_due(progress);
        self.pace_entry_for_e2e(progress);
    }

    fn note_pending(&self, path: &Path) {
        self.state.scope().host().announce(path);
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
