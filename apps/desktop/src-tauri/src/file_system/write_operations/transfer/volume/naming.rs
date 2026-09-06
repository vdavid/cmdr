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
use crate::file_system::volume::{Volume, VolumeError};

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

/// The suffix a rescued file wears, so a person meeting it in their pane can
/// tell what it is. Draft copy: filenames carry no locale here, the same way the
/// ` (N)` duplicate convention doesn't.
const RECOVERED_SUFFIX: &str = " (recovered)";

/// Gets the complete new bytes out of `.cmdr-tmp-*` space and into a real
/// filename next to where they were meant to land.
///
/// ❗ **This is what stops the hourly reap from eating them.**
/// `cleanup.rs::reap_stale_transfer_temps` matches on the `.cmdr-tmp-` marker
/// plus an age, and it runs at the start of every copy and every volume move
/// into a directory. A temp left here is committed data with no ledger entry
/// (`staged_write::commit` deregisters it before landing), so an hour later the
/// next transfer into the same folder would delete the user's only copy. A file
/// called `notes (recovered).txt` is one no sweep can match.
///
/// Answers where the bytes ARE, which is never nothing: if the rescue rename
/// fails too (the same dead connection that failed the finalize), the temp path
/// is the honest answer and the caller reports that instead.
///
/// ❗ **Only `AlreadyExists` earns another try.** The rename that brought us here
/// has already failed once, so a dead link, a read-only share, or a refused name
/// would fail identically under every candidate: walking eight of them would add
/// eight timeouts to an error path the user is waiting on. A name that is merely
/// TAKEN is the one answer a different name fixes.
pub(super) async fn rescue_out_of_temp_space(dest_volume: &Arc<dyn Volume>, temp: &Path, orig: &Path) -> PathBuf {
    let recovered = recovered_sibling(orig);
    match dest_volume.rename(temp, &recovered, false).await {
        Ok(()) => return recovered,
        Err(VolumeError::AlreadyExists(_)) => {}
        Err(_) => return keeps_its_temp_name(temp),
    }
    // Taken (an earlier rescue of the same file, or the user's own): continue
    // the house ` (N)` series off the recovered name.
    let mut candidates = NameCandidates::for_file(&recovered);
    while candidates.attempts() < RESCUE_NAME_ATTEMPTS {
        let candidate = candidates.current();
        match dest_volume.rename(temp, &candidate, false).await {
            Ok(()) => return candidate,
            Err(VolumeError::AlreadyExists(_)) => candidates.advance(),
            Err(_) => return keeps_its_temp_name(temp),
        }
    }
    keeps_its_temp_name(temp)
}

/// The rescue couldn't happen, so the bytes stay where they are and the caller
/// reports THAT path. Says so loudly: this is the one shape in which committed
/// data still wears a name `cleanup.rs::reap_stale_transfer_temps` matches.
fn keeps_its_temp_name(temp: &Path) -> PathBuf {
    log::warn!(
        "rescue_out_of_temp_space: couldn't move {} to a real filename, so the new data stays under its temp name",
        temp.display()
    );
    temp.to_path_buf()
}

/// How many ` (N)` variants a rescue tries before leaving the bytes under their
/// temp name. Deliberately small: a destination holding eight `notes (recovered)
/// (N)` files is one where something else is very wrong.
const RESCUE_NAME_ATTEMPTS: u32 = 8;

/// `/dir/notes.txt` → `/dir/notes (recovered).txt`, extension kept where it
/// belongs.
fn recovered_sibling(orig: &Path) -> PathBuf {
    let parent = orig.parent().unwrap_or(Path::new(""));
    let stem = orig.file_stem().map(|s| s.to_string_lossy().to_string());
    let name = match (stem, orig.extension()) {
        (Some(stem), Some(ext)) => format!("{stem}{RECOVERED_SUFFIX}.{}", ext.to_string_lossy()),
        (Some(stem), None) => format!("{stem}{RECOVERED_SUFFIX}"),
        // A path with no file name at all can't be helped; the caller's rename
        // will fail and report the temp.
        (None, _) => format!("cmdr{RECOVERED_SUFFIX}"),
    };
    parent.join(name)
}

#[cfg(test)]
#[path = "naming_tests.rs"]
mod tests;
