//! Getting committed data out of `.cmdr-tmp-*` space, and saying where it went.
//!
//! A staged write's temp stops being a partial the moment the bytes are all
//! there: from then on it holds the only complete copy of the new file, and the
//! landing rename is what gives it the user's name. When that rename fails after
//! the destination has already been cleared, the bytes are committed data
//! wearing a sweepable name, and both halves of this module answer that:
//! [`rescue_out_of_temp_space`] gives them a real filename, and
//! [`FinalizeFailure`] carries that name out to the user.
//!
//! It sits at `transfer/` level rather than inside `volume/` because BOTH
//! landings reach it: `volume::conflict::finalize_safe_replace` (a cross-volume
//! file→file Overwrite) and `staged_write::land` (any staged write onto a name
//! the caller claimed).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::unique_name::{NameCandidates, RESCUE_NAME_ATTEMPTS, recovered_sibling};
use crate::file_system::volume::{Volume, VolumeError};

/// What a failed landing leaves behind, and where.
///
/// ❗ The path is TYPED data, not prose: it travels to the user through
/// `WriteOperationError::NewDataKeptAt` so the dialog can name the file they
/// have to go and look at. ❌ Never parse it back out of a message.
#[derive(Debug)]
pub(in crate::file_system::write_operations) struct FinalizeFailure {
    /// What the destination said.
    pub error: VolumeError,
    /// Where the only complete copy of the NEW bytes is now, when the name they
    /// were meant to take is already cleared: the ` (recovered)` name, or the
    /// `.cmdr-tmp-*` temp when even that rename couldn't happen.
    ///
    /// `None` when nothing was cleared, so nothing was rescued and nothing was
    /// lost.
    pub new_data_at: Option<PathBuf>,
}

impl From<VolumeError> for FinalizeFailure {
    /// An ordinary failure, with nothing stranded anywhere.
    fn from(error: VolumeError) -> Self {
        Self {
            error,
            new_data_at: None,
        }
    }
}

/// Gets the complete new bytes out of `.cmdr-tmp-*` space and into a real
/// filename next to where they were meant to land.
///
/// ❗ **This is what stops the hourly reap from eating them.**
/// `cleanup.rs::reap_stale_transfer_temps` matches on the `.cmdr-tmp-` marker
/// plus an age, and it runs at the start of every copy and every volume move
/// into a directory. A temp left here is committed data with no ledger entry
/// (`staged_write::commit` deregisters it before landing), so an hour later the
/// next transfer into the same folder would delete the user's only copy. A file
/// called `notes (recovered).txt` is one no sweep can match.
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
pub(in crate::file_system::write_operations::transfer) async fn rescue_out_of_temp_space(
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    orig: &Path,
) -> PathBuf {
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

/// The rescue couldn't happen, so the bytes stay where they are and the caller
/// reports THAT path. Says so loudly: this is the one shape in which committed
/// data still wears a name `cleanup.rs::reap_stale_transfer_temps` matches.
fn keeps_its_temp_name(temp: &Path) -> PathBuf {
    log::warn!(
        "rescue_out_of_temp_space: couldn't move {} to a real filename, so the new data stays under its temp name",
        temp.display()
    );
    temp.to_path_buf()
}
