//! The archive-edit operation: runs a zip mutation (`ArchiveMutator`) as a real
//! managed write op, so it inherits the queue, lane admission, pause/resume,
//! cancel, progress/ETA, busy-volumes eject guard, and the `write-settled`
//! contract every other transfer/delete gets.
//!
//! A zip edit is NOT a metadata syscall — it's an O(archive) temp+rename rewrite
//! — so it flows through [`manager::spawn_managed`] (a progress bar, the parent
//! drive's lane) like copy/delete, NOT the instant path rename/mkdir take for a
//! plain filesystem. The driver is net-new but mirrors the volume-delete branch's
//! shape: a deferred async start owns the op end to end (settle guard, the
//! mutator run on the blocking pool, the terminal event, `on_settled`).
//!
//! ## What crosses the seam
//!
//! The caller hands an [`ArchiveEditRequest`]: the archive path, its parent drive
//! id (source of the lane + the eject-busy id), a resolved [`Changeset`], a queue
//! summary, and — for an into-archive MOVE only — the local sources to delete
//! AFTER the edit durably commits (the move invariant: never lose both copies).
//! Conflicts are resolved into the changeset before it reaches here, so the
//! mutator stays deterministic.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::manager::{self, ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::operation_intent::is_cancelled;
use super::state::{WriteOperationState, WriteSettledGuard};
use super::types::{
    WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError, WriteOperationPhase,
    WriteOperationStartResult, WriteOperationType, WriteProgressEvent,
};
use super::OperationEventSink;
use crate::file_system::get_volume_manager;
use crate::file_system::volume::LaneKey;
use crate::file_system::volume::backends::archive::mutator::{
    self, Changeset, MutationError, MutationHooks, MutationProgress,
};
use crate::ignore_poison::IgnorePoison;

/// Everything the driver needs to run one archive edit.
pub(crate) struct ArchiveEditRequest {
    /// Absolute path of the `.zip` being edited (on its parent drive).
    pub archive_path: PathBuf,
    /// The parent drive's volume id: the op's lane key comes from it (archive
    /// work shares the drive's serialization lane), and it's the id marked busy
    /// so the drive can't be ejected mid-edit. `"root"` for a local disk.
    pub parent_volume_id: String,
    /// The resolved batch of changes (conflicts already folded in).
    pub changeset: Changeset,
    /// Queue-window summary (e.g. the added item's name, "Delete note.txt").
    pub summary: OperationSummaryText,
    /// Local source files to delete AFTER the edit commits — set only for an
    /// into-archive MOVE. The move invariant: the source side is removed only
    /// once the destination (the rewritten archive) is durably in place, so a
    /// crash never loses both copies. Empty for copy-into and in-archive edits.
    pub move_sources_to_delete: Vec<PathBuf>,
}

/// Builds a Tauri-backed event sink from the startup-wired app handle, for
/// routing an archive-target instant op (mkdir / mkfile / rename) to the managed
/// edit driver. `None` before the app handle is wired (unit tests), so callers
/// fall back to a plain refusal rather than silently dropping the op.
pub(crate) fn global_tauri_sink() -> Option<Arc<dyn OperationEventSink>> {
    manager::operations_app_handle().map(|app| Arc::new(super::TauriEventSink::new(app)) as Arc<dyn OperationEventSink>)
}

/// Joins an archive-inner parent path and a new child name into a single
/// `/`-separated inner path (root-relative, no surrounding slashes).
pub(crate) fn join_inner_path(inner_parent: &Path, name: &str) -> String {
    let parent = inner_parent.to_string_lossy().replace('\\', "/");
    let parent = parent.trim_matches('/');
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Normalizes an archive-inner path (from `confirm_archive_boundary`) to the
/// `/`-separated, surrounding-slash-free form the changeset uses.
pub(crate) fn normalize_inner_path(inner: &Path) -> String {
    inner.to_string_lossy().replace('\\', "/").trim_matches('/').to_string()
}

/// Starts an archive edit as a managed operation, returning its id immediately
/// (the queue row shows at once; the mutator runs when the parent lane is free).
pub(crate) async fn archive_edit_start(
    events: Arc<dyn OperationEventSink>,
    request: ArchiveEditRequest,
    progress_interval_ms: u64,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    // Share the parent drive's lane so the op serializes against other work on
    // that device (falls back to the id itself if the volume isn't registered —
    // the mutator's own open will surface a real error on a truly-gone drive).
    let lane = get_volume_manager()
        .get(&request.parent_volume_id)
        .map(|v| v.lane_key())
        .unwrap_or_else(|| LaneKey::new(request.parent_volume_id.clone()));

    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::ArchiveEdit,
        lanes: vec![lane],
        // Mark the parent drive busy while editing (the manager drops `root`).
        volume_ids: vec![request.parent_volume_id.clone()],
        summary: request.summary,
    };

    let events_for_op = Arc::clone(&events);
    let op_id_outer = operation_id.clone();
    let state_for_op = Arc::clone(&state);
    let progress_interval = Duration::from_millis(progress_interval_ms);
    let ArchiveEditRequest {
        archive_path,
        parent_volume_id,
        changeset,
        move_sources_to_delete,
        ..
    } = request;

    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let events = events_for_op;
            let op_id = op_id_outer;
            let state = state_for_op;
            let task_guard = ManagedTaskGuard::new(op_id.clone());
            // `write-settled` fires at end of scope (after the terminal event and
            // `on_settled`). `root` local edits carry no volume id (matches the
            // local-op convention).
            let settle_volume = (parent_volume_id != "root").then(|| parent_volume_id.clone());
            let _settled = WriteSettledGuard::new(Arc::clone(&events), op_id.clone(), WriteOperationType::ArchiveEdit, settle_volume);

            let hooks = Arc::new(MutatorHooks {
                state: Arc::clone(&state),
                events: Arc::clone(&events),
                operation_id: op_id.clone(),
                progress_interval,
                last_emit: Mutex::new(None),
                latest: Mutex::new(MutationProgress::default()),
            });

            let hooks_for_blocking = Arc::clone(&hooks);
            let archive_for_blocking = archive_path.clone();
            let result = tokio::task::spawn_blocking(move || {
                mutator::apply(&archive_for_blocking, &changeset, &*hooks_for_blocking)
            })
            .await;

            let final_progress = *hooks.latest.lock_ignore_poison();
            match result {
                Ok(Ok(())) => {
                    // Move invariant: delete the local sources only now that the
                    // rewritten archive is durably committed. Best-effort — a
                    // failed source delete leaves the file in both places (an
                    // incomplete move), never loses data.
                    delete_move_sources(&move_sources_to_delete).await;
                    events.emit_complete(WriteCompleteEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::ArchiveEdit,
                        files_processed: final_progress.entries_total,
                        files_skipped: 0,
                        bytes_processed: final_progress.bytes_total,
                    });
                }
                Ok(Err(MutationError::Cancelled)) => {
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::ArchiveEdit,
                        files_processed: final_progress.entries_done,
                        rolled_back: false,
                    });
                }
                Ok(Err(err)) => {
                    events.emit_error(WriteErrorEvent::new(
                        op_id.clone(),
                        WriteOperationType::ArchiveEdit,
                        to_write_error(&archive_path, err),
                    ));
                }
                Err(join_error) => {
                    events.emit_error(WriteErrorEvent::new(
                        op_id.clone(),
                        WriteOperationType::ArchiveEdit,
                        WriteOperationError::IoError {
                            path: archive_path.display().to_string(),
                            message: format!("archive edit task failed: {join_error}"),
                        },
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

/// Deletes an into-archive move's local sources after the commit, on the blocking
/// pool. Best-effort per file (a failure is logged, never fatal).
async fn delete_move_sources(sources: &[PathBuf]) {
    for source in sources {
        let source = source.clone();
        let removed = tokio::task::spawn_blocking(move || std::fs::remove_file(&source).map_err(|e| (source, e))).await;
        if let Ok(Err((path, err))) = removed {
            log::warn!(target: "archive_edit", "couldn't remove moved source {}: {err}", path.display());
        }
    }
}

/// Maps a mutator failure onto the typed `WriteOperationError` the FE renders.
/// `Cancelled` never reaches here (it's handled as a `write-cancelled`).
fn to_write_error(archive_path: &Path, err: MutationError) -> WriteOperationError {
    let path = archive_path.display().to_string();
    match err {
        MutationError::Cancelled => WriteOperationError::Cancelled {
            message: "the archive edit was cancelled".to_string(),
        },
        MutationError::OpenOriginal(io) | MutationError::Io(io) => {
            super::error_classification::classify_io_error(&io, path)
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

/// Bridges the mutator's control seam to the operation's live state: cancel from
/// `OperationIntent`, pause from the `PauseGate`, throttled progress events, and
/// the downloads-watcher ignore registration for the temp + final paths.
struct MutatorHooks {
    state: Arc<WriteOperationState>,
    events: Arc<dyn OperationEventSink>,
    operation_id: String,
    progress_interval: Duration,
    /// Last time a `write-progress` was emitted, for throttling.
    last_emit: Mutex<Option<Instant>>,
    /// Latest progress snapshot, read by the driver for the terminal event's totals.
    latest: Mutex<MutationProgress>,
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

        // Throttle to the op's progress interval, but always let the final tick
        // (all bytes processed) through so the bar reaches 100%.
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

        let event = WriteProgressEvent::new(
            self.operation_id.clone(),
            WriteOperationType::ArchiveEdit,
            WriteOperationPhase::Copying,
            None,
            progress.entries_done,
            progress.entries_total,
            progress.bytes_done,
            progress.bytes_total,
        );
        self.state.emit_progress_via_sink(&*self.events, event);
    }

    fn note_pending(&self, path: &Path) {
        crate::downloads::note_pending_write_for_cmdr(path);
    }
}

#[cfg(test)]
#[path = "archive_edit_tests.rs"]
mod archive_edit_tests;
