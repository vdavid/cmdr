//! The engine's server-side copy attempt, asked before a file streams: one
//! `Volume::copy_on_server` call, staged the way the destination needs, with a
//! progress hook that answers Cancel and parks on Pause. `DETAILS.md` §
//! "Server-side copy".

use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use super::super::super::state::{WriteOperationState, is_cancelled};
use super::super::recovered_name::FinalizeFailure;
use super::super::staged_write::{StagedWrite, WriteStaging, note_pending_for_local_dest, resolve_staging};
use super::super::transfer_probe::{TaskPhase, set_task_bytes, set_task_phase};
use super::transfer_error::hard_abort_error;
use crate::file_system::volume::{ServerCopyProgress, Volume, VolumeError};

/// Asks the destination to copy the file on the server, from itself or from a
/// sibling place it can copy across (`Volume::copy_on_server`).
///
/// `Ok(None)` means "do it the ordinary way", and that is the answer for
/// everything except a clean success, a genuine cancel, and a refused name:
///
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
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    staging: WriteStaging,
) -> Result<Option<u64>, FinalizeFailure> {
    // ❌ Never the single-shot answer: only whole-publish holds for a copy.
    let staging = resolve_staging(staging, dest_volume.publishes_writes_whole());
    let staged = StagedWrite::begin(state, dest_path, staging);
    note_pending_for_local_dest(dest_volume, staged.target());
    set_task_phase(TaskPhase::Streaming);
    set_task_bytes(0, 0);
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

    match outcome {
        Ok(bytes) => {
            staged.commit(dest_volume).await?;
            Ok(Some(bytes))
        }
        // ❌ A cancel is never retried more slowly. The intent is consulted as
        // well as the variant, so a backend that labels its own stop something
        // else can't turn a Cancel click into a second, full-speed attempt.
        Err(e) if matches!(e, VolumeError::Cancelled(_)) || is_cancelled(&state.intent) => {
            staged.abandon(dest_volume).await;
            Err(e.into())
        }
        // Somebody holds the name: the streamed write would refuse it too.
        Err(e @ VolumeError::AlreadyExists(_)) => {
            staged.abandon(dest_volume).await;
            Err(e.into())
        }
        Err(VolumeError::NotSupported) => {
            staged.abandon_attempt(dest_volume).await;
            Ok(None)
        }
        Err(e) => {
            log::debug!(
                target: "copy",
                "try_server_side_copy: {} couldn't copy {} on the server ({e}); streaming it instead",
                dest_volume.name(),
                dest_path.display(),
            );
            staged.abandon_attempt(dest_volume).await;
            Ok(None)
        }
    }
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
