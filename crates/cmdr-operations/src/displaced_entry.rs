//! An entry an Overwrite renamed out of the way, held until the operation says
//! whether it goes or comes back, and the no-replace rename that puts it back.
//! Split from `overwrite.rs` (the local landing) because the copy transaction
//! (`ledger.rs`) and the rename (`rename.rs`) hold these in every build,
//! the browser's included, where the local landing isn't compiled.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::staging::StagingTemp;

use super::host::{AsideAttempt, OperationErrorReport};
use super::in_flight_temps::{self, ItemKind, TrackedRecord};
use super::state::WriteOperationState;
use super::types::{RecoveredOriginal, WriteOperationError};
use super::unique_name::{NameCandidates, RESCUE_NAME_ATTEMPTS, recovered_sibling};

/// `rename(2)` that fails with `AlreadyExists` instead of clobbering `dest`.
///
/// A copy landing its temp on a name the conflict check found free, and a
/// cancelled move renaming an item back to its original source. Both have a
/// window between the check and the rename in which a file can appear, and
/// destroying it would be silent and unrecoverable.
///
/// ❗ One line, by design: the pane's own renames go through the same primitive
/// (`volume::rename_local_exclusive`), and while the two were separate
/// implementations only this one degraded on a filesystem without the kernel's
/// no-replace flag. That cost every rename on a mounted SMB share (ERR-8RFN4).
pub(super) fn rename_no_replace(temp: &Path, dest: &Path) -> std::io::Result<()> {
    crate::local_disk::rename_exclusive(temp, dest)
}

/// An entry this operation renamed out of the way, still on disk under its
/// `dest.cmdr-temp-{uuid}` name, waiting for the operation to say whether it
/// goes or comes back.
///
/// `overwrite::safe_overwrite_dir` covers the landing whose new content is complete by
/// the time its closure returns. A folder→file Overwrite isn't that: the
/// directory appears the moment the first child needs a parent, and the subtree
/// lands leaf by leaf over the rest of the copy. So the file it displaced has to
/// stay recoverable for that whole stretch, which means the ledger holds one of
/// these ([`super::ledger::CopyTransaction::record_displaced`]) and answers on
/// commit or on reversal.
///
/// The guard rides along so the aside stays hidden from the pane for exactly as
/// long as it's on disk.
pub(crate) struct DisplacedEntry {
    aside: StagingTemp,
    /// Where it came from, and where [`DisplacedEntry::restore`] puts it back.
    original: PathBuf,
    /// The ledger's record of it, retired by whichever ending really happened.
    /// A record that outlives the process is what gets the file back after a
    /// crash, and the operation is the one that knows which ending it was.
    record: TrackedRecord,
    /// Kept so every ending can reach the ledger; `DisplacedEntry` outlives the
    /// call that made it (the transaction holds it for the rest of the copy).
    state: Arc<WriteOperationState>,
}

/// Hand-rolled rather than derived: the entry now carries the operation's
/// state, which has no `Debug` of its own and would say nothing useful in a
/// test dump anyway. The two paths are the whole story.
#[cfg(test)]
impl std::fmt::Debug for DisplacedEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DisplacedEntry")
            .field("aside", &self.aside.path())
            .field("original", &self.original)
            .finish()
    }
}

impl DisplacedEntry {
    /// Puts the entry back at its own name. Best-effort: something else standing
    /// there is left alone, and a failed rename leaves the aside on disk under
    /// its recognizable name, with its RECORD, so the sweep answers for it.
    pub(crate) fn restore(self) {
        let aside = self.aside.path();
        match rename_no_replace(aside, &self.original) {
            Ok(()) => in_flight_temps::retire(&self.state, &self.record),
            Err(e) => {
                self.state
                    .scope()
                    .host()
                    .events()
                    .error_report(OperationErrorReport::AsideStranded {
                        attempt: AsideAttempt::DisplacedRestore {
                            original: self.original.clone(),
                        },
                        aside: aside.to_path_buf(),
                        error: e.to_string(),
                    });
                in_flight_temps::keep_for_arrival(&self.state, self.record.clone());
            }
        }
    }

    /// Keeps the entry for the user under a ` (recovered)` name beside the
    /// folder that took its own, and answers where it went.
    ///
    /// ❗ **The outcome for a FAILED operation**, which keeps every file that
    /// landed (`transfer/copy/mod.rs`, `PostLoopIntent::Failed`). The folder is
    /// staying at the original name with only part of its subtree in it, so
    /// [`DisplacedEntry::restore`] has nowhere to put the file back and
    /// [`DisplacedEntry::discard`] would delete the user's only copy of it. A
    /// file called `notes (recovered).txt` is one they can find; a
    /// `.cmdr-temp-<uuid>` is one the pane hides and the next launch sweeps.
    ///
    /// Only `AlreadyExists` earns another candidate, and only
    /// [`RESCUE_NAME_ATTEMPTS`] of them: every other refusal (a read-only
    /// destination, a dead mount) would refuse each candidate identically. When
    /// nothing lands, the aside path is the honest answer and the caller reports
    /// THAT, the same shape the volume engine's rescue takes
    /// (`transfer/recovered_name.rs::rescue_out_of_temp_space`).
    pub(crate) fn keep_as_recovered_sibling(self) -> RecoveredOriginal {
        let recovered = recovered_sibling(&self.original);
        let mut candidates = NameCandidates::for_file(&recovered);
        // The bare ` (recovered)` name first; `NameCandidates` starts at ` (1)`.
        let mut candidate = recovered.clone();
        loop {
            match rename_no_replace(self.aside.path(), &candidate) {
                Ok(()) => {
                    in_flight_temps::retire(&self.state, &self.record);
                    return RecoveredOriginal::new(&self.original, &candidate);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    self.state
                        .scope()
                        .host()
                        .events()
                        .error_report(OperationErrorReport::AsideStranded {
                            attempt: AsideAttempt::RecoveredName,
                            aside: self.aside.path().to_path_buf(),
                            error: e.to_string(),
                        });
                    in_flight_temps::keep_for_arrival(&self.state, self.record.clone());
                    return RecoveredOriginal::new(&self.original, self.aside.path());
                }
            }
            if candidates.attempts() >= RESCUE_NAME_ATTEMPTS {
                log::warn!(
                    "DisplacedEntry::keep_as_recovered_sibling: every ` (N)` variant of {} is taken",
                    LogPrivate::path(&recovered)
                );
                in_flight_temps::keep_for_arrival(&self.state, self.record.clone());
                return RecoveredOriginal::new(&self.original, self.aside.path());
            }
            candidate = candidates.current();
            candidates.advance();
        }
    }

    /// Drops the entry for good, the operation having committed the thing that
    /// replaced it. Non-critical: a leftover wears a recognizable name, and
    /// keeps its record so a sweep gives it a real one.
    pub(crate) fn discard(self) {
        let aside = self.aside.path();
        let removed = if aside.is_dir() {
            fs::remove_dir_all(aside)
        } else {
            fs::remove_file(aside)
        };
        match removed {
            Ok(()) => in_flight_temps::retire(&self.state, &self.record),
            Err(e) => {
                log::debug!(
                    target: "copy",
                    "couldn't remove the displaced original at {}: {e}. It keeps its record.",
                    LogPrivate::path(aside)
                );
                in_flight_temps::keep_for_arrival(&self.state, self.record.clone());
            }
        }
    }
}

/// Renames whatever is at `dest` aside and stands a fresh directory in its
/// place, handing back the displaced entry for the caller's transaction to hold.
///
/// The folder→file Overwrite's opening move. Only the directory at `dest` is
/// created: any deeper level the caller needs is its own ordinary
/// create-and-record walk, so every directory this operation makes ends up in
/// the ledger. A failure after the rename puts the original straight back.
pub(super) fn displace_with_directory(
    state: &Arc<WriteOperationState>,
    dest: &Path,
) -> Result<DisplacedEntry, WriteOperationError> {
    let aside = StagingTemp::mint_aside(dest, Uuid::new_v4(), state.liveness_token());
    // Recorded before the rename, for the same reason `stage_and_land_file`
    // records its aside there: a crash in between would leave the user's file
    // under a scratch name with nothing that knows what it is.
    let record = in_flight_temps::track(
        state,
        ItemKind::DisplacedFile {
            destination: dest.to_path_buf(),
        },
        aside.path(),
    );
    if let Err(e) = fs::rename(dest, aside.path()) {
        in_flight_temps::retire(state, &record);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to set aside existing destination: {}", e).into(),
        });
    }
    let displaced = DisplacedEntry {
        aside,
        original: dest.to_path_buf(),
        record,
        state: Arc::clone(state),
    };
    if let Err(e) = fs::create_dir(dest) {
        displaced.restore();
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!(
                "Failed to create directory after setting the blocking file aside: {}",
                e
            )
            .into(),
        });
    }
    Ok(displaced)
}
