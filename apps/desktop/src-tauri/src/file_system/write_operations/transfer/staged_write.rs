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
//! Overwrite (`volume::finalize::temp_sibling_path`) and lands it itself, so a
//! write onto one of those is [`WriteStaging::AlreadyStaged`] and passes through
//! untouched — staging it again would only produce a `foo.cmdr-tmp-A.cmdr-tmp-B`.
//! A write the DESTINATION lands in one indivisible shot is
//! [`WriteStaging::SingleShot`] and needs no temp: there is no moment at which
//! the final name holds a partial. It has no landing either, so the no-replace
//! rule travels with it instead: a name the caller expected free is written
//! with [`WriteMode::CreateNew`], which the destination refuses atomically if
//! someone took it mid-upload ([`StagedWrite::write_mode`]). Every other write stages here
//! ([`WriteStaging::Stage`], or [`WriteStaging::StageOntoClaimedName`] when the
//! caller picked the final name itself).
//!
//! **Whose name it is.** The landing rename can answer `AlreadyExists`, and
//! what that means is the CALLER's to say ([`LandingName`]): a name it claimed
//! (a `Rename` pick's `O_EXCL` placeholder, a cross-type Overwrite's cleared
//! destination) may be cleared, a name it believed FREE may not. Without the
//! distinction a clash nobody resolved — a case-insensitive destination
//! answering for `Report.docx` with the user's `report.docx` — read as "clear
//! the way" and replaced a file under a Skip.
//!
//! **Cost.** One extra rename per staged file. On SMB that is one round trip,
//! which roughly doubles the wire cost of a file the compound
//! CREATE+WRITE+FLUSH+CLOSE fast path would otherwise finish in one — which is
//! why single-shot writes are exempt. That exemption is bought by
//! single-shot-ness, ❌ NEVER by smallness: the destination is asked
//! (`Volume::write_is_single_shot`), the caller never guesses from a size.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::in_flight_temps::TempHome;
use super::super::state::WriteOperationState;
use super::recovered_name::{
    AfterARefusedDelete, FinalizeFailure, discard_unplaced_temp, rescue_out_of_temp_space, what_a_refused_delete_left,
};
use super::transfer_probe::{TaskPhase, set_task_phase};
use crate::file_system::staging::StagingTemp;
use crate::file_system::volume::{Volume, VolumeError, WriteMode};

/// Who owns the `.cmdr-tmp-*` staging for one file write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteStaging {
    /// The path handed to the writer is the file's FINAL name: stage it here.
    /// The caller believes that name is FREE, so an `AlreadyExists` when the
    /// bytes come to take it is a clash nobody answered — see [`LandingName`].
    Stage,
    /// [`WriteStaging::Stage`] for a name inside a folder THIS operation created
    /// (the merge walker's `create_directory` succeeded), which the creation
    /// proved empty. Staged and landed exactly like `Stage`; a whole-publishing
    /// destination writes it with [`WriteMode::CreateNewInFreshFolder`], which
    /// lets an object store skip its per-object no-overwrite HEAD.
    StageInFreshFolder,
    /// Stage as [`WriteStaging::Stage`], but the final name is one the CALLER
    /// claimed or cleared: a `Rename` resolution's `O_EXCL` placeholder, or a
    /// cross-type Overwrite that removed what was there. Whatever the landing
    /// finds in the way is the caller's own doing, so it may clear it.
    StageOntoClaimedName,
    /// The path handed to the writer is already a `.cmdr-tmp-*` the CALLER
    /// minted and will land itself (the conflict layer's safe-replace, which
    /// keeps the original in place until the temp is complete). Write straight
    /// to it.
    AlreadyStaged,
    /// The DESTINATION lands this write in one indivisible shot (`Volume::
    /// write_is_single_shot`), or publishes every write whole by protocol
    /// (`Volume::publishes_writes_whole`, an object store), so the final name
    /// can never hold a partial and staging would buy nothing but a rename.
    /// Write straight to the final name.
    ///
    /// Carries the [`LandingName`] of the staged write it replaced, because
    /// with no landing rename to refuse a taken name, the write itself has to:
    /// an `ExpectedFree` name goes out as [`WriteMode::CreateNew`].
    SingleShot(LandingName),
}

/// What the caller believes sits at the final name when the staged bytes come
/// to take it, and therefore what an `AlreadyExists` from the landing rename
/// means.
///
/// The distinction is the difference between a late-detected conflict and a
/// deleted file. Only the caller knows which it is: the landing sees the same
/// `AlreadyExists` either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LandingName {
    /// Nobody resolved a conflict for this write: the name was free when the
    /// caller last looked. Something in the way now is a clash no policy and no
    /// person answered, so the landing REFUSES rather than clearing it.
    ExpectedFree,
    /// [`ExpectedFree`](Self::ExpectedFree), inside a folder this operation
    /// created and found empty. Means exactly the same to the landing (a clash
    /// nobody answered); only the write mode differs
    /// ([`WriteMode::CreateNewInFreshFolder`]).
    FreeInFreshFolder,
    /// The caller claimed or cleared this name and is entitled to what's in the
    /// way: a `Rename` pick reserved with an `O_EXCL` placeholder, a cross-type
    /// Overwrite whose destination delete has already run.
    ClaimedByTheCaller,
}

impl LandingName {
    /// A name nothing resolved: [`FreeInFreshFolder`](Self::FreeInFreshFolder)
    /// in a folder this operation's own `create_directory` made (which proved
    /// it empty), else [`ExpectedFree`](Self::ExpectedFree). A merge into a
    /// folder that already existed keeps every no-overwrite check: that's
    /// where another writer's files plausibly land.
    pub(super) fn free(in_a_folder_this_operation_made: bool) -> Self {
        if in_a_folder_this_operation_made {
            Self::FreeInFreshFolder
        } else {
            Self::ExpectedFree
        }
    }
}

/// One file write's staging: where the bytes go, and how they get their final
/// name.
pub(in crate::file_system::write_operations) struct StagedWrite {
    /// `Some(temp)` when we staged this write ourselves, `None` under
    /// [`WriteStaging::AlreadyStaged`] and [`WriteStaging::SingleShot`].
    ///
    /// The guard also keeps the temp out of the pane while it's being written
    /// (`file_system::staging`). Dropping it un-hides the file, which is why
    /// `commit` and `abandon` hold it until the rename or delete is done — and
    /// why a landing that FAILS is right to let it go: a temp that survives one
    /// (a rescue or a discard that couldn't happen) is something the user may
    /// need to see.
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
    /// What the caller believes about that name, which is what [`land`] needs to
    /// tell a clash nobody answered from a name the caller itself claimed.
    landing: LandingName,
    /// See [`write_mode`](Self::write_mode).
    write_mode: WriteMode,
    state: Arc<WriteOperationState>,
}

impl StagedWrite {
    /// Begins a tracked sibling stage for validated generated output. A target
    /// observed absent stays `ExpectedFree`, so a racing file is never cleared.
    pub(in crate::file_system::write_operations) fn begin_generated(
        state: &Arc<WriteOperationState>,
        final_path: &Path,
        replace_existing: bool,
    ) -> Self {
        let staging = if replace_existing {
            WriteStaging::StageOntoClaimedName
        } else {
            WriteStaging::Stage
        };
        Self::begin(state, final_path, staging)
    }

    /// Picks the staging path and, when we own it, records it as an in-flight
    /// partial on the operation so an abandoned task's litter can be found.
    pub(super) fn begin(state: &Arc<WriteOperationState>, final_path: &Path, staging: WriteStaging) -> Self {
        let mut temp = None;
        let mut caller_temp = None;
        match staging {
            WriteStaging::Stage | WriteStaging::StageInFreshFolder | WriteStaging::StageOntoClaimedName => {
                let staged = StagingTemp::mint(final_path, state.liveness_token());
                super::super::in_flight_temps::register(state, staged.path(), dest_home(state));
                temp = Some(staged);
            }
            // The caller's temp is the caller's to land; all we take is
            // responsibility for keeping it out of the pane while we write it.
            WriteStaging::AlreadyStaged => {
                caller_temp = Some(StagingTemp::adopt(final_path.to_path_buf(), state.liveness_token()))
            }
            // A single-shot write goes straight to the final name: no
            // intermediate state to track, and nothing to hide.
            WriteStaging::SingleShot(_) => {}
        }
        Self {
            temp,
            caller_temp,
            final_path: final_path.to_path_buf(),
            // `AlreadyStaged` never lands here and answers `ClaimedByTheCaller`
            // for the same reason it needs no landing: the name is the caller's,
            // and the caller does the swap. A single-shot write keeps the answer
            // of the staged write it replaced, which is what `write_mode` reads.
            landing: match staging {
                WriteStaging::Stage => LandingName::ExpectedFree,
                WriteStaging::StageInFreshFolder => LandingName::FreeInFreshFolder,
                WriteStaging::StageOntoClaimedName | WriteStaging::AlreadyStaged => LandingName::ClaimedByTheCaller,
                WriteStaging::SingleShot(landing) => landing,
            },
            write_mode: match staging {
                WriteStaging::SingleShot(LandingName::ExpectedFree) => WriteMode::CreateNew,
                WriteStaging::SingleShot(LandingName::FreeInFreshFolder) => WriteMode::CreateNewInFreshFolder,
                WriteStaging::SingleShot(LandingName::ClaimedByTheCaller)
                | WriteStaging::Stage
                | WriteStaging::StageInFreshFolder
                | WriteStaging::StageOntoClaimedName
                | WriteStaging::AlreadyStaged => WriteMode::CreateOrReplace,
            },
            state: Arc::clone(state),
        }
    }

    /// Where the streaming writer must put the bytes.
    pub(in crate::file_system::write_operations) fn target(&self) -> &Path {
        self.temp.as_ref().map_or(&self.final_path, StagingTemp::path)
    }

    /// What the write to [`target`](Self::target) may do to a file already
    /// there, for `Volume::write_from_stream`.
    ///
    /// ❗ `CreateNew` exactly when the bytes go straight to a FINAL name the
    /// caller expected free: a single-shot write with nothing resolved for it.
    /// With no staged landing to refuse a name someone took mid-upload, the
    /// destination's own create has to, atomically (SMB's `FileCreate`). A
    /// replacing write there reported success with the other writer's file
    /// gone. Pinned on a live share by
    /// `backend_suites/smb_transfer_safety_test.rs::smb_integration_a_single_shot_upload_never_replaces_a_name_taken_mid_upload`.
    ///
    /// Everything else may replace: our own freshly minted `.cmdr-tmp-*` (a
    /// retried attempt writes over its predecessor), the caller's safe-replace
    /// temp, or a final name the caller claimed, whose `O_EXCL` placeholder
    /// sits at the name and must be written over.
    pub(super) fn write_mode(&self) -> WriteMode {
        self.write_mode
    }

    /// The write SUCCEEDED: give the bytes their final name.
    ///
    /// The temp stays in the in-flight set until [`land`] says it stopped being
    /// a removable copy of ours: it landed, it was taken away, or the landing is
    /// about to clear the name, from which point it may be the only complete
    /// copy at the destination and ❗ nothing may sweep it. Until then a landing
    /// that never finishes (a cancel, the concurrent driver dropping its window,
    /// a crash) leaves it findable by the abandoned-write sweep and the startup
    /// sweep, which is safe: the name was never cleared, and the source still
    /// holds the bytes.
    ///
    /// `Err(VolumeError::NotSupported)` means this destination can't rename (or
    /// delete), so it can't stage at all; the caller may fall back to writing at
    /// the final name, and `land` has already taken the temp away. No production
    /// backend takes that branch.
    pub(in crate::file_system::write_operations) async fn commit(
        mut self,
        dest_volume: &Arc<dyn Volume>,
    ) -> Result<(), FinalizeFailure> {
        let Some(temp) = self.temp.take() else {
            // Nothing of ours to land: the caller stages and lands its own temp,
            // and a single-shot write already sits at its final name.
            return Ok(());
        };
        // Landing is a device round trip of its own; a dump has to be able to
        // name it rather than showing a task still "streaming" at EOF.
        set_task_phase(TaskPhase::Finalizing);
        // Once, however many of `land`'s exits call it: each deregistration
        // appends to the persisted log.
        let released = std::sync::atomic::AtomicBool::new(false);
        let release = || {
            if !released.swap(true, std::sync::atomic::Ordering::Relaxed) {
                self.deregister(temp.path());
            }
        };
        land(dest_volume, temp.path(), &self.final_path, self.landing, &release).await
    }

    /// Atomically replaces the final name with a completed generated file.
    ///
    /// Fresh ZIP creation uses this only on backends whose rename-with-force is
    /// one atomic replacement primitive (local POSIX). The original is
    /// therefore never removed in a separate operation. A refusal leaves the
    /// original intact and abandons the staged bytes; an ambiguous reply may
    /// have landed the new file, but cleanup still names only the distinct temp.
    pub(in crate::file_system::write_operations) async fn commit_atomic_replace(
        mut self,
        dest_volume: &Arc<dyn Volume>,
    ) -> Result<(), VolumeError> {
        let Some(temp) = self.temp.take() else {
            return Err(VolumeError::NotSupported);
        };
        match dest_volume.rename(temp.path(), &self.final_path, true).await {
            Ok(()) => {
                self.deregister(temp.path());
                Ok(())
            }
            Err(error) => {
                if dest_volume.delete(temp.path()).await.is_ok() {
                    self.deregister(temp.path());
                }
                Err(error)
            }
        }
    }

    /// Publishes on a backend without atomic replacement while keeping the old
    /// destination recoverable under a tracked aside. This is the MTP-safe
    /// fallback: the old file is restored if the final rename refuses, and is
    /// discarded only after the new file has its final name.
    pub(in crate::file_system::write_operations) async fn commit_with_displaced_original(
        mut self,
        dest_volume: &Arc<dyn Volume>,
    ) -> Result<(), super::super::types::WriteOperationError> {
        let Some(temp) = self.temp.take() else {
            return Err(super::super::types::WriteOperationError::WriteError {
                path: self.final_path.display().to_string(),
                message: "generated output was not staged".to_string(),
            });
        };
        let displaced =
            match super::volume::displace_destination(&self.state, dest_volume, &self.final_path, false).await {
                Ok(displaced) => displaced,
                Err(error) => {
                    if dest_volume.delete(temp.path()).await.is_ok() {
                        self.deregister(temp.path());
                    }
                    return Err(error);
                }
            };
        match dest_volume.rename(temp.path(), &self.final_path, false).await {
            Ok(()) => {
                self.deregister(temp.path());
                if let Some(displaced) = displaced {
                    displaced.discard(dest_volume).await;
                }
                Ok(())
            }
            Err(error) => {
                let recovered = match displaced {
                    Some(displaced) => displaced
                        .restore(dest_volume)
                        .await
                        .map(|kept_at| vec![super::super::types::RecoveredOriginal::new(&self.final_path, &kept_at)]),
                    None => None,
                };
                if dest_volume.delete(temp.path()).await.is_ok() {
                    self.deregister(temp.path());
                }
                let cause = super::super::types::WriteOperationError::WriteError {
                    path: self.final_path.display().to_string(),
                    message: error.to_string(),
                };
                Err(match recovered {
                    Some(recovered) => super::super::types::WriteOperationError::OriginalsKeptAside {
                        cause: Box::new(cause),
                        recovered,
                    },
                    None => cause,
                })
            }
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
    pub(in crate::file_system::write_operations) async fn abandon(mut self, dest_volume: &Arc<dyn Volume>) {
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
        super::super::in_flight_temps::deregister(&self.state, temp, dest_home(&self.state));
    }
}

/// The path space this operation's staged partials live in: the DESTINATION
/// volume's, since a staged temp is a sibling of the file being written.
///
/// `None` when the operation didn't name that volume, which every volume
/// copy/move deferred does and only a bare test state doesn't. The persisted
/// ledger then skips the path rather than guessing at a path space (see
/// `in_flight_temps::register`); the operation's own in-memory ledger still
/// carries it, and that one deletes through the volume handle it already holds.
fn dest_home(state: &WriteOperationState) -> Option<TempHome<'_>> {
    state.dest_volume_id().map(TempHome::Volume)
}

/// Moves a completed temp onto `final_path`.
///
/// Renames FIRST, and only clears `final_path` if that rename said something is
/// in the way. The conflict layer's `volume::finalize::finalize_safe_replace` is
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
/// ❗ **And only a name the CALLER claimed**, which is the other half of the
/// same question. `AlreadyExists` says something is in the way; only
/// [`LandingName`] says whose it is. Under
/// [`LandingName::ExpectedFree`] nothing resolved a conflict for this write, so
/// what's in the way is the user's file and a policy nobody applied to it: the
/// landing reports the clash and leaves both sides alone. A case-insensitive
/// destination is where that happens without any race — `Report.docx` renaming
/// onto a stored `report.docx` — and clearing the way there replaced files under
/// a Skip.
///
/// ❗ **A landing that fails with nothing cleared takes its temp away at once.**
/// The name still holds what it held, the source still holds the new bytes, and
/// a complete `.cmdr-tmp-*` of ours in the user's folder would otherwise wait
/// an hour for the stale-temp reap (`recovered_name::discard_unplaced_temp`).
/// That covers a rename refused for any reason but `AlreadyExists`, a clash on
/// a name the caller believed free, and a claimed name whose delete was refused
/// with the name still taken. When the way was CLEARED and the rename then
/// failed anyway, the temp is the only complete copy at the destination, so it
/// is never deleted: it leaves temp space instead ([`FinalizeFailure::new_data_at`]).
///
/// `release` is called the moment the temp stops being a removable copy of ours:
/// it landed, it was taken away, or the way is about to be cleared. Until then
/// it stays in the operation's in-flight set, so a landing that never finishes
/// is still swept (see [`StagedWrite::commit`]).
async fn land(
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    final_path: &Path,
    landing: LandingName,
    release: &(dyn Fn() + Sync),
) -> Result<(), FinalizeFailure> {
    let discard = || async {
        if discard_unplaced_temp(dest_volume, temp).await {
            release();
        }
    };
    let Err(first) = dest_volume.rename(temp, final_path, false).await else {
        release();
        return Ok(());
    };
    if !matches!(first, VolumeError::AlreadyExists(_)) {
        discard().await;
        return Err(first.into());
    }
    if matches!(landing, LandingName::ExpectedFree | LandingName::FreeInFreshFolder) {
        log::warn!(
            target: "copy",
            "staged write: {} is taken by something nobody resolved a conflict for; \
             leaving it alone and taking our temp {} away.",
            final_path.display(),
            temp.display(),
        );
        discard().await;
        return Err(first.into());
    }
    // From here the way may be cleared, and then the temp is the only complete
    // copy at the destination: no sweep may touch it.
    release();
    let way_is_clear = match dest_volume.delete(final_path).await {
        Ok(()) | Err(VolumeError::NotFound(_)) => true,
        Err(_) => match what_a_refused_delete_left(dest_volume, final_path).await {
            AfterARefusedDelete::Gone => true,
            // Couldn't clear the way, so nothing was lost: the destination still
            // holds whatever was in the way, the temp is a spare copy, and the
            // rename error is the one to report.
            AfterARefusedDelete::StillThere => {
                discard_unplaced_temp(dest_volume, temp).await;
                false
            }
            AfterARefusedDelete::Unknown => false,
        },
    };
    if !way_is_clear {
        return Err(first.into());
    }
    match dest_volume.rename(temp, final_path, false).await {
        Ok(()) => Ok(()),
        // ❗ The way is cleared and the bytes couldn't take the name, so the
        // temp is now the only complete copy of a file with nothing at its
        // destination — and it wears a `.cmdr-tmp-*` name
        // `cleanup.rs::reap_stale_transfer_temps` matches an hour later. Get
        // it out of temp space and report where it went. Same act, same
        // reason, as `finalize::finalize_safe_replace`'s.
        Err(error) => Err(FinalizeFailure {
            new_data_at: Some(rescue_out_of_temp_space(dest_volume, temp, final_path).await),
            error,
        }),
    }
}

/// What one resolved write does to a FILE already holding the destination name,
/// as conflict resolution decided it.
///
/// A required, typed answer rather than an `Option<PathBuf>` because a third
/// shape exists and each one drives a different finalize, cleanup, and journal:
/// confusing them either deletes the user's original on a failed write or
/// records an overwrite as a fresh file, which an undo would then delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::file_system::write_operations) enum Replaces {
    /// Nothing is replaced: a free name, a `Rename` pick, a merge into a
    /// folder, or a cross-type destination already set aside.
    Nothing,
    /// A file→file Overwrite by SAFE-REPLACE: the write goes to a temp sibling,
    /// and once it's complete the caller swaps it over `orig`
    /// (`volume::finalize::finalize_safe_replace`). The original survives the
    /// whole write.
    ViaTemp(PathBuf),
    /// A file→file Overwrite on a destination that publishes every write whole
    /// (`Volume::publishes_writes_whole`, an object store): the write goes to
    /// the original's own name with `CreateOrReplace`, and the destination
    /// swaps the content only when the write completes. Nothing to finalize,
    /// and ❗ a failed write leaves the ORIGINAL there, so nothing at the name
    /// is ours to clean.
    InPlace,
}

impl Replaces {
    /// Whether this write replaces a file the user had, which makes the
    /// operation not rollbackable (the original is gone).
    pub(in crate::file_system::write_operations) fn overwrites(&self) -> bool {
        !matches!(self, Self::Nothing)
    }

    /// The original a safe-replace temp is swapped over after the write, and
    /// so the name the bytes end up at. `None` for every other shape: the write
    /// target already IS the final name.
    pub(in crate::file_system::write_operations) fn swap_target(&self) -> Option<&Path> {
        match self {
            Self::ViaTemp(orig) => Some(orig),
            Self::Nothing | Self::InPlace => None,
        }
    }

    /// The file this write replaces, whatever the mechanism: a safe-replace's
    /// `orig`, or `write_path` itself for an in-place one. A same-volume move
    /// asks this, because it renames the SOURCE onto that name either way.
    pub(in crate::file_system::write_operations) fn replaced_file(self, write_path: &Path) -> Option<PathBuf> {
        match self {
            Self::ViaTemp(orig) => Some(orig),
            Self::InPlace => Some(write_path.to_path_buf()),
            Self::Nothing => None,
        }
    }
}

/// The staging every call site derives the same way: a conflict resolution that
/// handed back a temp to swap over an original ([`Replaces::ViaTemp`]) already
/// staged the write, an in-place replace on a whole-publishing destination
/// ([`Replaces::InPlace`]) writes the final name over, and anything else is
/// ours to stage.
///
/// `landing` is the OTHER thing only the caller knows: whether a conflict
/// resolution put this write at this name. It decides what the landing rename's
/// `AlreadyExists` means (see `staged_write.rs::land`), so ❌ never pass
/// `ClaimedByTheCaller` for a write nothing resolved — that is the reading that
/// clears the user's file.
///
/// `InPlace` answers `SingleShot(ClaimedByTheCaller)` here rather than waiting
/// for [`resolve_staging`]: the resolver chose it on the same whole-publish
/// answer, and the write must never fall back to a staged landing, whose
/// delete-then-rename would put the original at risk the in-place write exists
/// to avoid.
pub(super) fn staging_for(replaces: &Replaces, landing: LandingName) -> WriteStaging {
    match (replaces, landing) {
        (Replaces::ViaTemp(_), _) => WriteStaging::AlreadyStaged,
        (Replaces::InPlace, _) => WriteStaging::SingleShot(LandingName::ClaimedByTheCaller),
        (Replaces::Nothing, LandingName::ExpectedFree) => WriteStaging::Stage,
        (Replaces::Nothing, LandingName::FreeInFreshFolder) => WriteStaging::StageInFreshFolder,
        (Replaces::Nothing, LandingName::ClaimedByTheCaller) => WriteStaging::StageOntoClaimedName,
    }
}

/// Whether a FILE write handed `dest_path` with `staging` can leave anything of
/// THIS operation at `dest_path` when it fails, which is what makes that path a
/// partial for the post-loop cleanup to remove.
///
/// - `AlreadyStaged`: `dest_path` IS the caller's safe-replace temp. Ours.
/// - `StageOntoClaimedName`: the caller claimed the name (a `Rename` pick's
///   `O_EXCL` placeholder, a cleared cross-type destination). Ours.
/// - `Stage`: ❗ **never ours.** The bytes went to a `.cmdr-tmp-*` sibling that
///   `stream_pipe_file` abandons itself, and the name was FREE when the driver
///   looked. Anything there now is someone else's, and the likeliest reason the
///   write failed at all is that someone else took the name mid-upload (the
///   landing refuses, `staged_write.rs::land`). Cleaning `dest_path` then
///   deleted their file. Pinned by `copy_landing_race_tests.rs` and, on live
///   servers, `a_name_taken_mid_upload_is_never_replaced`.
/// - `SingleShot`: ❗ never ours either. A driver meets it for an in-place
///   replace on a whole-publishing destination ([`Replaces::InPlace`]), where
///   a failed write leaves the user's ORIGINAL at the name; an upgraded staged
///   write is resolved inside `stream_pipe_file`. Either way its failed attempt
///   is the backend's to clean.
pub(super) fn failed_write_leaves_ours_at(staging: WriteStaging) -> bool {
    match staging {
        WriteStaging::AlreadyStaged | WriteStaging::StageOntoClaimedName => true,
        WriteStaging::Stage | WriteStaging::StageInFreshFolder | WriteStaging::SingleShot(_) => false,
    }
}

/// Drops the staging for a write the DESTINATION lands in one indivisible shot.
///
/// Staging keeps a byte-incomplete file from wearing the user's real filename.
/// A single-shot write has no in-between state to protect against — it either
/// lands whole or leaves nothing — so the `.cmdr-tmp-*` and the rename that
/// lands it would buy nothing and cost a round trip per file (on SMB that
/// roughly doubles the wire cost of a file the compound fast path finishes in
/// one frame; on a 10k-tiny-file copy to a NAS that is the whole difference).
///
/// ❌ The question is single-shot-ness, never smallness, and only the
/// destination can answer it: the caller asks `Volume::write_is_single_shot`
/// about the same `size` `write_from_stream` gets, off the same stream, and the
/// backend answers with the very condition its one-shot path branches on. A
/// caller-side size threshold would drift from that condition the day a backend
/// retunes it, and drifting apart means truncated files at real names again.
///
/// The answer arrives as a plain `bool` rather than being probed here, because
/// `stream_pipe_file`'s OTHER consumer needs the raw fact: the destination-side
/// foreground yield exempts a single-shot write from the min-progress floor, and
/// the returned enum can't tell it apart from a staged one. ❗ Ask ONCE per
/// write and share the answer; two probes could straddle a reconnect and
/// disagree.
///
/// `AlreadyStaged` is never touched: the caller's temp keeps the ORIGINAL file
/// in place until the new bytes are complete, which is a stronger guarantee than
/// single-shot-ness and the caller's to land.
///
/// The single-shot answer keeps whose name it is: a `Stage` (name expected
/// free) becomes `SingleShot(ExpectedFree)`, which writes with
/// [`WriteMode::CreateNew`]; a `StageOntoClaimedName` becomes
/// `SingleShot(ClaimedByTheCaller)`, which may replace its own placeholder.
///
/// `lands_whole` is `write_is_single_shot` OR `Volume::publishes_writes_whole`:
/// an object store publishes every write whole by protocol, so it gets the same
/// unstaged write at the final name (`volume/DETAILS.md` § "Whole-publish
/// destinations"). Only the staging decision takes the OR; the yield floor and
/// the stall watchdog read the single-shot answer alone.
pub(super) fn resolve_staging(requested: WriteStaging, lands_whole: bool) -> WriteStaging {
    match requested {
        WriteStaging::Stage if lands_whole => WriteStaging::SingleShot(LandingName::ExpectedFree),
        WriteStaging::StageInFreshFolder if lands_whole => WriteStaging::SingleShot(LandingName::FreeInFreshFolder),
        WriteStaging::StageOntoClaimedName if lands_whole => WriteStaging::SingleShot(LandingName::ClaimedByTheCaller),
        other => other,
    }
}

/// Resolve `dest_path` against `dest_volume.local_path()` and register it
/// with the downloads watcher's ignore set. Skips silently when
/// `dest_volume` isn't local-FS-backed (MTP, SMB, in-memory): those paths
/// would never trigger the watcher anyway, and synthesizing a non-local
/// path into the ignore set would just churn the map for no benefit.
pub(super) fn note_pending_for_local_dest(dest_volume: &Arc<dyn Volume>, dest_path: &Path) {
    let Some(root) = dest_volume.local_path() else {
        return;
    };
    // The same anchoring `LocalPosixVolume::resolve` applies, so the path we
    // register matches the one `write_from_stream` will hit.
    let absolute = cmdr_fs::volume::root_anchored(&root, dest_path);
    crate::downloads::note_pending_write_for_cmdr(&absolute);
}

#[cfg(test)]
#[path = "staged_write_tests.rs"]
mod tests;
