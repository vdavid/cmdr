//! Naming a destination on a volume: the ` (N)` pick, and how it gets claimed.
//!
//! The candidates and the sequence rule are the shared ones
//! (`write_operations::unique_name`); what's volume-specific is HOW a pick is
//! held: an `O_CREAT|O_EXCL` placeholder where the destination is local-FS
//! backed, an `exists()` probe everywhere else, and the operation's
//! `ClaimedNames` ledger over both. Kept out of `conflict.rs`, which decides
//! conflict POLICY and only asks this for a name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::unique_name::{ClaimedNames, NameCandidates};
use crate::file_system::volume::Volume;

/// Finds a unique filename on a volume by appending " (1)", " (2)", etc.
///
/// On a **local-FS-backed** destination volume (`local_path().is_some()`) the
/// chosen name is atomically RESERVED with an `O_CREAT|O_EXCL` placeholder, the
/// same TOCTOU guard `unique_name::find_unique_name` uses for the local-FS copy
/// path. Without it, a concurrent writer (a second Cmdr op, a cloud-sync agent,
/// a backup tool) could land a real file at `name (N)` between our non-atomic
/// `exists()` probe and the streaming writer's create+truncate, and the copy
/// would silently clobber it. The streaming write then lands ON the placeholder
/// (the write site opens the dest with create+truncate), exactly like the
/// local-FS path's `needs_safe_overwrite` flow. The returned path is the volume
/// path; the placeholder is created at the resolved local path.
///
/// On backends without exclusive-create semantics (MTP / SMB / InMemory,
/// `local_path()` is `None`) we can't reserve, so we fall back to the
/// `exists()` probe and re-check existence immediately before returning to keep
/// the residual window as narrow as the backend allows.
///
/// A **directory** takes that probe branch on every backend, local-FS dest
/// included. The placeholder is a FILE, and one sitting where the copy is about
/// to create a directory makes `merge.rs::merge_level`'s `create_directory`
/// report `AlreadyExists`, so the walk would try to merge into it and list it.
/// Letting the merge walker create the directory itself is also what records it
/// in `CreatedPaths`, which a pre-created one would miss and rollback would then
/// leave behind.
///
/// Both branches record the pick in the operation's `ClaimedNames` ledger and
/// walk past what's already there, which is what the probe alone can't do for a
/// directory (never reserved) or for the concurrent driver resolving several
/// top-level sources at once. Without it `photo.jpg` and `photo (1).jpg`
/// duplicated together both land on `photo (2).jpg`.
///
/// Naming itself is not this function's business: the candidates come from
/// `unique_name::NameCandidates`, the same sequence the local-FS namer walks, so a
/// volume dest numbers identically (`photo (1).jpg` continues to `photo (2).jpg`,
/// and `is_directory` also picks the candidate KIND, so `my.dir` numbers whole).
/// This function owns only the reservation.
pub(super) async fn find_unique_volume_name(
    dest_volume: &Arc<dyn Volume>,
    path: &Path,
    is_directory: bool,
    claimed: &ClaimedNames,
) -> PathBuf {
    let local_root = dest_volume.local_path().filter(|_| !is_directory);
    let mut candidates = if is_directory {
        NameCandidates::for_directory(path)
    } else {
        NameCandidates::for_file(path)
    };

    loop {
        let new_path = candidates.current();

        if !claimed.claim(&new_path) {
            // Spoken for by another source of this same operation.
            candidates.advance();
            continue;
        }

        if let Some(root) = &local_root {
            // Local-FS dest: reserve the name with an O_CREAT|O_EXCL placeholder
            // so no concurrent writer can sneak a file in before our write lands.
            let local_path = resolve_local_path(root, &new_path);
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&local_path)
            {
                Ok(_) => return new_path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    candidates.advance();
                }
                Err(_) => {
                    // Anything else (parent unwritable, ENOSPC, …) leaks back to
                    // the caller's write attempt, which has its own error path.
                    return new_path;
                }
            }
        } else {
            // Non-local backend: best-effort `exists()` probe. Re-check right
            // before returning to keep the residual window as narrow as we can.
            if !dest_volume.exists(&new_path).await {
                return new_path;
            }
            candidates.advance();
        }

        // Safety limit to prevent an infinite loop.
        if candidates.attempts() > 1000 {
            // Extremely unlikely to happen.
            return candidates.current();
        }
    }
}

/// Resolves a destination-volume path against a local-FS volume root, so the
/// O_EXCL reservation lands at the same local path the volume's streaming
/// writer will later resolve `new_path` to. The rule itself is
/// `cmdr_fs::volume::root_anchored`, which `LocalPosixVolume::resolve` uses too:
/// that shared rule IS the guarantee the two paths agree.
fn resolve_local_path(root: &Path, path: &Path) -> PathBuf {
    cmdr_fs::volume::root_anchored(root, path)
}

#[cfg(test)]
#[path = "naming_tests.rs"]
mod tests;
