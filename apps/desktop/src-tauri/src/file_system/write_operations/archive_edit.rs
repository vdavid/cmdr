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
use super::conflict::{ApplyToAll, apply_to_all_effective, apply_to_all_record};
use super::manager::{self, ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::operation_intent::is_cancelled;
use super::state::{ConflictResolutionResponse, WriteOperationState, WriteSettledGuard};
use super::types::{
    ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteErrorEvent,
    WriteOperationError, WriteOperationPhase, WriteOperationStartResult, WriteOperationType, WriteProgressEvent,
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

    // Stop policy → the interactive per-file prompt. Planning must run INSIDE the
    // managed op (so the op is registered and `resolve_write_conflict(op_id)` can
    // reach the oneshot), so we hand the raw inputs to a dedicated start that
    // plans-then-applies inside its deferred. Pre-resolved policies keep the
    // up-front, non-interactive planning below.
    if matches!(conflict, ConflictResolution::Stop) {
        return archive_copy_into_interactive_start(
            events,
            absolute_sources,
            archive_path,
            dest_inner,
            parent_volume_id,
            is_move,
            progress_interval_ms,
        )
        .await;
    }

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

/// How a copy/move-into resolves collisions with existing archive entries.
enum ConflictMode<'a> {
    /// A pre-resolved policy applied to every collision, no prompt. `Stop` in this
    /// mode is a hard `DestinationExists` (the interactive path handles real Stop).
    Policy(ConflictResolution),
    /// Interactive per-file prompting (the FE's Stop UX): each file collision emits
    /// a `write-conflict` and blocks on the user's answer, honoring the shared
    /// `ApplyToAll` latch. Dir collisions never reach here (they merge silently).
    Interactive {
        events: &'a dyn OperationEventSink,
        operation_id: &'a str,
        state: &'a Arc<WriteOperationState>,
        apply_to_all: &'a mut ApplyToAll,
    },
}

/// A planning failure that separates a user cancel (archive untouched, nothing to
/// report as an error) from a genuine fault.
enum PlanError {
    /// The user cancelled a Stop prompt (the oneshot sender was dropped).
    Cancelled,
    /// A real planning fault (unreadable source, unparseable archive, Stop under a
    /// pre-resolved policy).
    Op(WriteOperationError),
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
    let index = ArchiveIndex::parse(&source).map_err(|e| {
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
    let mut any_skipped = false;

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
                archive_path,
                &index,
                mode,
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
    any_skipped: &mut bool,
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
                *any_skipped = true;
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

/// Produces the concrete resolution for a collision. `Policy` returns its fixed
/// choice; `Interactive` consults the `ApplyToAll` latch and otherwise prompts the
/// user (storing the oneshot sender BEFORE emitting `write-conflict`, then blocking
/// on the answer — the Stop-mode ordering must-know).
fn resolve_effective(
    mode: &mut ConflictMode<'_>,
    inner: &str,
    src_path: &Path,
    archive_path: &Path,
    index: &ArchiveIndex,
    is_file_to_folder: bool,
) -> Result<ConflictResolution, PlanError> {
    match mode {
        ConflictMode::Policy(c) => Ok(*c),
        ConflictMode::Interactive {
            events,
            operation_id,
            state,
            apply_to_all,
        } => {
            if let Some(saved) = apply_to_all_effective(apply_to_all, is_file_to_folder) {
                return Ok(saved);
            }
            let response = prompt_archive_conflict(
                *events,
                operation_id,
                state,
                index,
                inner,
                src_path,
                archive_path,
                is_file_to_folder,
            )?;
            apply_to_all_record(
                apply_to_all,
                is_file_to_folder,
                response.resolution,
                response.apply_to_all,
            );
            Ok(response.resolution)
        }
    }
}

/// Emits a `write-conflict` for an in-archive file collision and blocks on the
/// user's answer. Stores the oneshot sender BEFORE the emit (a responder can only
/// answer a conflict it has observed; emit-first races the take and hangs the
/// recv). A dropped sender (cancel) surfaces as `PlanError::Cancelled`.
#[allow(
    clippy::too_many_arguments,
    reason = "the prompt gathers both sides' metadata from distinct sources (local file + archive index); bundling adds ceremony"
)]
fn prompt_archive_conflict(
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    index: &ArchiveIndex,
    inner: &str,
    src_path: &Path,
    archive_path: &Path,
    is_file_to_folder: bool,
) -> Result<ConflictResolutionResponse, PlanError> {
    let node = index.get(inner);
    let dest_size = node.as_ref().and_then(|n| n.size);
    let dest_modified = node.as_ref().and_then(|n| n.modified);
    let src_meta = std::fs::metadata(src_path).ok();
    let source_size = src_meta.as_ref().map(std::fs::Metadata::len);
    let source_modified = src_meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let destination_is_newer = matches!((source_modified, dest_modified), (Some(s), Some(d)) if d > s);
    let size_difference = match (dest_size, source_size) {
        (Some(d), Some(s)) => Some(d as i64 - s as i64),
        _ => None,
    };

    // Store the sender BEFORE the emit (see doc comment); released as the
    // statement ends, never held across the emit or the blocking recv.
    let (tx, rx) = tokio::sync::oneshot::channel();
    *state.conflict_resolution_tx.lock_ignore_poison() = Some(tx);

    events.emit_conflict(WriteConflictEvent {
        operation_id: operation_id.to_string(),
        source_path: src_path.display().to_string(),
        destination_path: archive_path.join(inner).display().to_string(),
        source_size,
        destination_size: dest_size,
        source_modified,
        destination_modified: dest_modified,
        destination_is_newer,
        size_difference,
        source_is_directory: false,
        destination_is_directory: is_file_to_folder,
    });

    // Blocking recv: the planner runs on the blocking pool (like the local-FS Stop
    // path), so parking this thread on the oneshot is correct. A dropped sender
    // (cancel) returns `Err` → `Cancelled`.
    rx.blocking_recv().map_err(|_| PlanError::Cancelled)
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

/// Starts an INTERACTIVE copy/move INTO a zip (Stop policy) as a managed op. The
/// changeset is planned inside the op's deferred (on the blocking pool) so that
/// each in-archive file collision can emit a `write-conflict` and block on the
/// user's answer via the op's registered oneshot; only after planning resolves
/// does the mutator rewrite the archive. Dir-vs-dir collisions merge silently.
async fn archive_copy_into_interactive_start(
    events: Arc<dyn OperationEventSink>,
    absolute_sources: Vec<PathBuf>,
    archive_path: PathBuf,
    dest_inner: String,
    parent_volume_id: String,
    is_move: bool,
    progress_interval_ms: u64,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let operation_id = Uuid::new_v4().to_string();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    let lane = get_volume_manager()
        .get(&parent_volume_id)
        .map(|v| v.lane_key())
        .unwrap_or_else(|| LaneKey::new(parent_volume_id.clone()));
    let summary_source = absolute_sources
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

            let hooks = Arc::new(MutatorHooks {
                state: Arc::clone(&state),
                events: Arc::clone(&events),
                operation_id: op_id.clone(),
                operation_type: WriteOperationType::ArchiveEdit,
                progress_interval,
                last_emit: Mutex::new(None),
                latest: Mutex::new(MutationProgress::default()),
            });

            // Plan (with prompting) then apply, both on the blocking pool. Returns
            // the move sources to delete after a committed MOVE (empty otherwise).
            let hooks_for_blocking = Arc::clone(&hooks);
            let events_for_blocking = Arc::clone(&events);
            let state_for_blocking = Arc::clone(&state);
            let op_id_for_blocking = op_id.clone();
            let archive_for_blocking = archive_path.clone();
            let result = tokio::task::spawn_blocking(move || -> Result<Vec<PathBuf>, PlanError> {
                let plan = build_copy_into_changeset_interactive(
                    &archive_for_blocking,
                    &absolute_sources,
                    &dest_inner,
                    &*events_for_blocking,
                    &op_id_for_blocking,
                    &state_for_blocking,
                )?;
                let move_sources = if is_move && !plan.any_skipped {
                    absolute_sources.clone()
                } else {
                    Vec::new()
                };
                mutator::apply(&archive_for_blocking, &plan.changeset, &*hooks_for_blocking).map_err(|e| match e {
                    MutationError::Cancelled => PlanError::Cancelled,
                    other => PlanError::Op(to_write_error(&archive_for_blocking, other)),
                })?;
                Ok(move_sources)
            })
            .await;

            let final_progress = *hooks.latest.lock_ignore_poison();
            match result {
                Ok(Ok(move_sources)) => {
                    // Move invariant: delete the local sources only now that the
                    // archive rewrite is durably committed.
                    delete_move_sources(&move_sources).await;
                    events.emit_complete(WriteCompleteEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::ArchiveEdit,
                        files_processed: final_progress.entries_total,
                        files_skipped: 0,
                        bytes_processed: final_progress.bytes_total,
                    });
                }
                Ok(Err(PlanError::Cancelled)) => {
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::ArchiveEdit,
                        files_processed: final_progress.entries_done,
                        rolled_back: false,
                    });
                }
                Ok(Err(PlanError::Op(err))) => {
                    events.emit_error(WriteErrorEvent::new(
                        op_id.clone(),
                        WriteOperationType::ArchiveEdit,
                        err,
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

/// Routes a MOVE whose SOURCE is inside a zip (extract-out + delete-inside) to a
/// single managed compound op. The op extracts the selected entries to the
/// destination through the ordinary cross-volume copy engine, then — only if the
/// whole extract landed cleanly — rewrites the archive with a `{ delete }` of
/// those entries. This is the MOVE INVARIANT for archives: the archive-side
/// delete runs ONLY after every destination file is durably committed (the copy
/// engine's `write_from_stream` fsyncs each file), so a crash or cancel can never
/// lose both copies.
///
/// **Partial-move policy: batch all-or-nothing (skip / error / cancel aware).**
/// The archive entries are deleted ONLY when the extract completed with zero
/// skips, zero errors, and no cancel. Any skip (a destination collision the
/// policy resolved to Skip), any failure, or a cancel leaves the archive
/// byte-for-byte intact and deletes nothing — the landed copies stay at the
/// destination and the move degrades to a copy for that run. The archive delete
/// is itself one atomic O(archive) rewrite, so batch granularity is the natural
/// unit and this sidesteps the partial-merge-skip hazard (a directory source with
/// an inner skipped child must not have its whole subtree deleted). Per-entry
/// deletion is a future refinement.
///
/// `source_volume` is the read-only `ArchiveVolume`; `source_volume_id` is the
/// parent drive's id (the FE tab volume) used for the lane + busy set. All
/// `source_paths` must live in ONE archive (a multi-select can't straddle zips in
/// one pane).
#[allow(
    clippy::too_many_arguments,
    reason = "the archive-source→dest move threads both volumes with their ids plus the resolved dest path and config; a struct would just shuffle the same fields"
)]
pub(crate) async fn route_archive_move_out(
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_volume: Arc<dyn Volume>,
    dest_path: PathBuf,
    config: crate::file_system::VolumeCopyConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Resolve the one archive every source belongs to, and the inner path of each
    // (the `{ delete }` changeset needs archive-root-relative names).
    let first = source_paths.first().ok_or_else(|| WriteOperationError::IoError {
        path: String::new(),
        message: "no entries to move".to_string(),
    })?;
    let (archive_path, _) = archive::confirm_archive_boundary(first).ok_or_else(|| read_only_error(first))?;
    let mut inner_deletes = Vec::with_capacity(source_paths.len());
    for source in &source_paths {
        let (source_archive, inner) =
            archive::confirm_archive_boundary(source).ok_or_else(|| read_only_error(source))?;
        if source_archive != archive_path {
            return Err(WriteOperationError::IoError {
                path: source.display().to_string(),
                message: "can't move entries out of more than one archive at once".to_string(),
            });
        }
        inner_deletes.push(normalize_inner_path(&inner));
    }

    let operation_id = Uuid::new_v4().to_string();
    let progress_interval_ms = config.progress_interval_ms;
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)));

    // The op holds BOTH lanes (the archive's parent drive AND the destination),
    // so it serializes against other work on either device. The archive delete
    // runs on the parent drive's lane, which this op already owns.
    let lanes = vec![source_volume.lane_key(), dest_volume.lane_key()];
    let summary = OperationSummaryText {
        source: Some(source_volume.name().to_string()),
        destination: Some(dest_volume.name().to_string()),
    };
    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Move,
        lanes,
        volume_ids: vec![source_volume_id.clone(), dest_volume_id],
        summary,
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
            let settle_volume = (source_volume_id != "root").then(|| source_volume_id.clone());
            let _settled = WriteSettledGuard::new(
                Arc::clone(&events),
                op_id.clone(),
                WriteOperationType::Move,
                settle_volume,
            );

            // Phase 1 — extract. The suppressing sink forwards progress/conflict
            // to the FE but withholds the copy's terminal events (the compound op
            // owns the single Move terminal); it captures the completion so we can
            // read `files_skipped`.
            let suppress = Arc::new(SuppressTerminalsSink {
                inner: Arc::clone(&events),
                captured_complete: Mutex::new(None),
            });
            let extract = crate::file_system::write_operations::copy_volumes_with_progress(
                Arc::clone(&suppress) as Arc<dyn OperationEventSink>,
                &op_id,
                &state,
                Arc::clone(&source_volume),
                &source_paths,
                Arc::clone(&dest_volume),
                &dest_path,
                &config,
            )
            .await;

            let captured = suppress.captured_complete.lock_ignore_poison().take();
            let files_extracted = captured.as_ref().map(|c| c.files_processed).unwrap_or(0);
            let bytes_extracted = captured.as_ref().map(|c| c.bytes_processed).unwrap_or(0);
            let files_skipped = captured.as_ref().map(|c| c.files_skipped).unwrap_or(0);

            // A cancel that landed in the window between a clean extract and the
            // delete phase must still leave the archive untouched.
            if is_cancelled(&state.intent) {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: op_id.clone(),
                    operation_type: WriteOperationType::Move,
                    files_processed: files_extracted,
                    rolled_back: false,
                });
                task_guard.disarm();
                manager::manager().on_settled(&op_id);
                return;
            }

            match extract {
                Ok(()) if files_skipped == 0 => {
                    // Phase 2 — durable extract confirmed; rewrite the archive to
                    // drop the moved entries (the move invariant is satisfied: the
                    // copy engine fsynced every destination file before returning).
                    let changeset = Changeset {
                        deletes: inner_deletes,
                        ..Default::default()
                    };
                    let hooks = Arc::new(MutatorHooks {
                        state: Arc::clone(&state),
                        events: Arc::clone(&events),
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        progress_interval,
                        last_emit: Mutex::new(None),
                        latest: Mutex::new(MutationProgress::default()),
                    });
                    let hooks_for_blocking = Arc::clone(&hooks);
                    let archive_for_blocking = archive_path.clone();
                    let delete_result = tokio::task::spawn_blocking(move || {
                        mutator::apply(&archive_for_blocking, &changeset, &*hooks_for_blocking)
                    })
                    .await;
                    match delete_result {
                        Ok(Ok(())) => {
                            events.emit_complete(WriteCompleteEvent {
                                operation_id: op_id.clone(),
                                operation_type: WriteOperationType::Move,
                                files_processed: files_extracted,
                                files_skipped: 0,
                                bytes_processed: bytes_extracted,
                            });
                        }
                        Ok(Err(MutationError::Cancelled)) => {
                            // Cancelled during the archive rewrite: the extract
                            // already landed, the archive is intact (temp
                            // abandoned) — effectively a completed copy.
                            events.emit_cancelled(WriteCancelledEvent {
                                operation_id: op_id.clone(),
                                operation_type: WriteOperationType::Move,
                                files_processed: files_extracted,
                                rolled_back: false,
                            });
                        }
                        Ok(Err(err)) => {
                            // The extract landed but the archive couldn't be
                            // rewritten. The originals are intact and the copies
                            // are at the destination (no data loss), so surface
                            // the failure — the move degraded to a copy.
                            events.emit_error(WriteErrorEvent::new(
                                op_id.clone(),
                                WriteOperationType::Move,
                                to_write_error(&archive_path, err),
                            ));
                        }
                        Err(join_error) => {
                            events.emit_error(WriteErrorEvent::new(
                                op_id.clone(),
                                WriteOperationType::Move,
                                WriteOperationError::IoError {
                                    path: archive_path.display().to_string(),
                                    message: format!("archive delete task failed: {join_error}"),
                                },
                            ));
                        }
                    }
                }
                Ok(()) => {
                    // Extract completed but something was skipped (a destination
                    // collision the policy resolved to Skip). All-or-nothing:
                    // delete nothing, leave the archive intact. The landed copies
                    // stay at the destination.
                    events.emit_complete(WriteCompleteEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        files_processed: files_extracted,
                        files_skipped,
                        bytes_processed: bytes_extracted,
                    });
                }
                Err(failure) if matches!(failure.error, WriteOperationError::Cancelled { .. }) => {
                    // Cancel mid-extract: nothing was deleted, the archive is
                    // untouched. The copy engine already emitted nothing terminal
                    // (suppressed), so emit the Move cancel here.
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: op_id.clone(),
                        operation_type: WriteOperationType::Move,
                        files_processed: files_extracted,
                        rolled_back: false,
                    });
                }
                Err(failure) => {
                    // Extract failed: nothing deleted, archive untouched.
                    events.emit_error(WriteErrorEvent::new(
                        op_id.clone(),
                        WriteOperationType::Move,
                        failure.error,
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
        operation_type: WriteOperationType::Move,
    })
}

/// Wraps the real event sink for the extract phase of an out-of-zip move: it
/// forwards every non-terminal event (progress, conflict prompts, scan) to the FE
/// but WITHHOLDS the copy's terminal events (`write-complete` / `write-cancelled`
/// / `write-error`) — the compound move op emits the single Move terminal itself.
/// The completion is captured so the driver can read `files_skipped`.
struct SuppressTerminalsSink {
    inner: Arc<dyn OperationEventSink>,
    captured_complete: Mutex<Option<WriteCompleteEvent>>,
}

impl OperationEventSink for SuppressTerminalsSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        *self.captured_complete.lock_ignore_poison() = Some(event);
    }
    fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
    fn emit_error(&self, _event: WriteErrorEvent) {}
    fn emit_conflict(&self, event: WriteConflictEvent) {
        self.inner.emit_conflict(event);
    }
    fn emit_source_item_done(&self, event: super::types::WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
    }
    fn emit_scan_progress(&self, event: super::types::ScanProgressEvent) {
        self.inner.emit_scan_progress(event);
    }
    fn emit_scan_conflict(&self, conflict: super::types::ConflictInfo) {
        self.inner.emit_scan_conflict(conflict);
    }
    fn emit_dry_run_complete(&self, result: super::types::DryRunResult) {
        self.inner.emit_dry_run_complete(result);
    }
    fn emit_settled(&self, event: super::types::WriteSettledEvent) {
        self.inner.emit_settled(event);
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
                operation_type: WriteOperationType::ArchiveEdit,
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

#[cfg(test)]
#[path = "archive_edit_tests.rs"]
mod archive_edit_tests;
