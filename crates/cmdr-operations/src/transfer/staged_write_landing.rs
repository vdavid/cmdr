//! How a staged write's completed temp takes its final name ([`land`]), what
//! the in-flight ledgers hear as it does ([`Landing`]), and how a landing the
//! transport cut off is finished once the destination is back
//! ([`CutOffLanding`]). Split out of `staged_write.rs`, which owns where a write
//! goes; this owns how it arrives.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::in_flight_temps::{self, TrackedRecord};
use super::super::super::journal;
use super::super::super::state::WriteOperationState;
use super::super::interrupted_landing::{NameHeld, holds_the_source};
use super::super::recovered_name::{
    AfterARefusedDelete, FinalizeFailure, LandingAftermath, discard_unplaced_temp, rescue_out_of_temp_space,
    what_a_refused_delete_left,
};
use super::super::volume::DisplacedDestination;
use super::{LandingName, home_of};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::staging::StagingTemp;
use cmdr_fs::volume::{Volume, VolumeError};
use cmdr_operation_log::types::NotRollbackableReason;

#[path = "staged_write_landing_aside.rs"]
mod aside;
use aside::{
    after_a_refused_aside, give_up_onto_aside, rename_onto_aside, resume_setting_aside,
    run_again_with_the_original_home, swap_through_an_aside,
};

/// Why a landing didn't give the bytes their name.
#[derive(Debug)]
pub(crate) enum LandingError {
    /// It ended: whatever it left is settled in the ledgers, and the failure
    /// says where the new bytes are when the way was cleared.
    Failed(FinalizeFailure),
    /// A step met a transport fault (`VolumeError::transport_fault`), so the
    /// server may or may not have performed it. Nothing is settled yet: wait
    /// for the destination, then [`CutOffLanding::resume`], or
    /// [`CutOffLanding::give_up`] when the wait ends without it.
    CutOff(Box<CutOffLanding>),
}

/// One staged write's landing: its temp, where it's going, and where the temp
/// stands in the in-flight ledgers, told each time the landing changes what
/// the temp is.
///
/// ❗ Dropped mid-way (a task the concurrent driver let go, a cancel's drain
/// deadline), a temp recorded as a landing stays recorded and is handed to
/// the volume's next arrival, which finishes the landing: from the clearing
/// delete on it may be the destination's only copy. A swap's original held
/// aside goes to the volume's next arrival the same way, whose sweep puts it
/// back or, once the replacement is whole at the name, removes it. A temp
/// still recorded as a temp stays in the operation's in-flight set for the
/// abandoned-write sweep, which is safe: nothing it held was the only copy.
pub(super) struct Landing {
    state: Arc<WriteOperationState>,
    /// The temp, kept out of the pane until the landing settles
    /// (`cmdr_fs::staging`).
    temp: StagingTemp,
    final_path: PathBuf,
    name: LandingName,
    /// The byte count the write confirmed: what a whole temp holds.
    size: u64,
    /// The volume whose path space the temp is recorded in (the staged
    /// write's).
    home_volume: Option<String>,
    recorded: Recorded,
    /// A swap's original, from the moment its aside is recorded until the swap
    /// settles it ([`swap_through_an_aside`]).
    aside: Option<DisplacedDestination>,
    /// Set by every discard of the temp this landing asks for: a temp gone
    /// from the destination is proof of a landing only when nothing of ours
    /// deleted it.
    discard_asked: bool,
    /// What stood at a claimed name as [`clear_the_way`] set out to delete it,
    /// from one stat right before the delete; `None` until then, or when that
    /// stat didn't answer. Only the same entry may be cleared again on a resume
    /// ([`resume_with_a_whole_temp`]).
    in_the_way: Option<NameHeld>,
}

/// Where a landing's temp stands in the in-flight ledgers.
enum Recorded {
    /// A removable copy of ours: recorded as a temp and in the operation's
    /// in-flight set, so every sweep may take it.
    Temp,
    /// On its way to a name that may be cleared: recorded as a landing (on
    /// disk before the clearing delete, `in_flight_temps::hold_for_landing`;
    /// `None` on a ledger that keeps nothing) and in no in-memory set.
    Landing(Option<TrackedRecord>),
    /// Settled: nothing left to tell the ledgers.
    Settled,
}

impl Landing {
    pub(super) fn new(
        state: &Arc<WriteOperationState>,
        temp: StagingTemp,
        final_path: &Path,
        name: LandingName,
        size: u64,
        home_volume: Option<String>,
    ) -> Self {
        Self {
            state: Arc::clone(state),
            temp,
            final_path: final_path.to_path_buf(),
            name,
            size,
            home_volume,
            recorded: Recorded::Temp,
            aside: None,
            discard_asked: false,
            in_the_way: None,
        }
    }

    fn temp(&self) -> &Path {
        self.temp.path()
    }

    fn home(&self) -> Option<in_flight_temps::RecordHome> {
        home_of(self.home_volume.as_deref())
    }

    /// The temp stopped being anything of ours: it landed, or it's gone.
    fn settled(&mut self) {
        match std::mem::replace(&mut self.recorded, Recorded::Settled) {
            Recorded::Temp => in_flight_temps::retire_temp(&self.state, self.home(), self.temp.path()),
            Recorded::Landing(Some(record)) => in_flight_temps::retire(&self.state, &record),
            Recorded::Landing(None) | Recorded::Settled => {}
        }
    }

    /// The way to the final name is about to be cleared: the temp becomes a
    /// landing, on disk before this resolves. `false` when the record can't be
    /// confirmed on disk, and the temp stays what it was.
    async fn hold(&mut self) -> bool {
        if !matches!(self.recorded, Recorded::Temp) {
            return true;
        }
        let held =
            in_flight_temps::hold_for_landing(&self.state, self.home(), self.temp.path(), &self.final_path, self.size)
                .await;
        match held {
            Ok(record) => {
                self.recorded = Recorded::Landing(record);
                true
            }
            Err(in_flight_temps::NotOnDisk) => false,
        }
    }

    /// The temp may be the destination's only copy and the landing couldn't
    /// finish: the record stays, for the sweep that next reaches the volume,
    /// and the operation's stop names the file it's on its way to.
    fn keep_landing(&mut self) {
        if let Recorded::Landing(Some(record)) = std::mem::replace(&mut self.recorded, Recorded::Settled) {
            in_flight_temps::keep_for_arrival(&self.state, record);
            self.state
                .kept_landings
                .lock_ignore_poison()
                .push(self.final_path.clone());
        }
    }

    /// Nothing was cleared after all and the temp couldn't be taken away, so
    /// it's a removable copy of ours again: recorded as a temp (before the
    /// landing record goes) for the sweeps to remove.
    fn landing_back_to_temp(&mut self) {
        if let Recorded::Landing(held) = std::mem::replace(&mut self.recorded, Recorded::Temp) {
            in_flight_temps::back_to_temp(&self.state, self.home(), self.temp.path(), held);
        }
    }

    /// Takes the temp away, a copy nobody needs because nothing was cleared for
    /// it. Answers whether it's gone; a temp that isn't stays as recorded.
    async fn discard(&mut self, dest_volume: &Arc<dyn Volume>) -> bool {
        self.discard_asked = true;
        let gone = discard_unplaced_temp(dest_volume.as_ref(), self.temp.path()).await;
        if gone {
            self.settled();
        }
        gone
    }

    /// The new bytes took the user's original's name (or may have): the
    /// operation stops being rollbackable now, whatever happens next, because
    /// the file an undo would restore is no longer at its name. The ONE moment
    /// an overwrite rules out rollback on a failure path: a swap that never got
    /// here put the original back, and a landed swap's journal row says so as
    /// well.
    fn original_replaced(&self) {
        journal::note_not_rollbackable(self.state.scope(), NotRollbackableReason::Overwrote);
    }
}

impl Drop for Landing {
    fn drop(&mut self) {
        self.keep_landing();
        if let Some(aside) = self.aside.take() {
            aside.keep_for_arrival();
        }
    }
}

/// Whether a transport fault cuts the landing off for the caller's wait, or
/// meets the refusal rules like any other error (the wait already gave up).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Patience {
    Waits,
    GaveUp,
}

/// Which step a transport fault cut a landing off at, which is what the
/// settle needs to know about what may have happened.
#[derive(Debug)]
enum CutOffAt {
    /// A rename-first landing's first rename: nothing was cleared, and the
    /// temp is still a removable copy of ours. The server may have renamed it.
    FirstRename,
    /// The delete that clears a claimed name: it may have gone through. The
    /// temp is recorded as a landing. Carries the first rename's
    /// `AlreadyExists`, the error to report if the way can't be cleared.
    ClearingDelete(VolumeError),
    /// The rename onto the cleared name: the delete went through, so the temp
    /// may be the destination's only copy. The server may have renamed it.
    RenameOntoCleared,
    /// A swap's one replacing rename ([`replace_in_one_step`]): nothing was
    /// cleared, since the original is replaced in that step or not at all, so
    /// the temp is still a removable copy of ours. The server may have
    /// performed it.
    ReplacingRename,
    /// A swap's rename of the original onto its recorded aside
    /// ([`swap_through_an_aside`]): the server may have moved it. The temp is
    /// still a removable copy of ours either way, since the original is whole
    /// wherever it is.
    SettingAside,
    /// A swap's rename of the temp onto the name its original vacated: the
    /// server may have renamed it. The original is whole at its aside.
    RenameOntoAside,
}

/// Moves a completed temp onto its final name.
///
/// Renames FIRST, and only clears the final name if that rename said something
/// is in the way: it usually isn't (a fresh copy, or a conflict the resolver
/// already cleared), and a speculative delete would spend one extra round trip
/// per file on SMB and MTP for nothing. The name can still be taken: a
/// `Rename` resolution's `O_EXCL` placeholder, which the second attempt clears
/// ([`clear_the_way`]), or another writer, whose file the landing refuses.
///
/// ❗ **A swap is the other way round** ([`LandingName::ReplacesOriginal`], a
/// file→file Overwrite): the original is known to be in the way. Where the
/// destination's forced rename replaces a file in ONE step
/// (`Volume::supports_atomic_replace_rename`: a local disk's `rename(2)`, SMB's
/// `ReplaceIfExists`), the swap is that rename ([`replace_in_one_step`]): the
/// name never goes empty, so the temp is never the only copy and nothing needs
/// recording as a landing. Everywhere else the original is renamed ASIDE first,
/// under a record, the temp renamed onto the empty name, and only then is the
/// aside deleted ([`swap_through_an_aside`]): the original stays whole until the
/// new bytes are whole at its name, and ❌ never `rename(force)` there, because
/// MTP's doesn't delete what's there and can make a duplicate, and SFTP's may
/// delete it in a step of its own. The moment the new bytes take the name (or
/// may have), the operation stops being rollbackable
/// ([`Landing::original_replaced`]): the file an undo would restore is no
/// longer at it.
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
/// [`LandingName`] says whose it is, and a claimed name is cleared only of the
/// empty placeholder the caller put there. Under
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
/// a name the caller believed free, a claimed name whose delete was refused
/// with the name still taken, and a swap whose original went back to its name.
/// When the way was CLEARED and the rename then failed anyway, the temp is the
/// only complete copy at the destination, so it is never deleted: it leaves temp
/// space instead ([`LandingAftermath::NewDataKeptAt`]).
///
/// ❗ **Classify before acting.** A step that fails on the transport
/// ([`VolumeError::transport_fault`]) may have been performed by the server,
/// so the landing is cut off right there ([`LandingError::CutOff`]): no
/// discard, no refused-delete stat, no rescue. Any of those would run on
/// whatever session answers next (a sibling's fresh one, when the error was the
/// old session's) and act on a guess: discard a temp that landed, read the name
/// as still taken and throw the only copy away, or park new bytes under
/// ` (recovered)` for a blip. The caller waits and settles
/// ([`CutOffLanding`]); the refusal rules run only for a refusal, or once the
/// wait gave up.
///
/// ❗ **Before it clears a claimed name, the temp is re-recorded as a landing**
/// (`in_flight_temps::hold_for_landing`): on disk as an `ItemKind::Landing` at
/// the write's size before the delete goes out, and out of the operation's
/// in-flight set, so no in-operation sweep may delete it and a crash leaves a
/// record the sweep finishes the landing from. ❗ **And before a swap moves the
/// original aside, the aside is recorded** (`in_flight_temps::hold_aside`, an
/// `ItemKind::FileAside` at the write's size): a crash leaves a record the sweep
/// puts the original back from, or, once the replacement is whole at the name,
/// removes the aside by. When either record can't be confirmed on disk, nothing
/// is moved: the landing fails as a refusal with nothing cleared, the original
/// in place. A landing that ends unsure, or is cut off and never resumed,
/// leaves its records for the sweep.
pub(super) async fn land(dest_volume: &Arc<dyn Volume>, mut landing: Landing) -> Result<(), LandingError> {
    if landing.name == LandingName::ReplacesOriginal {
        if dest_volume.supports_atomic_replace_rename() {
            return replace_in_one_step(dest_volume, landing).await;
        }
        return swap_through_an_aside(dest_volume, landing).await;
    }
    let Err(first) = dest_volume.rename(landing.temp(), &landing.final_path, false).await else {
        landing.settled();
        return Ok(());
    };
    if !matches!(first, VolumeError::AlreadyExists(_)) {
        if first.transport_fault().is_some() {
            return Err(cut_off(landing, CutOffAt::FirstRename, first));
        }
        landing.discard(dest_volume).await;
        return Err(LandingError::Failed(first.into()));
    }
    if matches!(landing.name, LandingName::ExpectedFree | LandingName::FreeInFreshFolder) {
        log::warn!(
            target: "copy",
            "staged write: {} is taken by something nobody resolved a conflict for; \
             leaving it alone and taking our temp {} away.",
            LogPrivate::file_path(&landing.final_path),
            LogPrivate::file_path(landing.temp()),
        );
        landing.discard(dest_volume).await;
        return Err(LandingError::Failed(first.into()));
    }
    clear_the_way(dest_volume, landing, first, Patience::Waits).await
}

fn cut_off(landing: Landing, at: CutOffAt, error: VolumeError) -> LandingError {
    log::info!(
        target: "copy",
        "landing: {} was cut off on the transport ({error}); settling it once the destination is back",
        LogPrivate::file_path(&landing.final_path), error = LogError(&error),
    );
    LandingError::CutOff(Box::new(CutOffLanding { landing, at, error }))
}

/// A swap on a destination whose forced rename replaces a file in one step:
/// the temp is renamed over the original. A refusal left the original whole
/// (the replace happened in full or not at all), so the temp is a spare copy
/// and goes; a transport fault cuts the landing off, since the server may have
/// performed it.
///
/// ❗ A refusal is read against the temp first: a reply the server lost after
/// it performed the replace reads as a refusal too, and then the temp is gone.
/// Gone, with nothing of ours having discarded it, means the replace happened,
/// and discarding (or reporting a failure) would disown the edit that's now at
/// the name.
async fn replace_in_one_step(dest_volume: &Arc<dyn Volume>, mut landing: Landing) -> Result<(), LandingError> {
    match dest_volume.rename(landing.temp(), &landing.final_path, true).await {
        Ok(()) => {
            landing.original_replaced();
            landing.settled();
            Ok(())
        }
        Err(error) if error.transport_fault().is_some() => {
            landing.original_replaced();
            Err(cut_off(landing, CutOffAt::ReplacingRename, error))
        }
        Err(refused) => {
            if temp_is_gone(dest_volume, &landing).await {
                log::info!(
                    target: "copy",
                    "landing: the replace onto {} answered {refused}, but its temp is gone, so it landed",
                    LogPrivate::file_path(&landing.final_path), refused = LogError(&refused),
                );
                landing.original_replaced();
                landing.settled();
                return Ok(());
            }
            landing.discard(dest_volume).await;
            Err(LandingError::Failed(refused.into()))
        }
    }
}

/// Whether the temp is provably gone from the destination, which, with
/// nothing of ours having discarded it, means a rename that answered an
/// error performed it anyway.
async fn temp_is_gone(dest_volume: &Arc<dyn Volume>, landing: &Landing) -> bool {
    !landing.discard_asked
        && matches!(
            dest_volume.get_metadata(landing.temp()).await,
            Err(VolumeError::NotFound(_))
        )
}

/// The landing's cleared-way tail for a claimed name: note what's in the way,
/// record the temp as a landing, clear the final name, and rename the temp onto
/// it. `first` is the first rename's `AlreadyExists`, the error to report if the
/// way can't be cleared.
///
/// ❗ **Only our own EMPTY placeholder is cleared**: anything else in the way,
/// or a name one stat can't describe, is someone else's, and the landing
/// refuses as for a clash nobody resolved. That stat's note ([`NameHeld`]) also
/// lets a resume clear again only the same entry (`#576`). A stat the transport
/// cuts off cuts the landing off as a first rename: nothing was cleared yet.
async fn clear_the_way(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    first: VolumeError,
    patience: Patience,
) -> Result<(), LandingError> {
    landing.in_the_way = match dest_volume.get_metadata(&landing.final_path).await {
        Ok(entry) if NameHeld::of(&entry).is_an_empty_file() => Some(NameHeld::of(&entry)),
        // Gone already: nothing to clear, and nothing a resume could match, so
        // a resume never clears the name again.
        Err(VolumeError::NotFound(_)) => None,
        Err(error) if patience == Patience::Waits && error.transport_fault().is_some() => {
            return Err(cut_off(landing, CutOffAt::FirstRename, error));
        }
        _ => {
            log::warn!(
                target: "copy",
                "staged write: {} holds something that isn't our empty placeholder; \
                 leaving it alone and taking our temp {} away.",
                LogPrivate::file_path(&landing.final_path),
                LogPrivate::file_path(landing.temp()),
            );
            landing.discard(dest_volume).await;
            return Err(LandingError::Failed(first.into()));
        }
    };
    if !landing.hold().await {
        log::warn!(
            target: "copy",
            "landing: couldn't confirm the landing record for {} on disk, so {} stays where it is",
            LogPrivate::file_path(landing.temp()),
            LogPrivate::file_path(&landing.final_path),
        );
        landing.discard(dest_volume).await;
        return Err(LandingError::Failed(landing_not_recorded()));
    }
    match dest_volume.delete(&landing.final_path).await {
        Ok(()) | Err(VolumeError::NotFound(_)) => {}
        Err(error) if patience == Patience::Waits && error.transport_fault().is_some() => {
            return Err(cut_off(landing, CutOffAt::ClearingDelete(first), error));
        }
        Err(refused) => return after_a_refused_delete(dest_volume, landing, first, refused, patience).await,
    }
    rename_onto_cleared(dest_volume, landing, patience).await
}

/// The refusal for a landing whose record couldn't be confirmed on disk:
/// nothing was cleared, and the original (or whatever held the name) is where
/// it was. Typed ([`LandingAftermath::NotRecorded`]), so the dialog words it;
/// the `IoError` is a diagnostic for the logs, ❌ never shown.
fn landing_not_recorded() -> FinalizeFailure {
    FinalizeFailure {
        error: VolumeError::IoError {
            message: "the landing record couldn't be confirmed on disk".to_owned(),
            raw_os_error: None,
        },
        aftermath: Some(LandingAftermath::NotRecorded),
    }
}

/// The delete of what's in the way answered `refused`: one stat says whether
/// the name is still taken (the server can act and the answer still be lost).
async fn after_a_refused_delete(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    first: VolumeError,
    refused: VolumeError,
    patience: Patience,
) -> Result<(), LandingError> {
    match what_a_refused_delete_left(dest_volume.as_ref(), &landing.final_path).await {
        AfterARefusedDelete::Gone => rename_onto_cleared(dest_volume, landing, patience).await,
        // Couldn't clear the way, so nothing was lost: the destination still
        // holds whatever was in the way, and the temp is a spare copy.
        AfterARefusedDelete::StillThere => {
            log::warn!(
                target: "copy",
                "landing: {} still holds what was in the way after a refused delete ({refused}); \
                 taking the temp {} away",
                LogPrivate::file_path(&landing.final_path),
                LogPrivate::file_path(landing.temp()), refused = LogError(&refused),
            );
            if !landing.discard(dest_volume).await {
                landing.landing_back_to_temp();
            }
            Err(LandingError::Failed(first.into()))
        }
        // Maybe cleared, maybe not: the temp may be the only copy, so the
        // landing record stays for the sweep to finish or rescue.
        AfterARefusedDelete::Unknown => {
            landing.keep_landing();
            Err(LandingError::Failed(first.into()))
        }
    }
}

/// The way is clear: rename the temp onto the final name.
async fn rename_onto_cleared(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    patience: Patience,
) -> Result<(), LandingError> {
    match dest_volume.rename(landing.temp(), &landing.final_path, false).await {
        Ok(()) => {
            landing.settled();
            Ok(())
        }
        Err(error) if patience == Patience::Waits && error.transport_fault().is_some() => {
            Err(cut_off(landing, CutOffAt::RenameOntoCleared, error))
        }
        Err(error) => Err(LandingError::Failed(rescue(dest_volume, landing, error).await)),
    }
}

/// ❗ The way is cleared and the bytes couldn't take the name, so the temp is
/// now the only complete copy of a file with nothing at its destination — and
/// it wears a `.cmdr-tmp-*` name `cleanup.rs::reap_stale_transfer_temps`
/// matches once nothing records it. Get it out of temp space and report where
/// it went. A rescue that couldn't rename either leaves the bytes on the temp,
/// recorded.
async fn rescue(dest_volume: &Arc<dyn Volume>, mut landing: Landing, error: VolumeError) -> FinalizeFailure {
    let new_data_at = rescue_out_of_temp_space(dest_volume.as_ref(), landing.temp(), &landing.final_path).await;
    if new_data_at == landing.temp() {
        // ❗ All that holds the new bytes now. A swap's temp is still recorded
        // as a temp (its original went aside, never cleared), and a temp is
        // what the abandoned-write sweep deletes on sight: it becomes a
        // landing first.
        landing.hold().await;
        landing.keep_landing();
    } else {
        landing.settled();
        // Whoever ends up reporting this (the item's own failure, or a stop
        // or a failure drain that swallows it) names where the bytes went.
        landing
            .state
            .rescued_landings
            .lock_ignore_poison()
            .push(new_data_at.clone());
    }
    FinalizeFailure {
        error,
        aftermath: Some(LandingAftermath::NewDataKeptAt(new_data_at)),
    }
}

/// A landing a transport fault cut off ([`LandingError::CutOff`]): the server
/// may or may not have performed the step it was on.
pub(crate) struct CutOffLanding {
    landing: Landing,
    at: CutOffAt,
    error: VolumeError,
}

impl std::fmt::Debug for CutOffLanding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CutOffLanding")
            .field("temp", &self.landing.temp())
            .field("final_path", &self.landing.final_path)
            .field("at", &self.at)
            .field("error", &self.error)
            .finish()
    }
}

/// The source a landed file's bytes came from, for the settle's byte compare.
#[derive(Clone, Copy)]
pub(crate) struct LandingSource<'a> {
    pub(crate) volume: &'a Arc<dyn Volume>,
    pub(crate) path: &'a Path,
}

/// What resuming a cut-off landing came to.
pub(crate) enum Resumed {
    /// The file is whole at its name.
    Landed,
    /// Nothing proves it landed, and nothing of it is left to land: run the
    /// file again from its first byte. Carries the error that cut it off, for
    /// a caller that can't run it again.
    RunAgain(VolumeError),
    /// It ended without landing; the failure is settled as any landing's.
    Failed(FinalizeFailure),
    /// Cut off again: wait once more.
    CutOff(Box<CutOffLanding>),
}

impl From<Result<(), LandingError>> for Resumed {
    fn from(landed: Result<(), LandingError>) -> Self {
        match landed {
            Ok(()) => Self::Landed,
            Err(LandingError::Failed(failure)) => Self::Failed(failure),
            Err(LandingError::CutOff(cut_off)) => Self::CutOff(cut_off),
        }
    }
}

impl CutOffLanding {
    /// What cut it off.
    pub(crate) fn error(&self) -> &VolumeError {
        &self.error
    }

    /// The destination is back: settles what the cut-off step did and lands
    /// what's still to land, without streaming the bytes again when the temp is
    /// still whole (`interrupted_landing.rs` has the proofs).
    ///
    /// - **The temp is there and whole**: land it from where the cut-off left
    ///   off. A first rename lands again from the top, and a one-step replace
    ///   runs again (the original is still whole). A clearing delete that
    ///   may have gone through asks the name: empty means it did, and the
    ///   rename alone finishes the landing; taken by the very entry the
    ///   landing noted before its delete ([`NameHeld`]) means it never reached
    ///   the server, and the way is cleared again; taken by anything else means
    ///   it did, and someone saved there since. After a delete that went
    ///   through, ❗ the rename alone, ❌ never another delete: a file
    ///   someone put at the name during the outage makes it refuse, and the
    ///   new bytes are rescued and reported, never their file deleted. A move
    ///   aside asks the aside: there means it moved, and the rename lands; gone
    ///   means it never did, and the move runs again.
    /// - **The temp is gone** and nothing of ours discarded it, so the server
    ///   performed our rename: landed when the name holds what we wrote. A
    ///   first rename's proof is the size (nothing cleared, so nothing else
    ///   could have moved our temp onto a name that was free); after the way
    ///   was cleared, a one-step replace, or a move aside (whose name held the
    ///   original, possibly at the same size), ❗ the name must hold exactly the
    ///   SOURCE's bytes, compared in full, and with no `source` to compare
    ///   against (a one-pass extract) it never counts as landed. A landed swap's
    ///   aside goes; an unproven one's original goes back to its name first.
    /// - **Anything else** (a temp that isn't whole, a name that doesn't prove
    ///   it): run the file again, a swap's original back at its name first.
    ///
    /// A step of this that meets the transport again cuts the landing off
    /// again; one that meets a refusal settles by the refusal rules.
    pub(crate) async fn resume(self, dest_volume: &Arc<dyn Volume>, source: Option<LandingSource<'_>>) -> Resumed {
        let Self { mut landing, at, error } = self;
        match dest_volume.get_metadata(landing.temp()).await {
            Ok(entry) if !entry.is_directory && !entry.is_symlink && entry.size == Some(landing.size) => {
                resume_with_a_whole_temp(dest_volume, landing, at).await
            }
            // Not the bytes we wrote, so nothing to land. A temp that never
            // left the in-flight set goes; one recorded as a landing stays
            // recorded (the sweep leaves a short one alone).
            Ok(_) => {
                if matches!(
                    at,
                    CutOffAt::FirstRename
                        | CutOffAt::ReplacingRename
                        | CutOffAt::SettingAside
                        | CutOffAt::RenameOntoAside
                ) {
                    landing.discard(dest_volume).await;
                } else {
                    landing.keep_landing();
                }
                run_again_with_the_original_home(dest_volume, landing, error).await
            }
            Err(VolumeError::NotFound(_)) => {
                let landed = !landing.discard_asked
                    && match (&at, source) {
                        (CutOffAt::FirstRename, _) => {
                            holds_our_size(dest_volume, &landing.final_path, landing.size).await
                        }
                        (_, Some(source)) => {
                            holds_the_source(
                                source.volume,
                                source.path,
                                landing.size,
                                dest_volume,
                                &landing.final_path,
                            )
                            .await
                        }
                        (_, None) => false,
                    };
                // The temp is gone either way: nothing left to protect.
                landing.settled();
                if landed {
                    log::info!(
                        target: "copy",
                        "landing: {} landed before the drop swallowed the reply; counting it done",
                        LogPrivate::file_path(&landing.final_path),
                    );
                    landing.discard_aside(dest_volume).await;
                    Resumed::Landed
                } else {
                    run_again_with_the_original_home(dest_volume, landing, error).await
                }
            }
            Err(stat) if stat.transport_fault().is_some() => Resumed::CutOff(Box::new(Self {
                landing,
                at,
                error: stat,
            })),
            Err(_) => Self { landing, at, error }.give_up(dest_volume).await.into(),
        }
    }

    /// The wait ended without the destination: it gave up, the person
    /// cancelled, or there was nothing to wait for. The step's error now meets
    /// the refusal rules, as any refusal would: nothing cleared leaves the temp
    /// to the abandoned-write cleanup; a clearing delete takes one stat (taken:
    /// the temp is a spare copy and goes; can't tell: it stays recorded as a
    /// landing for the next sweep that reaches the share); a rename after the
    /// delete asks the temp first (`give_up_onto_cleared`): gone with our bytes
    /// at the name is landed, still there rescues the bytes out of temp space,
    /// and a share that can't say leaves them recorded. A swap that moved the original
    /// aside puts it back, or leaves its aside record for the next sweep when
    /// the share can't say. `Ok` when the share answered after all and the
    /// landing finished.
    pub(crate) async fn give_up(self, dest_volume: &Arc<dyn Volume>) -> Result<(), FinalizeFailure> {
        let Self { landing, at, error } = self;
        let ended = match at {
            CutOffAt::FirstRename | CutOffAt::ReplacingRename => return Err(error.into()),
            CutOffAt::ClearingDelete(first) => {
                after_a_refused_delete(dest_volume, landing, first, error, Patience::GaveUp).await
            }
            CutOffAt::RenameOntoCleared => return give_up_onto_cleared(dest_volume, landing, error).await,
            CutOffAt::SettingAside => after_a_refused_aside(dest_volume, landing, error, Patience::GaveUp).await,
            CutOffAt::RenameOntoAside => return give_up_onto_aside(dest_volume, landing, error).await,
        };
        ended.map_err(|ended| match ended {
            LandingError::Failed(failure) => failure,
            // Unreachable: a landing that gave up cuts nothing off.
            LandingError::CutOff(cut_off) => cut_off.error.into(),
        })
    }
}

/// [`CutOffLanding::give_up`] of the rename onto a cleared name: the temp says
/// whether it happened, so a rename the server performed before its reply was
/// lost is never "rescued" from a temp that's no longer there.
///
/// - **The temp is there**: the rename never happened, and the bytes leave
///   temp space ([`rescue`]).
/// - **The temp is gone** and nothing of ours discarded it: the rename
///   happened, when the name holds exactly what we wrote. Anything else at the
///   name is no proof, and with the temp gone there's nothing left to protect.
/// - **The share can't say**: the landing stays recorded for the next sweep
///   that reaches it, which finishes the landing.
async fn give_up_onto_cleared(
    dest_volume: &Arc<dyn Volume>,
    mut landing: Landing,
    error: VolumeError,
) -> Result<(), FinalizeFailure> {
    match dest_volume.get_metadata(landing.temp()).await {
        Ok(_) => Err(rescue(dest_volume, landing, error).await),
        Err(VolumeError::NotFound(_)) if !landing.discard_asked => {
            let landed = holds_our_size(dest_volume, &landing.final_path, landing.size).await;
            landing.settled();
            if landed { Ok(()) } else { Err(error.into()) }
        }
        _ => Err(error.into()),
    }
}

impl From<Result<(), FinalizeFailure>> for Resumed {
    fn from(ended: Result<(), FinalizeFailure>) -> Self {
        match ended {
            Ok(()) => Self::Landed,
            Err(failure) => Self::Failed(failure),
        }
    }
}

/// [`CutOffLanding::resume`] with the temp still whole.
async fn resume_with_a_whole_temp(dest_volume: &Arc<dyn Volume>, landing: Landing, at: CutOffAt) -> Resumed {
    match at {
        CutOffAt::FirstRename => land(dest_volume, landing).await.into(),
        CutOffAt::ReplacingRename => replace_in_one_step(dest_volume, landing).await.into(),
        CutOffAt::RenameOntoCleared => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
        CutOffAt::RenameOntoAside => rename_onto_aside(dest_volume, landing, Patience::Waits).await.into(),
        CutOffAt::SettingAside => resume_setting_aside(dest_volume, landing).await,
        CutOffAt::ClearingDelete(first) => match dest_volume.get_metadata(&landing.final_path).await {
            // The delete went through before the drop.
            Err(VolumeError::NotFound(_)) => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
            // It never reached the server: what was in the way still is.
            Ok(entry) if landing.in_the_way == Some(NameHeld::of(&entry)) => {
                clear_the_way(dest_volume, landing, first, Patience::Waits).await.into()
            }
            // ❗ Something else is at the name: the delete went through, and
            // someone saved there during the outage. The rename alone, which
            // refuses, so the new bytes are rescued beside their file, ❌ never
            // their file cleared.
            Ok(_) => rename_onto_cleared(dest_volume, landing, Patience::Waits).await.into(),
            Err(stat) if stat.transport_fault().is_some() => Resumed::CutOff(Box::new(CutOffLanding {
                landing,
                at: CutOffAt::ClearingDelete(first),
                error: stat,
            })),
            Err(stat) => CutOffLanding {
                landing,
                at: CutOffAt::ClearingDelete(first),
                error: stat,
            }
            .give_up(dest_volume)
            .await
            .into(),
        },
    }
}

/// Whether `final_path` holds a file of exactly `size` bytes.
pub(super) async fn holds_our_size(dest_volume: &Arc<dyn Volume>, final_path: &Path, size: u64) -> bool {
    matches!(
        dest_volume.get_metadata(final_path).await,
        Ok(entry) if !entry.is_directory && !entry.is_symlink && entry.size == Some(size)
    )
}
