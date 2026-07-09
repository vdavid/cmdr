//! Remote zip edits: pull the `.zip` local, edit it with the SAME local
//! [`ArchiveMutator`](crate::file_system::volume::backends::archive::mutator),
//! upload the result under a remote temp name, then swap it into place.
//!
//! ## Why pull-local
//!
//! The `zip` crate's `raw_copy_file` (retained entries copied verbatim, no
//! recompress) needs a `Read + Seek` source, which async ranged reads can't give
//! it. So a remote edit downloads the old archive to a LOCAL temp, runs the
//! ordinary local mutator against it, and uploads the whole rewritten file. The
//! pull is the honest cost (O(archive) network); there's no remote random-access
//! WRITE adapter involved (that's only M-append's true in-place path).
//!
//! ## Data-safety contract (the whole point of this module)
//!
//! The remote ORIGINAL is byte-for-byte untouched until the very last swap:
//!
//! - **Pull** reads the remote `.zip`; it writes nothing remote.
//! - **Apply** rewrites only the LOCAL temp (the mutator's own temp+rename).
//! - **Upload** streams the edited temp to a NEW remote name
//!   (`foo.zip.cmdr-tmp-<uuid>`); the original keeps its name and bytes.
//! - **Swap** is the only step that changes the original. It prefers an atomic
//!   rename-overwrite where the backend supports it (SMB with `ReplaceIfExists`),
//!   and otherwise deletes the original then renames the temp into place. That
//!   delete-then-rename leaves exactly ONE crash window (between the delete and
//!   the rename): the NEW, fully-uploaded data survives under the temp name — it
//!   is never lost, only briefly misnamed. MTP always takes this path (it has no
//!   rename-overwrite and allows same-name siblings, so a rename onto a live name
//!   would DUPLICATE, not replace).
//!
//! A cancel at ANY point before the swap completes leaves the remote original
//! intact (the local temp and any partial remote temp are cleaned up). Pinned by
//! `archive_remote_edit_tests`.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::scratch_dir::ScratchDir;
use super::state::{WriteOperationState, is_cancelled};
use super::types::WriteOperationError;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};

/// Same-directory temp infix: `foo.zip` uploads as `foo.zip.cmdr-tmp-<uuid>`.
/// Mirrors the local mutator's convention and the app-wide `.cmdr-` crash-
/// recoverable-temp prefix. Used to BUILD the upload temp name and to MATCH stale
/// leftovers for reaping — the two must stay in lockstep.
const TEMP_INFIX: &str = ".cmdr-tmp-";

/// The minimum age a remote `<archive>.cmdr-tmp-*` leftover must reach before the
/// next edit of the same archive reaps it. Deliberately generous: unlike the LOCAL
/// reap (edits of one archive serialize on the parent lane, so any local leftover
/// is always an abandoned build), a remote share is multi-machine, so a temp with
/// this exact shape may be a LIVE upload from another Cmdr instance mid-flight. The
/// threshold must comfortably exceed the longest plausible single-archive upload
/// (tens of GB over a slow link still finishes in well under a day) plus clock skew
/// between this machine and the remote's mtime clock (SMB reports server mtime, MTP
/// the device's). A leftover is harmless while it waits (the original is intact and
/// the temp holds the fully-uploaded NEW bytes), so erring long costs almost
/// nothing; erring short risks deleting a legitimate in-flight upload. See the
/// module docs and `write_operations/DETAILS.md` § "Remote edit".
const REMOTE_TEMP_REAP_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// A failure from the remote pull / upload / swap orchestration. Structurally the
/// twin of `archive_edit::engine::PlanError` (a `From` impl in that module bridges them),
/// so the driver's terminal-event handling stays uniform local vs remote.
///
/// `pub(crate)` (not `pub(super)`) so the live-SMB and MTP integration suites
/// under `file_system::volume::backends` can drive [`pull_apply_upload_swap`]
/// directly against a real remote volume.
pub(crate) enum RemoteEditError {
    /// The op was cancelled before the swap committed — the remote original is
    /// untouched.
    Cancelled,
    /// A real fault at a remote stage (pull, upload, or swap) or in the local
    /// apply closure.
    Op(WriteOperationError),
}

/// Runs a local plan+apply closure against a REMOTE archive by pulling it to a
/// local temp first and uploading+swapping after. `plan_and_apply` is exactly the
/// closure the local path runs (it plans against, and mutates, the path it's
/// given) — here it's handed the LOCAL working copy, so planning reads the pulled
/// bytes and the mutator rewrites the pulled file.
///
/// The closure runs on the blocking pool (it decompresses/compresses/fsyncs); the
/// pull, upload, and swap are async I/O against the parent volume.
pub(crate) async fn pull_apply_upload_swap<T, E, F>(
    parent: Arc<dyn Volume>,
    archive_path: PathBuf,
    state: Arc<WriteOperationState>,
    plan_and_apply: F,
) -> Result<T, RemoteEditError>
where
    F: FnOnce(&Path) -> Result<T, E> + Send + 'static,
    E: Into<RemoteEditError> + Send + 'static,
    T: Send + 'static,
{
    if is_cancelled(&state.intent) {
        return Err(RemoteEditError::Cancelled);
    }

    // Reap any stale upload temp left on the remote by a prior crash between an
    // upload and its swap. Mirrors the local mutator's start-of-edit sibling reap,
    // but age-gated (see `reap_remote_temps`). Best-effort and non-blocking: it
    // never fails or delays the edit.
    reap_remote_temps(parent.as_ref(), &archive_path).await;

    // A private local scratch dir; its `Drop` removes the working copy however we
    // leave this function (success, error, or cancel).
    let scratch = ScratchDir::new("cmdr-remote-archive-edit").map_err(|e| io_op("scratch dir", &e.to_string()))?;
    let working = scratch.path().join("archive.zip");

    // 1) Pull the remote `.zip` to the local working copy (streamed, cancelable).
    pull_archive(parent.as_ref(), &archive_path, &working, &state).await?;

    // 2) Plan + apply on the local working copy. The mutator's temp+rename commits
    //    the edit onto `working`; a cancel/fault leaves `working` as the pulled
    //    original (its own temp abandoned) — nothing remote has changed yet.
    let working_for_blocking = working.clone();
    let value = match tokio::task::spawn_blocking(move || plan_and_apply(&working_for_blocking)).await {
        Ok(result) => result.map_err(Into::into)?,
        Err(join) => return Err(io_op(&working.display().to_string(), &join.to_string())),
    };

    if is_cancelled(&state.intent) {
        return Err(RemoteEditError::Cancelled);
    }

    // 3+4) Upload the edited local copy under a remote TEMP name, then swap it into
    //       place — the only step that touches the original. See `place_local_file`.
    place_local_file(parent.as_ref(), &working, &archive_path, &state).await?;

    Ok(value)
}

/// Durably places a LOCAL file at a REMOTE path via the SAME upload-to-temp + swap
/// discipline a remote edit commits with: stream the local file to a
/// `.cmdr-tmp-<uuid>` sibling, then swap it into place. The remote target keeps its
/// old bytes — or its ABSENCE, for a brand-new target — until the atomic swap, so a
/// cancel/fault before the swap leaves it untouched with no torn file. Used to SEED
/// a remote compress target with a valid empty zip (see `archive_edit::compress`),
/// and as `pull_apply_upload_swap`'s own commit.
///
/// `pub(crate)` so the compress seed (in `archive_edit`) can reuse it.
pub(crate) async fn place_local_file(
    parent: &dyn Volume,
    local_file: &Path,
    remote_path: &Path,
    state: &WriteOperationState,
) -> Result<(), RemoteEditError> {
    let remote_temp = remote_temp_sibling(remote_path);
    upload_archive(parent, local_file, &remote_temp, state).await?;
    swap_into_place(parent, &remote_temp, remote_path).await?;
    Ok(())
}

/// Streams the remote `.zip` to a local file, checking cancel between chunks.
/// Nothing remote is written. `fsync`s the local copy so the subsequent parse
/// sees complete bytes even under memory pressure.
async fn pull_archive(
    parent: &dyn Volume,
    remote_path: &Path,
    local_working: &Path,
    state: &WriteOperationState,
) -> Result<(), RemoteEditError> {
    let mut stream = parent
        .open_read_stream(remote_path)
        .await
        .map_err(vol_op(remote_path))?;
    let mut file = tokio::fs::File::create(local_working)
        .await
        .map_err(|e| io_op(&local_working.display().to_string(), &e.to_string()))?;

    while let Some(chunk) = stream.next_chunk().await {
        if is_cancelled(&state.intent) {
            return Err(RemoteEditError::Cancelled);
        }
        let chunk = chunk.map_err(vol_op(remote_path))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| io_op(&local_working.display().to_string(), &e.to_string()))?;
    }
    file.flush()
        .await
        .map_err(|e| io_op(&local_working.display().to_string(), &e.to_string()))?;
    file.sync_all()
        .await
        .map_err(|e| io_op(&local_working.display().to_string(), &e.to_string()))?;
    Ok(())
}

/// Uploads the edited local copy to `remote_temp` via `write_from_stream`
/// (streamed, never whole-buffered). On cancel or any fault, the partial remote
/// temp is deleted best-effort so no debris lingers at the user's destination.
async fn upload_archive(
    parent: &dyn Volume,
    local_working: &Path,
    remote_temp: &Path,
    state: &WriteOperationState,
) -> Result<(), RemoteEditError> {
    let size = std::fs::metadata(local_working)
        .map_err(|e| io_op(&local_working.display().to_string(), &e.to_string()))?
        .len();

    // A `LocalPosixVolume` gives us a streaming reader over the local working file
    // (the same primitive the cross-volume copy engine uses for a local source).
    let local = LocalPosixVolume::new("archive-upload", "/");
    let stream = local
        .open_read_stream(local_working)
        .await
        .map_err(vol_op(local_working))?;

    let progress = |_written: u64, _total: u64| {
        if is_cancelled(&state.intent) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };

    match parent.write_from_stream(remote_temp, size, stream, &progress).await {
        Ok(_) => Ok(()),
        Err(err) => {
            // Remove the partial upload so the user's remote dir isn't left with a
            // stray `.cmdr-tmp-*` (harmless — the original is intact — but tidy).
            let _ = parent.delete(remote_temp).await;
            if is_cancelled(&state.intent) {
                Err(RemoteEditError::Cancelled)
            } else {
                Err(RemoteEditError::Op(to_write_error(remote_temp, &err)))
            }
        }
    }
}

/// Swaps the uploaded temp into the original's place — the ONLY step that changes
/// the remote original. See the module-level data-safety contract.
async fn swap_into_place(parent: &dyn Volume, remote_temp: &Path, archive_path: &Path) -> Result<(), RemoteEditError> {
    // Prefer an ATOMIC rename-overwrite where the backend rejects a same-name
    // collision (SMB, local FS): if the server supports `ReplaceIfExists` the
    // rename replaces the original in one step; if not, it fails and we fall
    // through to delete-then-rename. A backend that ALLOWS same-name siblings
    // (MTP) must NOT attempt this — a rename onto the live name would duplicate,
    // not replace — so it goes straight to delete-then-rename.
    if parent.create_directory_errors_on_existing_dir() && parent.rename(remote_temp, archive_path, true).await.is_ok()
    {
        return Ok(());
    }

    // Delete-then-rename. Tolerate a MISSING original: a brand-new seed target has
    // nothing to delete (and `place_local_file` reaches here for MTP, which allows
    // same-name siblings). Any OTHER delete fault is real. The crash window (between
    // the two) leaves the NEW, fully-uploaded data under the temp name: recoverable,
    // never lost.
    match parent.delete(archive_path).await {
        Ok(()) => {}
        Err(VolumeError::NotFound(_)) => {}
        Err(err) => return Err(RemoteEditError::Op(to_write_error(archive_path, &err))),
    }
    parent
        .rename(remote_temp, archive_path, false)
        .await
        .map_err(vol_op(remote_temp))?;
    Ok(())
}

/// `foo.zip` → `foo.zip.cmdr-tmp-<uuid>` in the same remote directory. A fresh uuid
/// each edit, so a leftover from a crashed prior edit (an abandoned upload — the
/// original is intact) is never mistaken for this one.
fn remote_temp_sibling(archive_path: &Path) -> PathBuf {
    let name = archive_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive.zip".to_string());
    let temp_name = format!("{name}{TEMP_INFIX}{}", Uuid::new_v4());
    match archive_path.parent() {
        Some(parent) => parent.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

/// Reaps stale upload temps left on the remote by a crash between a prior upload
/// and its swap (a `<archive>.cmdr-tmp-*` sibling holding the fully-uploaded new
/// bytes under a temp name — harmless, but untidy). Runs at the start of the next
/// edit of the SAME remote archive, the mirror of the local mutator's
/// `reap_sibling_temps`, but with two remote-specific guards:
///
/// - **One round-trip.** A single `list_directory` of the archive's parent, then a
///   `delete` per stale match. Nothing on the read path, no polling.
/// - **Age-gated.** Only leftovers older than [`REMOTE_TEMP_REAP_MIN_AGE`] are
///   removed, so a live upload from another Cmdr instance (same temp-name shape, a
///   fresh mtime) is never deleted mid-flight. An entry with no reported mtime is
///   treated as fresh and spared.
///
/// Matches ONLY this archive's own temp shape (`<archive-name>.cmdr-tmp-…`, files
/// only), so a sibling archive's temps and unrelated files are untouched.
///
/// Best-effort throughout: a listing or delete failure is logged at debug and
/// never fails or delays the user's edit.
async fn reap_remote_temps(parent: &dyn Volume, archive_path: &Path) {
    let (Some(dir), Some(file_name)) = (archive_path.parent(), archive_path.file_name()) else {
        return;
    };
    let prefix = format!("{}{TEMP_INFIX}", file_name.to_string_lossy());

    let entries = match parent.list_directory(dir, None).await {
        Ok(entries) => entries,
        Err(err) => {
            log::debug!(target: "archive_remote_edit", "skipping remote temp reap ({}): {err}", dir.display());
            return;
        }
    };

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let min_age_secs = REMOTE_TEMP_REAP_MIN_AGE.as_secs();

    for entry in entries {
        if entry.is_directory || !entry.name.starts_with(&prefix) {
            continue;
        }
        // Spare anything without a confirmably-old mtime: an unknown mtime, or one
        // younger than the threshold (a possible live upload), is left alone.
        let old_enough = entry
            .modified_at
            .is_some_and(|modified| now_secs.saturating_sub(modified) >= min_age_secs);
        if !old_enough {
            continue;
        }
        let temp_path = dir.join(&entry.name);
        if let Err(err) = parent.delete(&temp_path).await {
            log::debug!(target: "archive_remote_edit", "couldn't reap remote temp {}: {err}", temp_path.display());
        }
    }
}

fn vol_op(path: &Path) -> impl Fn(VolumeError) -> RemoteEditError + '_ {
    move |err| RemoteEditError::Op(to_write_error(path, &err))
}

fn to_write_error(path: &Path, err: &VolumeError) -> WriteOperationError {
    WriteOperationError::IoError {
        path: path.display().to_string(),
        message: err.to_string(),
    }
}

fn io_op(path: &str, message: &str) -> RemoteEditError {
    RemoteEditError::Op(WriteOperationError::IoError {
        path: path.to_string(),
        message: message.to_string(),
    })
}

#[cfg(test)]
#[path = "archive_remote_edit_tests.rs"]
mod archive_remote_edit_tests;
