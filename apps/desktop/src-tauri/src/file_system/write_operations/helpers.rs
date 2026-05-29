//! Helper functions for write operations.
//!
//! Contains validation, conflict resolution, and utility functions.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[cfg(target_os = "macos")]
use super::macos_copy::{CopyProgressContext, copy_single_file_native};

use super::state::WriteOperationState;
#[cfg(not(target_os = "macos"))]
use super::types::IoResultExt;
use super::types::{
    ConflictInfo, ConflictResolution, OperationEventSink, WriteConflictEvent, WriteOperationConfig, WriteOperationError,
};

// ============================================================================
// Validation helpers
// ============================================================================

// ============================================================================
// Apply-to-all state (two-bucket latches)
// ============================================================================

/// Per-operation "apply to all" latch state for conflict resolution.
///
/// Splits into two buckets so the destructive file-to-folder clash variant
/// (replacing a directory with a file) can be tracked separately from the
/// normal (file↔file / folder↔folder / folder↔file) variants. See
/// `apply_to_all_tests` for the full rule set; the short version:
///
/// - A choice latched on a *normal* clash applies to subsequent normal
///   clashes. Only Skip / Rename carry over to file-to-folder; Overwrite
///   variants don't.
/// - A choice latched on a *file-to-folder* clash applies to subsequent
///   file-to-folder clashes. If it was the **first** clash of the whole
///   operation, the latch spreads to the normal bucket too.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ApplyToAll {
    normal: Option<ConflictResolution>,
    file_to_folder: Option<ConflictResolution>,
    /// `false` until the first clash (any kind) has been resolved. Used to
    /// decide whether a "* all" choice in a file-to-folder dialog should
    /// spread to the normal bucket — only if the file-to-folder clash was
    /// the very first one the user saw.
    has_seen_clash: bool,
}

/// Returns the latched resolution that applies to the next clash, or `None`
/// if there's nothing latched yet for the given clash type. Encodes the
/// Skip/Rename carry-over rule: when looking up a file-to-folder clash, fall
/// back to the normal bucket only when the latched value there is one of
/// the safe variants.
pub(super) fn apply_to_all_effective(state: &ApplyToAll, is_file_to_folder: bool) -> Option<ConflictResolution> {
    if is_file_to_folder {
        state.file_to_folder.or(match state.normal {
            Some(r @ (ConflictResolution::Skip | ConflictResolution::Rename)) => Some(r),
            _ => None,
        })
    } else {
        state.normal
    }
}

/// Records a user response. `apply_to_all == false` doesn't latch but still
/// flips `has_seen_clash`, so a later file-to-folder "* all" choice won't be
/// considered "first" and won't spread to the normal bucket.
pub(super) fn apply_to_all_record(
    state: &mut ApplyToAll,
    is_file_to_folder: bool,
    resolution: ConflictResolution,
    apply_to_all: bool,
) {
    let was_first_clash = !state.has_seen_clash;
    state.has_seen_clash = true;
    if !apply_to_all {
        return;
    }
    if is_file_to_folder {
        state.file_to_folder = Some(resolution);
        // File-to-folder clash + "* all" + first-ever clash → spread to
        // normal too. After this point both buckets agree.
        if was_first_clash {
            state.normal = Some(resolution);
        }
    } else {
        state.normal = Some(resolution);
    }
}

pub(crate) fn validate_sources(sources: &[PathBuf]) -> Result<(), WriteOperationError> {
    for source in sources {
        // Use symlink_metadata to check existence without following symlinks
        if fs::symlink_metadata(source).is_err() {
            return Err(WriteOperationError::SourceNotFound {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination(destination: &Path) -> Result<(), WriteOperationError> {
    // Destination must exist and be a directory
    if !destination.exists() {
        return Err(WriteOperationError::SourceNotFound {
            path: destination.display().to_string(),
        });
    }
    if !destination.is_dir() {
        return Err(WriteOperationError::IoError {
            path: destination.display().to_string(),
            message: "Destination must be a directory".to_string(),
        });
    }
    Ok(())
}

pub(crate) fn validate_not_same_location(sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
    for source in sources {
        if let Some(parent) = source.parent()
            && parent == destination
        {
            return Err(WriteOperationError::SameLocation {
                path: source.display().to_string(),
            });
        }
    }
    Ok(())
}

pub(crate) fn validate_destination_not_inside_source(
    sources: &[PathBuf],
    destination: &Path,
) -> Result<(), WriteOperationError> {
    // Canonicalize destination to resolve symlinks and ".." segments that could
    // bypass a naive starts_with check (like /foo/bar/../foo/sub → /foo/sub).
    //
    // Pre-fix this used `unwrap_or_else(|_| destination.to_path_buf())` for
    // both paths, silently degrading the guard to a naive `starts_with` on
    // raw inputs whenever canonicalize failed. That's the data-safety bug —
    // a `dest` that lexically doesn't start with `source` but canonically
    // does (symlink shenanigans) would pass the check and the copy would
    // recurse into itself until disk-full. Fail closed instead.
    let canonical_dest = canonicalize_or_parent(destination).map_err(|e| WriteOperationError::IoError {
        path: destination.display().to_string(),
        message: format!("Couldn't resolve destination path: {e}"),
    })?;

    for source in sources {
        if source.is_dir() {
            let canonical_source = source.canonicalize().map_err(|e| WriteOperationError::IoError {
                path: source.display().to_string(),
                message: format!("Couldn't resolve source path: {e}"),
            })?;
            if canonical_dest.starts_with(&canonical_source) {
                return Err(WriteOperationError::DestinationInsideSource {
                    source: source.display().to_string(),
                    destination: destination.display().to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Canonicalizes `path`, falling back to canonicalizing its parent and
/// re-appending the trailing segment when the path doesn't exist yet (the
/// only legitimate case for `canonicalize` to fail on the destination during
/// a write op). Any other I/O error propagates so the caller can fail closed.
fn canonicalize_or_parent(path: &Path) -> std::io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(canonical) => Ok(canonical),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or(e)?;
            let canonical_parent = parent.canonicalize()?;
            match path.file_name() {
                Some(name) => Ok(canonical_parent.join(name)),
                // Path was just `..` / `.` / empty — refuse to fall back.
                None => Ok(canonical_parent),
            }
        }
        Err(e) => Err(e),
    }
}

/// Checks whether the destination directory is writable using access(W_OK).
#[cfg(unix)]
pub(crate) fn validate_destination_writable(destination: &Path) -> Result<(), WriteOperationError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(destination.as_os_str().as_bytes()).map_err(|_| WriteOperationError::IoError {
        path: destination.display().to_string(),
        message: "Invalid path".to_string(),
    })?;

    // SAFETY: c_path is a valid null-terminated C string
    let result = unsafe { libc::access(c_path.as_ptr(), libc::W_OK) };
    if result != 0 {
        return Err(WriteOperationError::PermissionDenied {
            path: destination.display().to_string(),
            message: "Destination folder is not writable. Check folder permissions in Finder.".to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn validate_destination_writable(_destination: &Path) -> Result<(), WriteOperationError> {
    Ok(())
}

/// Checks available disk space on the destination volume against required bytes.
///
/// On macOS, uses `NSURLVolumeAvailableCapacityForImportantUsageKey` which includes purgeable
/// space (APFS snapshots, iCloud caches), matching what Finder reports. Falls back to `statvfs`
/// if the NSURL query fails. On Linux, uses `statvfs` directly (no purgeable space concept).
#[cfg(unix)]
pub(crate) fn validate_disk_space(destination: &Path, required_bytes: u64) -> Result<(), WriteOperationError> {
    let available = get_available_space(destination).unwrap_or({
        // Cannot determine space. Return u64::MAX so the check passes and we let the OS
        // report ENOSPC if it actually happens during the copy.
        u64::MAX
    });

    if required_bytes > available {
        let volume_name = destination
            .ancestors()
            .find(|p| p.parent().is_some_and(|pp| pp == Path::new("/Volumes")))
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        return Err(WriteOperationError::InsufficientSpace {
            required: required_bytes,
            available,
            volume_name,
        });
    }

    Ok(())
}

/// Returns available bytes for a path, using the best API for the platform.
///
/// macOS: `NSURLVolumeAvailableCapacityForImportantUsageKey` (includes purgeable space).
/// Linux: `statvfs` `f_bavail * f_frsize`.
#[cfg(unix)]
fn get_available_space(path: &Path) -> Option<u64> {
    // On macOS, prefer the NSURL API that accounts for purgeable space.
    #[cfg(target_os = "macos")]
    {
        if let Some(space) = crate::volumes::get_volume_space(&path.to_string_lossy()) {
            return Some(space.available_bytes);
        }
    }

    // Fallback (and Linux primary path): statvfs
    get_available_space_statvfs(path)
}

/// Returns available bytes using `statvfs`. Used as the primary method on Linux and as a
/// fallback on macOS.
#[cfg(unix)]
fn get_available_space_statvfs(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: c_path is a valid null-terminated C string, stat is a valid pointer
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: statvfs succeeded, stat is initialized
    let stat = unsafe { stat.assume_init() };
    #[allow(
        clippy::unnecessary_cast,
        reason = "Required for macOS where statvfs fields are not u64"
    )]
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

#[cfg(not(unix))]
pub(crate) fn validate_disk_space(_destination: &Path, _required_bytes: u64) -> Result<(), WriteOperationError> {
    Ok(())
}

/// Checks if source and destination resolve to the same file (same inode + device).
/// This prevents data loss when copying a file over itself via a symlink.
#[cfg(unix)]
pub(crate) fn is_same_file(source: &Path, destination: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    let src_meta = match fs::metadata(source) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let dst_meta = match fs::metadata(destination) {
        Ok(m) => m,
        Err(_) => return false,
    };

    src_meta.dev() == dst_meta.dev() && src_meta.ino() == dst_meta.ino()
}

#[cfg(not(unix))]
pub(crate) fn is_same_file(_source: &Path, _destination: &Path) -> bool {
    false
}

/// Returns `true` when `path` already names something we should treat as a
/// conflict — including dangling symlinks.
///
/// `Path::exists()` follows symlinks: it returns `false` for a symlink whose
/// target is missing. Using it alone for the "does the destination exist?"
/// gate lets a dangling symlink slip past conflict resolution; the subsequent
/// write then follows the symlink and either clobbers wherever it points or
/// surfaces a confusing `ENOENT` from the target's parent. Pair it with
/// `symlink_metadata` so the gate fires for symlinks (broken or not).
pub(crate) fn path_exists_or_is_symlink(path: &Path) -> bool {
    path.exists() || fs::symlink_metadata(path).is_ok()
}

// ============================================================================
// Path length validation
// ============================================================================

/// Maximum file name length in bytes (APFS/HFS+ limit)
const MAX_NAME_BYTES: usize = 255;
/// Maximum path length in bytes (macOS PATH_MAX)
const MAX_PATH_BYTES: usize = 1024;

/// Validates that a destination path doesn't exceed filesystem name/path length limits.
pub(crate) fn validate_path_length(dest_path: &Path) -> Result<(), WriteOperationError> {
    // Check total path length
    let path_str = dest_path.as_os_str();
    if path_str.len() > MAX_PATH_BYTES {
        return Err(WriteOperationError::IoError {
            path: dest_path.display().to_string(),
            message: format!("Path exceeds maximum length of {} bytes", MAX_PATH_BYTES),
        });
    }

    // Check file name component length
    if let Some(name) = dest_path.file_name()
        && name.len() > MAX_NAME_BYTES
    {
        return Err(WriteOperationError::IoError {
            path: dest_path.display().to_string(),
            message: format!("File name exceeds maximum length of {} bytes", MAX_NAME_BYTES),
        });
    }

    Ok(())
}

// ============================================================================
// Symlink loop detection
// ============================================================================

/// Checks if a path creates a symlink loop.
pub(super) fn is_symlink_loop(path: &Path, visited: &HashSet<PathBuf>) -> bool {
    if let Ok(canonical) = path.canonicalize() {
        visited.contains(&canonical)
    } else {
        false
    }
}

// ============================================================================
// Filesystem detection
// ============================================================================

/// Checks if two paths are on the same filesystem using device IDs.
#[cfg(unix)]
pub(crate) fn is_same_filesystem(source: &Path, destination: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let source_meta = fs::metadata(source)?;
    let dest_meta = fs::metadata(destination)?;

    Ok(source_meta.dev() == dest_meta.dev())
}

#[cfg(not(unix))]
pub(crate) fn is_same_filesystem(_source: &Path, _destination: &Path) -> std::io::Result<bool> {
    // On non-Unix, assume different filesystem to be safe (will use copy+delete)
    Ok(false)
}

// ============================================================================
// Targeted durability (fdatasync per created destination)
// ============================================================================

/// Emits a `Flushing`-phase progress event, then `fdatasync`s every freshly
/// created destination so "complete" means "durable on disk", not "buffered in
/// the OS page cache". Blocks until the flush finishes.
///
/// Reuses the operation's own list of created destinations (`created_files`
/// from `CopyTransaction`); no parallel path tracking. `already_synced` holds
/// the destinations the per-file copy strategy already made durable (chunked
/// copy's inline `sync_data`) or for which a flush is moot (APFS clonefile /
/// reflink — the data shares copy-on-write extents with the source, so there's
/// nothing of our own to flush). Those are skipped here, so a long chunked
/// batch keeps its "durable-as-each-file-completes" property without a
/// redundant second `fdatasync` at the end.
///
/// `fdatasync` (Rust `File::sync_data`) is the floor: file data + size become
/// durable; mtime may lag, which is fine. We also best-effort `fsync` each
/// distinct parent directory so the directory entry (the rename-into-place from
/// the temp+rename / staging paths) is durable too. Directory fsync failures
/// are logged and ignored — not every filesystem supports it, and the file
/// data is already safe by then.
///
/// All steps are best-effort on error: a failed `sync_data` is logged, not
/// propagated. The bytes are written either way; failing the whole operation
/// at the final flush would be worse UX than a logged warning, and the typed
/// error paths already covered the cases where the write itself failed.
#[allow(
    clippy::too_many_arguments,
    reason = "These are the natural operation-wide values a progress emit needs; bundling them into a struct adds ceremony without cleaning anything up, matching WriteProgressEvent::new."
)]
pub(super) fn flush_created_destinations(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: super::types::WriteOperationType,
    state: &Arc<WriteOperationState>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    created_files: &[PathBuf],
    already_synced: &HashSet<PathBuf>,
) {
    use super::types::{WriteOperationPhase, WriteProgressEvent};

    // Announce the closing flush so the FE can show "Writing the last piece…"
    // instead of a bar frozen at 100% on slow media.
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::Flushing,
            None,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        ),
    );

    let mut synced_dirs: HashSet<PathBuf> = HashSet::new();
    for dest in created_files {
        if already_synced.contains(dest) {
            continue;
        }
        // Skip symlinks: opening one follows it, and a symlink carries no file
        // data of its own to flush. `symlink_metadata` doesn't follow.
        match fs::symlink_metadata(dest) {
            Ok(meta) if meta.file_type().is_symlink() => continue,
            Ok(_) => {}
            Err(e) => {
                log::warn!(
                    target: "write_durability",
                    "flush: couldn't stat {} before fdatasync: {e}",
                    dest.display()
                );
                continue;
            }
        }

        match fs::File::open(dest) {
            Ok(f) => {
                if let Err(e) = f.sync_data() {
                    log::warn!(
                        target: "write_durability",
                        "flush: fdatasync failed for {}: {e}",
                        dest.display()
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    target: "write_durability",
                    "flush: couldn't open {} for fdatasync: {e}",
                    dest.display()
                );
            }
        }

        // Best-effort: fsync the parent directory so the directory entry is
        // durable too. De-duped across files in the same directory.
        if let Some(parent) = dest.parent()
            && synced_dirs.insert(parent.to_path_buf())
            && let Err(e) = fsync_dir(parent)
        {
            log::debug!(
                target: "write_durability",
                "flush: parent dir fsync skipped for {}: {e}",
                parent.display()
            );
        }
    }
}

/// Opens a directory and `fsync`s it so directory-entry changes (the final
/// rename-into-place) are durable. Best-effort; some filesystems reject this.
fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    let f = fs::File::open(dir)?;
    f.sync_all()
}

// ============================================================================
// Background cleanup (detached, best-effort)
// ============================================================================

/// Deletes a file on a detached thread. Returns immediately. Best-effort.
pub(super) fn remove_file_in_background(path: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = fs::remove_file(&path) {
            log::warn!("background cleanup: failed to remove {}: {}", path.display(), e);
        }
    });
}

/// Deletes a directory tree on a detached thread. Returns immediately. Best-effort.
pub(super) fn remove_dir_all_in_background(path: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = fs::remove_dir_all(&path) {
            log::warn!("background cleanup: failed to remove {}: {}", path.display(), e);
        }
    });
}

// ============================================================================
// Conflict handling helpers
// ============================================================================

/// Result of applying a conflict resolution.
#[derive(Debug)]
pub(super) struct ResolvedDestination {
    /// The path to write to
    pub path: PathBuf,
    /// Whether this is an overwrite that needs safe handling
    pub needs_safe_overwrite: bool,
}

/// Resolves a file conflict based on the configured resolution mode.
/// Returns the resolved destination info, or None if the file should be skipped.
/// Also returns whether the resolution should be applied to all future conflicts.
#[allow(
    clippy::too_many_arguments,
    reason = "Recursive fn requires passing state through multiple levels"
)]
pub(super) fn resolve_conflict(
    source: &Path,
    dest_path: &Path,
    config: &WriteOperationConfig,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    apply_to_all_resolution: &mut ApplyToAll,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    // Pre-fetch metadata once; reused for the conflict event, the "is file →
    // folder?" classification, and the conditional-variant reduction.
    let source_meta = fs::metadata(source).ok();
    let dest_meta = fs::metadata(dest_path).ok();
    let is_file_to_folder = matches!(
        (
            source_meta.as_ref().map(|m| m.is_dir()),
            dest_meta.as_ref().map(|m| m.is_dir())
        ),
        (Some(false), Some(true)),
    );

    // Determine effective conflict resolution
    let resolution = if let Some(saved_resolution) = apply_to_all_effective(apply_to_all_resolution, is_file_to_folder)
    {
        // Use saved "apply to all" resolution
        saved_resolution
    } else if config.overwrite {
        ConflictResolution::Overwrite
    } else {
        config.conflict_resolution
    };

    match resolution {
        ConflictResolution::Stop => {
            // Emit conflict event for frontend to handle. Folder sizes come
            // from the drive index — we never walk the destination tree
            // synchronously to compute one. `None` is the legitimate
            // "(unknown)" rendering on the FE.
            let source_size_for_dir = if matches!(source_meta.as_ref().map(|m| m.is_dir()), Some(true)) {
                lookup_indexed_size(source)
            } else {
                None
            };
            let destination_size_for_dir = if matches!(dest_meta.as_ref().map(|m| m.is_dir()), Some(true)) {
                lookup_indexed_size(dest_path)
            } else {
                None
            };
            let event = build_conflict_event(
                operation_id,
                source,
                dest_path,
                source_meta.as_ref(),
                dest_meta.as_ref(),
                source_size_for_dir,
                destination_size_for_dir,
            );
            events.emit_conflict(event);

            // Create a oneshot channel for this conflict resolution
            let (tx, rx) = tokio::sync::oneshot::channel();
            *state.conflict_resolution_tx.lock().unwrap() = Some(tx);

            // Wait for user to call resolve_write_conflict.
            // The sender is dropped on cancel_write_operation, which unblocks the
            // receiver immediately. No timeout needed (the old 30s timeout was a
            // safety net; sender-drop is strictly better).
            // TEMPORARY: blocking_recv because this runs inside spawn_blocking.
            // Will become rx.await in milestone 2 full async migration.
            match rx.blocking_recv() {
                Ok(response) => {
                    // Save the original (unreduced) variant under the right bucket so
                    // subsequent conflicts re-evaluate the conditional variants against
                    // their own metadata, not the file that originally prompted.
                    apply_to_all_record(
                        apply_to_all_resolution,
                        is_file_to_folder,
                        response.resolution,
                        response.apply_to_all,
                    );
                    // Reduce conditional variants to Overwrite / Skip against this
                    // file's already-fetched metadata, then apply.
                    let effective =
                        reduce_conditional_resolution(response.resolution, source_meta.as_ref(), dest_meta.as_ref());
                    apply_resolution(effective, dest_path)
                }
                Err(_) => {
                    // Sender dropped = operation cancelled
                    Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".to_string(),
                    })
                }
            }
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => apply_resolution(ConflictResolution::Overwrite, dest_path),
        ConflictResolution::Rename => apply_resolution(ConflictResolution::Rename, dest_path),
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            let effective = reduce_conditional_resolution(resolution, source_meta.as_ref(), dest_meta.as_ref());
            apply_resolution(effective, dest_path)
        }
    }
}

/// Maps the conditional variants (`OverwriteSmaller` / `OverwriteOlder`) to a
/// concrete `Overwrite` or `Skip` for the file at hand, based on its source/dest
/// metadata. Non-conditional variants pass through unchanged. Comparisons are
/// strict: equal sizes / equal mtimes / missing metadata all reduce to `Skip`,
/// so a borderline file is never silently overwritten.
///
/// Logs the *reason* on Skip (kept vs missing-metadata vs equal) so users
/// running an SMB / MTP copy who pick "Overwrite all older" against a backend
/// that doesn't surface `modified_at` can see in the operation log why every
/// conflict was skipped, rather than wondering why nothing happened.
fn reduce_conditional_resolution(
    resolution: ConflictResolution,
    source_meta: Option<&fs::Metadata>,
    dest_meta: Option<&fs::Metadata>,
) -> ConflictResolution {
    match resolution {
        ConflictResolution::OverwriteSmaller => {
            match (source_meta.map(fs::Metadata::len), dest_meta.map(fs::Metadata::len)) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(src), Some(dst)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller: skipping — destination not strictly smaller (src={src}, dst={dst})"
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteSmaller: skipping — size unknown for source or destination"
                    );
                    ConflictResolution::Skip
                }
            }
        }
        ConflictResolution::OverwriteOlder => {
            let src_time = source_meta.and_then(|m| m.modified().ok());
            let dst_time = dest_meta.and_then(|m| m.modified().ok());
            match (src_time, dst_time) {
                (Some(src), Some(dst)) if dst < src => ConflictResolution::Overwrite,
                (Some(_), Some(_)) => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder: skipping — destination not strictly older than source"
                    );
                    ConflictResolution::Skip
                }
                _ => {
                    log::info!(
                        target: "conflict_resolution",
                        "OverwriteOlder: skipping — modified time unknown for source or destination"
                    );
                    ConflictResolution::Skip
                }
            }
        }
        other => other,
    }
}

/// Applies a specific conflict resolution to a destination path.
/// Returns None for Skip, or ResolvedDestination with path and overwrite flag.
fn apply_resolution(
    resolution: ConflictResolution,
    dest_path: &Path,
) -> Result<Option<ResolvedDestination>, WriteOperationError> {
    match resolution {
        ConflictResolution::Stop => {
            // Should not happen - Stop waits for user input
            Err(WriteOperationError::DestinationExists {
                path: dest_path.display().to_string(),
            })
        }
        ConflictResolution::Skip => Ok(None),
        ConflictResolution::Overwrite => {
            // Don't delete here - the copy function will use safe overwrite pattern
            Ok(Some(ResolvedDestination {
                path: dest_path.to_path_buf(),
                needs_safe_overwrite: true,
            }))
        }
        ConflictResolution::Rename => {
            // Find a unique name by appending " (1)", " (2)", etc. `find_unique_name`
            // atomically RESERVES the chosen name by creating a 0-byte placeholder
            // file (TOCTOU guard, see its doc comment). The caller's write must
            // therefore land *on* that placeholder, overwriting it — so we flag
            // `needs_safe_overwrite`. Without it the same-APFS-volume copy path
            // (`copyfile(3)` with `COPYFILE_EXCL`) refuses to write over the
            // existing placeholder and fails with `DestinationExists`, losing the
            // incoming bytes. The overwrite path consumes the placeholder cleanly
            // and the reservation still closes the race window.
            let unique_path = find_unique_name(dest_path);
            Ok(Some(ResolvedDestination {
                path: unique_path,
                needs_safe_overwrite: true,
            }))
        }
        ConflictResolution::OverwriteSmaller | ConflictResolution::OverwriteOlder => {
            // Conditional variants are always reduced to Overwrite / Skip by
            // `reduce_conditional_resolution` before reaching this function.
            unreachable!("conditional conflict resolutions must be reduced before apply_resolution")
        }
    }
}

/// Finds a unique filename by appending " (1)", " (2)", etc., **atomically
/// reserving** the chosen name via `O_CREAT|O_EXCL` so a concurrent process
/// (backup tool, cloud-sync agent, second Cmdr op) can't land a file at the
/// same path between our pick and the caller's write.
///
/// Pre-fix this returned the first non-existing candidate after an
/// `if !new_path.exists()` check; the caller then copied or renamed to that
/// path, leaving a ~ms TOCTOU window during which a concurrent write could
/// land an unrelated file at the same name — silently clobbered the next time
/// our copy / rename hit the path. By creating an empty placeholder under the
/// reserved name and letting downstream operations (`fs::copy` truncates;
/// `fs::rename` atomically replaces; `copyfile(3)` / `copy_file_range(2)` open
/// the dest with create+truncate) overwrite it, the race window collapses to
/// microseconds. Callers never observe the placeholder.
///
/// On the rare loss-of-the-placeholder edge case (a third party deletes our
/// empty file before the caller writes), the caller's write still succeeds
/// (creating fresh).
pub(super) fn find_unique_name(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());

    let mut counter: u32 = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{} ({}).{}", stem, counter, ext),
            None => format!("{} ({})", stem, counter),
        };
        let new_path = parent.join(new_name);

        match fs::OpenOptions::new().write(true).create_new(true).open(&new_path) {
            Ok(_) => return new_path,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                counter = counter.saturating_add(1);
            }
            Err(_) => {
                // Anything else (parent unwritable, ENOSPC, …) leaks back to
                // the caller's write attempt, which has its own error path.
                // Mirror the pre-fix behaviour of returning the path so the
                // caller surfaces the real error against the right operation.
                return new_path;
            }
        }
    }
}

// ============================================================================
// Safe overwrite helpers
// ============================================================================

/// Performs a safe overwrite using temp+rename pattern.
/// This ensures the original file is preserved if the copy fails.
///
/// Steps:
/// 1. Copy source to `dest.cmdr-tmp-{uuid}` (temp file in same directory)
/// 2. Rename original dest to `dest.cmdr-temp-{uuid}` (aside)
/// 3. Rename temp to final dest path
/// 4. Delete the renamed-aside original
///
/// If any step fails before step 3 completes, the original dest is intact.
///
/// **File→folder overwrite (incoming source file, existing dest folder).**
/// Local FS `rename(2)` happily swaps a directory aside under a new name, and
/// the streaming writer lands the source file at the original path. The aside
/// is then removed via `remove_dir_all`. The window during which the original
/// directory is gone-but-replaceable is bounded by step 3 (a single `rename`
/// syscall). A crash between step 2 and step 3 leaves a stray
/// `dest.cmdr-temp-<uuid>/` that a user can recognize and restore from.
pub(super) fn safe_overwrite_file(
    source: &Path,
    dest: &Path,
    #[cfg(target_os = "macos")] context: Option<&CopyProgressContext>,
) -> Result<u64, WriteOperationError> {
    let uuid = Uuid::new_v4();
    let parent = dest.parent().unwrap_or(Path::new("."));
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let temp_path = parent.join(format!("{}.cmdr-tmp-{}", file_name, uuid));
    let aside_path = parent.join(format!("{}.cmdr-temp-{}", file_name, uuid));

    // Step 1: Copy source to temp
    #[cfg(target_os = "macos")]
    let bytes = copy_single_file_native(source, &temp_path, false, context)?;
    #[cfg(not(target_os = "macos"))]
    let bytes = fs::copy(source, &temp_path).with_path(source)?;

    // Step 2: Rename original dest aside
    if let Err(e) = fs::rename(dest, &aside_path) {
        // Failed to rename aside - clean up temp and return error
        let _ = fs::remove_file(&temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to set aside existing destination: {}", e),
        });
    }

    // Step 3: Rename temp to final dest
    if let Err(e) = fs::rename(&temp_path, dest) {
        // Failed to rename - restore aside and clean up
        let _ = fs::rename(&aside_path, dest);
        let _ = fs::remove_file(&temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to finalize overwrite: {}", e),
        });
    }

    // Step 4: Delete the renamed-aside original (non-critical, ignore errors).
    // Use remove_dir_all for directory asides (file-over-folder overwrite).
    if aside_path.is_dir() {
        let _ = fs::remove_dir_all(&aside_path);
    } else {
        let _ = fs::remove_file(&aside_path);
    }

    Ok(bytes)
}

/// Performs a safe overwrite of `dest` by setting the existing entry aside
/// under `dest.cmdr-temp-{uuid}`, then running the caller's `materialize`
/// closure to land the new content at `dest`. On materialize failure or
/// cancellation the aside is rolled back, restoring the original entry.
///
/// The helper is type-agnostic: `dest` may hold a file or a directory before
/// the call, and `materialize` may create either a file or a directory. The
/// two cmdr-cross-type cases that motivated it:
///
/// - **Folder→file overwrite (copy/move):** source is a directory whose
///   contents will be materialized at `dest`, which currently holds a file.
///   The closure creates a fresh directory and populates it; on success the
///   blocking file is removed via `remove_file`.
/// - **File→folder overwrite (copy/move):** source is a file whose bytes
///   will be materialized at `dest`, which currently holds a directory. The
///   closure writes the file; on success the existing folder is removed via
///   `remove_dir_all`.
///
/// Steps:
/// 1. Sets aside the existing `dest` as `dest.cmdr-temp-{uuid}` via a single
///    `rename(2)`.
/// 2. Runs `materialize(dest)` to land the new content. The closure decides
///    whether `dest` becomes a file or a directory.
/// 3. On `Ok`, removes the aside (`remove_dir_all` for directory asides,
///    `remove_file` for file asides).
/// 4. On `Err`, removes whatever the closure left at `dest` and renames the
///    aside back to `dest`, then propagates the error.
///
/// **Atomicity guarantee:** at every observable moment after this function
/// is called and before it returns, `dest` is either the original
/// (untouched) or the new materialized content. The closure may briefly
/// leave a half-written entry at `dest`, but the original is recoverable
/// from the aside even on a crash — the aside has the recognizable
/// `cmdr-temp-` prefix so a user can restore it by hand.
pub(super) fn safe_overwrite_dir<F>(dest: &Path, materialize: F) -> Result<(), WriteOperationError>
where
    F: FnOnce(&Path) -> Result<(), WriteOperationError>,
{
    let uuid = Uuid::new_v4();
    let parent = dest.parent().unwrap_or(Path::new("."));
    let file_name = dest
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let aside_path = parent.join(format!("{}.cmdr-temp-{}", file_name, uuid));

    // Step 1: Rename existing dest aside. This survives a crash: the original
    // is recognizable on next launch and the user can rename it back by hand.
    if let Err(e) = fs::rename(dest, &aside_path) {
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to set aside existing destination: {}", e),
        });
    }

    // Step 2: Run the caller's materialize step. The caller is responsible
    // for creating the dest directory and populating it.
    let materialize_result = materialize(dest);

    match materialize_result {
        Ok(()) => {
            // Step 3: Remove the aside. Best-effort; a leftover is recognizable.
            if aside_path.is_dir() {
                let _ = fs::remove_dir_all(&aside_path);
            } else {
                let _ = fs::remove_file(&aside_path);
            }
            Ok(())
        }
        Err(e) => {
            // Failure or cancellation: clean up whatever materialize created at
            // dest and rename the aside back.
            if dest.exists() {
                if dest.is_dir() {
                    let _ = fs::remove_dir_all(dest);
                } else {
                    let _ = fs::remove_file(dest);
                }
            }
            if let Err(restore_err) = fs::rename(&aside_path, dest) {
                crate::log_error!(
                    "safe_overwrite_dir: failed to restore aside {} -> {}: {}",
                    aside_path.display(),
                    dest.display(),
                    restore_err
                );
            }
            Err(e)
        }
    }
}

/// Looks up a directory's recursive size from the drive index. Returns `None`
/// when the index doesn't cover the path (network mount, MTP, outside-scope
/// path, indexer not yet initialised). The BE intentionally never *walks*
/// the tree to compute this — `(unknown)` on the FE is the legitimate
/// fallback when the cached value isn't available.
fn lookup_indexed_size(path: &Path) -> Option<u64> {
    crate::indexing::get_dir_stats(&path.to_string_lossy())
        .ok()
        .flatten()
        .map(|s| s.recursive_size)
}

/// Builds a `WriteConflictEvent` from the source / destination metadata pair.
/// Extracted from `resolve_conflict` so the source/destination type-mismatch
/// flags can be unit-tested in isolation. Pre-fix the inline event omitted
/// `source_is_directory` / `destination_is_directory` entirely; the FE Stop
/// dialog couldn't tell the user "you're about to replace a folder with a
/// file" and silently took the user's "Overwrite" click as consent to drop
/// an entire directory tree.
fn build_conflict_event(
    operation_id: &str,
    source: &Path,
    dest_path: &Path,
    source_meta: Option<&fs::Metadata>,
    dest_meta: Option<&fs::Metadata>,
    // Recursive size of the *source* when it's a directory (from the
    // pre-flight scan's per-source-root total). Ignored when source is a
    // file — files use `metadata.len()` directly. Always `Some` for folder
    // sources after pre-flight; the rare MCP / skip-preflight path may pass
    // `None`, in which case source_size falls back to 0.
    source_size_for_dir: Option<u64>,
    // Recursive size of the *destination* when it's a directory. The caller
    // looks it up in the drive index; `None` means "the index doesn't cover
    // this path" (network mount, MTP, paths outside the index scope) and
    // surfaces to the FE as the `(unknown)` rendering. Files always use
    // `metadata.len()` and this override is ignored.
    destination_size_for_dir: Option<u64>,
) -> WriteConflictEvent {
    let destination_is_newer = match (source_meta, dest_meta) {
        (Some(s), Some(d)) => {
            let src_time = s.modified().ok();
            let dst_time = d.modified().ok();
            matches!((src_time, dst_time), (Some(src), Some(dst)) if dst > src)
        }
        _ => false,
    };

    let source_is_directory = source_meta.map(|m| m.is_dir()).unwrap_or(false);
    let destination_is_directory = dest_meta.map(|m| m.is_dir()).unwrap_or(false);

    // Files: use `metadata.len()` directly. Directories: use the caller-
    // supplied recursive total (the BE never walks a destination tree).
    let source_size = if source_is_directory {
        source_size_for_dir.unwrap_or(0)
    } else {
        source_meta.map(|m| m.len()).unwrap_or(0)
    };
    let destination_size = if destination_is_directory {
        destination_size_for_dir
    } else {
        dest_meta.map(|m| m.len())
    };
    let size_difference = destination_size.map(|d| d as i64 - source_size as i64);

    let unix_secs = |m: Option<&fs::Metadata>| -> Option<i64> {
        m?.modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    };

    WriteConflictEvent {
        operation_id: operation_id.to_string(),
        source_path: source.display().to_string(),
        destination_path: dest_path.display().to_string(),
        source_size,
        destination_size,
        source_modified: unix_secs(source_meta),
        destination_modified: unix_secs(dest_meta),
        destination_is_newer,
        size_difference,
        source_is_directory,
        destination_is_directory,
    }
}

// ============================================================================
// Conflict info helpers
// ============================================================================

/// Calculates destination path for a source file relative to source root.
pub(super) fn calculate_dest_path(
    path: &Path,
    source_root: &Path,
    dest_root: &Path,
) -> Result<PathBuf, WriteOperationError> {
    // If path is the source root itself, use the file name in dest_root
    if path == source_root {
        let file_name = path.file_name().ok_or_else(|| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Invalid source path".to_string(),
        })?;
        return Ok(dest_root.join(file_name));
    }

    // Otherwise, strip the source root's parent and join with dest_root
    let source_parent = source_root.parent().unwrap_or(source_root);
    let relative = path
        .strip_prefix(source_parent)
        .map_err(|_| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: "Failed to calculate relative path".to_string(),
        })?;

    Ok(dest_root.join(relative))
}

/// Creates ConflictInfo for a source/destination pair.
pub(super) fn create_conflict_info(
    source: &Path,
    dest: &Path,
    source_metadata: &fs::Metadata,
) -> Result<Option<ConflictInfo>, WriteOperationError> {
    let dest_metadata = match fs::symlink_metadata(dest) {
        Ok(m) => m,
        Err(_) => return Ok(None), // No conflict if dest doesn't exist
    };

    let source_modified = source_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let dest_modified = dest_metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    let destination_is_newer = match (source_modified, dest_modified) {
        (Some(s), Some(d)) => d > s,
        _ => false,
    };

    Ok(Some(ConflictInfo {
        source_path: source.display().to_string(),
        destination_path: dest.display().to_string(),
        source_size: source_metadata.len(),
        destination_size: dest_metadata.len(),
        source_modified,
        destination_modified: dest_modified,
        destination_is_newer,
        is_directory: source_metadata.is_dir(),
    }))
}

/// Samples conflicts if there are too many, using reservoir sampling.
pub(super) fn sample_conflicts(conflicts: Vec<ConflictInfo>, max_count: usize) -> (Vec<ConflictInfo>, bool) {
    if conflicts.len() <= max_count {
        return (conflicts, false);
    }

    // Use reservoir sampling for uniform random selection
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut sampled: Vec<ConflictInfo> = conflicts.iter().take(max_count).cloned().collect();

    for (i, conflict) in conflicts.iter().enumerate().skip(max_count) {
        // Deterministic "random" based on path hash for reproducibility
        let mut hasher = DefaultHasher::new();
        conflict.source_path.hash(&mut hasher);
        i.hash(&mut hasher);
        let hash = hasher.finish();
        let j = (hash as usize) % (i + 1);

        if j < max_count {
            sampled[j] = conflict.clone();
        }
    }

    (sampled, true)
}

// ============================================================================
// Cancellation-aware execution
// ============================================================================

/// Interval for checking cancellation while waiting for blocking operations.
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Runs a closure on a background thread with polling-based cancellation.
///
/// Spawns `work` on a new thread and polls for results every 100ms, checking the
/// cancellation flag between polls. This ensures quick cancellation response even
/// when filesystem I/O blocks (for example, on stuck network drives).
pub(super) fn run_cancellable<T>(
    work: impl FnOnce() -> Result<T, WriteOperationError> + Send + 'static,
    state: &Arc<WriteOperationState>,
    context: &str,
    operation_id: &str,
) -> Result<T, WriteOperationError>
where
    T: Send + 'static,
{
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let _ = tx.send(work());
    });

    loop {
        if super::state::is_cancelled(&state.intent) {
            log::debug!("{context}: cancellation detected during polling op={operation_id}");
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        match rx.recv_timeout(CANCELLATION_POLL_INTERVAL) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(WriteOperationError::IoError {
                    path: context.to_string(),
                    message: format!("{context} thread terminated unexpectedly"),
                });
            }
        }
    }
}

/// Scoped variant of [`run_cancellable`] that allows the work closure to borrow
/// non-`'static` data (for example, a `&dyn OperationEventSink` reference).
///
/// Uses `std::thread::scope`, so the call blocks until the worker thread
/// finishes or cancellation is observed. Behavior is otherwise identical to
/// `run_cancellable`: the cancellation flag is polled every 100ms while the
/// worker runs, and the function returns early on cancellation.
pub(super) fn run_cancellable_scoped<'env, T, F>(
    work: F,
    state: &Arc<WriteOperationState>,
    context: &str,
    operation_id: &str,
) -> Result<T, WriteOperationError>
where
    F: FnOnce() -> Result<T, WriteOperationError> + Send + 'env,
    T: Send + 'env,
{
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(move || {
            let _ = tx.send(work());
        });

        loop {
            if super::state::is_cancelled(&state.intent) {
                log::debug!("{context}: cancellation detected during polling op={operation_id}");
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".to_string(),
                });
            }

            match rx.recv_timeout(CANCELLATION_POLL_INTERVAL) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(WriteOperationError::IoError {
                        path: context.to_string(),
                        message: format!("{context} thread terminated unexpectedly"),
                    });
                }
            }
        }
    })
}

#[cfg(test)]
mod conditional_resolution_tests {
    //! Tests for `reduce_conditional_resolution` — the gate that decides whether
    //! the user's "Overwrite all smaller" / "Overwrite all older" choice
    //! actually overwrites this particular file, or skips it.
    //!
    //! **Data-safety contract pinned here**: a destination is overwritten ONLY
    //! when strictly smaller (by byte count) or strictly older (by mtime) than
    //! the source. Equal-or-bigger / equal-or-newer / unknown metadata always
    //! reduces to `Skip`. Bugs in this function would silently overwrite files
    //! the user expected to keep, so the tests below are exhaustive across the
    //! comparison axes.
    use super::*;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;
    use uuid::Uuid;

    fn temp_with_size_and_mtime(dir: &Path, name: &str, size: usize, mtime_secs: i64) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, vec![0u8; size]).unwrap();
        let mtime = filetime::FileTime::from_unix_time(mtime_secs, 0);
        filetime::set_file_mtime(&path, mtime).unwrap();
        path
    }

    /// Snapshot the metadata once. `reduce_conditional_resolution` borrows
    /// `&Metadata`, so the helper has to return owned values the caller
    /// stores in locals.
    fn meta(path: &Path) -> fs::Metadata {
        fs::metadata(path).unwrap()
    }

    fn unique_dir() -> TempDir {
        // Per-test unique dir keeps parallel runs from racing on identical
        // names. Default TempDir is already unique but the explicit prefix
        // makes failing-test debug output easier to interpret.
        tempfile::Builder::new()
            .prefix(&format!("cmdr-cond-resolve-{}", Uuid::new_v4()))
            .tempdir()
            .unwrap()
    }

    // ------------------------------------------------------------------
    // OverwriteSmaller — strict by byte count
    // ------------------------------------------------------------------

    #[test]
    fn smaller_overwrites_only_when_dest_strictly_smaller() {
        // Pin: dst (smaller) < src → Overwrite. Any other relation → Skip.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Overwrite,
            "Strictly smaller dst must be overwritten under OverwriteSmaller"
        );
    }

    #[test]
    fn smaller_skips_when_dest_equal_size() {
        // Pin: dst.len == src.len → Skip. Critical to data safety: a same-size
        // dst is NOT obviously stale; user picked "smaller" meaning STRICTLY.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 1000, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Equal-size dst must NOT be overwritten under OverwriteSmaller"
        );
    }

    #[test]
    fn smaller_skips_when_dest_strictly_larger() {
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 1000, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Larger dst must NOT be overwritten under OverwriteSmaller — would corrupt the user's keeper file"
        );
    }

    #[test]
    fn smaller_skips_on_zero_byte_dest_when_source_also_zero() {
        // Edge: 0 bytes vs 0 bytes is NOT strictly smaller; must skip.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 0, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 0, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m));
        assert_eq!(resolved, ConflictResolution::Skip);
    }

    #[test]
    fn smaller_skips_when_source_metadata_missing() {
        // Source meta missing means we can't prove dst is smaller; fail closed (Skip).
        let dir = unique_dir();
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, None, Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Unknown source size must fail closed to Skip; we cannot prove dst is smaller"
        );
    }

    #[test]
    fn smaller_skips_when_dest_metadata_missing() {
        // Dest meta missing: the dest exists (we're in conflict resolution
        // because of that) but we can't size it — skip rather than overwrite blindly.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let src_m = meta(&src);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), None);
        assert_eq!(resolved, ConflictResolution::Skip);
    }

    #[test]
    fn smaller_skips_when_both_metadata_missing() {
        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, None, None);
        assert_eq!(resolved, ConflictResolution::Skip);
    }

    // ------------------------------------------------------------------
    // OverwriteOlder — strict by mtime
    // ------------------------------------------------------------------

    #[test]
    fn older_overwrites_only_when_dest_strictly_older() {
        // Pin: dst mtime < src mtime → Overwrite. The user wants stale files replaced.
        let dir = unique_dir();
        // 2020 dst, 2024 src
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_700_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(resolved, ConflictResolution::Overwrite);
    }

    #[test]
    fn older_skips_when_dest_equal_mtime() {
        // Equal mtime is not strictly older — skip.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Equal-mtime dst must NOT be overwritten under OverwriteOlder"
        );
    }

    #[test]
    fn older_skips_when_dest_strictly_newer() {
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_700_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(
            resolved,
            ConflictResolution::Skip,
            "Newer dst must NOT be overwritten under OverwriteOlder — would clobber the user's fresher file"
        );
    }

    #[test]
    fn older_one_second_difference_still_counts_as_older() {
        // Subsecond paranoia: cross the second boundary so the comparison is unambiguous.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 100, 1_600_000_001);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 100, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        let resolved = reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m));
        assert_eq!(resolved, ConflictResolution::Overwrite);
    }

    #[test]
    fn older_skips_when_metadata_missing() {
        // Both sides missing: skip. Source missing: skip. Dest missing: skip.
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, None, None),
            ConflictResolution::Skip
        );
        let dir = unique_dir();
        let some = temp_with_size_and_mtime(dir.path(), "f", 100, 1_600_000_000);
        let some_m = meta(&some);
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, None, Some(&some_m)),
            ConflictResolution::Skip,
            "Missing source mtime → fail closed to Skip"
        );
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&some_m), None),
            ConflictResolution::Skip,
            "Missing dest mtime → fail closed to Skip"
        );
    }

    // ------------------------------------------------------------------
    // Pass-through variants
    // ------------------------------------------------------------------

    #[test]
    fn non_conditional_variants_pass_through_unchanged() {
        // The reduction must be a no-op for the four pre-existing variants;
        // callers depend on this when they pass `response.resolution` blindly.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 500, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 500, 1_600_000_000);
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        for v in [
            ConflictResolution::Stop,
            ConflictResolution::Skip,
            ConflictResolution::Overwrite,
            ConflictResolution::Rename,
        ] {
            assert_eq!(
                reduce_conditional_resolution(v, Some(&src_m), Some(&dst_m)),
                v,
                "Variant {v:?} must pass through unchanged"
            );
        }
    }

    // ------------------------------------------------------------------
    // Independence of axes
    // ------------------------------------------------------------------

    #[test]
    fn smaller_ignores_mtime() {
        // A dst that's smaller AND newer must still be overwritten under
        // OverwriteSmaller — the user opted in to size-only comparison.
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 1000, 1_600_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 100, 1_700_000_000); // newer + smaller
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteSmaller, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Overwrite
        );
    }

    #[test]
    fn older_ignores_size() {
        // A dst that's older AND larger must still be overwritten under
        // OverwriteOlder. (Common case: a stub file replaced by a fuller version
        // — the user wants the newer file regardless of size.)
        let dir = unique_dir();
        let src = temp_with_size_and_mtime(dir.path(), "src", 100, 1_700_000_000);
        let dst = temp_with_size_and_mtime(dir.path(), "dst", 5000, 1_600_000_000); // older + larger
        let src_m = meta(&src);
        let dst_m = meta(&dst);

        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Overwrite
        );
    }

    // ------------------------------------------------------------------
    // Realistic mtime span via SystemTime (in case filetime crate ever drifts).
    // ------------------------------------------------------------------

    #[test]
    fn older_works_with_systemtime_offsets_from_now() {
        // Belt-and-suspenders: don't depend on hardcoded epoch values.
        // Use "now" and "now - 1 hour" to prove the comparison works for
        // realistic timestamps the OS would produce live.
        let dir = unique_dir();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        fs::write(&src, b"src").unwrap();
        fs::write(&dst, b"dst").unwrap();

        let now = SystemTime::now();
        let an_hour_ago = now - Duration::from_secs(3600);
        filetime::set_file_mtime(&src, filetime::FileTime::from_system_time(now)).unwrap();
        filetime::set_file_mtime(&dst, filetime::FileTime::from_system_time(an_hour_ago)).unwrap();

        let src_m = meta(&src);
        let dst_m = meta(&dst);
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Overwrite,
            "dst from an hour ago must be older than src from now"
        );
        // And the inverse: dst newer than src → Skip.
        filetime::set_file_mtime(&src, filetime::FileTime::from_system_time(an_hour_ago)).unwrap();
        filetime::set_file_mtime(&dst, filetime::FileTime::from_system_time(now)).unwrap();
        let src_m = meta(&src);
        let dst_m = meta(&dst);
        assert_eq!(
            reduce_conditional_resolution(ConflictResolution::OverwriteOlder, Some(&src_m), Some(&dst_m)),
            ConflictResolution::Skip,
            "dst from now must NOT be overwritten by src from an hour ago"
        );
    }
}

#[cfg(test)]
mod build_conflict_event_tests {
    //! Regression for the low-severity audit finding: the Stop-mode
    //! conflict event used to carry no `is_directory` flags, so the FE
    //! dialog rendered a generic "file already exists" prompt even when
    //! the collision was a type mismatch (file → directory or vice versa).
    //! User clicked "Overwrite" thinking they were replacing a file, ended
    //! up dropping a whole directory tree without warning.
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn file_over_directory_marks_destination_is_directory() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("notes.txt");
        let dest = temp.path().join("conflicting");
        fs::write(&source, b"a file").unwrap();
        fs::create_dir(&dest).unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op-1",
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            None,
            Some(12345),
        );

        assert!(!event.source_is_directory, "source is a file");
        assert!(event.destination_is_directory, "destination is a directory");
    }

    #[test]
    fn directory_over_file_marks_source_is_directory() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("conflicting");
        let dest = temp.path().join("notes.txt");
        fs::create_dir(&source).unwrap();
        fs::write(&dest, b"a file").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op-2",
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            Some(67890),
            None,
        );

        assert!(event.source_is_directory, "source is a directory");
        assert!(!event.destination_is_directory, "destination is a file");
    }

    #[test]
    fn file_over_file_flags_both_false() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let dest = temp.path().join("b.txt");
        fs::write(&source, b"a").unwrap();
        fs::write(&dest, b"b").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event("op-3", &source, &dest, Some(&source_meta), Some(&dest_meta), None, None);

        assert!(!event.source_is_directory);
        assert!(!event.destination_is_directory);
    }

    #[test]
    fn file_dest_uses_metadata_len_ignoring_override() {
        // Files always have a known size via metadata. The override exists
        // only for directories (where metadata.len() is the inode entry
        // size, not the recursive content size).
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("a.txt");
        let dest = temp.path().join("b.txt");
        fs::write(&source, b"hello").unwrap();
        fs::write(&dest, b"world!").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            Some(99999),
            Some(99999),
        );

        assert_eq!(event.source_size, 5);
        assert_eq!(event.destination_size, Some(6));
        assert_eq!(event.size_difference, Some(1));
    }

    #[test]
    fn folder_dest_uses_override_size() {
        // For dir destinations the recursive size lives in the drive index;
        // the caller fetches it (or `None` when the index doesn't cover the
        // path) and hands it to us.
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("notes.txt");
        let dest = temp.path().join("conflicting");
        fs::write(&source, b"a").unwrap();
        fs::create_dir(&dest).unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            None,
            Some(4_096_000),
        );

        assert_eq!(event.source_size, 1);
        assert_eq!(event.destination_size, Some(4_096_000));
        assert_eq!(event.size_difference, Some(4_095_999));
    }

    #[test]
    fn folder_dest_with_unknown_size_surfaces_none() {
        // The index doesn't cover the destination (network mount, MTP, …).
        // Report `(unknown)` on the wire as `None`; size_difference also
        // collapses to `None` since one side is unknown.
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("notes.txt");
        let dest = temp.path().join("conflicting");
        fs::write(&source, b"a").unwrap();
        fs::create_dir(&dest).unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event("op", &source, &dest, Some(&source_meta), Some(&dest_meta), None, None);

        assert_eq!(event.source_size, 1);
        assert_eq!(event.destination_size, None);
        assert_eq!(event.size_difference, None);
    }

    #[test]
    fn folder_source_uses_override_size() {
        // Folder-source sizes come from the pre-flight scan's per-source-root
        // total. The override is always Some for source-folder clashes.
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("payload");
        let dest = temp.path().join("notes.txt");
        fs::create_dir(&source).unwrap();
        fs::write(&dest, b"hi").unwrap();

        let source_meta = fs::metadata(&source).unwrap();
        let dest_meta = fs::metadata(&dest).unwrap();

        let event = build_conflict_event(
            "op",
            &source,
            &dest,
            Some(&source_meta),
            Some(&dest_meta),
            Some(123_456),
            None,
        );

        assert_eq!(event.source_size, 123_456);
        assert_eq!(event.destination_size, Some(2));
        assert_eq!(event.size_difference, Some(2 - 123_456));
    }
}

#[cfg(test)]
mod find_unique_name_tests {
    //! Regression for the low-severity audit finding: pre-fix
    //! `find_unique_name` picked a name by exists()-checking each candidate
    //! and returning the first miss. Between the check and the caller's
    //! write, a concurrent process (backup tool, cloud-sync agent, second
    //! Cmdr op) could land a file at the same name and the next copy /
    //! rename would silently clobber it. The fix atomically reserves the
    //! chosen name via O_EXCL.
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn reserves_the_chosen_name_on_disk() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("notes.txt");
        fs::write(&target, b"original").unwrap();

        let unique = find_unique_name(&target);

        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
        // O_EXCL placeholder must already exist after the call.
        assert!(unique.exists(), "reservation must create the placeholder");
        // Second call goes to (2), proving the first reservation persisted.
        let next = find_unique_name(&target);
        assert_eq!(next.file_name().unwrap().to_string_lossy(), "notes (2).txt");
    }

    #[test]
    fn keeps_extension_in_the_right_place() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("report.pdf");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "report (1).pdf");
    }

    #[test]
    fn handles_extensionless_filenames() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("README");
        fs::write(&target, b"x").unwrap();
        let unique = find_unique_name(&target);
        assert_eq!(unique.file_name().unwrap().to_string_lossy(), "README (1)");
    }
}

#[cfg(all(test, unix))]
mod path_exists_or_is_symlink_tests {
    //! Regression for the medium-severity audit finding: the regular-file
    //! copy branch (and both move-op branches) used `Path::exists()` for
    //! conflict detection, which follows symlinks and returns `false` for
    //! a dangling symlink at the destination — the copy then followed the
    //! symlink and silently clobbered (or failed mid-batch with a confusing
    //! ENOENT against the target's parent).
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    #[test]
    fn flags_dangling_symlink_at_destination() {
        let temp = TempDir::new().unwrap();
        let dest = temp.path().join("notes.txt");
        // Symlink target intentionally never exists.
        symlink(temp.path().join("missing-target"), &dest).unwrap();

        // `Path::exists()` is the pre-fix gate — must return false for a
        // dangling symlink (this is the trap).
        assert!(!dest.exists(), "exists() must NOT see a dangling symlink");
        // Our helper closes the trap.
        assert!(
            path_exists_or_is_symlink(&dest),
            "dangling symlink must be treated as an existing destination"
        );
    }

    #[test]
    fn flags_live_symlink_and_regular_paths() {
        let temp = TempDir::new().unwrap();
        let real = temp.path().join("real.txt");
        fs::write(&real, b"data").unwrap();
        let link = temp.path().join("link.txt");
        symlink(&real, &link).unwrap();

        assert!(path_exists_or_is_symlink(&real));
        assert!(path_exists_or_is_symlink(&link));
    }

    #[test]
    fn returns_false_for_missing_path() {
        let temp = TempDir::new().unwrap();
        assert!(!path_exists_or_is_symlink(&temp.path().join("absent")));
    }
}

#[cfg(test)]
mod apply_to_all_tests {
    //! Pure-state tests for the two-bucket `ApplyToAll` latch model.
    //!
    //! Rules (per UX spec):
    //!   1. Normal clash → choice lands in the `normal` bucket only.
    //!   2. File-to-folder clash → choice lands in the `file_to_folder` bucket
    //!      only.
    //!   3. Special case: if the FIRST clash of the operation is a
    //!      file-to-folder one, a "* all" choice spreads to both buckets.
    //!   4. Carry-over: Skip/Rename in the `normal` bucket apply to subsequent
    //!      file-to-folder clashes too (these are universally safe). Overwrite
    //!      variants never carry over from normal → file-to-folder.
    use super::*;

    fn fresh() -> ApplyToAll {
        ApplyToAll::default()
    }

    #[test]
    fn default_state_is_empty() {
        let state = fresh();
        assert!(apply_to_all_effective(&state, false).is_none());
        assert!(apply_to_all_effective(&state, true).is_none());
    }

    #[test]
    fn normal_overwrite_all_stays_in_normal_bucket() {
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, false),
            Some(ConflictResolution::Overwrite)
        );
        // Does NOT spread to file-to-folder — user has to be re-prompted.
        assert_eq!(apply_to_all_effective(&state, true), None);
    }

    #[test]
    fn normal_skip_all_carries_over_to_file_to_folder() {
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Skip, true);
        assert_eq!(apply_to_all_effective(&state, false), Some(ConflictResolution::Skip));
        // Safe action: skip the file-to-folder one too without re-prompting.
        assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Skip));
    }

    #[test]
    fn normal_rename_all_carries_over_to_file_to_folder() {
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Rename, true);
        assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Rename));
    }

    #[test]
    fn normal_conditional_variants_do_not_carry_over() {
        // OverwriteSmaller / OverwriteOlder are destructive — same rule as
        // Overwrite. They never reach file-to-folder without an explicit prompt.
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::OverwriteSmaller, true);
        assert_eq!(apply_to_all_effective(&state, true), None);

        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::OverwriteOlder, true);
        assert_eq!(apply_to_all_effective(&state, true), None);
    }

    #[test]
    fn file_to_folder_first_overwrite_all_spreads_to_normal() {
        // Spec: "if a file-to-folder clash is the first one, then any '* all'
        // choices should apply to ALL types of clashes."
        let mut state = fresh();
        apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, true),
            Some(ConflictResolution::Overwrite)
        );
        assert_eq!(
            apply_to_all_effective(&state, false),
            Some(ConflictResolution::Overwrite)
        );
    }

    #[test]
    fn file_to_folder_later_overwrite_all_does_not_spread() {
        // Spec example: user picks Overwrite all on a normal clash; later a
        // file-to-folder clash comes up and the user picks Skip all in it —
        // that Skip all applies to file-to-folder only.
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Overwrite, true);
        // Now a file-to-folder clash arrives. Even though a normal "Overwrite all"
        // is set, file-to-folder is destructive enough to re-prompt → user picks
        // Skip all in the file-to-folder dialog.
        apply_to_all_record(&mut state, true, ConflictResolution::Skip, true);

        // Normal bucket keeps the original Overwrite — the new Skip is
        // file-to-folder-only.
        assert_eq!(
            apply_to_all_effective(&state, false),
            Some(ConflictResolution::Overwrite)
        );
        assert_eq!(apply_to_all_effective(&state, true), Some(ConflictResolution::Skip));
    }

    #[test]
    fn single_choice_does_not_set_apply_to_all_but_still_seeds_first_clash_flag() {
        // A non-"apply to all" choice doesn't latch, but it DOES mean the next
        // file-to-folder clash isn't "the first" any more, so its "* all"
        // choice shouldn't spread to normal.
        let mut state = fresh();
        apply_to_all_record(
            &mut state,
            false,
            ConflictResolution::Overwrite,
            /* apply_to_all */ false,
        );

        // Nothing latched yet.
        assert_eq!(apply_to_all_effective(&state, false), None);
        assert_eq!(apply_to_all_effective(&state, true), None);

        // Now a file-to-folder clash; user picks Overwrite all. Because a
        // normal clash already happened, this is NOT the first clash any more
        // → don't spread.
        apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, true),
            Some(ConflictResolution::Overwrite)
        );
        assert_eq!(apply_to_all_effective(&state, false), None);
    }

    #[test]
    fn file_to_folder_latch_wins_over_normal_carry_over() {
        // If both buckets have a value, the directly-set file-to-folder one
        // wins (don't fall back to the normal-bucket Skip/Rename carry-over).
        let mut state = fresh();
        apply_to_all_record(&mut state, false, ConflictResolution::Skip, true);
        apply_to_all_record(&mut state, true, ConflictResolution::Overwrite, true);
        assert_eq!(
            apply_to_all_effective(&state, true),
            Some(ConflictResolution::Overwrite)
        );
    }
}
