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

use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::state::{WriteOperationState, is_cancelled};
use super::types::WriteOperationError;
use crate::file_system::volume::{LocalPosixVolume, Volume, VolumeError};

/// A failure from the remote pull / upload / swap orchestration. Structurally the
/// twin of `archive_edit::PlanError` (a `From` impl in that module bridges them),
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

    // A private local scratch dir; its `Drop` removes the working copy however we
    // leave this function (success, error, or cancel).
    let scratch = ScratchDir::new()?;
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

    // 3) Upload the edited local copy under a remote TEMP name. The original keeps
    //    its name and bytes throughout; a cancel/fault deletes the partial temp.
    let remote_temp = remote_temp_sibling(&archive_path);
    upload_archive(parent.as_ref(), &working, &remote_temp, &state).await?;

    // 4) Swap the remote temp into place. The only step that touches the original.
    swap_into_place(parent.as_ref(), &remote_temp, &archive_path).await?;

    Ok(value)
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

    // Delete-then-rename. The crash window (between the two) leaves the NEW,
    // fully-uploaded data under the temp name: recoverable, never lost.
    parent.delete(archive_path).await.map_err(vol_op(archive_path))?;
    parent
        .rename(remote_temp, archive_path, false)
        .await
        .map_err(vol_op(remote_temp))?;
    Ok(())
}

/// `foo.zip` → `foo.zip.cmdr-tmp-<uuid>` in the same remote directory. Matches the
/// local mutator's temp convention and the `.cmdr-` recoverable-temp prefix. A
/// fresh uuid each edit, so a leftover from a crashed prior edit (an abandoned
/// build — the original is intact) is never mistaken for this one.
fn remote_temp_sibling(archive_path: &Path) -> PathBuf {
    let name = archive_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "archive.zip".to_string());
    let temp_name = format!("{name}.cmdr-tmp-{}", Uuid::new_v4());
    match archive_path.parent() {
        Some(parent) => parent.join(temp_name),
        None => PathBuf::from(temp_name),
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

/// A local scratch dir for the pulled/edited working copy, removed on `Drop`
/// however the edit ends (success, error, or cancel) — the local temp never
/// outlives the operation.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new() -> Result<Self, RemoteEditError> {
        let dir = std::env::temp_dir().join(format!("cmdr-remote-archive-edit-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).map_err(|e| io_op(&dir.display().to_string(), &e.to_string()))?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
#[path = "archive_remote_edit_tests.rs"]
mod archive_remote_edit_tests;
