//! How a local write lands: the bytes always go to a `.cmdr-tmp-*` sibling and
//! take the destination's real name by a single same-directory `rename(2)`. When
//! an existing entry is in the way, it is renamed aside first, so the user's
//! original survives every failure up to the swap.
//!
//! **The invariant.** A local destination path must never hold a partial file.
//! Whatever a crash, a force-quit, or an abandoned worker thread leaves behind
//! wears a `.cmdr-tmp-*` name nobody mistakes for their data. Staging is what
//! makes abandoning a worker safe: it is writing to a temp nobody will rename.
//!
//! [`stage_and_land_file`] is the single landing for every local file copy —
//! APFS clone, chunked copy, `copy_file_range`, and the `std::fs::copy`
//! fallback all hand it a closure that puts bytes at a path.
//! [`safe_overwrite_dir`] is its type-agnostic sibling, for a materializer that
//! creates a directory rather than a file.
//!
//! **Rename-blocking metadata goes on AFTER the landing rename.** `uchg` or a
//! `deny delete` ACL on the temp makes the rename refuse (`EPERM`/`EACCES`), so
//! the closure hands that set back instead of writing it ([`Staged::landed`]),
//! and a temp that arrives wearing one anyway (an APFS clone copies the source's
//! flags and ACL in the same call) is cleared and retried once. Either way the
//! set lands on the FINAL name through `cmdr_fs::metadata::apply_landed`, or
//! goes back to a caller that renames the item again later ([`PostLanding`]).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{LandedMetadata, Unblock, apply_landed, clear_blockers};

use super::error_classification::classify_io_error;
use super::host::{Announced, AsideAttempt, OperationErrorReport};
use super::in_flight_temps::{self, ItemKind, TrackedRecord};
use super::mount_boundary::refuse_a_mount_inside;
use super::state::WriteOperationState;
use super::types::{RecoveredOriginal, WriteOperationError};
use super::unique_name::{NameCandidates, RESCUE_NAME_ATTEMPTS, recovered_sibling};
use cmdr_fs::staging::StagingTemp;

/// Result of applying a conflict resolution.
#[derive(Debug)]
pub(super) struct ResolvedDestination {
    /// The path to write to
    pub path: PathBuf,
    /// Whether this is an overwrite that needs safe handling
    pub needs_safe_overwrite: bool,
}

/// What a write closure put at the staging path.
pub(super) struct Staged {
    pub bytes: u64,
    /// What has to go on after the landing rename (an ACL a chunked copy read
    /// off its source), so the temp never wears it. Empty for most files.
    pub landed: LandedMetadata,
}

impl Staged {
    /// A write with nothing to put on after landing.
    #[cfg_attr(
        all(target_os = "linux", not(test)),
        expect(dead_code, reason = "both Linux arms hand their source's ACL to the landing")
    )]
    pub(super) fn bytes(bytes: u64) -> Self {
        Staged {
            bytes,
            landed: LandedMetadata::default(),
        }
    }
}

/// Who puts the rename-blocking set on the landed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PostLanding {
    /// The landing does, on the final name. Every copy that lands where it stays.
    Apply,
    /// The caller does, after it renames the item again: a cross-FS move's
    /// staging landing, which Phase 3 still has to rename (and which a Locked or
    /// `deny delete` item would refuse).
    HandBack,
}

/// What [`stage_and_land_file`] landed.
pub(super) struct Landed {
    pub bytes: u64,
    /// The rename-blocking set the caller owes the item, under
    /// [`PostLanding::HandBack`]. Always empty under [`PostLanding::Apply`].
    pub handed_back: LandedMetadata,
}

/// Lands one local file write at `dest` through a staging sibling, replacing an
/// existing entry only when `replacing` says so.
///
/// `write_bytes` is handed the STAGING path and must put the complete file
/// there; it never sees `dest`. That is the whole point: whatever it leaves
/// behind on a failure, a cancel, or a thread that never returns is a
/// recognizable temp, not a truncated file wearing the user's filename.
///
/// Steps:
/// 1. Run `write_bytes` against `dest.cmdr-tmp-{uuid}` (a sibling, so the
///    landing rename stays same-directory and therefore atomic).
/// 2. `replacing` only: rename the existing entry to `dest.cmdr-temp-{uuid}`
///    (the aside).
/// 3. Rename the temp onto `dest`. Without `replacing` this refuses to clobber
///    an entry that appeared underneath us ([`rename_no_replace`]), keeping the
///    `O_EXCL`-shaped guarantee a direct create used to give. A refusal the temp
///    itself causes (`uchg` or a `deny delete` ACL, which an APFS clone carries
///    over) is cleared and the rename retried ONCE; a second refusal surfaces the
///    first error, and the temp, now cleared, is removable.
/// 4. `replacing` only: delete the aside.
/// 5. The rename-blocking set (the closure's plus whatever step 3 cleared) goes
///    on `dest` under [`PostLanding::Apply`], or back to the caller. Best effort:
///    a refusal is logged, the file stays landed. A crash between step 3 and
///    here leaves the file without its ACL or flags, and nothing records that.
///
/// A failure at any step before 3 completes leaves the original `dest`
/// untouched. The temp is registered as an in-flight partial for exactly as
/// long as it exists ([`super::in_flight_temps`]), so a quit or a crash mid-copy
/// leaves a swept-at-next-launch leftover rather than permanent litter.
///
/// **File→folder overwrite (incoming source file, existing dest folder).**
/// Local FS `rename(2)` happily swaps a directory aside under a new name, and
/// step 3 lands the source file at the original path. The aside is then removed
/// via `remove_dir_all`. The window during which the original directory is
/// gone-but-replaceable is bounded by step 3 (a single `rename` syscall). A
/// crash between step 2 and step 3 leaves a stray `dest.cmdr-temp-<uuid>/` that
/// a user can recognize and restore from.
pub(super) fn stage_and_land_file<F>(
    state: &Arc<WriteOperationState>,
    dest: Announced<'_>,
    replacing: bool,
    post_landing: PostLanding,
    write_bytes: F,
) -> Result<Landed, WriteOperationError>
where
    F: FnOnce(&Path) -> Result<Staged, WriteOperationError>,
{
    let dest = dest.path();
    replacing.then(|| refuse_a_mount_inside(dest)).transpose()?; // step 4 removes a replaced folder as a tree
    // The temp's guard lives to the end of this function (and the aside's too,
    // when there is one), keeping the scratch out of the pane for as long as
    // it's on disk. One uuid covers both, so a crash leftover reads as two
    // halves of one overwrite.
    let uuid = Uuid::new_v4();
    let owner = state.liveness_token();
    let temp = StagingTemp::mint_with_uuid(dest, uuid, owner.clone());
    let temp_path = temp.path();

    // Both halves of the write go into the downloads watcher's ignore set: the
    // CREATE lands on the temp and the RENAME carries it to the final name, and
    // the watcher keys on exact paths. Registering only the final name would let
    // a copy into ~/Downloads toast its own `.cmdr-tmp-*`. No-ops elsewhere.
    // The caller announced `dest` already (it's an `Announced`); the second
    // announce, right beside the landing, is a no-op bump.
    state.scope().host().announce(temp_path);
    state.scope().host().announce(dest);

    // Findable from the moment the file can exist until the moment it can't:
    // unlike the async cross-volume path (`transfer/staged_write.rs`), landing
    // here is one synchronous syscall, so there is no window in which the temp
    // holds the only complete copy of anything.
    let temp_record = in_flight_temps::track(state, ItemKind::Temp, temp_path);

    // Step 1: fill the temp.
    let Staged { bytes, mut landed } = match write_bytes(temp_path) {
        Ok(staged) => staged,
        Err(e) => {
            discard_temp(state, &temp_record);
            // The temp is a name the user never saw: the failure is about `dest`
            // (and the transfer tree finds its row by it).
            return Err(e.on_item_instead(temp_path, dest));
        }
    };

    // Step 2: move the existing entry out of the way (overwrite only).
    //
    // Recorded BEFORE the rename, with the size the replacement is about to
    // reach: a crash between the rename and the record would leave the user's
    // original wearing a scratch name with nothing that knows what it is. A
    // record for a rename that then fails is retired a line later, and would
    // have cost one `already gone` either way.
    // The aside is a third name this landing creates (the rename below), so the
    // host hears it first, same as the temp.
    let aside = replacing.then(|| StagingTemp::mint_aside(dest, uuid, owner));
    if let Some(aside) = &aside {
        state.scope().host().announce(aside.path());
    }
    let aside_record = aside.as_ref().map(|aside| {
        in_flight_temps::track(
            state,
            ItemKind::FileAside {
                destination: dest.to_path_buf(),
                expected_size: bytes,
            },
            aside.path(),
        )
    });
    if let Some(aside) = &aside
        && let Err(e) = fs::rename(dest, aside.path())
    {
        if let Some(record) = &aside_record {
            in_flight_temps::retire(state, record);
        }
        discard_temp(state, &temp_record);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to set aside existing destination: {}", e).into(),
        });
    }

    // Test seam: hold the overwrite open with the user's original sitting under
    // its scratch name, which is the exact window this file's ledger exists for.
    // No-op in production, which installs no park.
    #[cfg(test)]
    if aside.is_some() {
        state.scope().aside_park().park_if_armed();
    }

    // Step 3: give the bytes their real name.
    if let Err(e) = land_temp_clearing_blockers(temp_path, dest, replacing, &mut landed) {
        // Restore the aside if we set one. If the restore ALSO fails, the user's
        // original survives orphaned under the recognizable `.cmdr-temp-<uuid>`
        // name, and its RECORD survives with it, so the next launch (or the
        // drive's return) puts it back rather than leaving it for someone to
        // find by hand (AGENTS.md principle 1: protect the user's data).
        //
        // Through the no-replace primitive, so putting the original back can't
        // itself destroy something that took the name while this write was in
        // flight. That refusal lands in the `Err` arm below, which is already
        // the right answer: the original keeps its record and the sweep gives
        // it a real name.
        if let Some(aside) = &aside {
            match rename_no_replace(aside.path(), dest) {
                Ok(()) => {
                    if let Some(record) = &aside_record {
                        in_flight_temps::retire(state, record);
                    }
                }
                Err(restore_err) => {
                    state
                        .scope()
                        .host()
                        .events()
                        .error_report(OperationErrorReport::AsideStranded {
                            attempt: AsideAttempt::LandingRestore {
                                original: dest.to_path_buf(),
                            },
                            aside: aside.path().to_path_buf(),
                            error: restore_err.to_string(),
                        });
                    if let Some(record) = aside_record {
                        in_flight_temps::keep_for_arrival(state, record);
                    }
                }
            }
        }
        discard_temp(state, &temp_record);
        // A destination that appeared underneath a non-replacing write is a
        // typed outcome the caller acts on, not an opaque IO failure. So is a
        // name the destination can't hold: the temp is shortened to fit
        // (`cmdr_fs::staging::names`), so a too-long final name surfaces HERE,
        // and it must still tell the user to pick a shorter one.
        return Err(match e.kind() {
            std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists {
                path: dest.display().to_string(),
            },
            std::io::ErrorKind::InvalidFilename => classify_io_error(&e, dest.display().to_string()),
            _ => WriteOperationError::IoError {
                path: dest.display().to_string(),
                message: format!("Failed to finalize the copy: {}", e).into(),
            },
        });
    }
    // The temp is gone (it IS `dest` now), so it stops being a partial.
    in_flight_temps::retire(state, &temp_record);

    // Step 4: Delete the renamed-aside original (non-critical, ignore errors).
    // Use remove_dir_all for directory asides (file-over-folder overwrite).
    //
    // Intentional: we do NOT retain a backup of the overwritten original for
    // rollback. Keeping per-file backups for the whole operation risks
    // unexpectedly filling the user's drive on a large Overwrite. Consequence:
    // rollback removes new files but can't restore overwritten originals.
    // Revisit if users complain. See transfer/volume/DETAILS.md § "Overwrite isn't reversible".
    //
    // The record only retires when the removal really happened. One that didn't
    // leaves the user's original on disk under a scratch name, and the sweep's
    // exact-size rule then settles it against the file that took its place.
    if let Some(aside) = &aside {
        let removed = if aside.path().is_dir() {
            fs::remove_dir_all(aside.path())
        } else {
            fs::remove_file(aside.path())
        };
        match (removed, aside_record) {
            (_, None) => {}
            (Ok(()), Some(record)) => in_flight_temps::retire(state, &record),
            (Err(e), Some(record)) => {
                log::debug!(
                    target: "copy",
                    "couldn't remove the replaced original at {}: {e}. It keeps its record.",
                    LogPrivate::file_path(aside.path())
                );
                in_flight_temps::keep_for_arrival(state, record);
            }
        }
    }

    // Step 5: the rename-blocking set, on the name the file now has.
    if landed.is_empty() {
        return Ok(Landed {
            bytes,
            handed_back: landed,
        });
    }
    match post_landing {
        PostLanding::Apply => {
            let report = apply_landed(dest, &landed);
            if report.any_refused() {
                log::warn!(
                    target: "copy",
                    "{} landed without some of its metadata: {report:?}",
                    LogPrivate::file_path(dest)
                );
            }
            Ok(Landed {
                bytes,
                handed_back: LandedMetadata::default(),
            })
        }
        PostLanding::HandBack => Ok(Landed {
            bytes,
            handed_back: landed,
        }),
    }
}

/// [`land_temp`], and when the TEMP refuses it (`uchg`, a `deny delete` ACL):
/// clear those, retry once, and fold what was cleared into `landed` so it goes
/// back on the landed file. The landing rename is the probe, so an ordinary
/// temp costs exactly one rename.
///
/// A second refusal (the folder itself refuses) answers the FIRST error, the
/// one that names why the landing couldn't happen.
fn land_temp_clearing_blockers(
    temp: &Path,
    dest: &Path,
    replacing: bool,
    landed: &mut LandedMetadata,
) -> std::io::Result<()> {
    let refused = match land_temp(temp, dest, replacing) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => e,
        other => return other,
    };
    let Unblock::Cleared(cleared) = clear_blockers(temp) else {
        return Err(refused);
    };
    match land_temp(temp, dest, replacing) {
        Ok(()) => {
            landed.merge(cleared);
            Ok(())
        }
        Err(retry) => {
            log::debug!(
                target: "copy",
                "{} still refused its landing after clearing its flags and ACL: {retry}",
                LogPrivate::file_path(temp)
            );
            Err(refused)
        }
    }
}

/// Removes a temp whose write or landing failed, and stops tracking it.
///
/// The bytes there are a partial (or a complete copy whose source is still on
/// disk, since the caller reports the item failed and never deletes a source),
/// so there is nothing to preserve. Best-effort: a temp a wedged thread still
/// holds open may refuse to go, which is why it wears a recognizable name.
///
/// ❗ The record only retires on a removal that HAPPENED. A `NotFound` is the
/// settled answer only while the destination drive is still listed: on a drive
/// that was pulled, "not found" is the mount being gone, and the partial is
/// still sitting on the drive in the user's hand. That record stays, and joins
/// the pending set, so plugging the drive back in this same session sweeps it.
fn discard_temp(state: &Arc<WriteOperationState>, temp: &TrackedRecord) {
    let settled = match fs::remove_file(temp.absolute()) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => !destination_has_left(state),
        Err(e) => {
            log::debug!(
                target: "copy",
                "couldn't remove the partial at {}: {e}. It keeps its record.",
                LogPrivate::file_path(temp.absolute())
            );
            false
        }
    };
    if settled {
        in_flight_temps::retire(state, temp);
    } else {
        in_flight_temps::keep_for_arrival(state, temp.clone());
    }
}

/// Whether the mount table SAYS this operation's destination drive is gone.
///
/// `false` for an operation with no second side and for a table that couldn't be
/// read: neither is evidence of leaving.
fn destination_has_left(state: &WriteOperationState) -> bool {
    state.sides.as_ref().is_some_and(|sides| sides.destination.has_left())
}

/// Renames `temp` onto `dest`, refusing to replace an existing entry unless
/// `replacing`.
fn land_temp(temp: &Path, dest: &Path, replacing: bool) -> std::io::Result<()> {
    if replacing {
        fs::rename(temp, dest)
    } else {
        rename_no_replace(temp, dest)
    }
}

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
    cmdr_local::rename_local_exclusive(temp, dest)
}

/// An entry this operation renamed out of the way, still on disk under its
/// `dest.cmdr-temp-{uuid}` name, waiting for the operation to say whether it
/// goes or comes back.
///
/// [`safe_overwrite_dir`] covers the landing whose new content is complete by
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

/// Performs a safe overwrite of `dest` by setting the existing entry aside
/// under `dest.cmdr-temp-{uuid}`, then running the caller's `materialize`
/// closure to land the new content at `dest`. On materialize failure or
/// cancellation the aside is rolled back, restoring the original entry.
///
/// For a landing that ISN'T finished when the closure returns, reach for
/// [`displace_with_directory`] instead.
///
/// The helper is type-agnostic: `dest` may hold a file or a directory before
/// the call, and `materialize` may create either a file or a directory. The
/// two cmdr-cross-type cases that motivated it:
///
/// - **Folder→file overwrite (copy/move):** source is a directory whose
///   contents will be materialized at `dest`, which currently holds a file.
///   The closure creates a fresh directory and populates it; on success the
///   blocking file is removed via `remove_file`.
/// - **File→folder overwrite (copy/move):** source is a file whose bytes
///   will be materialized at `dest`, which currently holds a directory. The
///   closure writes the file; on success the existing folder is removed via
///   `remove_dir_all`.
///
/// Steps:
/// 1. Sets aside the existing `dest` as `dest.cmdr-temp-{uuid}` via a single
///    `rename(2)`.
/// 2. Runs `materialize(dest)` to land the new content. The closure decides
///    whether `dest` becomes a file or a directory.
/// 3. On `Ok`, removes the aside (`remove_dir_all` for directory asides,
///    `remove_file` for file asides).
/// 4. On `Err`, removes whatever the closure left at `dest` and renames the
///    aside back to `dest`, then propagates the error.
///
/// **Atomicity guarantee:** at every observable moment after this function
/// is called and before it returns, `dest` is either the original
/// (untouched) or the new materialized content. The closure may briefly
/// leave a half-written entry at `dest`, but the original is recoverable
/// from the aside even on a crash — the aside has the recognizable
/// `cmdr-temp-` prefix so a user can restore it by hand.
pub(super) fn safe_overwrite_dir<F>(
    state: &Arc<WriteOperationState>,
    dest: &Path,
    materialize: F,
) -> Result<(), WriteOperationError>
where
    F: FnOnce(&Path) -> Result<(), WriteOperationError>,
{
    refuse_a_mount_inside(dest)?; // step 3 removes a folder aside as a tree
    // The guard lives to the end of this function, keeping the aside out of the
    // pane for as long as it's on disk.
    let aside = StagingTemp::mint_aside(dest, Uuid::new_v4(), None);
    let aside_path = aside.path();
    // And the ledger outlives the function, which is what the guard can't do: a
    // force-quit while `materialize` is halfway through a subtree leaves this
    // aside holding the only copy of whatever was at `dest`.
    let record = in_flight_temps::track(
        state,
        ItemKind::DirOverwriteAside {
            destination: dest.to_path_buf(),
        },
        aside_path,
    );

    // Step 1: Rename existing dest aside. This survives a crash: the original
    // is recognizable on next launch and the sweep puts it back.
    if let Err(e) = fs::rename(dest, aside_path) {
        in_flight_temps::retire(state, &record);
        return Err(WriteOperationError::IoError {
            path: dest.display().to_string(),
            message: format!("Failed to set aside existing destination: {}", e).into(),
        });
    }

    // Step 2: Run the caller's materialize step. The caller is responsible
    // for creating the dest directory and populating it.
    let materialize_result = materialize(dest);

    match materialize_result {
        Ok(()) => {
            // Step 3: Remove the aside. Best-effort; a leftover is recognizable
            // and keeps its record, so a sweep gives it a real name.
            let removed = if aside_path.is_dir() {
                fs::remove_dir_all(aside_path)
            } else {
                fs::remove_file(aside_path)
            };
            settle_aside_record(state, record, removed, aside_path);
            Ok(())
        }
        Err(e) => {
            // Failure or cancellation: clean up whatever materialize created at
            // dest and rename the aside back.
            let restored = restore_after_materialize_failure(aside_path, dest, |path| fs::symlink_metadata(path));
            if let Err(restore_err) = &restored {
                state
                    .scope()
                    .host()
                    .events()
                    .error_report(OperationErrorReport::AsideStranded {
                        attempt: AsideAttempt::DirOverwriteRestore {
                            original: dest.to_path_buf(),
                        },
                        aside: aside_path.to_path_buf(),
                        error: restore_err.to_string(),
                    });
            }
            settle_aside_record(state, record, restored, aside_path);
            Err(e)
        }
    }
}

/// Removes what a failed materializer left behind and restores the original.
///
/// The metadata read is injected so a test can model a transient stat failure:
/// on a real filesystem, permissions that make this read fail generally make
/// the adjacent remove and rename fail too.
///
/// Only `NotFound` proves the name is free. Any other failed stat leaves the
/// original aside for the in-flight sweep rather than risking a rename over an
/// entry the filesystem would not describe. The exclusive rename closes the
/// separate race in which something takes the name after cleanup.
fn restore_after_materialize_failure(
    aside: &Path,
    dest: &Path,
    metadata: impl FnOnce(&Path) -> std::io::Result<fs::Metadata>,
) -> std::io::Result<()> {
    match metadata(dest) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(dest)?,
        Ok(_) => fs::remove_file(dest)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    rename_no_replace(aside, dest)
}

/// Retires an aside's record when the thing that was meant to happen to it
/// really did, and keeps it for a sweep when it didn't.
///
/// ❗ The asymmetry is the point: a record that stays costs one line in a log
/// file, and a record retired over a removal that silently failed costs the user
/// their file.
fn settle_aside_record(
    state: &Arc<WriteOperationState>,
    record: TrackedRecord,
    outcome: std::io::Result<()>,
    aside_path: &Path,
) {
    match outcome {
        Ok(()) => in_flight_temps::retire(state, &record),
        Err(e) => {
            log::debug!(
                target: "copy",
                "the aside at {} is still there ({e}), so it keeps its record",
                LogPrivate::path(aside_path)
            );
            in_flight_temps::keep_for_arrival(state, record);
        }
    }
}

/// Test seam: holds a safe-overwrite open between its two renames.
#[cfg(test)]
#[path = "overwrite_aside_park.rs"]
pub(crate) mod aside_park;

/// The staged file landing, the folder landing, and what `displace_with_directory`
/// leaves behind.
#[cfg(test)]
#[path = "overwrite_tests.rs"]
mod tests;
