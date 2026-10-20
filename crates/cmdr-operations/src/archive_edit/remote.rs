//! Remote zip edits: pull the `.zip` local, edit it with the SAME local
//! [`ArchiveMutator`](cmdr_archive::mutator), upload the result under a remote
//! temp name, then swap it into place.
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
//! - **Swap** is the only step that changes the original. It's a staged
//!   write's swap landing (`StagedWrite::begin_over_original`, the copy
//!   engine's file→file Overwrite): delete the original, then rename the temp
//!   into place, ❌ never a rename onto a live name (MTP would DUPLICATE). From
//!   the delete on, the uploaded temp may be the only copy, so it's recorded as
//!   a landing first and ❌ never thrown away: a failed rename rescues it out of
//!   temp space, and a crash leaves it for the sweep that finishes the landing.
//!
//! **A dropped share is waited for** (`transfer/volume_outage.rs`), at every
//! step: the pull runs again, the upload runs again from the local copy, and a
//! swap step the transport cut off settles what the server did once the share
//! is back (`CutOffLanding`), comparing the archive against the local copy.
//! A wait that gives up says "Lost the connection to {server}".
//!
//! A cancel at ANY point before the swap's delete leaves the remote original
//! intact (the local temp and any partial remote temp are cleaned up). Pinned by
//! `remote_tests` and `remote_outage_tests`.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::AsyncWriteExt;

use super::super::in_flight_temps;
use super::super::journal;
use super::super::scratch_dir::ScratchDir;
use super::super::state::WriteOperationState;
use super::super::transfer::StagedWrite;
use super::super::transfer::retry::after_a_cut_off_landing_wait;
use super::super::transfer::staged_write::{LandingError, LandingSource, Resumed};
use super::super::transfer::volume::PathRole;
use super::super::transfer::volume_outage::{OutageEnd, StartedOn, lost_connection, wait_after_failure};
use super::super::types::WriteOperationError;
use super::edit_error::EditError;
use super::outage_wait::{Next, StepError, landing_failure, rerun_through_outages, wait_out};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{Volume, VolumeError};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::types::NotRollbackableReason;

#[derive(Debug, Clone, Copy)]
pub enum RemoteCommitProgress {
    Transferring { bytes_done: u64, bytes_total: u64 },
    Finishing,
}

pub(crate) type RemoteProgressObserver = Arc<dyn Fn(RemoteCommitProgress) + Send + Sync>;

/// `foo.zip` uploads as the same-directory sibling `foo.zip.cmdr-tmp-<uuid>`, the
/// app-wide scratch name (shortened when it wouldn't fit), minted by the staged
/// write. The same module names the prefix that matches stale leftovers, so the
/// two can't drift.
use cmdr_fs::staging::temp_sibling_prefix;

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
/// module docs and `archive_edit/DETAILS.md` § "Remote edit".
const REMOTE_TEMP_REAP_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Runs a local plan+apply closure against a REMOTE archive by pulling it to a
/// local temp first and uploading+swapping after. `plan_and_apply` is exactly the
/// closure the local path runs (it plans against, and mutates, the path it's
/// given) — here it's handed the LOCAL working copy, so planning reads the pulled
/// bytes and the mutator rewrites the pulled file.
///
/// The closure runs on the blocking pool (it decompresses/compresses/fsyncs); the
/// pull, upload, and swap are async I/O against the parent volume.
///
/// `parent_volume_id` is the parent's registered id: the swap records its temp
/// in that volume's path space (`StagedWrite::begin_over_original`), which is
/// what keeps a swap a crash or an outage cut off finishable by the volume's
/// next sweep.
///
/// `pub(crate)` so the live-SMB and MTP integration suites can drive it directly
/// against a real remote volume.
pub async fn pull_apply_upload_swap<T, F>(
    parent: Arc<dyn Volume>,
    parent_volume_id: &str,
    archive_path: PathBuf,
    state: Arc<WriteOperationState>,
    progress: Option<RemoteProgressObserver>,
    plan_and_apply: F,
) -> Result<T, EditError>
where
    F: FnOnce(&Path) -> Result<T, EditError> + Send + 'static,
    T: Send + 'static,
{
    if state.intent().is_cancelled() {
        return Err(EditError::Cancelled);
    }

    // Reap any stale upload temp left on the remote by a prior crash between an
    // upload and its swap. Mirrors the local mutator's start-of-edit sibling reap,
    // but age-gated (see `reap_remote_temps`). Best-effort and non-blocking: it
    // never fails or delays the edit.
    reap_remote_temps(&state, parent.as_ref(), parent_volume_id, &archive_path).await;

    // A private local scratch dir; its `Drop` removes the working copy however we
    // leave this function (success, error, or cancel).
    let scratch = ScratchDir::new("cmdr-remote-archive-edit").map_err(|e| io_op("scratch dir", &e.to_string()))?;
    let working = scratch.path().join("archive.zip");

    // 1) Pull the remote `.zip` to the local working copy (streamed, cancelable).
    //    A read, so a dropped share is waited for and the pull runs again.
    rerun_through_outages(&state, &[&parent], &archive_path, || {
        pull_archive(parent.as_ref(), &archive_path, &working, &state)
    })
    .await?;

    // 2) Plan + apply on the local working copy. The mutator's temp+rename commits
    //    the edit onto `working`; a cancel/fault leaves `working` as the pulled
    //    original (its own temp abandoned) — nothing remote has changed yet.
    let working_for_blocking = working.clone();
    let applying = state
        .scope()
        .host()
        .runtime()
        .spawn_blocking(move || plan_and_apply(&working_for_blocking));
    let value = match applying.await {
        Ok(result) => result?,
        Err(join) => return Err(io_op(&working.display().to_string(), &join.to_string())),
    };

    if state.intent().is_cancelled() {
        return Err(EditError::Cancelled);
    }

    // 3+4) Upload the edited local copy under a remote TEMP name, then swap it into
    //       place — the only step that touches the original. See `place_local_file`.
    place_local_file(
        &parent,
        parent_volume_id,
        &working,
        &archive_path,
        &state,
        progress.as_ref(),
    )
    .await?;

    Ok(value)
}

/// Durably places a LOCAL file at a REMOTE path via the SAME upload-to-temp + swap
/// discipline a remote edit commits with: stream the local file to a
/// `.cmdr-tmp-<uuid>` sibling, then swap it into place. The remote target keeps its
/// old bytes until the swap, so a cancel/fault before the swap leaves it untouched
/// with no torn file. This is `pull_apply_upload_swap`'s commit.
///
/// A dropped share is waited for: an upload it cut off runs again from the local
/// file, and a swap step it cut off is settled against the local file once the
/// share is back ([`swap_into_place`]). One outage cap covers both, as for one
/// file of a copy.
async fn place_local_file(
    parent: &Arc<dyn Volume>,
    parent_volume_id: &str,
    local_file: &Path,
    remote_path: &Path,
    state: &Arc<WriteOperationState>,
    progress: Option<&RemoteProgressObserver>,
) -> Result<(), EditError> {
    let size = std::fs::metadata(local_file)
        .map_err(|e| io_op(&local_file.display().to_string(), &e.to_string()))?
        .len();
    // A `LocalPosixVolume` gives us a streaming reader over the local working file
    // (the same primitive the cross-volume copy engine uses for a local source),
    // and is what a settled swap compares the remote archive against.
    let local: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "archive-upload",
        "/",
        state.scope().host().volume_host().clone(),
    ));
    let volumes = [parent];
    let mut outages = 0;
    loop {
        if state.intent().is_cancelled() {
            return Err(EditError::Cancelled);
        }
        // A fresh uuid each attempt, so a leftover from a crashed prior edit (an
        // abandoned upload; the original is intact) is never mistaken for this one.
        let target = state.scope().host().announce_on(parent.as_ref(), remote_path);
        let staged = StagedWrite::begin_over_original(state, parent.as_ref(), target, parent_volume_id);
        let started_on = StartedOn::now(&volumes);
        if let Err(failed) =
            upload_archive(parent, &local, local_file, size, &staged, remote_path, state, progress).await
        {
            let next = wait_out(state, failed, &volumes, &started_on, &mut outages, remote_path).await;
            // The partial upload goes once the share can take the delete, so
            // after the wait; one it can't take stays recorded for the sweep.
            staged.abandon(parent).await;
            match next {
                Next::Again => continue,
                Next::Stop(ended) => return Err(ended),
            }
        }
        // A user can cancel while the backend closes the completed upload, after
        // its final progress callback but before publication. The temp is ours;
        // remove it and leave the target untouched instead of turning that cancel
        // into a successful swap.
        if state.intent().is_cancelled() {
            staged.abandon(parent).await;
            return Err(EditError::Cancelled);
        }
        // An archive edit is never rollbackable, for its own reason. Noted
        // first, it's the one the journal keeps: the swap's delete of the
        // original notes `Overwrote`, which would word the refusal as a copy's.
        journal::note_not_rollbackable(state.scope(), NotRollbackableReason::ZipEditUnsupported);
        let source = LandingSource {
            volume: &local,
            path: local_file,
        };
        match swap_into_place(parent, staged, size, source, remote_path, state, &mut outages).await {
            Swapped::Landed => return Ok(()),
            Swapped::UploadAgain => continue,
            Swapped::Ended(ended) => return Err(ended),
        }
    }
}

/// Streams the remote `.zip` to a local file, checking cancel between chunks.
/// Nothing remote is written. `fsync`s the local copy so the subsequent parse
/// sees complete bytes even under memory pressure. Truncates the local file
/// first, so a pull that runs again after an outage starts clean.
async fn pull_archive(
    parent: &dyn Volume,
    remote_path: &Path,
    local_working: &Path,
    state: &WriteOperationState,
) -> Result<(), StepError> {
    let local_error = |e: std::io::Error| StepError::Ended(io_op(&local_working.display().to_string(), &e.to_string()));
    let remote_error = |e: VolumeError| StepError::at(remote_path, PathRole::Source, e);
    let mut stream = parent.open_read_stream(remote_path).await.map_err(remote_error)?;
    let mut file = tokio::fs::File::create(local_working).await.map_err(local_error)?;

    while let Some(chunk) = stream.next_chunk().await {
        if state.intent().is_cancelled() {
            return Err(StepError::Ended(EditError::Cancelled));
        }
        let chunk = chunk.map_err(remote_error)?;
        file.write_all(&chunk).await.map_err(local_error)?;
    }
    file.flush().await.map_err(local_error)?;
    file.sync_all().await.map_err(local_error)?;
    Ok(())
}

/// Uploads the edited local copy to the staged write's temp via
/// `write_from_stream` (streamed, never whole-buffered). The caller abandons
/// the staged write on any failure, which removes the partial.
#[allow(
    clippy::too_many_arguments,
    reason = "one upload: both ends, the staged write, its size, the path it reports, and its progress"
)]
async fn upload_archive(
    parent: &Arc<dyn Volume>,
    local: &Arc<dyn Volume>,
    local_working: &Path,
    size: u64,
    staged: &StagedWrite,
    remote_path: &Path,
    state: &WriteOperationState,
    progress_observer: Option<&RemoteProgressObserver>,
) -> Result<(), StepError> {
    let stream = local
        .open_read_stream(local_working)
        .await
        .map_err(|e| StepError::Ended(io_op(&local_working.display().to_string(), &e.to_string())))?;

    if let Some(observer) = progress_observer {
        observer(RemoteCommitProgress::Transferring {
            bytes_done: 0,
            bytes_total: size,
        });
    }
    let progress = |progress: cmdr_fs::volume::StreamWriteProgress| {
        let written = progress.bytes_written;
        let total = size;
        if let Some(observer) = progress_observer {
            if written >= total {
                observer(RemoteCommitProgress::Finishing);
            } else {
                observer(RemoteCommitProgress::Transferring {
                    bytes_done: written,
                    bytes_total: total,
                });
            }
        }
        if state.intent().is_cancelled() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };

    match parent
        .write_from_stream(
            staged.target(),
            staged.write_mode(),
            cmdr_fs::volume::StreamLength::Known(size),
            stream,
            &progress,
        )
        .await
    {
        Ok(_) => {
            if let Some(observer) = progress_observer {
                observer(RemoteCommitProgress::Finishing);
            }
            Ok(())
        }
        Err(_) if state.intent().is_cancelled() => Err(StepError::Ended(EditError::Cancelled)),
        Err(err) => Err(StepError::at(remote_path, PathRole::Destination, err)),
    }
}

/// What the swap came to.
enum Swapped {
    /// The edited archive is at its name.
    Landed,
    /// Nothing proves the swap landed and no whole temp is left to land: upload
    /// the local copy again.
    UploadAgain,
    Ended(EditError),
}

/// Swaps the uploaded temp into the original's place, the ONLY step that
/// changes the remote original: the staged write's swap landing (delete the
/// original, then rename the temp onto its name; see the module docs).
///
/// A step the transport cut off waits for the share, then settles what the
/// server did (`CutOffLanding::resume`, the bytes compared against the local
/// copy, `source`) and lands what's left. A wait that ends without the share
/// gives the landing up under the refusal rules: from the delete on, the temp
/// is rescued out of temp space or kept recorded as a landing, ❌ never thrown
/// away.
async fn swap_into_place(
    parent: &Arc<dyn Volume>,
    staged: StagedWrite,
    size: u64,
    source: LandingSource<'_>,
    archive_path: &Path,
    state: &Arc<WriteOperationState>,
    outages: &mut u32,
) -> Swapped {
    let volumes = [parent];
    let mut started_on = StartedOn::now(&volumes);
    let mut cut_off = match staged.commit(parent, size).await {
        Ok(()) => return Swapped::Landed,
        Err(LandingError::Failed(failure)) => return Swapped::Ended(landing_failure(archive_path, failure, None)),
        Err(LandingError::CutOff(cut_off)) => cut_off,
    };
    let mut attempt = 1;
    loop {
        let ended = wait_after_failure(state, cut_off.error(), &volumes, &started_on, outages).await;
        let ended = after_a_cut_off_landing_wait(ended, cut_off.error(), &mut attempt, state).await;
        if ended != OutageEnd::Back {
            let lost = lost_connection(ended, cut_off.error(), &volumes);
            return match cut_off.give_up(parent).await {
                Ok(()) => Swapped::Landed,
                Err(_) if ended == OutageEnd::Cancelled => Swapped::Ended(EditError::Cancelled),
                Err(failure) => Swapped::Ended(landing_failure(archive_path, failure, lost)),
            };
        }
        started_on = StartedOn::now(&volumes);
        match cut_off.resume(parent, Some(source)).await {
            Resumed::Landed => return Swapped::Landed,
            Resumed::RunAgain(_) => return Swapped::UploadAgain,
            Resumed::Failed(failure) => return Swapped::Ended(landing_failure(archive_path, failure, None)),
            Resumed::Stopped => return Swapped::Ended(EditError::Cancelled),
            Resumed::CutOff(again) => cut_off = again,
        }
    }
}

/// Reaps stale upload temps left on the remote by a crash between a prior upload
/// and its swap (a `<archive>.cmdr-tmp-*` sibling holding the fully-uploaded new
/// bytes under a temp name — harmless, but untidy). Runs at the start of the next
/// edit of the SAME remote archive, the mirror of the local mutator's
/// `reap_sibling_temps`, but with three remote-specific guards:
///
/// - **One round-trip.** A single `list_directory` of the archive's parent, then a
///   `delete` per stale match. Nothing on the read path, no polling.
/// - **Age-gated.** Only leftovers older than [`REMOTE_TEMP_REAP_MIN_AGE`] are
///   removed, so a live upload from another Cmdr instance (same temp-name shape, a
///   fresh mtime) is never deleted mid-flight. An entry with no reported mtime is
///   treated as fresh and spared.
/// - **❌ Never a temp the in-flight ledger records** for this volume: a
///   recorded temp is a live upload of ours, or a swap a crash or an outage cut
///   off after the original's delete, whose temp is the archive's ONLY copy
///   until the volume's sweep lands it.
///
/// Matches ONLY this archive's own temp shape (`<archive-name>.cmdr-tmp-…`, files
/// only), so a sibling archive's temps and unrelated files are untouched.
///
/// Best-effort throughout: a listing or delete failure is logged at debug and
/// never fails or delays the user's edit.
async fn reap_remote_temps(
    state: &WriteOperationState,
    parent: &dyn Volume,
    parent_volume_id: &str,
    archive_path: &Path,
) {
    let (Some(dir), Some(file_name)) = (archive_path.parent(), archive_path.file_name()) else {
        return;
    };
    // The mint's own prefix, so a temp shortened to fit is matched too.
    let prefix = temp_sibling_prefix(&file_name.to_string_lossy());

    let entries = match parent.list_directory(dir, None).await {
        Ok(entries) => entries,
        Err(err) => {
            log::debug!(target: "archive_remote_edit", "skipping remote temp reap ({}): {err}", LogPrivate::dir_path(dir), err = LogError(&err));
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
        let temp_path = dir.join(&entry.name);
        if !old_enough || in_flight_temps::is_recorded_on(state, parent_volume_id, parent.root(), &temp_path) {
            continue;
        }
        if let Err(err) = parent.delete(&temp_path).await {
            log::debug!(target: "archive_remote_edit", "couldn't reap remote temp {}: {err}", LogPrivate::file_path(&temp_path), err = LogError(&err));
        }
    }
}

fn io_op(path: &str, message: &str) -> EditError {
    EditError::Op(WriteOperationError::IoError {
        path: path.to_string(),
        message: message.to_string(),
    })
}

#[cfg(test)]
#[path = "remote_outage_tests.rs"]
mod remote_outage_tests;
#[cfg(test)]
#[path = "remote_tests.rs"]
mod remote_tests;
