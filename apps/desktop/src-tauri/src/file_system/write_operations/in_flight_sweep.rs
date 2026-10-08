//! What a launch, and a drive coming back, do with what the ledger recorded.
//!
//! One set of rules, run from two places: `init_and_sweep` at startup, and the
//! volume registry's arrival announcement whenever a volume a record waits on
//! shows up. ❗ They must stay ONE set. A leftover on a USB stick meets whichever
//! of the two happens first, and rules that drifted apart would mean the drive's
//! fate depended on whether it was plugged in at launch.
//!
//! ## The rules, and the one thing none of them may do
//!
//! - A **temp** holds bytes on their way in. Nothing else has them, so it goes.
//! - An **aside** holds bytes that were already the user's. It goes back to its
//!   own name when that name is free; it is removed ONLY when the replacement
//!   that displaced it is provably complete, which for a file means a regular
//!   file of exactly the recorded size; and in every other case it keeps the
//!   bytes under a ` (recovered)` name beside where they belong.
//! - A **staging directory** is a cross-filesystem move's half-built tree. It is
//!   only ever `remove_dir`'d, ❌ never recursively: a destination that left
//!   before Phase 3 leaves the whole staged tree in there, on a drive that comes
//!   back later, and the user's originals can already be gone.
//!
//! ❌ **Nothing here may remove something it hasn't looked at.** Every removal
//! is gated on a presence read, and a read that FAILS is never treated as
//! "nothing there" — it defers. `Standing::Unknown` exists for exactly that.
//!
//! ❌ **Neither entry point may run on a runtime worker.** These rules are
//! `async` so the `Surface::Volume` arm can await a backend, but every
//! `Surface::Local` arm is plain blocking `std::fs` — pointed at a removable
//! drive, which is exactly where a `stat` or an `unlink` can sit for 30-120 s on
//! a slow or wedged mount. The launch sweep gets its own thread and the arrival
//! sweep a `spawn_blocking`; both `block_on` from there.
//!
//! ## Which filesystem answers
//!
//! [`Surface`] is the four primitives the rules need, pointed at either this
//! machine's filesystem or one volume's own namespace. The record says which
//! ([`ItemHome`]), and ❗ that is never re-derived here: a direct SMB session
//! roots at `/` in its own namespace, so guessing from the path is how a sweep
//! points `remove_file` at the user's Mac.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::{ItemHome, ItemKind, Ledger, Record, RecordedTemp, SweepTally, TrackedItem};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{Volume, VolumeError, rename_local_exclusive};
use crate::file_system::write_operations::transfer_sides::root_is_listed;
use crate::file_system::write_operations::types::MoveLeftoversKeptEvent;
use crate::file_system::write_operations::unique_name::{NameCandidates, RESCUE_NAME_ATTEMPTS, recovered_sibling};

/// Settles the records an earlier run left that the local filesystem can answer
/// for on its own, then takes whatever pending volume is already reachable.
///
/// Runs on its own thread (`init_and_sweep` says why nothing waits on it), so
/// blocking filesystem calls inside the async rules are free here.
pub(super) fn persisted_orphans(ledger: &Ledger, locals: &[Record]) -> SweepTally {
    let mut tally = SweepTally::default();
    for record in locals {
        tauri::async_runtime::block_on(settle_and_record(ledger, record, &mut tally));
    }

    // A volume registered before the sweep thread got here (the boot volume, an
    // external disk) can be served right away; everything else waits for its
    // arrival. Asking the registry whether an ID is present is a lock and a hash
    // lookup, so a dead NAS costs nothing and blocks nobody.
    for volume_id in ledger.pending_volume_ids() {
        if get_volume_manager().get(&volume_id).is_none() {
            continue;
        }
        let claimed = ledger.claim_pending(&volume_id);
        tally.add(tauri::async_runtime::block_on(on_volume(ledger, &volume_id, claimed)));
    }
    // ASSIGNED, not added: a record a reachable volume then refused is already
    // back in `pending`, so counting both would report it twice. What's still
    // waiting when the launch finishes is the one honest number. A LOCAL record
    // the sweep couldn't settle has nothing to wait for and is counted as left
    // alone instead.
    tally.deferred = ledger.pending_count();

    report(&tally);
    tally
}

/// The volume registry took on `volume_id`: settle whatever the ledger has been
/// holding for it.
///
/// Cheap when there's nothing waiting, which is every registration after the
/// first launch that had a leftover. The work goes elsewhere, so a registration
/// never waits on a share (the listener runs INSIDE the registration).
///
/// ❗ `spawn_blocking`, ❌ never `spawn`. Every `Surface::Local` rule is ordinary
/// blocking `std::fs`, and the drive it runs against has just this moment
/// arrived — the one place a `stat` or an `unlink` can sit on a slow or wedged
/// mount for 30-120 s. On a runtime worker that parks a thread the whole app
/// shares. "It's only a handful of records" is today's shape, not a property: a
/// drive back from a long trip can carry many.
pub(super) fn on_volume_arrival(ledger: &Ledger, volume_id: &str) {
    let claimed = ledger.claim_pending(volume_id);
    if claimed.is_empty() {
        return;
    }
    let volume_id = volume_id.to_string();
    let ledger = ledger.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // The same shape the launch sweep runs in: a blocking context that
        // `block_on`s the volume-backed rules. One shape for both entry points
        // is what keeps the rules themselves a single set.
        let tally = tauri::async_runtime::block_on(on_volume(&ledger, &volume_id, claimed));
        report(&tally);
    });
}

/// Settles `records` against the volume that owns them.
///
/// Anything it can't settle goes back to pending, so the next time that volume
/// arrives (this session or a later launch) the sweep tries again. ❌ Never
/// reconnects or authenticates: the volume is used exactly as the registry hands
/// it over.
async fn on_volume(ledger: &Ledger, volume_id: &str, records: Vec<Record>) -> SweepTally {
    let mut tally = SweepTally::default();
    for record in &records {
        settle_and_record(ledger, record, &mut tally).await;
    }
    log::debug!(target: "copy", "settled {} recorded leftovers on `{volume_id}`: {tally:?}", records.len());
    tally
}

/// Settles one record and puts the ledger in step with what happened.
async fn settle_and_record(ledger: &Ledger, record: &Record, tally: &mut SweepTally) {
    match settle(record).await {
        Outcome::Deferred => {
            ledger.defer(vec![record.clone()]);
            // A volume-homed record is counted once at the end, by what's still
            // waiting. A local one has no arrival to wait for — it's re-recorded
            // for the next launch — so it's counted here.
            if record.volume_id().is_none() {
                tally.left_alone += 1;
            }
        }
        settled => {
            ledger.retire_record(record);
            settled.tally_into(tally);
        }
    }
}

/// How one record ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Removed, because nothing else had those bytes.
    Swept,
    /// Not there any more, which for a temp is the healthy case.
    AlreadyGone,
    /// Put back at its own name.
    Restored,
    /// Kept under a ` (recovered)` name, because its own was taken.
    Recovered,
    /// Left exactly where it is, with nothing more to try.
    LeftAlone,
    /// Left where it is, with the record kept for a later try.
    Deferred,
}

impl Outcome {
    fn tally_into(self, tally: &mut SweepTally) {
        match self {
            Self::Swept => tally.swept += 1,
            Self::AlreadyGone => tally.already_gone += 1,
            Self::Restored => tally.restored += 1,
            Self::Recovered => tally.recovered += 1,
            Self::LeftAlone => tally.left_alone += 1,
            // Counted by its caller, which knows whether anything will come back
            // for it.
            Self::Deferred => {}
        }
    }
}

/// Decides one record's fate without touching the ledger.
async fn settle(record: &Record) -> Outcome {
    match record {
        Record::Legacy(temp) => settle_legacy(temp).await,
        Record::Tracked(item) => settle_tracked(item).await,
    }
}

/// A `+` line: a staged partial, and nothing else.
///
/// ❗ Only a `.cmdr-tmp-` name is removed here. The old shared name test also
/// accepted the ASIDE marker, which would have made this the one path that
/// deletes a user's original on sight — an aside says the replacement that
/// displaced it never landed. A kinded record is the only way to ask for
/// anything but a plain removal.
async fn settle_legacy(temp: &RecordedTemp) -> Outcome {
    match temp {
        RecordedTemp::Local(path) => {
            if !is_a_staged_partial(path) {
                return Outcome::LeftAlone;
            }
            Surface::Local.remove_file(path).await.into_outcome()
        }
        RecordedTemp::OnVolume(temp) => {
            if !is_a_staged_partial(&temp.path) {
                return Outcome::LeftAlone;
            }
            // `resolve`, not `get`: the site is passing a path, and a read-only
            // routed volume (an archive, a git snapshot) is one this sweep must
            // never delete through.
            let resolved = get_volume_manager().resolve(&temp.volume_id, &temp.path).await;
            let is_routed = resolved.is_routed();
            let Some(volume) = resolved.volume.filter(|_| !is_routed) else {
                return Outcome::Deferred;
            };
            Surface::Volume(volume).remove_file(&resolved.path).await.into_outcome()
        }
    }
}

/// Whether a legacy record really names a staged partial.
fn is_a_staged_partial(path: &Path) -> bool {
    let named = path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().contains(cmdr_fs::staging::STAGING_TEMP_MARKER));
    if !named {
        log::warn!(
            target: "copy",
            "the in-flight ledger holds a plain-removal record that isn't a staged partial, leaving it: {}",
            path.display()
        );
    }
    named
}

/// An `A` line: the kind decides.
async fn settle_tracked(item: &TrackedItem) -> Outcome {
    let Located::At {
        surface,
        path,
        destination,
        drive_name,
    } = locate(item).await
    else {
        return Outcome::Deferred;
    };
    if !name_matches_kind(&item.kind, &path) {
        log::warn!(
            target: "copy",
            "the in-flight ledger holds a {:?} record whose name isn't that shape, leaving it: {}",
            item.kind,
            path.display()
        );
        return Outcome::LeftAlone;
    }
    settle_kind(
        &surface,
        &item.kind,
        &path,
        destination.as_deref(),
        drive_name.as_deref(),
    )
    .await
}

/// Where a record's thing is right now, and what may be pointed at it.
enum Located {
    At {
        surface: Surface,
        path: PathBuf,
        /// Where the thing belongs, for the kinds that displaced something.
        destination: Option<PathBuf>,
        /// The drive's display name, for the one rule that says something out
        /// loud. `None` for a Mac-internal path, which names no drive.
        drive_name: Option<String>,
    },
    /// The volume isn't here, so nothing can be decided yet.
    Unreachable,
}

/// Resolves a record's stored paths into paths some filesystem can answer for.
async fn locate(item: &TrackedItem) -> Located {
    let destination = item.kind.destination().map(Path::to_path_buf);
    match &item.home {
        ItemHome::Local => Located::At {
            surface: Surface::Local,
            path: item.path.clone(),
            destination,
            drive_name: None,
        },
        ItemHome::Mount { volume_id } => {
            let Some(volume) = get_volume_manager().get(volume_id) else {
                return Located::Unreachable;
            };
            let root = volume.root().to_path_buf();
            // The registry can be a step ahead of the mount table on a drive
            // that is leaving again. An unreadable table is not evidence of
            // leaving, so only a definite "gone" holds the record back.
            if root_is_listed(&root) == Some(false) {
                return Located::Unreachable;
            }
            // A path `RecordHome` couldn't make relative was stored whole, and
            // joining an absolute path replaces the root, so both shapes land
            // where they belong.
            Located::At {
                surface: Surface::Local,
                path: root.join(&item.path),
                destination: destination.map(|d| root.join(d)),
                drive_name: Some(volume.name().to_string()),
            }
        }
        ItemHome::VolumeSpace { volume_id } => {
            let resolved = get_volume_manager().resolve(volume_id, &item.path).await;
            let is_routed = resolved.is_routed();
            let Some(volume) = resolved.volume.filter(|_| !is_routed) else {
                return Located::Unreachable;
            };
            let drive_name = Some(volume.name().to_string());
            Located::At {
                surface: Surface::Volume(volume),
                path: resolved.path,
                destination,
                drive_name,
            }
        }
    }
}

/// Whether the thing really wears the name its kind must wear.
///
/// The sweep removes and renames, and it follows a RECORD to decide what, so a
/// corrupted or hand-edited store must not become a delete-anything primitive.
/// Each kind gets the strict test for its own shape.
fn name_matches_kind(kind: &ItemKind, path: &Path) -> bool {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return false;
    };
    match kind {
        ItemKind::Temp => name.contains(cmdr_fs::staging::STAGING_TEMP_MARKER),
        ItemKind::FileAside { .. }
        | ItemKind::DisplacedFile { .. }
        | ItemKind::DirOverwriteAside { .. }
        | ItemKind::VolumeAside { .. } => name.contains(cmdr_fs::staging::STAGING_ASIDE_MARKER),
        ItemKind::StagingDir => cmdr_fs::staging::is_staging_dir_name(&name),
    }
}

/// The rules themselves, one arm per kind.
async fn settle_kind(
    surface: &Surface,
    kind: &ItemKind,
    path: &Path,
    destination: Option<&Path>,
    drive_name: Option<&str>,
) -> Outcome {
    match kind {
        ItemKind::Temp => surface.remove_file(path).await.into_outcome(),
        ItemKind::StagingDir => match surface.remove_empty_dir(path).await {
            Removal::Removed => Outcome::Swept,
            Removal::AlreadyGone => Outcome::AlreadyGone,
            // ❗ The `remove_dir` REFUSING is the safety net working, not a
            // failure: what's inside can be the only copy of a move's files.
            Removal::Refused => {
                log::warn!(
                    target: "copy",
                    "an unfinished move's staging folder still holds files, so it stays where it is: {}",
                    path.display()
                );
                announce_leftovers_kept(drive_name, path);
                Outcome::LeftAlone
            }
        },
        ItemKind::FileAside { expected_size, .. } => {
            let Some(destination) = destination else {
                return Outcome::LeftAlone;
            };
            match surface.standing_at(destination).await {
                Standing::Missing => restore(surface, path, destination).await,
                // The one case where the aside is provably redundant: the
                // replacement is a whole file of exactly the size it was
                // supposed to reach.
                Standing::File { size } if size == *expected_size => match surface.standing_at(path).await {
                    Standing::File { .. } => surface.remove_file(path).await.into_outcome(),
                    // A file→folder overwrite sets a whole DIRECTORY aside. The
                    // replacement is provably complete, so the user did get what
                    // they asked for — but removing a directory means recursing,
                    // and ❌ no sweep here recurses over something that was the
                    // user's. It gets a real name instead, and they decide.
                    Standing::Other => recover(surface, path, destination).await,
                    Standing::Missing => Outcome::AlreadyGone,
                    Standing::Unknown => Outcome::Deferred,
                },
                Standing::File { .. } | Standing::Other => recover(surface, path, destination).await,
                Standing::Unknown => Outcome::Deferred,
            }
        }
        // The three kinds whose replacement can't be checked against a number: a
        // directory that filled leaf by leaf, or one a closure materialized. The
        // original is kept whatever is standing there.
        ItemKind::DisplacedFile { .. } | ItemKind::DirOverwriteAside { .. } | ItemKind::VolumeAside { .. } => {
            let Some(destination) = destination else {
                return Outcome::LeftAlone;
            };
            match surface.standing_at(destination).await {
                Standing::Missing => restore(surface, path, destination).await,
                Standing::Unknown => Outcome::Deferred,
                Standing::File { .. } | Standing::Other => recover(surface, path, destination).await,
            }
        }
    }
}

/// Puts an aside back at its own name, the destination having been free.
async fn restore(surface: &Surface, aside: &Path, destination: &Path) -> Outcome {
    match surface.rename_no_replace(aside, destination).await {
        Renaming::Renamed => {
            log::info!(
                target: "copy",
                "put {} back at {}: the copy that was replacing it never finished",
                aside.display(),
                destination.display()
            );
            Outcome::Restored
        }
        // Something appeared between the read and the rename. The bytes are
        // still the user's, so they go beside it rather than over it.
        Renaming::Occupied => recover(surface, aside, destination).await,
        Renaming::Refused => Outcome::Deferred,
    }
}

/// Keeps an aside's bytes under a ` (recovered)` name beside where they belong,
/// because something else is standing at their own name.
///
/// The same convention `DisplacedEntry::keep_as_recovered_sibling` and the
/// volume engine's rescue use, so a person meets one shape of rescued file
/// rather than three.
async fn recover(surface: &Surface, aside: &Path, destination: &Path) -> Outcome {
    let recovered = recovered_sibling(destination);
    let mut candidates = NameCandidates::for_file(&recovered);
    // The bare ` (recovered)` name first; `NameCandidates` starts at ` (1)`.
    let mut candidate = recovered.clone();
    loop {
        match surface.rename_no_replace(aside, &candidate).await {
            Renaming::Renamed => {
                log::warn!(
                    target: "copy",
                    "{} was displaced by something that never finished landing, and its own name is taken, \
                     so it's kept at {}",
                    destination.display(),
                    candidate.display()
                );
                return Outcome::Recovered;
            }
            Renaming::Occupied => {}
            // A read-only destination or a dead mount refuses every candidate
            // identically, so trying more of them buys nothing.
            Renaming::Refused => return Outcome::Deferred,
        }
        if candidates.attempts() >= RESCUE_NAME_ATTEMPTS {
            log::warn!(
                target: "copy",
                "every ` (N)` variant of {} is taken, so {} keeps its scratch name for now",
                recovered.display(),
                aside.display()
            );
            return Outcome::Deferred;
        }
        candidate = candidates.current();
        candidates.advance();
    }
}

/// Which filesystem answers a record's paths, and the four primitives the rules
/// need from it.
enum Surface {
    /// This machine's filesystem.
    Local,
    /// One volume's own namespace.
    Volume(Arc<dyn Volume>),
}

/// What is standing at a path.
///
/// ❗ [`Unknown`](Self::Unknown) is never read as [`Missing`](Self::Missing): a
/// restore aimed at a path nobody could look at would bury whatever is there.
enum Standing {
    Missing,
    /// A regular file, and how big it is.
    File {
        size: u64,
    },
    /// A directory, a symlink, or anything else that isn't a plain file.
    Other,
    /// The question couldn't be answered.
    Unknown,
}

/// How a removal ended.
enum Removal {
    Removed,
    AlreadyGone,
    /// Including a `remove_dir` on a directory that isn't empty, which is the
    /// staging-folder rule working.
    Refused,
}

impl Removal {
    fn into_outcome(self) -> Outcome {
        match self {
            Self::Removed => Outcome::Swept,
            Self::AlreadyGone => Outcome::AlreadyGone,
            Self::Refused => Outcome::Deferred,
        }
    }
}

/// How a no-replace rename ended.
enum Renaming {
    Renamed,
    /// The name is taken, so the caller may try another one.
    Occupied,
    Refused,
}

impl Surface {
    async fn standing_at(&self, path: &Path) -> Standing {
        match self {
            Self::Local => match std::fs::symlink_metadata(path) {
                Ok(meta) if meta.is_file() => Standing::File { size: meta.len() },
                Ok(_) => Standing::Other,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Standing::Missing,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't read what's at {}: {e}", path.display());
                    Standing::Unknown
                }
            },
            Self::Volume(volume) => match volume.get_metadata(path).await {
                Ok(entry) if entry.is_directory || entry.is_symlink => Standing::Other,
                // A backend that can't say how big a file is can't prove a
                // replacement finished, so the aside is kept.
                Ok(entry) => entry.size.map_or(Standing::Other, |size| Standing::File { size }),
                Err(VolumeError::NotFound(_)) => Standing::Missing,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't read what's at {}: {e}", path.display());
                    Standing::Unknown
                }
            },
        }
    }

    async fn remove_file(&self, path: &Path) -> Removal {
        match self {
            Self::Local => match std::fs::remove_file(path) {
                Ok(()) => Removal::Removed,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Removal::AlreadyGone,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't remove the leftover {}: {e}", path.display());
                    Removal::Refused
                }
            },
            Self::Volume(volume) => match volume.delete(path).await {
                Ok(()) => Removal::Removed,
                Err(VolumeError::NotFound(_)) => Removal::AlreadyGone,
                Err(e) => {
                    log::debug!(target: "copy", "couldn't remove the leftover {}: {e}", path.display());
                    Removal::Refused
                }
            },
        }
    }

    /// Removes a directory ONLY if it is empty.
    ///
    /// ❌ Never `remove_dir_all`, on any surface. `std::fs::remove_dir` gives
    /// the empty-only promise in the kernel; a volume has no primitive that
    /// does, so it refuses rather than listing and racing.
    async fn remove_empty_dir(&self, path: &Path) -> Removal {
        match self {
            Self::Local => match std::fs::remove_dir(path) {
                Ok(()) => Removal::Removed,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Removal::AlreadyGone,
                Err(e) => {
                    log::debug!(target: "copy", "leaving the staging folder {}: {e}", path.display());
                    Removal::Refused
                }
            },
            Self::Volume(_) => {
                log::warn!(
                    target: "copy",
                    "leaving the staging folder {}: only the local filesystem can promise an empty-only removal",
                    path.display()
                );
                Removal::Refused
            }
        }
    }

    async fn rename_no_replace(&self, from: &Path, to: &Path) -> Renaming {
        match self {
            // The primitive `overwrite::rename_no_replace` delegates to, named at
            // its home: `overwrite` tracks through this ledger, so reaching back
            // up into it would weld the two modules into a cycle.
            Self::Local => match rename_local_exclusive(from, to) {
                Ok(()) => Renaming::Renamed,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Renaming::Occupied,
                Err(e) => {
                    log::debug!(
                        target: "copy",
                        "couldn't put {} at {}: {e}",
                        from.display(),
                        to.display()
                    );
                    Renaming::Refused
                }
            },
            // `force = false` is the backends' no-clobber rename, the same one
            // `displaced_destination` leans on.
            Self::Volume(volume) => match volume.rename(from, to, false).await {
                Ok(()) => Renaming::Renamed,
                Err(VolumeError::AlreadyExists(_)) => Renaming::Occupied,
                Err(e) => {
                    log::debug!(
                        target: "copy",
                        "couldn't put {} at {}: {e}",
                        from.display(),
                        to.display()
                    );
                    Renaming::Refused
                }
            },
        }
    }
}

/// The `AppHandle` the staging-folder notice is emitted through, stashed once
/// from `lib.rs::setup`.
///
/// The sweep runs off the launch thread and, later, inside a volume-arrival
/// task, so it has no operation and no event sink to speak through. `None`
/// before setup, and in every test, where the notice is simply not emitted.
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Gives the leftover sweep a way to reach the frontend. Startup only.
pub fn init_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

/// Tells the frontend a staging folder is staying where it is, so the files
/// inside it have a findable home rather than being a silent absence at the
/// source.
///
/// Silent when the record doesn't name a drive: the copy names one, and a
/// Mac-internal staging folder (a move whose destination is the boot volume)
/// gets the `warn` alone. Its disk never goes away, so it meets this rule again
/// at the next launch.
fn announce_leftovers_kept(drive_name: Option<&str>, path: &Path) {
    use tauri_specta::Event;
    let (Some(app), Some(volume_name)) = (APP_HANDLE.get(), drive_name) else {
        return;
    };
    let Some(folder_name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return;
    };
    if let Err(e) = (MoveLeftoversKeptEvent {
        volume_name: volume_name.to_string(),
        folder_name,
    })
    .emit(app)
    {
        log::warn!(target: "copy", "couldn't tell the frontend about the kept staging folder: {e}");
    }
}

/// Says what the sweep did, so a leftover that survives leaves a trail.
fn report(tally: &SweepTally) {
    if tally.is_empty() {
        return;
    }
    log::info!(
        target: "copy",
        "recorded transfer leftovers: {} swept, {} already gone, {} restored, {} recovered, \
         {} waiting for their volume, {} left alone",
        tally.swept, tally.already_gone, tally.restored, tally.recovered, tally.deferred, tally.left_alone
    );
}

#[cfg(test)]
#[path = "in_flight_sweep_tests.rs"]
mod tests;
