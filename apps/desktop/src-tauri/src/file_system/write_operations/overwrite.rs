//! Safe-overwrite machinery: temp+rename-aside atomicity for replacing an
//! existing destination without risking the user's original data on failure.

use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use super::macos_copy::{CopyProgressContext, copy_single_file_native};
#[cfg(not(target_os = "macos"))]
use super::types::IoResultExt;
use super::types::WriteOperationError;

/// Result of applying a conflict resolution.
#[derive(Debug)]
pub(super) struct ResolvedDestination {
    /// The path to write to
    pub path: PathBuf,
    /// Whether this is an overwrite that needs safe handling
    pub needs_safe_overwrite: bool,
}

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
        // Failed to rename - restore aside and clean up. If the restore ALSO
        // fails, the user's original survives orphaned under the recognizable
        // `.cmdr-temp-<uuid>` aside name; log so the trail tells anyone it's
        // recoverable (AGENTS.md principle #4: protect the user's data).
        if let Err(restore_err) = fs::rename(&aside_path, dest) {
            crate::log_error!(
                "safe_overwrite_file: failed to restore aside {} -> {}: {}",
                aside_path.display(),
                dest.display(),
                restore_err
            );
        }
        let _ = fs::remove_file(&temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to finalize overwrite: {}", e),
        });
    }

    // Step 4: Delete the renamed-aside original (non-critical, ignore errors).
    // Use remove_dir_all for directory asides (file-over-folder overwrite).
    //
    // Intentional: we do NOT retain a backup of the overwritten original for
    // rollback. Keeping per-file backups for the whole operation risks
    // unexpectedly filling the user's drive on a large Overwrite. Consequence:
    // rollback removes new files but can't restore overwritten originals.
    // Revisit if users complain. See transfer/CLAUDE.md § "Overwrite isn't reversible".
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
