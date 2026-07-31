//! Staging for one cross-volume file write: bytes land on a `.cmdr-tmp-*`
//! sibling and take the file's final name only after the last one arrives.
//!
//! **The invariant.** A destination file must never carry its final name while
//! it is still being written. The 2026-07-31 wedge was force-quit mid-transfer
//! and left two phone backups at their final names — one at zero bytes, one
//! truncated at 4 MiB — indistinguishable from complete files
//! (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Neither had a
//! conflict, so neither took the conflict layer's safe-replace temp; a fresh copy
//! streamed straight to the destination path. Staging every write closes that:
//! whatever a crash leaves behind wears a `.cmdr-tmp-*` name nobody mistakes for
//! their data.
//!
//! **Who stages.** The conflict layer already mints a temp for a file→file
//! Overwrite (`volume_conflict::temp_sibling_path`) and lands it itself, so a
//! write onto one of those is [`WriteStaging::AlreadyStaged`] and passes through
//! untouched — staging it again would only produce a `foo.cmdr-tmp-A.cmdr-tmp-B`.
//! Every other write is [`WriteStaging::Stage`].
//!
//! **Cost.** One extra rename per file. On SMB that is one round trip, which
//! roughly doubles the wire cost of a small file taking the compound
//! CREATE+WRITE+CLOSE fast path. That is the price of the invariant, and it is
//! deliberate.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::state::WriteOperationState;
use super::transfer_probe::{TaskPhase, set_task_phase};
use super::volume_conflict::temp_sibling_path;
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// Who owns the `.cmdr-tmp-*` staging for one file write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteStaging {
    /// The path handed to the writer is the file's FINAL name: stage it here.
    Stage,
    /// The path handed to the writer is already a `.cmdr-tmp-*` the CALLER
    /// minted and will land itself (the conflict layer's safe-replace, which
    /// keeps the original in place until the temp is complete). Write straight
    /// to it.
    AlreadyStaged,
}

/// One file write's staging: where the bytes go, and how they get their final
/// name.
pub(super) struct StagedWrite {
    /// `Some(temp)` when we staged this write ourselves, `None` under
    /// [`WriteStaging::AlreadyStaged`].
    temp: Option<PathBuf>,
    /// Where the file must end up. Under `AlreadyStaged` this IS the caller's
    /// temp, and landing it is the caller's job.
    final_path: PathBuf,
    state: Arc<WriteOperationState>,
}

impl StagedWrite {
    /// Picks the staging path and, when we own it, records it as an in-flight
    /// partial on the operation so an abandoned task's litter can be found.
    pub(super) fn begin(state: &Arc<WriteOperationState>, final_path: &Path, staging: WriteStaging) -> Self {
        let temp = match staging {
            WriteStaging::Stage => {
                let temp = temp_sibling_path(final_path);
                state.in_flight_temps.lock_ignore_poison().push(temp.clone());
                Some(temp)
            }
            WriteStaging::AlreadyStaged => None,
        };
        Self {
            temp,
            final_path: final_path.to_path_buf(),
            state: Arc::clone(state),
        }
    }

    /// Where the streaming writer must put the bytes.
    pub(super) fn target(&self) -> &Path {
        self.temp.as_deref().unwrap_or(&self.final_path)
    }

    /// The write SUCCEEDED: give the bytes their final name.
    ///
    /// Deregisters the temp first. From this point the temp holds committed data,
    /// not a partial: if the landing then fails (a disconnect between the delete
    /// and the rename), the temp is the only complete copy of the new bytes and
    /// MUST survive on disk. Nothing may sweep it, which is exactly what dropping
    /// it from the in-flight set guarantees.
    ///
    /// `Err(VolumeError::NotSupported)` means this destination can't rename (or
    /// delete), so it can't stage at all; the caller may fall back to writing at
    /// the final name. No production backend takes that branch.
    pub(super) async fn commit(mut self, dest_volume: &Arc<dyn Volume>) -> Result<(), VolumeError> {
        let Some(temp) = self.temp.take() else {
            return Ok(()); // the caller staged it and lands it itself
        };
        self.deregister(&temp);
        // Landing is a device round trip of its own; a dump has to be able to
        // name it rather than showing a task still "streaming" at EOF.
        set_task_phase(TaskPhase::Finalizing);
        match land(dest_volume, &temp, &self.final_path).await {
            Err(VolumeError::NotSupported) => {
                // This backend can't land a staged write at all, so the caller
                // will rewrite the file at its final name. Drop the temp here:
                // it isn't the only copy of anything, and leaving it would litter
                // the destination on every single file.
                let _ = dest_volume.delete(&temp).await;
                Err(VolumeError::NotSupported)
            }
            other => other,
        }
    }

    /// The write FAILED: the staged bytes are a partial, so remove them.
    ///
    /// Best-effort — the backend usually deleted its own partial already, and a
    /// leftover `.cmdr-tmp-*` is untidy rather than dangerous.
    pub(super) async fn abandon(mut self, dest_volume: &Arc<dyn Volume>) {
        let Some(temp) = self.temp.take() else {
            return;
        };
        self.deregister(&temp);
        if let Err(e) = dest_volume.delete(&temp).await {
            log::debug!(
                target: "copy",
                "staged write: couldn't remove the partial {} after a failed write: {e}",
                temp.display()
            );
        }
    }

    fn deregister(&self, temp: &Path) {
        self.state.in_flight_temps.lock_ignore_poison().retain(|p| p != temp);
    }
}

/// Moves a completed temp onto `final_path`.
///
/// Renames FIRST, and only clears `final_path` if that fails. The conflict
/// layer's `volume_conflict::finalize_safe_replace` is the other way round
/// because there the original is known to be in the way; here it usually isn't (a fresh
/// copy, or a conflict the resolver already cleared), and a speculative delete
/// would spend one extra round trip per file on SMB and MTP for nothing. The
/// name can still be taken — a `Rename` resolution's `O_EXCL` placeholder, a
/// cross-type Overwrite whose dest delete failed, a racing writer — so the
/// second attempt covers it.
///
/// On failure `temp` is left alone: past this point it holds the file's only
/// complete copy.
async fn land(dest_volume: &Arc<dyn Volume>, temp: &Path, final_path: &Path) -> Result<(), VolumeError> {
    let Err(first) = dest_volume.rename(temp, final_path, false).await else {
        return Ok(());
    };
    match dest_volume.delete(final_path).await {
        Ok(()) | Err(VolumeError::NotFound(_)) => dest_volume.rename(temp, final_path, false).await,
        // Couldn't clear the way either; the rename error is the one to report.
        Err(_) => Err(first),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::volume::InMemoryVolume;
    use std::time::Duration;

    fn state() -> Arc<WriteOperationState> {
        Arc::new(WriteOperationState::new(Duration::from_millis(50)))
    }

    /// A staged write must never hand the writer the final name, and the temp it
    /// picks must be a recognizable sibling.
    #[test]
    fn staging_writes_to_a_recognizable_temp_sibling() {
        let state = state();
        let staged = StagedWrite::begin(&state, Path::new("/dir/notes.txt"), WriteStaging::Stage);
        assert_ne!(staged.target(), Path::new("/dir/notes.txt"));
        assert_eq!(staged.target().parent(), Some(Path::new("/dir")));
        assert!(
            staged.target().to_string_lossy().contains(".cmdr-tmp-"),
            "got {}",
            staged.target().display()
        );
        assert_eq!(
            state.in_flight_temps.lock_ignore_poison().len(),
            1,
            "the partial must be findable while it is being written"
        );
    }

    /// A caller-staged write is passed through: no second temp, and nothing
    /// registered (the caller owns that path's lifetime).
    #[test]
    fn a_caller_staged_write_is_not_staged_again() {
        let state = state();
        let caller_temp = Path::new("/dir/notes.txt.cmdr-tmp-abc");
        let staged = StagedWrite::begin(&state, caller_temp, WriteStaging::AlreadyStaged);
        assert_eq!(staged.target(), caller_temp);
        assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
    }

    /// Committing lands the bytes at the final name and drops the temp from the
    /// in-flight set, so nothing can sweep the committed data afterwards.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn commit_lands_the_bytes_and_stops_tracking_them() {
        let state = state();
        let inner = Arc::new(InMemoryVolume::new("dest"));
        let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

        let staged = StagedWrite::begin(&state, Path::new("/notes.txt"), WriteStaging::Stage);
        let temp = staged.target().to_path_buf();
        inner.create_file(&temp, b"NEW").await.unwrap();

        staged.commit(&dest).await.unwrap();

        assert!(inner.exists(Path::new("/notes.txt")).await);
        assert!(!inner.exists(&temp).await);
        assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
    }

    /// Abandoning removes the partial and stops tracking it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn abandon_removes_the_partial() {
        let state = state();
        let inner = Arc::new(InMemoryVolume::new("dest"));
        let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

        let staged = StagedWrite::begin(&state, Path::new("/notes.txt"), WriteStaging::Stage);
        let temp = staged.target().to_path_buf();
        inner.create_file(&temp, b"half").await.unwrap();

        staged.abandon(&dest).await;

        assert!(!inner.exists(&temp).await);
        assert!(state.in_flight_temps.lock_ignore_poison().is_empty());
    }
}
