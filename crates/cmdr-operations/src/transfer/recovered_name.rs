//! Getting committed data out of `.cmdr-tmp-*` space, and saying where it went.
//!
//! A staged write's temp stops being a partial the moment the bytes are all
//! there: from then on it holds the only complete copy of the new file, and the
//! landing rename is what gives it the user's name. When that rename fails after
//! the destination has already been cleared, the bytes are committed data
//! wearing a sweepable name, and both halves of this module answer that:
//! [`rescue_out_of_temp_space`] gives them a real filename, and
//! [`FinalizeFailure`] carries that name out to the user, or [`KeptNewData`]
//! does when a stop or a sibling's failure swallows the item's own error.
//!
//! The other half is the landing that fails with nothing cleared: the user's
//! file (or someone else's) still holds the name, the source still holds the new
//! bytes, and the temp is a complete-but-unplaced copy of ours that nobody
//! needs. [`discard_unplaced_temp`] takes it away at once, so the user's folder
//! doesn't carry a `.cmdr-tmp-*` until the hourly reap, and
//! [`what_a_refused_delete_left`] is how a landing whose delete answered an
//! error finds out which of the two situations it is in.
//!
//! It sits at `transfer/` level rather than inside `volume/` because
//! `staged_write::land` reaches it (every staged write, a file→file
//! Overwrite's swap included), as do `interrupted_landing.rs`, the
//! cross-type aside's put-back (`volume/displaced_destination.rs`), and a
//! remote zip edit's swap (`archive_edit::remote::swap_into_place`).

use std::path::{Path, PathBuf};

use super::super::state::WriteOperationState;
use super::super::types::{StagedLeftoverFate, StagedLeftovers, WriteOperationError};
use super::super::unique_name::{NameCandidates, RESCUE_NAME_ATTEMPTS, recovered_sibling};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::{Volume, VolumeError};

/// What a failed landing leaves behind, and where.
///
/// ❗ The path is TYPED data, not prose: it travels to the user through
/// `WriteOperationError::NewDataKeptAt` so the dialog can name the file they
/// have to go and look at. ❌ Never parse it back out of a message.
#[derive(Debug)]
pub(crate) struct FinalizeFailure {
    /// What the destination said.
    pub error: VolumeError,
    /// What the user has to know past `error`, when the landing left something
    /// they'd otherwise go looking for. `None` for an ordinary failure: nothing
    /// was cleared, so nothing was rescued and nothing was lost.
    pub aftermath: Option<LandingAftermath>,
}

/// What a failed landing did that changes what the user is told, beyond what
/// the destination said. One or the other, ❌ never both: a refusal clears
/// nothing, so it rescues nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LandingAftermath {
    /// The name the new bytes were meant to take is already cleared, and the
    /// only complete copy of them is here now: the ` (recovered)` name, or the
    /// `.cmdr-tmp-*` temp when even that rename couldn't happen
    /// (`WriteOperationError::NewDataKeptAt`).
    NewDataKeptAt(PathBuf),
    /// Cmdr refused to clear the way: the landing record couldn't be confirmed
    /// on disk, so whatever held the name is still there and the temp is gone
    /// (`WriteOperationError::LandingNotRecorded`). `error` is a diagnostic only.
    NotRecorded,
}

impl From<VolumeError> for FinalizeFailure {
    /// An ordinary failure, with nothing stranded anywhere.
    fn from(error: VolumeError) -> Self {
        Self { error, aftermath: None }
    }
}

/// Gets the complete new bytes out of `.cmdr-tmp-*` space and into a real
/// filename next to where they were meant to land.
///
/// ❗ **This is what stops the hourly reap from eating them.**
/// `cleanup.rs::reap_stale_transfer_temps` matches on the `.cmdr-tmp-` marker
/// plus an age, and it runs at the start of every copy and every volume move
/// into a directory, and it spares only what the in-flight ledger records. A
/// `staged_write::land` temp is recorded as a landing until it lands or is
/// rescued, but a record is only as durable as the ledger's log, and a
/// landing's settles once it's rescued. A file called `notes (recovered).txt`
/// is one no sweep can match, recorded or not.
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
pub(crate) async fn rescue_out_of_temp_space(dest_volume: &dyn Volume, temp: &Path, orig: &Path) -> PathBuf {
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

/// What a landing's delete of the name in the way left there, after the delete
/// answered with an error.
///
/// The answer to a delete isn't the delete: over a network backend the server
/// can act and the response still get lost, so the error alone can't say
/// whether the name is still taken. One stat can, and it runs only on this
/// failure path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AfterARefusedDelete {
    /// The name is still taken: the delete really didn't happen, nothing was
    /// cleared, and the temp is a spare copy of bytes the source still holds.
    StillThere,
    /// The name is empty after all (the delete went through despite its answer,
    /// or someone else removed the file). The way is clear, so the landing goes
    /// ahead: leaving the temp unplaced would leave NEITHER file at the name.
    Gone,
    /// The destination couldn't say. ❗ The temp stays: if the name IS empty, it
    /// is the only copy at the destination, and this is no moment to guess.
    Unknown,
}

pub(crate) async fn what_a_refused_delete_left(dest_volume: &dyn Volume, path: &Path) -> AfterARefusedDelete {
    match dest_volume.get_metadata(path).await {
        Ok(_) => AfterARefusedDelete::StillThere,
        Err(VolumeError::NotFound(_)) => AfterARefusedDelete::Gone,
        Err(e) => {
            log::warn!(
                target: "copy",
                "landing: couldn't tell whether {} survived a refused delete ({e}), so the temp stays",
                LogPrivate::path(path), e = LogError(&e)
            );
            AfterARefusedDelete::Unknown
        }
    }
}

/// Takes away a complete temp whose bytes will NOT take their name, because
/// nothing was cleared for them: the name still holds what it held, and the
/// source still holds the new bytes.
///
/// ❗ Only ever called with a temp THIS operation minted (a `.cmdr-tmp-<uuid>`
/// name nobody else picks), and ❌ never once a landing has cleared the name:
/// from then on the temp may be the only copy at the destination, and it goes
/// through [`rescue_out_of_temp_space`] instead.
///
/// Best effort, one delete, and answers whether the temp is gone. A delete that
/// fails too (the same dead link that failed the landing) is logged and left to
/// the sweeps: the operation's abandoned-write sweep if the temp is still
/// registered, and `cleanup.rs::reap_stale_transfer_temps` on the next transfer
/// into that folder once it is an hour old.
pub(crate) async fn discard_unplaced_temp(dest_volume: &dyn Volume, temp: &Path) -> bool {
    match dest_volume.delete(temp).await {
        Ok(()) | Err(VolumeError::NotFound(_)) => true,
        Err(e) => {
            log::warn!(
                target: "copy",
                "landing: couldn't remove the unplaced temp {} ({e}); the stale-temp sweep will",
                LogPrivate::file_path(temp), e = LogError(&e)
            );
            false
        }
    }
}

/// The new files an operation's stop or failure kept at the destination, for
/// its terminal event to name: landings left recorded for the destination's
/// next arrival (by the name each is on its way to), and bytes rescued under a
/// ` (recovered)` name (by that name).
#[derive(Debug)]
pub(crate) struct KeptNewData {
    pub(crate) landings: Vec<PathBuf>,
    pub(crate) rescued: Vec<PathBuf>,
}

impl KeptNewData {
    /// Takes what `state`'s landings kept, so one terminal event names it.
    ///
    /// ❗ A stop, or a failure drain, swallows the item's own error, so this is
    /// the only place the person hears of a file a sibling's landing kept.
    pub(crate) fn take(state: &WriteOperationState) -> Self {
        Self {
            landings: std::mem::take(&mut *state.kept_landings.lock_ignore_poison()),
            rescued: std::mem::take(&mut *state.rescued_landings.lock_ignore_poison()),
        }
    }

    /// Leaves out the file `failure` already names (a `NewDataKeptAt`'s), so
    /// the person reads each file once.
    #[must_use]
    pub(crate) fn beside(mut self, failure: &WriteOperationError) -> Self {
        if let WriteOperationError::NewDataKeptAt { path, kept_at, .. } = failure {
            self.landings.retain(|landing| landing != Path::new(path));
            self.rescued.retain(|rescue| rescue != Path::new(kept_at));
        }
        self
    }

    /// One summary group per fate, on `volume_name`.
    pub(crate) fn groups(&self, volume_name: &str) -> impl Iterator<Item = StagedLeftovers> {
        [
            StagedLeftovers::of(StagedLeftoverFate::LandsWhenBack, &self.landings, volume_name),
            StagedLeftovers::of(StagedLeftoverFate::Rescued, &self.rescued, volume_name),
        ]
        .into_iter()
        .flatten()
    }
}

/// The rescue couldn't happen, so the bytes stay where they are and the caller
/// reports THAT path. Says so loudly: this is the one shape in which committed
/// data still wears a name `cleanup.rs::reap_stale_transfer_temps` matches
/// (spared only while a ledger record holds it, as `staged_write::land` keeps
/// its landing's).
fn keeps_its_temp_name(temp: &Path) -> PathBuf {
    log::warn!(
        "rescue_out_of_temp_space: couldn't move {} to a real filename, so the new data stays under its temp name",
        LogPrivate::file_path(temp)
    );
    temp.to_path_buf()
}
