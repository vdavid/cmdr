//! The shared apply engine: the single chokepoint that runs a plan+apply closure
//! against an archive (LOCAL in place, or REMOTE pull-apply-upload-swap), the
//! [`PlanError`] cancel-vs-fault split, the mutator control-seam [`MutatorHooks`]
//! (cancel/pause/progress/downloads-ignore), the mutator-error mapping, and the
//! post-commit source deletion for an into-archive move.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::OperationEventSink;
use super::super::operation_intent::is_cancelled;
use super::super::state::WriteOperationState;
use super::super::types::{WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use crate::file_system::get_volume_manager;
use crate::file_system::volume::backends::archive::mutator::{MutationError, MutationHooks, MutationProgress};
use crate::ignore_poison::IgnorePoison;

/// A planning failure that separates a user cancel (archive untouched, nothing to
/// report as an error) from a genuine fault.
pub(super) enum PlanError {
    /// The user cancelled a Stop prompt (the oneshot sender was dropped).
    Cancelled,
    /// A real planning fault (unreadable source, unparseable archive, Stop under a
    /// pre-resolved policy).
    Op(WriteOperationError),
}

impl From<PlanError> for super::super::archive_remote_edit::RemoteEditError {
    fn from(e: PlanError) -> Self {
        match e {
            PlanError::Cancelled => Self::Cancelled,
            PlanError::Op(w) => Self::Op(w),
        }
    }
}

impl From<super::super::archive_remote_edit::RemoteEditError> for PlanError {
    fn from(e: super::super::archive_remote_edit::RemoteEditError) -> Self {
        match e {
            super::super::archive_remote_edit::RemoteEditError::Cancelled => Self::Cancelled,
            super::super::archive_remote_edit::RemoteEditError::Op(w) => Self::Op(w),
        }
    }
}

/// Runs a plan+apply closure against an archive, transparently LOCAL or REMOTE.
///
/// The closure is exactly the blocking plan+apply the local path always ran (it
/// plans against, and mutates, the path it's handed). For a LOCAL parent this is
/// byte-identical to before — the closure runs on the real archive file via
/// `spawn_blocking`, and the mutator's own temp+rename commits the edit. For a
/// REMOTE parent (direct SMB / MTP) it routes through
/// [`super::super::archive_remote_edit::pull_apply_upload_swap`]: pull the `.zip`
/// to a local temp, run the closure there, upload the result under a remote temp
/// name, and swap. The remote original is untouched until that final swap; a
/// cancel or fault anywhere before it leaves the original intact.
///
/// `parent_volume_id` is the drive holding the `.zip` (`"root"` for a local disk);
/// an unregistered id falls back to the local path (a plain-file edit that will
/// surface its own not-found).
pub(super) async fn run_managed_edit<T, F>(
    parent_volume_id: &str,
    archive_path: PathBuf,
    state: Arc<WriteOperationState>,
    plan_and_apply: F,
) -> Result<T, PlanError>
where
    F: FnOnce(&Path) -> Result<T, PlanError> + Send + 'static,
    T: Send + 'static,
{
    let parent = get_volume_manager().get(parent_volume_id);
    let is_remote = parent.as_ref().is_some_and(|p| !p.supports_local_fs_access());

    if !is_remote {
        // LOCAL: run plan+apply on the real archive file (mutator temp+rename).
        let path = archive_path.clone();
        return match tokio::task::spawn_blocking(move || plan_and_apply(&path)).await {
            Ok(result) => result,
            Err(join) => Err(PlanError::Op(WriteOperationError::IoError {
                path: archive_path.display().to_string(),
                message: format!("archive edit task failed: {join}"),
            })),
        };
    }

    let parent = parent.expect("is_remote is only true when the parent is registered");
    super::super::archive_remote_edit::pull_apply_upload_swap(parent, archive_path, state, plan_and_apply)
        .await
        .map_err(PlanError::from)
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

/// Deletes an into-archive move's local sources after the commit, on the blocking
/// pool. Handles both files and directory trees. Best-effort per source (a
/// failure leaves the file in both places — an incomplete move, never data loss).
pub(super) async fn delete_move_sources(sources: &[PathBuf]) {
    for source in sources {
        let source = source.clone();
        let removed = tokio::task::spawn_blocking(move || {
            let result = match std::fs::symlink_metadata(&source) {
                Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(&source),
                _ => std::fs::remove_file(&source),
            };
            result.map_err(|e| (source, e))
        })
        .await;
        if let Ok(Err((path, err))) = removed {
            log::warn!(target: "archive_edit", "couldn't remove moved source {}: {err}", path.display());
        }
    }
}

/// Bridges the mutator's control seam to the operation's live state: cancel from
/// `OperationIntent`, pause from the `PauseGate`, throttled progress events, and
/// the downloads-watcher ignore registration for the temp + final paths.
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
    /// Latest progress snapshot, read by the driver for the terminal event's totals.
    latest: Mutex<MutationProgress>,
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
            latest: Mutex::new(MutationProgress::default()),
        }
    }

    /// The latest progress snapshot, for the terminal event's totals.
    pub(super) fn latest_progress(&self) -> MutationProgress {
        *self.latest.lock_ignore_poison()
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
            self.operation_type,
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
