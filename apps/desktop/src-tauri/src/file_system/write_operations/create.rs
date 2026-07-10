//! New-folder / new-file creation and the managed create mutations.
//!
//! The command layer (`commands/file_system/write_ops.rs`) is a thin
//! pass-through: it expands tilde, resolves the `volume_id`, calls
//! `create_directory_managed` / `create_file_managed` wrapped in its 5 s IPC
//! timeout, and maps the returned `String` error to `IpcError`. All the logic
//! lives here per "smart backend / thin frontend"; the backend never names the
//! command layer's `IpcError` or `expand_tilde`.
//!
//! Create is a managed instant op: the mutation runs inside
//! `manager::run_instant`, so it registers a `Running` record + marks its volume
//! busy (eject guard) for its sub-second duration, yet still runs inline and
//! returns the new path to the caller. It does NOT reserve a lane or queue behind
//! transfers (see `manager::run_instant`). There's no inner timeout: the
//! command's outer 5 s timeout drops the whole future on a hang, and the
//! `InstantTaskGuard` releases the busy set on that drop.
//!
//! The synthetic-listing-diff update (`emit_synthetic_entry_diff` /
//! `should_emit_synthetic_diff`) lives here, co-located with the create op it
//! serves: it's the listing-cache half of "a new entry appeared", and keeping it
//! next to the create keeps the command a pure pass-through.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::archive_edit::{self, ArchiveEditRequest};
use super::manager::{self, OperationDescriptor, OperationSummaryText};
use super::types::WriteOperationType;
use crate::file_system::get_volume_manager;
use crate::file_system::volume::backends::archive;
use crate::file_system::volume::backends::archive::mutator::{AddEntry, AddSource, Changeset};

/// Whether a routed archive add creates a directory entry or an empty file.
enum ArchiveEntryKind {
    Dir,
    File,
}

/// Parent-aware "does this create target land at or inside a `.zip`?" — the
/// routing gate for `create_directory_managed` / `create_file_managed`. `None`
/// volume means the local `"root"` drive. Uses `path_crosses` (not
/// `path_is_inside`) because the parent can BE the `.zip` file itself (creating an
/// entry at the archive root).
async fn parent_crosses_archive_boundary(volume_id: Option<&str>, parent_path: &str) -> bool {
    get_volume_manager()
        .path_crosses_archive_boundary(volume_id.unwrap_or("root"), Path::new(parent_path))
        .await
}

/// Creates a folder as a managed instant op and returns its new path. `parent_path`
/// is already tilde-expanded by the command layer.
///
/// Wraps `create_directory_core` in `manager::run_instant` (busy-marking the
/// volume + registering a brief `Running` record) and emits the synthetic listing
/// diff on success for local-FS-backed volumes.
pub(crate) async fn create_directory_managed(
    volume_id: Option<String>,
    parent_path: String,
    name: String,
) -> Result<String, String> {
    // A parent that crosses into a `.zip` means the new folder lands INSIDE the
    // archive: route to the managed archive-edit driver (an O(archive) rewrite
    // with a real progress bar), not the instant path. Returns the operation id
    // (the FE queue window shows it), not a filesystem path. Parent-aware detection
    // (not the `std::fs`-only sync predicate) so creating inside a REMOTE zip
    // (direct SMB / MTP) routes too.
    if parent_crosses_archive_boundary(volume_id.as_deref(), &parent_path).await {
        return route_archive_create(&parent_path, &name, ArchiveEntryKind::Dir, volume_id).await;
    }

    let volume_id_for_diff = volume_id.clone();
    // Journal local (root) creates only; volume (MTP/SMB) run_instant capture is M2f.
    let is_local = volume_id.as_deref().unwrap_or("root") == "root";
    let descriptor = instant_descriptor(WriteOperationType::CreateFolder, volume_id.as_deref(), &name);
    let op_id = descriptor.operation_id.clone();
    if is_local {
        super::journal::open_local_op(
            &op_id,
            crate::operation_log::types::OpKind::CreateFolder,
            crate::operation_log::types::Initiator::User,
            1,
        );
    }

    let result = manager::manager()
        .run_instant(descriptor, create_directory_core(volume_id, &parent_path, &name))
        .await;
    if is_local {
        super::journal::journal_instant_create(
            &op_id,
            crate::operation_log::types::OpKind::CreateFolder,
            crate::operation_log::types::EntryType::Dir,
            result.as_ref().ok().map(|(new_path, _)| new_path.as_path()),
        );
    }
    let (new_path, expanded_path) = result?;

    // Synthetic diff only works for volumes backed by the local filesystem.
    // Protocol-only volumes (MTP) handle UI updates through their own event systems.
    if should_emit_synthetic_diff(volume_id_for_diff.as_deref()) {
        emit_synthetic_entry_diff(volume_id_for_diff.as_deref(), &new_path, &PathBuf::from(&expanded_path));
    }
    Ok(new_path.to_string_lossy().to_string())
}

/// Creates an empty file as a managed instant op and returns its new path.
/// Same shape as [`create_directory_managed`].
pub(crate) async fn create_file_managed(
    volume_id: Option<String>,
    parent_path: String,
    name: String,
) -> Result<String, String> {
    // See `create_directory_managed`: a `.zip`-crossing parent routes the new
    // (empty) file into the archive via the managed edit driver (parent-aware, so
    // a REMOTE zip routes too).
    if parent_crosses_archive_boundary(volume_id.as_deref(), &parent_path).await {
        return route_archive_create(&parent_path, &name, ArchiveEntryKind::File, volume_id).await;
    }

    let volume_id_for_diff = volume_id.clone();
    let is_local = volume_id.as_deref().unwrap_or("root") == "root";
    let descriptor = instant_descriptor(WriteOperationType::CreateFile, volume_id.as_deref(), &name);
    let op_id = descriptor.operation_id.clone();
    if is_local {
        super::journal::open_local_op(
            &op_id,
            crate::operation_log::types::OpKind::CreateFile,
            crate::operation_log::types::Initiator::User,
            1,
        );
    }

    let result = manager::manager()
        .run_instant(descriptor, create_file_core(volume_id, &parent_path, &name))
        .await;
    if is_local {
        super::journal::journal_instant_create(
            &op_id,
            crate::operation_log::types::OpKind::CreateFile,
            crate::operation_log::types::EntryType::File,
            result.as_ref().ok().map(|(new_path, _)| new_path.as_path()),
        );
    }
    let (new_path, expanded_path) = result?;

    if should_emit_synthetic_diff(volume_id_for_diff.as_deref()) {
        emit_synthetic_entry_diff(volume_id_for_diff.as_deref(), &new_path, &PathBuf::from(&expanded_path));
    }
    Ok(new_path.to_string_lossy().to_string())
}

/// Routes an in-archive mkdir/mkfile to the managed archive-edit driver. Builds
/// a one-entry changeset (an explicit directory entry, or a zero-byte file),
/// starts the managed op, and returns its operation id.
///
/// Note the return-value shift versus the instant path: this yields the
/// OPERATION ID, not the new filesystem path (an archive-inner path isn't a real
/// FS path, and the edit is asynchronous). The FE distinguishes archive-target
/// creates and reads the id as an operation handle, not a cursor target.
async fn route_archive_create(
    parent_path: &str,
    name: &str,
    kind: ArchiveEntryKind,
    volume_id: Option<String>,
) -> Result<String, String> {
    // Confirmation happened at the routing site (parent-aware
    // `path_crosses_archive_boundary`), so a pure string split suffices — and it
    // works for a REMOTE zip, where the `std::fs` confirm would wrongly fail.
    let (archive_path, inner_parent) = archive::archive_boundary_candidate(Path::new(parent_path))
        .ok_or_else(|| "This archive can't be edited right now.".to_string())?;
    // Only zip archives are writable; tar and 7z are browse + extract only.
    archive_edit::ensure_zip_writable(&archive_path).map_err(|_| "This archive is read-only.".to_string())?;
    let inner_path = archive_edit::join_inner_path(&inner_parent, name);

    // Reject a duplicate up front with the same friendly message the real-FS
    // mkdir/mkfile paths use, so the FE shows the standard "already exists" copy
    // instead of the raw `zip` "Duplicate filename" the mutator would hit at
    // write time — and no temp is built for a doomed edit.
    let parent_volume_id = volume_id.as_deref().unwrap_or("root");
    if archive_edit::archive_inner_exists(parent_volume_id, &archive_path, &inner_path).await {
        return Err(format!("'{name}' already exists"));
    }

    let changeset = match kind {
        ArchiveEntryKind::Dir => Changeset {
            mkdirs: vec![inner_path],
            ..Default::default()
        },
        ArchiveEntryKind::File => Changeset {
            adds: vec![AddEntry {
                inner_path,
                source: AddSource::Bytes(Vec::new()),
            }],
            ..Default::default()
        },
    };

    let events =
        archive_edit::global_tauri_sink().ok_or_else(|| "The app isn't ready to edit archives yet.".to_string())?;
    let request = ArchiveEditRequest {
        archive_path,
        parent_volume_id: volume_id.unwrap_or_else(|| "root".to_string()),
        changeset,
        summary: OperationSummaryText {
            source: Some(name.to_string()),
            destination: None,
        },
        move_sources_to_delete: Vec::new(),
        skipped_count: 0,
    };
    let started = archive_edit::archive_edit_start(events, request, 200)
        .await
        .map_err(|e| format!("Couldn't start the archive edit: {e:?}"))?;
    Ok(started.operation_id)
}

/// Builds the instant-op descriptor: no lanes, the new entry's name as the
/// summary, and the volume marked busy for non-root volumes (`root` is never
/// ejectable, so it stays out of the busy set — no eject-menu churn for local
/// creates).
fn instant_descriptor(op_type: WriteOperationType, volume_id: Option<&str>, name: &str) -> OperationDescriptor {
    let volume_ids = match volume_id {
        None | Some("root") => vec![],
        Some(id) => vec![id.to_string()],
    };
    OperationDescriptor {
        operation_id: Uuid::new_v4().to_string(),
        operation_type: op_type,
        lanes: vec![],
        volume_ids,
        summary: OperationSummaryText {
            source: Some(name.to_string()),
            destination: None,
        },
    }
}

/// Core mkdir logic: validates the name, resolves the volume, registers the
/// downloads-watcher ignore, and issues the `create_directory` syscall. Returns
/// `(new_path, parent_path)` (the parent is the already-expanded path passed in).
/// Kept separate from the managed wrapper so it's testable without the operation
/// manager. `parent_path` must already be tilde-expanded by the caller.
pub(crate) async fn create_directory_core(
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), String> {
    if name.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\0') {
        return Err("Folder name contains invalid characters".to_string());
    }

    // Defensive fallback: the managed wrapper (`create_directory_managed`) routes
    // an archive-crossing parent to `route_archive_create` BEFORE reaching core,
    // so in production this branch never fires. It stays as a guard for a direct
    // `*_core` caller (tests): core's plain-FS create can't reach inside a `.zip`,
    // so it refuses rather than attempting a bogus filesystem write. Uses
    // `path_crosses` (NOT `path_is_inside`) so a `.zip` file AS the parent (a child
    // at the archive root) is caught too.
    if archive::path_crosses_archive_boundary(Path::new(parent_path)) {
        return Err("Creating inside an archive goes through the archive-edit path, not here".to_string());
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());
    let expanded_path = parent_path.to_string();

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let new_path = PathBuf::from(&expanded_path).join(name);

        // Register the new directory path with the downloads watcher's
        // ignore set; no-ops for paths outside ~/Downloads.
        crate::downloads::note_pending_write_for_cmdr(&new_path);

        volume.create_directory(&new_path).await.map_err(|e| match e {
            crate::file_system::VolumeError::AlreadyExists(_) => format!("'{}' already exists", name),
            crate::file_system::VolumeError::PermissionDenied(_) => {
                format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
            }
            _ => format!("Couldn't create folder: {}", e),
        })?;

        return Ok((new_path, expanded_path));
    }

    // "root" and every mounted volume is always registered in `VolumeManager`,
    // so reaching here means the volume was unregistered out from under us (e.g.
    // an unmount race). Error out instead of falling back to an untimed
    // synchronous `std::fs::create_dir` on the async executor, which would
    // violate this module's "every FS-touching command is timed" contract on a
    // hung mount.
    Err(format!("Volume not found: {}", volume_id))
}

/// Core file-creation logic. Same shape as [`create_directory_core`].
pub(crate) async fn create_file_core(
    volume_id: Option<String>,
    parent_path: &str,
    name: &str,
) -> Result<(PathBuf, String), String> {
    if name.is_empty() {
        return Err("File name cannot be empty".to_string());
    }
    if name.contains('/') || name.contains('\0') {
        return Err("File name contains invalid characters".to_string());
    }

    // Defensive fallback, same as `create_directory_core`: the managed wrapper
    // (`create_file_managed`) routes an archive-crossing parent to
    // `route_archive_create` before reaching core, so this branch is unreachable
    // in production. It guards a direct `*_core` caller (tests) from a bogus
    // plain-FS write into a `.zip`. `path_crosses` also catches a `.zip`-file parent.
    if archive::path_crosses_archive_boundary(Path::new(parent_path)) {
        return Err("Creating inside an archive goes through the archive-edit path, not here".to_string());
    }

    let volume_id = volume_id.unwrap_or_else(|| "root".to_string());
    let expanded_path = parent_path.to_string();

    // Try to use Volume abstraction
    if let Some(volume) = get_volume_manager().get(&volume_id) {
        let new_path = PathBuf::from(&expanded_path).join(name);

        // Register the new file path with the downloads watcher's ignore
        // set; no-ops for paths outside ~/Downloads.
        crate::downloads::note_pending_write_for_cmdr(&new_path);

        volume.create_file(&new_path, b"").await.map_err(|e| match e {
            crate::file_system::VolumeError::AlreadyExists(_) => format!("'{}' already exists", name),
            crate::file_system::VolumeError::PermissionDenied(_) => {
                format!("Permission denied: cannot create '{}' in '{}'", name, parent_path)
            }
            _ => format!("Couldn't create file: {}", e),
        })?;

        return Ok((new_path, expanded_path));
    }

    // See `create_directory_core`: an unregistered volume means an unmount race;
    // error out instead of an untimed `std::fs::File::create_new` fallback.
    Err(format!("Volume not found: {}", volume_id))
}

/// Returns true if a synthetic entry diff should be emitted for this volume.
/// Protocol-only volumes (like MTP) don't support `std::fs` access, so synthetic
/// diffs would fail. These volumes handle UI updates through their own event systems.
///
/// `pub(super)` so the sibling paste-clipboard writer reuses the same
/// listing-cache update the create op emits (cursor-land parity with mkfile).
pub(super) fn should_emit_synthetic_diff(volume_id: Option<&str>) -> bool {
    match volume_id {
        None => true, // No volume_id means local filesystem
        Some(id) => get_volume_manager()
            .get(id)
            .is_none_or(|v| v.supports_local_fs_access()),
    }
}

/// Queues a synthetic `directory-diff` event for a newly created entry.
///
/// Best-effort: if any step fails (stat, cache lookup, etc.) we log a warning
/// and return. The watcher will pick up the change later.
///
/// `pub(super)` so the sibling paste-clipboard writer reuses it (see
/// `should_emit_synthetic_diff`).
pub(super) fn emit_synthetic_entry_diff(volume_id: Option<&str>, entry_path: &Path, parent_path: &Path) {
    use crate::file_system::listing::diff_emitter::enqueue_diff;
    use crate::file_system::listing::reading::get_single_entry;
    use crate::file_system::listing::{find_listings_for_path, insert_entry_sorted};
    use crate::file_system::watcher::DiffChange;

    // 1. Construct a FileEntry for the new entry
    let mut entry = match get_single_entry(entry_path) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Synthetic entry diff: couldn't stat new entry: {}", e);
            return;
        }
    };

    // 2. Enrich with index data. `None` means the local filesystem (`root`); this
    // path only runs for local-FS volumes (`should_emit_synthetic_diff`).
    let volume_id = volume_id.unwrap_or(crate::indexing::ROOT_VOLUME_ID);
    crate::indexing::enrich_entries_with_index_on_volume(volume_id, std::slice::from_mut(&mut entry));

    // 3. Find affected listings
    let listings = find_listings_for_path(parent_path);
    if listings.is_empty() {
        return;
    }

    // 4. For each listing, insert and enqueue
    for (listing_id, _sort_by, _sort_order, _dir_sort_mode) in listings {
        // insert_entry_sorted acquires LISTING_CACHE write lock and releases it on return
        let Some(index) = insert_entry_sorted(&listing_id, entry.clone()) else {
            continue; // Already exists or listing gone
        };

        enqueue_diff(
            &listing_id,
            vec![DiffChange {
                change_type: "add".to_string(),
                entry: entry.clone(),
                index,
            }],
        );
    }
}

#[cfg(test)]
mod tests;
