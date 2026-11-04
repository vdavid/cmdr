//! The engine's server-side copy attempt, asked before a file streams: one
//! `Volume::copy_on_server` call, staged the way the destination needs, with a
//! progress hook that answers Cancel and parks on Pause. `DETAILS.md` §
//! "Server-side copy".

use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use super::super::super::host::Announced;
use super::super::super::state::WriteOperationState;
use super::super::recovered_name::FinalizeFailure;
use super::super::staged_write::{CutOffLanding, LandingError, StagedWrite, WriteStaging, resolve_staging};
use super::super::transfer_probe::{TaskPhase, set_task_bytes, set_task_phase};
use super::super::volume_outage::StartedOn;
use super::piped_file::{PipedFile, landed_at_final_name};
use super::transfer_error::hard_abort_error;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{ServerCopyProgress, Volume, VolumeError};

/// Asks the destination to copy the file on the server, from itself or from a
/// sibling place it can copy across (`Volume::copy_on_server`).
///
/// [`ServerSideCopy::Streams`] means "do it the ordinary way", and that is the
/// answer for everything except a clean success, a landing the transport cut
/// off (the copy is done; only its name is unsettled), a genuine cancel, and a
/// refused name:
///
/// - **A pair that can't copy on the server at all**
///   (`Volume::may_copy_on_server_from`), answered before anything is staged.
///   That's nearly every transfer (anything to or from a local disk, SMB, or
///   MTP), and an attempt costs a stage, two ledger records, and a delete of a
///   temp nobody wrote, per file. One of those deletes is where a user's
///   SMB-to-local move sat for 100 s (ERR-PTRGY).
/// - **`NotSupported`**, from a backend with no server-side copy, one whose
///   server lacks the extension, or one that doesn't recognize `source` as a
///   place it can copy from. ❗ That last decision is the BACKEND's, from the
///   source's concrete type and identity: a copy on a server the source path
///   doesn't belong to would copy whatever sits at that path there, which is
///   not a failure but the wrong file, silently. The default answers only for
///   the very same volume instance.
/// - **Any other failure.** The streaming path below has the retry policy, the
///   stall watchdog, and the pause checkpoints; a fast path that failed for a
///   real reason will fail there too, with better handling and a better report.
///
/// ❌ A cancel is NOT a fall-through. The user asked for it to stop, and running
/// the file again the slow way is the opposite of stopping. Nor is a taken
/// name (`AlreadyExists`): the streamed write would refuse it too.
///
/// **Staging** follows the destination: a whole-publishing one (an object
/// store) publishes a server-side copy whole as well, so the copy goes to the
/// final name (`CreateNew` when the caller expected it free) and a temp would
/// only cost a landing rename, which there is a second full copy. Everywhere
/// else the destination holds a byte-incomplete file while the copy runs, so
/// it stages exactly as a streamed write does, single-shot exemption or not.
///
/// **A pause lands at the backend's checkpoints** (between an S3 copy's
/// parts); a backend that copies in one call (SFTP's `copy-data`) has none, so
/// its pause lands at the next file, as the local chunk loop's does.
#[allow(
    clippy::too_many_arguments,
    reason = "One file's whole copy context, the same set `stream_pipe_file` carries"
)]
pub(super) async fn try_server_side_copy(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    announced_dest: Announced<'_>,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    staging: WriteStaging,
) -> Result<ServerSideCopy, FinalizeFailure> {
    let dest_path = announced_dest.path();
    if !dest_volume.may_copy_on_server_from(source_volume.as_ref()) {
        return Ok(ServerSideCopy::Streams);
    }
    // Before the stage, so a dump of a task stuck anywhere in here names this
    // step and never reads as `streaming 0/0`.
    set_task_phase(TaskPhase::CopyingOnServer);
    set_task_bytes(0, 0);
    let started = crate::rt::now();
    // ❌ Never the single-shot answer: only whole-publish holds for a copy.
    let staging = resolve_staging(staging, dest_volume.publishes_writes_whole());
    let staged = StagedWrite::begin(state, &**dest_volume, announced_dest, staging);
    log::debug!(
        target: "copy",
        "try_server_side_copy: asking {} to copy {} on the server, into {}",
        LogPrivate::volume(dest_volume.name()),
        LogPrivate::file_path(source_path),
        LogPrivate::file_path(staged.target()),
    );
    let progress = EngineCopyProgress {
        state,
        on_file_progress,
    };

    // The quit deadline rides the same `select!` it rides for a streamed write.
    // Nothing is cleaned up on that arm: the delete would go back through a
    // connection that is already not answering, and the temp is registered for
    // the startup sweep.
    let outcome = tokio::select! {
        biased;
        () = state.backend_abort.cancelled() => return Err(hard_abort_error(dest_path).into()),
        result = dest_volume.copy_on_server(
            source_volume.as_ref(),
            source_path,
            staged.target(),
            staged.write_mode(),
            &progress,
        ) => result,
    };
    match &outcome {
        Ok(written) => log::debug!(
            target: "copy",
            "try_server_side_copy: {} answered for {} after {:?}: done, size {bytes}",
            LogPrivate::volume(dest_volume.name()),
            LogPrivate::file_path(dest_path),
            crate::rt::since(started),
            bytes = written.bytes,
        ),
        Err(e) => log::debug!(
            target: "copy",
            "try_server_side_copy: {} answered for {} after {:?}: {}",
            LogPrivate::volume(dest_volume.name()),
            LogPrivate::file_path(dest_path),
            crate::rt::since(started),
            LogError(e),
        ),
    }

    match outcome {
        Ok(written) => {
            let bytes = written.bytes;
            let piped = PipedFile {
                bytes,
                landed: landed_at_final_name(&written, staged.target(), dest_path),
            };
            let started_on = StartedOn::now(&[source_volume, dest_volume]);
            match staged.commit(dest_volume, bytes).await {
                Ok(()) => Ok(ServerSideCopy::Landed(piped)),
                Err(LandingError::Failed(failure)) => Err(failure),
                Err(LandingError::CutOff(cut_off)) => Ok(ServerSideCopy::LandingCutOff {
                    cut_off,
                    started_on,
                    bytes,
                }),
            }
        }
        // ❌ A cancel is never retried more slowly. The intent is consulted as
        // well as the variant, so a backend that labels its own stop something
        // else can't turn a Cancel click into a second, full-speed attempt.
        Err(e) if matches!(e, VolumeError::Cancelled(_)) || state.work_stopped() => {
            staged.abandon(dest_volume).await;
            Err(e.into())
        }
        // Somebody holds the name: the streamed write would refuse it too.
        Err(e @ VolumeError::AlreadyExists(_)) => {
            staged.abandon(dest_volume).await;
            Err(e.into())
        }
        // `NotSupported` or a real failure; the answer above already logged which.
        Err(_) => {
            staged.abandon_attempt(dest_volume).await;
            log::debug!(
                target: "copy",
                "try_server_side_copy: cleared the stage for {} after {:?}; streaming it instead",
                LogPrivate::file_path(dest_path),
                crate::rt::since(started),
            );
            Ok(ServerSideCopy::Streams)
        }
    }
}

/// How a server-side copy attempt came out, short of a failure.
pub(super) enum ServerSideCopy {
    /// The server copied the file and it has its final name.
    Landed(PipedFile),
    /// Do it the ordinary way: stream it.
    Streams,
    /// The server copied all `bytes` and the landing was cut off on the
    /// transport: `stream_pipe_file`'s landing wait settles it, as it settles a
    /// streamed file's.
    LandingCutOff {
        cut_off: Box<CutOffLanding>,
        started_on: StartedOn,
        bytes: u64,
    },
}

/// The engine's half of a server-side copy's progress hook: bytes go to the
/// file's progress callback (where a Cancel answers), and each checkpoint is
/// the operation's own cooperative boundary, so a pause parks there.
struct EngineCopyProgress<'a> {
    state: &'a Arc<WriteOperationState>,
    on_file_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
}

impl ServerCopyProgress for EngineCopyProgress<'_> {
    fn advanced(&self, done: u64, total: u64) -> ControlFlow<()> {
        (self.on_file_progress)(done, total)
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async move {
            if self.state.stop_or_park_async().await {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
    }
}
