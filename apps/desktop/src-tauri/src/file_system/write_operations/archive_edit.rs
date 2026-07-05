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

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use uuid::Uuid;
use walkdir::WalkDir;

use super::OperationEventSink;
use super::manager::{self, ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::operation_intent::is_cancelled;
use super::state::{WriteOperationState, WriteSettledGuard};
use super::types::{
    ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteErrorEvent, WriteOperationError,
    WriteOperationPhase, WriteOperationStartResult, WriteOperationType, WriteProgressEvent,
};
use crate::file_system::get_volume_manager;
use crate::file_system::volume::backends::archive;
use crate::file_system::volume::backends::archive::mutator::{
    self, AddEntry, AddSource, Changeset, MutationError, MutationHooks, MutationProgress,
};
use crate::file_system::volume::backends::archive::{ArchiveIndex, LocalFileSource};
use crate::file_system::volume::{LaneKey, Volume};
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

/// Routes an in-archive delete (one or more entries inside the SAME zip) to the
/// managed edit driver as a single `{ delete }` changeset. All sources must
/// resolve to one archive (a multi-select can't span archives in one pane).
/// `parent_volume_id` is the display drive id (`"root"` for a local zip).
pub(crate) async fn route_archive_delete(
    events: Arc<dyn OperationEventSink>,
    sources: &[PathBuf],
    parent_volume_id: &str,
    progress_interval_ms: u64,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let first = sources.first().ok_or_else(|| WriteOperationError::IoError {
        path: String::new(),
        message: "no entries to delete".to_string(),
    })?;
    let (archive_path, _) = archive::confirm_archive_boundary(first).ok_or_else(|| read_only_error(first))?;

    let mut deletes = Vec::with_capacity(sources.len());
    for source in sources {
        let (source_archive, inner) =
            archive::confirm_archive_boundary(source).ok_or_else(|| read_only_error(source))?;
        if source_archive != archive_path {
            return Err(WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "can't delete entries from more than one archive at once".to_string(),
            });
        }
        deletes.push(normalize_inner_path(&inner));
    }

    let summary_source = deletes.first().map(|d| d.rsplit('/').next().unwrap_or(d).to_string());
    let request = ArchiveEditRequest {
        archive_path,
        parent_volume_id: parent_volume_id.to_string(),
        changeset: Changeset {
            deletes,
            ..Default::default()
        },
        summary: OperationSummaryText {
            source: summary_source,
            destination: None,
        },
        move_sources_to_delete: Vec::new(),
    };
    archive_edit_start(events, request, progress_interval_ms).await
}

/// The read-only refusal for a path that should have crossed an archive boundary
/// but didn't confirm (a mislabeled or vanished `.zip`).
fn read_only_error(path: &Path) -> WriteOperationError {
    WriteOperationError::ReadOnlyDevice {
        path: path.display().to_string(),
        device_name: None,
    }
}

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

    let (archive_path, dest_inner) =
        archive::confirm_archive_boundary(&dest_full_path).ok_or_else(|| read_only_error(&dest_full_path))?;
    let dest_inner = normalize_inner_path(&dest_inner);

    // Enumerate the sources + parse the index off the async executor.
    let archive_for_build = archive_path.clone();
    let sources_for_build = absolute_sources.clone();
    let plan = tokio::task::spawn_blocking(move || {
        build_copy_into_changeset(&archive_for_build, &sources_for_build, &dest_inner, conflict)
    })
    .await
    .map_err(|e| WriteOperationError::IoError {
        path: String::new(),
        message: format!("the archive copy couldn't be planned: {e}"),
    })??;

    let move_sources_to_delete = if is_move && !plan.any_skipped {
        absolute_sources
    } else {
        Vec::new()
    };
    let summary_source = plan
        .changeset
        .adds
        .first()
        .map(|a| a.inner_path.rsplit('/').next().unwrap_or(&a.inner_path).to_string());

    let request = ArchiveEditRequest {
        archive_path,
        parent_volume_id,
        changeset: plan.changeset,
        summary: OperationSummaryText {
            source: summary_source,
            destination: None,
        },
        move_sources_to_delete,
    };
    archive_edit_start(events, request, progress_interval_ms).await
}

/// The planned changeset for a copy/move-into, plus whether any file was skipped
/// (which suppresses the source deletion on a move).
struct CopyIntoPlan {
    changeset: Changeset,
    any_skipped: bool,
}

/// Walks the local sources and builds the `{ add + delete + mkdir }` changeset,
/// resolving each file conflict against the archive index per `conflict`.
/// Blocking (parses the index, stats/walks the tree) — call from `spawn_blocking`.
fn build_copy_into_changeset(
    archive_path: &Path,
    absolute_sources: &[PathBuf],
    dest_inner: &str,
    conflict: ConflictResolution,
) -> Result<CopyIntoPlan, WriteOperationError> {
    let source = LocalFileSource::open(archive_path).map_err(|e| WriteOperationError::WriteError {
        path: archive_path.display().to_string(),
        message: e.to_string(),
    })?;
    let index = ArchiveIndex::parse(&source).map_err(|e| WriteOperationError::WriteError {
        path: archive_path.display().to_string(),
        message: e.to_string(),
    })?;

    let mut adds: Vec<AddEntry> = Vec::new();
    let mut mkdirs: Vec<String> = Vec::new();
    let mut deletes: Vec<String> = Vec::new();
    // Inner paths this changeset already claims (dedup + Rename uniqueness).
    let mut planned: HashSet<String> = HashSet::new();
    let mut any_skipped = false;

    for src in absolute_sources {
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let base_inner = join_inner_str(dest_inner, name);
        let meta = std::fs::symlink_metadata(src).map_err(|e| WriteOperationError::ReadError {
            path: src.display().to_string(),
            message: e.to_string(),
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
                        &index,
                        conflict,
                        &mut adds,
                        &mut deletes,
                        &mut planned,
                        &mut any_skipped,
                    )?;
                }
                // Symlinks and special files inside the tree are skipped (never
                // materialize a symlink from source into the archive).
            }
        } else if meta.is_file() {
            plan_file_add(
                base_inner,
                src,
                &index,
                conflict,
                &mut adds,
                &mut deletes,
                &mut planned,
                &mut any_skipped,
            )?;
        }
    }

    Ok(CopyIntoPlan {
        changeset: Changeset {
            adds,
            mkdirs,
            deletes,
            renames: Vec::new(),
        },
        any_skipped,
    })
}

/// Resolves one file's conflict against the index + already-planned paths and
/// appends the resulting add (and a delete, for an overwrite). Mutates the
/// accumulators in place.
#[allow(
    clippy::too_many_arguments,
    reason = "shared accumulators for one pass; bundling them into a struct adds ceremony without clarity"
)]
fn plan_file_add(
    inner: String,
    src_path: &Path,
    index: &ArchiveIndex,
    conflict: ConflictResolution,
    adds: &mut Vec<AddEntry>,
    deletes: &mut Vec<String>,
    planned: &mut HashSet<String>,
    any_skipped: &mut bool,
) -> Result<(), WriteOperationError> {
    let in_index = index.exists(&inner);
    let collides = in_index || planned.contains(&inner);

    let target = if collides {
        match conflict {
            ConflictResolution::Skip => {
                *any_skipped = true;
                return Ok(());
            }
            ConflictResolution::Stop => {
                return Err(WriteOperationError::DestinationExists { path: inner });
            }
            ConflictResolution::Overwrite => {
                if in_index {
                    deletes.push(inner.clone());
                }
                inner
            }
            ConflictResolution::Rename => find_unique_inner(&inner, index, planned),
            ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
                if in_index && conditional_overwrites(conflict, index, &inner, src_path) {
                    deletes.push(inner.clone());
                    inner
                } else {
                    *any_skipped = true;
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

/// Whether a conditional policy overwrites the existing entry: `OverwriteSmaller`
/// only when the destination is strictly smaller than the source, `OverwriteOlder`
/// only when the destination is strictly older. Missing metadata never overwrites
/// (strict comparison, matching the local-FS conflict reducer).
fn conditional_overwrites(conflict: ConflictResolution, index: &ArchiveIndex, inner: &str, src_path: &Path) -> bool {
    let Some(node) = index.get(inner) else {
        return false;
    };
    let Ok(src_meta) = std::fs::metadata(src_path) else {
        return false;
    };
    match conflict {
        ConflictResolution::OverwriteSmaller => node.size.is_some_and(|dest_size| dest_size < src_meta.len()),
        ConflictResolution::OverwriteOlder => {
            let (Some(dest_mtime), Ok(src_mtime)) = (node.modified, src_meta.modified()) else {
                return false;
            };
            let Ok(src_secs) = src_mtime.duration_since(std::time::UNIX_EPOCH) else {
                return false;
            };
            dest_mtime < src_secs.as_secs() as i64
        }
        _ => false,
    }
}

/// Finds a unique inner path by appending ` (1)`, ` (2)`, … before the extension,
/// avoiding both existing archive entries and already-planned paths.
fn find_unique_inner(inner: &str, index: &ArchiveIndex, planned: &HashSet<String>) -> String {
    let (stem, ext) = match inner.rsplit_once('.') {
        // Keep an extension only when there's a stem before the dot (not a dotfile).
        Some((stem, ext)) if !stem.rsplit('/').next().unwrap_or(stem).is_empty() => {
            (stem.to_string(), format!(".{ext}"))
        }
        _ => (inner.to_string(), String::new()),
    };
    for n in 1..=9999 {
        let candidate = format!("{stem} ({n}){ext}");
        if !index.exists(&candidate) && !planned.contains(&candidate) {
            return candidate;
        }
    }
    // Astronomically unlikely; fall back to a uuid suffix so we never loop forever.
    format!("{stem} ({}){ext}", Uuid::new_v4())
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
            let _settled = WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::ArchiveEdit,
                settle_volume,
            );

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
/// pool. Handles both files and directory trees. Best-effort per source (a
/// failure leaves the file in both places — an incomplete move, never data loss).
async fn delete_move_sources(sources: &[PathBuf]) {
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
