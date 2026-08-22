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
//! Overwrite (`volume::conflict::temp_sibling_path`) and lands it itself, so a
//! write onto one of those is [`WriteStaging::AlreadyStaged`] and passes through
//! untouched — staging it again would only produce a `foo.cmdr-tmp-A.cmdr-tmp-B`.
//! A write the DESTINATION lands in one indivisible shot is
//! [`WriteStaging::SingleShot`] and needs no temp: there is no moment at which
//! the final name holds a partial. Every other write is [`WriteStaging::Stage`].
//!
//! **Cost.** One extra rename per staged file. On SMB that is one round trip,
//! which roughly doubles the wire cost of a file the compound
//! CREATE+WRITE+FLUSH+CLOSE fast path would otherwise finish in one — which is
//! why single-shot writes are exempt. That exemption is bought by
//! single-shot-ness, ❌ NEVER by smallness: the destination is asked
//! (`Volume::write_is_single_shot`), the caller never guesses from a size.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::state::WriteOperationState;
use super::transfer_probe::{TaskPhase, set_task_phase};
use crate::file_system::staging::StagingTemp;
use crate::file_system::volume::{Volume, VolumeError};

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
    /// The DESTINATION lands this write in one indivisible shot (`Volume::
    /// write_is_single_shot`), so the final name can never hold a partial and
    /// staging would buy nothing but a rename round trip. Write straight to the
    /// final name.
    SingleShot,
}

/// One file write's staging: where the bytes go, and how they get their final
/// name.
pub(super) struct StagedWrite {
    /// `Some(temp)` when we staged this write ourselves, `None` under
    /// [`WriteStaging::AlreadyStaged`] and [`WriteStaging::SingleShot`].
    ///
    /// The guard also keeps the temp out of the pane while it's being written
    /// (`file_system::staging`). Dropping it un-hides the file, which is why
    /// `commit` and `abandon` hold it until the rename or delete is done — and
    /// why a landing that FAILS is right to let it go: the temp that survives is
    /// the file's only complete copy, and the user needs to see it.
    temp: Option<StagingTemp>,
    /// Keeps a CALLER-minted temp out of the pane for the length of the write
    /// ([`WriteStaging::AlreadyStaged`]).
    ///
    /// The conflict layer's safe-replace temp is minted several layers up and
    /// passed down as a plain path (`ResolvedConflict::write_path`), through code
    /// that clones it, so it can't carry its own guard. This adopts it for the
    /// window that matters — the streaming write — and lets go at commit, a
    /// rename short of the caller's `finalize_safe_replace`. Worst case that
    /// leaves the temp visible for one round trip, which shows as a flicker and
    /// never as a stuck entry: the pane re-reads through the same filter, so an
    /// entry it shows once can always be taken away again.
    #[allow(dead_code, reason = "Held for its Drop: the guard IS the hiding")]
    caller_temp: Option<StagingTemp>,
    /// Where the file must end up. Under `AlreadyStaged` this IS the caller's
    /// temp, and landing it is the caller's job.
    final_path: PathBuf,
    state: Arc<WriteOperationState>,
}

impl StagedWrite {
    /// Picks the staging path and, when we own it, records it as an in-flight
    /// partial on the operation so an abandoned task's litter can be found.
    pub(super) fn begin(state: &Arc<WriteOperationState>, final_path: &Path, staging: WriteStaging) -> Self {
        let mut temp = None;
        let mut caller_temp = None;
        match staging {
            WriteStaging::Stage => {
                let staged = StagingTemp::mint(final_path, state.liveness_token());
                super::super::in_flight_temps::register(state, staged.path());
                temp = Some(staged);
            }
            // The caller's temp is the caller's to land; all we take is
            // responsibility for keeping it out of the pane while we write it.
            WriteStaging::AlreadyStaged => {
                caller_temp = Some(StagingTemp::adopt(final_path.to_path_buf(), state.liveness_token()))
            }
            // A single-shot write goes straight to the final name: no
            // intermediate state to track, and nothing to hide.
            WriteStaging::SingleShot => {}
        }
        Self {
            temp,
            caller_temp,
            final_path: final_path.to_path_buf(),
            state: Arc::clone(state),
        }
    }

    /// Where the streaming writer must put the bytes.
    pub(super) fn target(&self) -> &Path {
        self.temp.as_ref().map_or(&self.final_path, StagingTemp::path)
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
            // Nothing of ours to land: the caller stages and lands its own temp,
            // and a single-shot write already sits at its final name.
            return Ok(());
        };
        self.deregister(temp.path());
        // Landing is a device round trip of its own; a dump has to be able to
        // name it rather than showing a task still "streaming" at EOF.
        set_task_phase(TaskPhase::Finalizing);
        match land(dest_volume, temp.path(), &self.final_path).await {
            Err(VolumeError::NotSupported) => {
                // This backend can't land a staged write at all, so the caller
                // will rewrite the file at its final name. Drop the temp here:
                // it isn't the only copy of anything, and leaving it would litter
                // the destination on every single file.
                let _ = dest_volume.delete(temp.path()).await;
                Err(VolumeError::NotSupported)
            }
            other => other,
        }
    }

    /// The write FAILED: the staged bytes are a partial, so remove them.
    ///
    /// Best-effort — the backend usually deleted its own partial already, and a
    /// leftover `.cmdr-tmp-*` is untidy rather than dangerous.
    ///
    /// A `SingleShot` write has nothing to remove: the destination promised the
    /// bytes either all landed or none did, and cleaning up after its own failed
    /// attempt is the backend's job (it is the only layer that can tell "the
    /// server created the file and then refused the bytes" from "the file was
    /// already there and we never touched it").
    pub(super) async fn abandon(mut self, dest_volume: &Arc<dyn Volume>) {
        let Some(temp) = self.temp.take() else {
            return;
        };
        self.deregister(temp.path());
        if let Err(e) = dest_volume.delete(temp.path()).await {
            log::debug!(
                target: "copy",
                "staged write: couldn't remove the partial {} after a failed write: {e}",
                temp.path().display()
            );
        }
    }

    /// This ATTEMPT failed and another one is about to run the same file
    /// (`retry.rs`): clear whatever the failed attempt left at the write target,
    /// so the next attempt writes onto a clean path.
    ///
    /// Wider than [`abandon`](Self::abandon) by exactly one case, and the reason
    /// is that the next writer is US, not the caller. Under
    /// [`WriteStaging::AlreadyStaged`] the target is the CALLER's safe-replace
    /// temp, which `abandon` deliberately leaves alone because the caller owns its
    /// lifetime — but between two attempts nobody else can be looking at it, it
    /// holds nothing but the partial we just gave up on, and the ORIGINAL it will
    /// eventually replace is untouched either way. Leaving it would make the next
    /// attempt's behavior depend on how each backend treats a write onto an
    /// existing path: `LocalPosixVolume` truncates, `InMemoryVolume` refuses with
    /// `AlreadyExists`, and MTP can happily make a second object with the same
    /// name.
    ///
    /// A `SingleShot` write is still left entirely to its backend: only the
    /// backend can tell "the server created the file and then refused the bytes"
    /// from "the file was already there and we never touched it", and that target
    /// is the user's real filename.
    pub(super) async fn abandon_attempt(self, dest_volume: &Arc<dyn Volume>) {
        if self.temp.is_some() {
            self.abandon(dest_volume).await;
            return;
        }
        if self.caller_temp.is_some() {
            let target = self.final_path.clone();
            if let Err(e) = dest_volume.delete(&target).await {
                log::debug!(
                    target: "copy",
                    "staged write: couldn't clear {} before the next attempt: {e}",
                    target.display()
                );
            }
        }
    }

    fn deregister(&self, temp: &Path) {
        super::super::in_flight_temps::deregister(&self.state, temp);
    }
}

/// Moves a completed temp onto `final_path`.
///
/// Renames FIRST, and only clears `final_path` if that rename said something is
/// in the way. The conflict layer's `volume::conflict::finalize_safe_replace` is
/// the other way round because there the original is known to be in the way;
/// here it usually isn't (a fresh copy, or a conflict the resolver already
/// cleared), and a speculative delete would spend one extra round trip per file
/// on SMB and MTP for nothing. The name can still be taken — a `Rename`
/// resolution's `O_EXCL` placeholder, a cross-type Overwrite whose dest delete
/// failed, a racing writer — so the second attempt covers it.
///
/// ❗ **Only `AlreadyExists` earns the delete**, and this is the difference
/// between a transient blip and a destroyed file. A rename over a network
/// backend fails for plenty of reasons that say nothing about the destination:
/// the session blinked, the server refused, SFTP v3 collapsed an errno into its
/// one catch-all code. Clearing the way on every `Err` would delete a file the
/// user still has and then report the blip that "justified" it. It also covers a
/// backend that can delete but not rename, which would otherwise destroy the
/// destination and answer `NotSupported`.
///
/// On failure `temp` is left alone: past this point it holds the file's only
/// complete copy.
async fn land(dest_volume: &Arc<dyn Volume>, temp: &Path, final_path: &Path) -> Result<(), VolumeError> {
    let Err(first) = dest_volume.rename(temp, final_path, false).await else {
        return Ok(());
    };
    if !matches!(first, VolumeError::AlreadyExists(_)) {
        return Err(first);
    }
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
    use crate::ignore_poison::IgnorePoison;
    use std::time::Duration;

    /// What `path` reports right now, which is how these cells tell "the
    /// destination is untouched" from "we replaced it".
    async fn size_of(volume: &InMemoryVolume, path: &str) -> Option<u64> {
        volume.get_metadata(Path::new(path)).await.ok().and_then(|e| e.size)
    }

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

    /// A single-shot write goes to the final name with no temp and nothing
    /// tracked: the destination lands it whole or not at all, so there is no
    /// partial to find, sweep, or land.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_single_shot_write_targets_the_final_name_and_needs_no_landing() {
        let state = state();
        let inner = Arc::new(InMemoryVolume::new("dest"));
        let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;

        let staged = StagedWrite::begin(&state, Path::new("/notes.txt"), WriteStaging::SingleShot);
        assert_eq!(staged.target(), Path::new("/notes.txt"));
        assert!(state.in_flight_temps.lock_ignore_poison().is_empty());

        // The backend wrote the whole file in one shot; committing is a no-op
        // that must not touch the destination.
        inner.create_file(Path::new("/notes.txt"), b"NEW").await.unwrap();
        staged.commit(&dest).await.unwrap();
        assert!(inner.exists(Path::new("/notes.txt")).await);
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

    /// Landing onto a name something else already holds is the case the second
    /// attempt exists for: clear the way, then rename again.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn landing_clears_a_destination_that_is_genuinely_in_the_way() {
        let inner = Arc::new(InMemoryVolume::new("dest"));
        let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
        inner.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
        inner.create_file(Path::new("/temp"), b"NEW").await.unwrap();

        land(&dest, Path::new("/temp"), Path::new("/notes.txt")).await.unwrap();

        assert!(!inner.exists(Path::new("/temp")).await);
        assert_eq!(size_of(&inner, "/notes.txt").await, Some(3), "the new bytes landed");
    }

    /// A rename that failed for ANY OTHER reason must leave the destination
    /// alone.
    ///
    /// The transient case is the one that costs a file. Over SFTP or SMB a
    /// rename can fail because the session blinked, and SFTP v3 collapses most
    /// of errno into one catch-all code, so a landing that cleared the way on
    /// every `Err` would delete the user's existing file and then report the
    /// blip. `AlreadyExists` is the only answer that says something is in the
    /// way; everything else says the destination is none of our business.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_rename_that_failed_for_another_reason_leaves_the_destination_alone() {
        let (inner, outcome) = land_with_a_rename_that_fails(VolumeError::DeviceDisconnected("blip".to_string())).await;

        assert!(
            matches!(outcome, Err(VolumeError::DeviceDisconnected(_))),
            "the rename's own failure is what the caller has to see; got {outcome:?}"
        );
        assert_eq!(
            size_of(&inner, "/notes.txt").await,
            Some(15),
            "a rename that never said the destination was in the way must not have cost the user their file"
        );
        assert!(
            inner.exists(Path::new("/temp")).await,
            "and the only complete copy of the new bytes must still be under the temp name"
        );
    }

    /// The same shape, one flavor further: a backend that can delete but can't
    /// rename must not destroy the destination and then report `NotSupported`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_backend_without_rename_does_not_delete_what_it_cannot_replace() {
        let (inner, outcome) = land_with_a_rename_that_fails(VolumeError::NotSupported).await;

        assert!(matches!(outcome, Err(VolumeError::NotSupported)), "got {outcome:?}");
        assert!(inner.exists(Path::new("/notes.txt")).await);
    }

    /// A landing onto a destination the user already has, over a backend whose
    /// rename fails with `failure`. Answers the volume so a cell can ask what
    /// survived.
    async fn land_with_a_rename_that_fails(
        failure: VolumeError,
    ) -> (Arc<InMemoryVolume>, Result<(), VolumeError>) {
        let inner = Arc::new(InMemoryVolume::new("dest").with_rename_failing(failure));
        let dest: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
        inner
            .create_file(Path::new("/notes.txt"), b"THE USER'S FILE")
            .await
            .unwrap();
        inner.create_file(Path::new("/temp"), b"NEW").await.unwrap();

        let outcome = land(&dest, Path::new("/temp"), Path::new("/notes.txt")).await;
        (inner, outcome)
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
