//! Rename validation and the managed rename mutation.
//!
//! The command layer (`commands/rename.rs`) is a thin pass-through: it expands
//! tilde, resolves the `volume_id`, and calls into here wrapped in its IPC
//! timeout tiers (2 s validity/permission, 5 s rename). All the business logic
//! lives here per "smart backend / thin frontend".
//!
//! - **Validation** (`check_rename_permission_sync`, `check_rename_validity_impl`)
//!   is the snappy, UNMANAGED path: read-only, runs per-keystroke / on-commit,
//!   never touches the operation manager.
//! - **The mutation** (`rename_managed`) is a managed instant op: it runs the
//!   actual rename inside `manager::run_instant`, so it registers a `Running`
//!   record + marks its volume busy (eject guard) for its sub-second duration,
//!   yet still runs inline and returns its `Result` to the caller. It does NOT
//!   reserve a lane or queue behind transfers (see `manager::run_instant`).

use std::path::{Path, PathBuf};

use super::archive_edit::{self, ArchiveEditRequest};
use super::manager::{self, OperationDescriptor, OperationSummaryText};
use super::mutation_error::MutationError;
use super::types::WriteOperationType;
use crate::file_system::volume::backends::archive;
use crate::file_system::volume::backends::archive::mutator::Changeset;
use crate::operation_log::types::{Initiator, OpKind};

mod bulk;

pub(crate) use bulk::{BulkRenameRow, start_bulk_rename};

/// Result of a rename validity check.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RenameValidityResult {
    /// Whether the new name is valid (passes filename validation).
    pub valid: bool,
    /// Validation error message, if any.
    pub error: Option<crate::file_system::validation::ValidationError>,
    /// Whether a conflict exists (a sibling with the same name).
    pub has_conflict: bool,
    /// If there's a conflict, whether it's a case-only rename of the same file (same inode).
    pub is_case_only_rename: bool,
    /// Conflicting file info, if any.
    pub conflict: Option<ConflictFileInfo>,
}

/// Metadata about a conflicting sibling file.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConflictFileInfo {
    pub name: String,
    /// In bytes.
    pub size: u64,
    /// Unix timestamp in seconds.
    pub modified: Option<i64>,
    pub is_directory: bool,
}

/// Renames a file or directory as a managed instant op. When `force` is true,
/// proceeds even if the destination exists.
///
/// Runs inside `manager::run_instant`, so the volume is marked busy for the
/// mutation's (sub-second) duration and the op shows briefly in the queue, while
/// the rename still runs inline and its `Result` is returned to the caller. The
/// closure registers BOTH halves of the rename with the downloads watcher's
/// ignore set (a must-know invariant: `to` so the rename-arrival event is
/// suppressed, `from` so a Cmdr-initiated move OUT of Downloads is also
/// suppressed) BEFORE the syscall, then notifies the listing cache.
///
/// `from`/`to` are already tilde-expanded (root) or volume-relative (non-root)
/// by the command layer. `volume_id` is `"root"` for the local filesystem.
pub(crate) async fn rename_managed(
    from: PathBuf,
    to: PathBuf,
    force: bool,
    volume_id: String,
    initiator: Initiator,
) -> Result<(), MutationError> {
    // The emit wraps the whole driver rather than sitting at its ends: the archive
    // route is an early return, so a per-branch emit would count the filesystem
    // renames and quietly miss every in-zip one.
    let (result, target) = rename_managed_inner(from, to, force, volume_id, initiator).await;
    super::analytics::emit_rename_analytics(initiator, target, &result);
    result
}

/// The rename itself. Returns where it landed alongside its result so the wrapper
/// above can label the event without re-deriving the route.
async fn rename_managed_inner(
    from: PathBuf,
    to: PathBuf,
    force: bool,
    volume_id: String,
    initiator: Initiator,
) -> (Result<(), MutationError>, super::analytics::InstantTarget) {
    // Renaming a path INSIDE an archive is a zip mutation: route it to the
    // managed archive-edit driver. The `.zip` file itself is a regular file —
    // renaming it must work like any other file — so only a genuinely-inner path
    // routes here. Parent-aware detection (not the `std::fs`-only sync predicate)
    // so a rename inside a REMOTE zip (direct SMB / MTP) routes too.
    let manager = crate::file_system::volume::manager::get_volume_manager();
    if manager.path_is_inside_archive(&volume_id, &from).await || manager.path_is_inside_archive(&volume_id, &to).await
    {
        return (
            route_archive_rename(&from, &to, &volume_id).await,
            super::analytics::InstantTarget::Archive,
        );
    }

    let is_root = volume_id == "root";
    let descriptor = rename_descriptor(&from, &to, &volume_id);
    // Journal the rename as a single-item op under the REAL volume id. Snapshot the
    // source kind BEFORE the closure moves `from`/`to` and the rename fires: local
    // reads size + mtime for free from `symlink_metadata`; a volume probes only the
    // kind (dir vs file) for the row's `entry_type` — one metadata call for a
    // single-item op, since size/mtime aren't cheaply available on MTP (recorded
    // `None`, acceptable for a record).
    let op_id = descriptor.operation_id.clone();
    // Cloned so the record after the op (which moves `volume_id` into the closure)
    // still has the id.
    let journal_volume_id = volume_id.clone();
    let local_meta = if is_root {
        std::fs::symlink_metadata(&from).ok()
    } else {
        None
    };
    let volume_is_dir = if is_root {
        false
    } else {
        match manager.get(&volume_id) {
            // Truthful-enough default, unlike the copy/move/delete probes: this
            // value only labels the journal row's entry type for the undo
            // history. It reaches no destructive branch, so a mislabeled undo
            // entry is the whole cost of getting it wrong.
            // allowed-probe-unwrap: labels an undo-history row only; reaches no destructive branch
            Some(v) => v.is_directory(&from).await.unwrap_or(false),
            None => false,
        }
    };
    super::journal::open_volume_op(&op_id, OpKind::Rename, initiator, &journal_volume_id, None, 1);
    let journal_snapshot = Some((from.clone(), to.clone(), local_meta, volume_is_dir));

    let result = manager::manager()
        .run_instant(descriptor, async move {
            // Register both halves of the rename with the downloads watcher's
            // ignore set BEFORE the syscall (no-ops outside ~/Downloads).
            crate::downloads::note_pending_write_for_cmdr(&from);
            crate::downloads::note_pending_write_for_cmdr(&to);

            if !is_root {
                // Volume-aware rename (MTP, SMB, and other non-local volumes).
                // The volume's `rename` calls `notify_mutation` internally, so
                // the listing cache updates automatically.
                let volume = crate::file_system::volume::manager::get_volume_manager()
                    .get(&volume_id)
                    .ok_or(MutationError::VolumeGone {
                        volume_id: volume_id.clone(),
                    })?;
                // A taken name is reported by the NAME, matching the local branch and
                // the live validation the user just saw; everything else rides as the
                // volume's own typed answer.
                volume.rename(&from, &to, force).await.map_err(|e| match e {
                    crate::file_system::VolumeError::AlreadyExists(_) => {
                        MutationError::AlreadyExists { name: name_of(&to) }
                    }
                    other => MutationError::Volume { error: other },
                })
            } else {
                // Local filesystem rename on the blocking pool.
                let from_syscall = from.clone();
                let to_syscall = to.clone();
                tokio::task::spawn_blocking(move || {
                    if !force && from_syscall != to_syscall && std::fs::symlink_metadata(&to_syscall).is_ok() {
                        return Err(MutationError::AlreadyExists {
                            name: name_of(&to_syscall),
                        });
                    }
                    // The two paths mean different things: `ENOENT` is the source
                    // that's gone, `EEXIST` the destination that isn't free.
                    std::fs::rename(&from_syscall, &to_syscall).map_err(|e| MutationError::Volume {
                        error: crate::file_system::volume::backends::rename_volume_error(
                            &e,
                            &from_syscall,
                            &to_syscall,
                        ),
                    })
                })
                .await
                .map_err(|e| MutationError::Unexpected {
                    detail: format!("the rename task didn't finish: {e}"),
                })??;

                // Notify the listing cache about the rename (the volume path does
                // this itself via `notify_mutation`; the local path must do it
                // explicitly).
                notify_rename_in_listing(&volume_id, &from, &to).await;
                Ok(())
            }
        })
        .await;

    // Journal the rename as a single-item op: source → dest, one row (the whole
    // subtree moves by one rename). Rollbackable iff unchanged (rechecked at
    // rollback time).
    if let Some((from_path, to_path, meta, volume_is_dir)) = journal_snapshot {
        use crate::operation_log::types::{EntryType, ExecutionStatus, ItemOutcome, OpKind};
        match &result {
            Ok(()) => {
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(volume_is_dir);
                let entry_type = if is_dir { EntryType::Dir } else { EntryType::File };
                super::journal::record_volume_leaf(
                    &op_id,
                    entry_type,
                    &journal_volume_id,
                    &from_path,
                    Some((&journal_volume_id, &to_path)),
                    meta.as_ref().map(|m| m.len() as i64),
                    meta.as_ref().and_then(super::journal::mtime_secs),
                    false,
                    ItemOutcome::Done,
                );
                super::journal::finalize_op(&op_id, OpKind::Rename, ExecutionStatus::Done);
            }
            Err(_) => super::journal::finalize_op(&op_id, OpKind::Rename, ExecutionStatus::Failed),
        }
    }
    (result, super::analytics::InstantTarget::Volume)
}

/// Routes an in-archive rename to the managed archive-edit driver. Both `from`
/// and `to` must resolve to the SAME archive (a rename within the zip). A
/// cross-boundary rename (in↔out of the archive) is a move, not a rename, and is
/// refused here — the FE routes those through copy/move.
///
/// Returns `Ok(())` once the managed op has STARTED (it runs asynchronously and
/// emits `write-progress`/`write-complete`), unlike a plain rename which
/// completes inline. The op id rides on the `operations-changed` queue snapshot.
async fn route_archive_rename(from: &Path, to: &Path, volume_id: &str) -> Result<(), MutationError> {
    // Confirmation happened at the routing site (parent-aware `path_is_inside_archive`),
    // so a pure string split suffices — and it works for a REMOTE zip, where the
    // `std::fs` confirm would wrongly fail. A `to` with no archive component means a
    // rename OUT of the archive (a move), which is refused here.
    let (from_archive, from_inner) =
        archive::archive_boundary_candidate(from).ok_or(MutationError::ArchiveNotEditable)?;
    let (to_archive, to_inner) = archive::archive_boundary_candidate(to).ok_or(MutationError::RenameOutOfArchive)?;
    if from_archive != to_archive {
        return Err(MutationError::RenameAcrossArchives);
    }
    // Only zip archives are writable; tar and 7z are browse + extract only.
    archive_edit::ensure_zip_writable(&from_archive).map_err(|_| MutationError::ArchiveReadOnly)?;

    let from_inner = archive_edit::normalize_inner_path(&from_inner);
    let to_inner = archive_edit::normalize_inner_path(&to_inner);
    if from_inner.is_empty() || to_inner.is_empty() {
        return Err(MutationError::ArchiveNotEditable);
    }

    // Reject renaming onto an existing inner name up front with the same friendly
    // message the real-FS rename uses, so the FE shows the standard "already
    // exists" copy instead of the raw `zip` "Duplicate filename" the mutator would
    // hit at write time — and no temp is built. (A no-op rename to the same name
    // is left to proceed; the mutator handles it harmlessly.)
    if to_inner != from_inner && archive_edit::archive_inner_exists(volume_id, &from_archive, &to_inner).await {
        return Err(MutationError::AlreadyExists { name: leaf(&to_inner) });
    }

    let events = archive_edit::global_tauri_sink().ok_or(MutationError::ArchiveEditNotReady)?;
    let summary = OperationSummaryText {
        source: Some(leaf(&from_inner)),
        destination: Some(leaf(&to_inner)),
    };
    let request = ArchiveEditRequest {
        archive_path: from_archive,
        parent_volume_id: volume_id.to_string(),
        changeset: Changeset {
            renames: vec![(from_inner, to_inner)],
            ..Default::default()
        },
        summary,
        move_sources_to_delete: Vec::new(),
        skipped_count: 0,
        // No scan preview: nothing walked a tree to plan this edit.
        preview_id: None,
    };
    archive_edit::archive_edit_start(events, request, 200)
        .await
        .map_err(|e| MutationError::ArchiveEditCouldntStart {
            detail: format!("{e:?}"),
        })?;
    Ok(())
}

/// The final component of a path, for the "already taken" message the rename
/// editor shows inline (which quotes the NAME, never the whole path).
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// The last `/`-separated component of an inner path (for the queue summary).
fn leaf(inner_path: &str) -> String {
    inner_path.rsplit('/').next().unwrap_or(inner_path).to_string()
}

/// Builds the instant-op descriptor for a rename: no lanes, a `from → to`
/// basename summary, and the volume marked busy for its duration for non-root
/// volumes. Root is never ejectable, so it marks nothing busy (no eject-menu
/// churn for local renames).
fn rename_descriptor(from: &Path, to: &Path, volume_id: &str) -> OperationDescriptor {
    fn name(p: &Path) -> String {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned())
    }
    let volume_ids = if volume_id == "root" {
        vec![]
    } else {
        vec![volume_id.to_string()]
    };
    OperationDescriptor {
        operation_id: crate::operation_log::new_operation_id(),
        operation_type: WriteOperationType::Rename,
        lanes: vec![],
        volume_ids,
        summary: OperationSummaryText {
            source: Some(name(from)),
            destination: Some(name(to)),
        },
        // An instant metadata op has no partial state, and no cancel path that
        // could catch it mid-flight.
        supports_rollback: false,
        // No scan preview: nothing walked a tree to plan this op.
        preview_id: None,
    }
}

/// Notifies the listing cache about a rename via the volume's `notify_mutation`.
async fn notify_rename_in_listing(volume_id: &str, from: &Path, to: &Path) {
    use crate::file_system::volume::MutationEvent;

    // Plain `get`, not `resolve`: a rename INSIDE an archive is rejected upstream,
    // so this only ever runs for a normal file (incl. the `.zip` file itself),
    // which must notify through its own volume — never route to the ArchiveVolume.
    let volume = match crate::file_system::volume::manager::get_volume_manager().get(volume_id) {
        Some(v) => v,
        None => return,
    };

    if let (Some(from_parent), Some(from_name), Some(to_name)) = (from.parent(), from.file_name(), to.file_name()) {
        if from.parent() == to.parent() {
            // Same-directory rename
            volume
                .notify_mutation(
                    volume_id,
                    from_parent,
                    MutationEvent::Renamed {
                        from: from_name.to_string_lossy().to_string(),
                        to: to_name.to_string_lossy().to_string(),
                    },
                )
                .await;
        } else {
            // Cross-directory move
            volume
                .notify_mutation(
                    volume_id,
                    from_parent,
                    MutationEvent::Deleted(from_name.to_string_lossy().to_string()),
                )
                .await;
            if let Some(to_parent) = to.parent() {
                volume
                    .notify_mutation(
                        volume_id,
                        to_parent,
                        MutationEvent::Created(to_name.to_string_lossy().to_string()),
                    )
                    .await;
            }
        }
    }
}

/// Synchronous permission check: file exists, parent writable, and (macOS) not
/// immutable / SIP-protected. Runs in `spawn_blocking` at the command layer.
pub(crate) fn check_rename_permission_sync(path: &Path) -> Result<(), MutationError> {
    // Check that the file itself exists
    if std::fs::symlink_metadata(path).is_err() {
        return Err(MutationError::NotFound {
            path: path.display().to_string(),
        });
    }

    // Check parent directory is writable
    let parent = path.parent().ok_or(MutationError::CantRenameVolumeRoot)?;
    check_dir_writable(parent)?;

    // Check macOS-specific flags (immutable, SIP, locks)
    #[cfg(target_os = "macos")]
    check_macos_flags(path)?;

    Ok(())
}

/// Checks if a directory is writable using access(W_OK).
#[cfg(unix)]
fn check_dir_writable(dir: &Path) -> Result<(), MutationError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(dir.as_os_str().as_bytes()).map_err(|_| MutationError::NameHasDisallowedCharacter)?;
    // SAFETY: c_path is a valid null-terminated C string
    let result = unsafe { libc::access(c_path.as_ptr(), libc::W_OK) };
    if result != 0 {
        return Err(MutationError::ParentNotWritable {
            path: dir.display().to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_dir_writable(_dir: &Path) -> Result<(), MutationError> {
    Ok(())
}

/// Checks macOS-specific immutable/SIP/lock flags.
#[cfg(target_os = "macos")]
fn check_macos_flags(path: &Path) -> Result<(), MutationError> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| MutationError::NameHasDisallowedCharacter)?;

    let mut stat = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: c_path is valid, stat is a valid pointer
    let result = unsafe { libc::lstat(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        // Can't stat; file may have been deleted, let the rename itself fail with a clear error
        return Ok(());
    }

    // SAFETY: lstat succeeded
    let stat = unsafe { stat.assume_init() };

    // UF_IMMUTABLE (user immutable / "uchg" flag)
    const UF_IMMUTABLE: u32 = 0x00000002;
    // SF_IMMUTABLE (system immutable, set by SIP)
    const SF_IMMUTABLE: u32 = 0x00020000;

    if (stat.st_flags & UF_IMMUTABLE) != 0 {
        return Err(MutationError::FileLocked {
            path: path.display().to_string(),
        });
    }
    if (stat.st_flags & SF_IMMUTABLE) != 0 {
        return Err(MutationError::SipProtected {
            path: path.display().to_string(),
        });
    }

    Ok(())
}

/// Validates a new filename and checks for conflicts in the same directory.
/// Uses inode comparison to detect case-only renames (valid on case-insensitive
/// APFS). When `volume_id` is not `"root"`, uses the Volume trait for conflict
/// detection (needed for MTP and other non-local volumes).
pub(crate) async fn check_rename_validity_impl(
    dir: String,
    old_name: String,
    new_name: String,
    volume_id: String,
) -> RenameValidityResult {
    use crate::file_system::validation::{validate_filename, validate_path_length};

    let trimmed = new_name.trim();

    // Validate filename
    if let Err(error) = validate_filename(trimmed) {
        return RenameValidityResult {
            valid: false,
            error: Some(error),
            has_conflict: false,
            is_case_only_rename: false,
            conflict: None,
        };
    }

    // Validate resulting path length
    let new_path = PathBuf::from(&dir).join(trimmed);
    if let Err(error) = validate_path_length(&new_path) {
        return RenameValidityResult {
            valid: false,
            error: Some(error),
            has_conflict: false,
            is_case_only_rename: false,
            conflict: None,
        };
    }

    // Check for conflict: does a sibling with this name already exist?
    let old_path = PathBuf::from(&dir).join(&old_name);

    if volume_id != "root" {
        // Non-local volume: use Volume trait for conflict detection
        let conflict_info = check_sibling_conflict_via_volume(&volume_id, &new_path).await;
        RenameValidityResult {
            valid: true,
            error: None,
            has_conflict: conflict_info.0,
            // MTP is case-sensitive, no case-only rename ambiguity
            is_case_only_rename: false,
            conflict: conflict_info.1,
        }
    } else {
        // Local filesystem: use symlink_metadata with inode comparison
        let conflict_info = check_sibling_conflict(&old_path, &new_path);
        RenameValidityResult {
            valid: true,
            error: None,
            has_conflict: conflict_info.0,
            is_case_only_rename: conflict_info.1,
            conflict: conflict_info.2,
        }
    }
}

/// Checks if a file with `new_path` exists and whether it's the same inode as `old_path`
/// (case-only rename on case-insensitive FS).
#[cfg(unix)]
fn check_sibling_conflict(old_path: &Path, new_path: &Path) -> (bool, bool, Option<ConflictFileInfo>) {
    use std::os::unix::fs::MetadataExt;

    let new_meta = match std::fs::symlink_metadata(new_path) {
        Ok(m) => m,
        Err(_) => return (false, false, None), // No conflict
    };

    // Check if it's the same inode (case-only rename)
    let is_same_inode = std::fs::symlink_metadata(old_path)
        .map(|old_meta| old_meta.dev() == new_meta.dev() && old_meta.ino() == new_meta.ino())
        .unwrap_or(false);

    let modified = new_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let conflict = ConflictFileInfo {
        name: new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: new_meta.len(),
        modified,
        is_directory: new_meta.is_dir(),
    };

    (true, is_same_inode, Some(conflict))
}

#[cfg(not(unix))]
fn check_sibling_conflict(_old_path: &Path, new_path: &Path) -> (bool, bool, Option<ConflictFileInfo>) {
    let new_meta = match std::fs::symlink_metadata(new_path) {
        Ok(m) => m,
        Err(_) => return (false, false, None),
    };

    let modified = new_meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let conflict = ConflictFileInfo {
        name: new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        size: new_meta.len(),
        modified,
        is_directory: new_meta.is_dir(),
    };

    // Without inode comparison, we can't detect case-only renames
    (true, false, Some(conflict))
}

/// Checks if a file with `new_path` exists on a non-local volume using the Volume trait's
/// `get_metadata`.
async fn check_sibling_conflict_via_volume(volume_id: &str, new_path: &Path) -> (bool, Option<ConflictFileInfo>) {
    // Plain `get`, not `resolve`: renaming INTO an archive is rejected upstream, so
    // the target is always a normal sibling (incl. a `.zip` file), checked on its
    // own volume — routing to the ArchiveVolume would mis-consult the zip's index.
    let volume = match crate::file_system::volume::manager::get_volume_manager().get(volume_id) {
        Some(v) => v,
        None => return (false, None),
    };

    let entry = match volume.get_metadata(new_path).await {
        Ok(e) => e,
        Err(_) => return (false, None), // No conflict: file doesn't exist
    };

    let conflict = ConflictFileInfo {
        name: entry.name,
        size: entry.size.unwrap_or(0),
        modified: entry.modified_at.map(|t| t as i64),
        is_directory: entry.is_directory,
    };

    (true, Some(conflict))
}

#[cfg(test)]
mod tests;
