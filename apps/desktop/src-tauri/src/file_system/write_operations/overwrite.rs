//! How a local write lands: the bytes always go to a `.cmdr-tmp-*` sibling and
//! take the destination's real name by a single same-directory `rename(2)`. When
//! an existing entry is in the way, it is renamed aside first, so the user's
//! original survives every failure up to the swap.
//!
//! **The invariant.** A local destination path must never hold a partial file.
//! Whatever a crash, a force-quit, or an abandoned worker thread leaves behind
//! wears a `.cmdr-tmp-*` name nobody mistakes for their data. Staging is what
//! makes abandoning a worker safe: it is writing to a temp nobody will rename.
//!
//! [`stage_and_land_file`] is the single landing for every local file copy —
//! APFS clone, chunked copy, `copy_file_range`, and the `std::fs::copy`
//! fallback all hand it a closure that puts bytes at a path.
//! [`safe_overwrite_dir`] is its type-agnostic sibling, for a materializer that
//! creates a directory rather than a file.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use super::state::WriteOperationState;
use super::types::WriteOperationError;
use crate::file_system::staging::StagingTemp;

/// Result of applying a conflict resolution.
#[derive(Debug)]
pub(super) struct ResolvedDestination {
    /// The path to write to
    pub path: PathBuf,
    /// Whether this is an overwrite that needs safe handling
    pub needs_safe_overwrite: bool,
}

/// Lands one local file write at `dest` through a staging sibling, replacing an
/// existing entry only when `replacing` says so.
///
/// `write_bytes` is handed the STAGING path and must put the complete file
/// there; it never sees `dest`. That is the whole point: whatever it leaves
/// behind on a failure, a cancel, or a thread that never returns is a
/// recognizable temp, not a truncated file wearing the user's filename.
///
/// Steps:
/// 1. Run `write_bytes` against `dest.cmdr-tmp-{uuid}` (a sibling, so the
///    landing rename stays same-directory and therefore atomic).
/// 2. `replacing` only: rename the existing entry to `dest.cmdr-temp-{uuid}`
///    (the aside).
/// 3. Rename the temp onto `dest`. Without `replacing` this refuses to clobber
///    an entry that appeared underneath us ([`rename_no_replace`]), keeping the
///    `O_EXCL`-shaped guarantee a direct create used to give.
/// 4. `replacing` only: delete the aside.
///
/// A failure at any step before 3 completes leaves the original `dest`
/// untouched. The temp is registered as an in-flight partial for exactly as
/// long as it exists ([`super::in_flight_temps`]), so a quit or a crash mid-copy
/// leaves a swept-at-next-launch leftover rather than permanent litter.
///
/// **File→folder overwrite (incoming source file, existing dest folder).**
/// Local FS `rename(2)` happily swaps a directory aside under a new name, and
/// step 3 lands the source file at the original path. The aside is then removed
/// via `remove_dir_all`. The window during which the original directory is
/// gone-but-replaceable is bounded by step 3 (a single `rename` syscall). A
/// crash between step 2 and step 3 leaves a stray `dest.cmdr-temp-<uuid>/` that
/// a user can recognize and restore from.
pub(super) fn stage_and_land_file<F>(
    state: &Arc<WriteOperationState>,
    dest: &Path,
    replacing: bool,
    write_bytes: F,
) -> Result<u64, WriteOperationError>
where
    F: FnOnce(&Path) -> Result<u64, WriteOperationError>,
{
    // Both guards live to the end of this function, keeping the two scratch
    // files out of the pane for as long as they're on disk. Sharing one uuid
    // makes a crash leftover legible as two halves of one overwrite.
    let uuid = Uuid::new_v4();
    let owner = state.liveness_token();
    let temp = StagingTemp::mint_with_uuid(dest, uuid, owner.clone());
    let temp_path = temp.path();

    // Both halves of the write go into the downloads watcher's ignore set: the
    // CREATE lands on the temp and the RENAME carries it to the final name, and
    // the watcher keys on exact paths. Registering only the final name would let
    // a copy into ~/Downloads toast its own `.cmdr-tmp-*`. No-ops elsewhere.
    crate::downloads::note_pending_write_for_cmdr(temp_path);
    crate::downloads::note_pending_write_for_cmdr(dest);

    // Findable from the moment the file can exist until the moment it can't:
    // unlike the async cross-volume path (`transfer/staged_write.rs`), landing
    // here is one synchronous syscall, so there is no window in which the temp
    // holds the only complete copy of anything.
    super::in_flight_temps::register(state, temp_path);

    // Step 1: fill the temp.
    let bytes = match write_bytes(temp_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            discard_temp(state, temp_path);
            return Err(e);
        }
    };

    // Step 2: move the existing entry out of the way (overwrite only).
    let aside = replacing.then(|| StagingTemp::mint_aside(dest, uuid, owner));
    if let Some(aside) = &aside
        && let Err(e) = fs::rename(dest, aside.path())
    {
        discard_temp(state, temp_path);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to set aside existing destination: {}", e),
        });
    }

    // Step 3: give the bytes their real name.
    if let Err(e) = land_temp(temp_path, dest, replacing) {
        // Restore the aside if we set one. If the restore ALSO fails, the user's
        // original survives orphaned under the recognizable `.cmdr-temp-<uuid>`
        // name; log so the trail tells anyone it's recoverable (AGENTS.md
        // principle 1: protect the user's data).
        if let Some(aside) = &aside
            && let Err(restore_err) = fs::rename(aside.path(), dest)
        {
            crate::log_error!(
                "stage_and_land_file: failed to restore aside {} -> {}: {}",
                aside.path().display(),
                dest.display(),
                restore_err
            );
        }
        discard_temp(state, temp_path);
        // A destination that appeared underneath a non-replacing write is a
        // typed outcome the caller acts on, not an opaque IO failure.
        return Err(match e.kind() {
            std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists {
                path: dest.display().to_string(),
            },
            _ => WriteOperationError::IoError {
                path: dest.display().to_string(),
                message: format!("Failed to finalize the copy: {}", e),
            },
        });
    }
    // The temp is gone (it IS `dest` now), so it stops being a partial.
    super::in_flight_temps::deregister(state, temp_path);

    // Step 4: Delete the renamed-aside original (non-critical, ignore errors).
    // Use remove_dir_all for directory asides (file-over-folder overwrite).
    //
    // Intentional: we do NOT retain a backup of the overwritten original for
    // rollback. Keeping per-file backups for the whole operation risks
    // unexpectedly filling the user's drive on a large Overwrite. Consequence:
    // rollback removes new files but can't restore overwritten originals.
    // Revisit if users complain. See transfer/CLAUDE.md § "Overwrite isn't reversible".
    if let Some(aside) = &aside {
        if aside.path().is_dir() {
            let _ = fs::remove_dir_all(aside.path());
        } else {
            let _ = fs::remove_file(aside.path());
        }
    }

    Ok(bytes)
}

/// Removes a temp whose write or landing failed, and stops tracking it.
///
/// The bytes there are a partial (or a complete copy whose source is still on
/// disk, since the caller reports the item failed and never deletes a source),
/// so there is nothing to preserve. Best-effort: a temp a wedged thread still
/// holds open may refuse to go, which is why it wears a recognizable name.
fn discard_temp(state: &Arc<WriteOperationState>, temp_path: &Path) {
    let _ = fs::remove_file(temp_path);
    super::in_flight_temps::deregister(state, temp_path);
}

/// Renames `temp` onto `dest`, refusing to replace an existing entry unless
/// `replacing`.
fn land_temp(temp: &Path, dest: &Path, replacing: bool) -> std::io::Result<()> {
    if replacing {
        fs::rename(temp, dest)
    } else {
        rename_no_replace(temp, dest)
    }
}

/// `rename(2)` that fails with `AlreadyExists` instead of clobbering `dest`.
///
/// Staging moved the create off the destination name, and a plain POSIX rename
/// replaces silently — so without this a non-overwrite copy would quietly
/// destroy a file that appeared between the conflict check and the landing,
/// where the old direct-create-with-`O_EXCL`/`COPYFILE_EXCL` refused. Uses the
/// kernel's atomic flag where there is one (`RENAME_EXCL` on macOS,
/// `RENAME_NOREPLACE` on Linux) and degrades to a check-then-rename on a
/// filesystem that doesn't support it, which is racy but still strictly better
/// than an unconditional clobber.
fn rename_no_replace(temp: &Path, dest: &Path) -> std::io::Result<()> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        if let (Ok(from), Ok(to)) = (
            CString::new(temp.as_os_str().as_bytes()),
            CString::new(dest.as_os_str().as_bytes()),
        ) {
            // SAFETY: both pointers are live, NUL-terminated C strings held across
            // the call, and the flag is the documented no-replace constant for the
            // platform. The call touches no memory of ours.
            #[cfg(target_os = "macos")]
            let rc = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
            // SAFETY: as above; `AT_FDCWD` makes both paths cwd-relative, matching
            // the absolute paths we pass.
            #[cfg(target_os = "linux")]
            let rc = unsafe {
                libc::renameat2(
                    libc::AT_FDCWD,
                    from.as_ptr(),
                    libc::AT_FDCWD,
                    to.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            if rc == 0 {
                return Ok(());
            }
            let err = std::io::Error::last_os_error();
            let unsupported = matches!(
                err.raw_os_error(),
                Some(libc::ENOTSUP) | Some(libc::EINVAL) | Some(libc::ENOSYS)
            );
            if !unsupported {
                return Err(err);
            }
            log::debug!(
                target: "copy",
                "rename_no_replace: {} doesn't support an atomic no-replace rename ({err}); checking first instead",
                dest.display()
            );
        }
    }

    if fs::symlink_metadata(dest).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "destination already exists",
        ));
    }
    fs::rename(temp, dest)
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
    // The guard lives to the end of this function, keeping the aside out of the
    // pane for as long as it's on disk.
    let aside = StagingTemp::mint_aside(dest, Uuid::new_v4(), None);
    let aside_path = aside.path();

    // Step 1: Rename existing dest aside. This survives a crash: the original
    // is recognizable on next launch and the user can rename it back by hand.
    if let Err(e) = fs::rename(dest, aside_path) {
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
                let _ = fs::remove_dir_all(aside_path);
            } else {
                let _ = fs::remove_file(aside_path);
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
            if let Err(restore_err) = fs::rename(aside_path, dest) {
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
