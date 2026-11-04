//! What a launch, and a drive coming back, do with what the ledger recorded.
//!
//! One set of rules, run from two places: the engine's ledger claim at startup
//! (`Operations::claim_ledger`), and the volume registry's arrival announcement
//! whenever a volume a record waits on shows up. Both run on the claiming
//! engine's host, which they're handed. ❗ They must stay ONE set. A leftover on a USB stick meets whichever
//! of the two happens first, and rules that drifted apart would mean the drive's
//! fate depended on whether it was plugged in at launch.
//!
//! ## The rules, and the one thing none of them may do
//!
//! - A **temp** holds bytes on their way in. Nothing else has them, so it goes.
//! - A **landing** is a complete temp whose way to its name may have been
//!   cleared, so it can be the only copy. ❌ Never removed: its temp is read
//!   first (gone means the landing went through, and the record retires), then
//!   a whole one takes its free name, or a ` (recovered)` one beside whatever
//!   holds it. One at any other size isn't provably the file, so it never takes
//!   the name: it gets a ` (recovered)` name too, with a notice saying where
//!   (left on its scratch name, the name+age reap would take it once the record
//!   retired). A path recorded as both a temp and a landing (a crash inside the
//!   kind change) settles as the landing.
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
use std::sync::Arc;

use super::{ItemHome, ItemKind, Ledger, Record, RecordedTemp, SweepTally, TrackedItem};
use crate::host::EngineHost;
use crate::transfer_sides::root_is_listed;
use crate::types::{KeptLeftover, LeftoversKeptEvent};
use crate::unique_name::{NameCandidates, RESCUE_NAME_ATTEMPTS, recovered_sibling};
use cmdr_fs::log_private::LogPrivate;

/// Settles the records an earlier run left that the local filesystem can answer
/// for on its own, then takes whatever pending volume is already reachable.
///
/// Runs on its own thread (`Operations::claim_ledger` says why nothing waits on it), so
/// blocking filesystem calls inside the async rules are free here.
#[cfg(not(target_family = "wasm"))]
pub(super) fn persisted_orphans(ledger: &Ledger, host: &EngineHost, locals: &[Record]) -> SweepTally {
    // Legal here and only here: this is the sweep's own thread, never a runtime
    // worker, so `block_on` parks nothing the app shares.
    let runtime = host.runtime();
    let mut tally = SweepTally::default();
    for record in &without_superseded_temps(ledger, locals.to_vec()) {
        runtime.block_on(settle_and_record(ledger, host, record, &mut tally));
    }

    // A volume registered before the sweep thread got here (the boot volume, an
    // external disk) can be served right away; everything else waits for its
    // arrival. Asking the registry whether an ID is present is a lock and a hash
    // lookup, so a dead NAS costs nothing and blocks nobody.
    for volume_id in ledger.pending_volume_ids() {
        if host.volumes().get(&volume_id).is_none() {
            continue;
        }
        let claimed = ledger.claim_pending(&volume_id);
        tally.add(runtime.block_on(on_volume(ledger, host, &volume_id, claimed)));
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
#[cfg(not(target_family = "wasm"))]
pub(super) fn on_volume_arrival(ledger: &Ledger, host: &Arc<EngineHost>, volume_id: &str) {
    let claimed = ledger.claim_pending(volume_id);
    if claimed.is_empty() {
        return;
    }
    let volume_id = volume_id.to_string();
    let ledger = ledger.clone();
    let host = Arc::clone(host);
    let runtime = host.runtime();
    runtime.clone().spawn_blocking(move || {
        // The same shape the launch sweep runs in: a blocking context that
        // `block_on`s the volume-backed rules. One shape for both entry points
        // is what keeps the rules themselves a single set. A blocking-pool
        // thread is no runtime worker, so `block_on` is legal here.
        let tally = runtime.block_on(on_volume(&ledger, &host, &volume_id, claimed));
        report(&tally);
    });
}

/// The browser build's launch sweep: it never has persisted records (its ledger
/// runs unpersisted), and it has no thread to `block_on` from.
#[cfg(target_family = "wasm")]
pub(super) fn persisted_orphans(_ledger: &Ledger, _host: &EngineHost, _locals: &[Record]) -> SweepTally {
    SweepTally::default()
}

/// The browser build's arrival sweep: the same rules, awaited on the page's
/// event loop (no blocking pool, no `block_on`).
#[cfg(target_family = "wasm")]
pub(super) fn on_volume_arrival(ledger: &Ledger, host: &Arc<EngineHost>, volume_id: &str) {
    let claimed = ledger.claim_pending(volume_id);
    if claimed.is_empty() {
        return;
    }
    let volume_id = volume_id.to_string();
    let ledger = ledger.clone();
    let host = Arc::clone(host);
    drop(host.runtime().spawn(async move {
        let tally = on_volume(&ledger, &host, &volume_id, claimed).await;
        report(&tally);
    }));
}

/// Settles `records` against the volume that owns them.
///
/// Anything it can't settle goes back to pending, so the next time that volume
/// arrives (this session or a later launch) the sweep tries again. ❌ Never
/// reconnects or authenticates: the volume is used exactly as the registry hands
/// it over.
async fn on_volume(ledger: &Ledger, host: &EngineHost, volume_id: &str, records: Vec<Record>) -> SweepTally {
    let mut tally = SweepTally::default();
    let records = without_superseded_temps(ledger, records);
    // ❗ In the set's order: a landing before the aside its swap left
    // (`in_flight_records.rs::ItemKind`).
    for record in &records {
        settle_and_record(ledger, host, record, &mut tally).await;
    }
    log::debug!(target: "copy", "settled {} recorded leftovers on `{volume_id}`: {tally:?}", records.len());
    tally
}

/// Retires every `Temp` record whose path the same batch also records under
/// another kind, and answers the rest.
///
/// ❗ A kind change writes its new record BEFORE it retires the `Temp` one
/// (`in_flight_temps::hold_for_landing`), so a crash between the two leaves
/// both. The path is then settled once, by the kind that knows it may be the
/// only copy: settled as a temp, it would be deleted. Not counted on its own:
/// the tally counts paths, and the other record answers for this one.
fn without_superseded_temps(ledger: &Ledger, records: Vec<Record>) -> Vec<Record> {
    let (superseded, kept): (Vec<Record>, Vec<Record>) = records.iter().cloned().partition(|record| {
        let Record::Tracked(
            temp @ TrackedItem {
                kind: ItemKind::Temp, ..
            },
        ) = record
        else {
            return false;
        };
        records.iter().any(|other| {
            matches!(other, Record::Tracked(item)
                if !matches!(item.kind, ItemKind::Temp) && item.home == temp.home && item.path == temp.path)
        })
    });
    for record in &superseded {
        ledger.retire_record(record);
    }
    kept
}

/// Settles one record and puts the ledger in step with what happened.
async fn settle_and_record(ledger: &Ledger, host: &EngineHost, record: &Record, tally: &mut SweepTally) {
    match settle(host, record).await {
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
async fn settle(host: &EngineHost, record: &Record) -> Outcome {
    match record {
        Record::Legacy(temp) => settle_legacy(host, temp).await,
        Record::Tracked(item) => settle_tracked(host, item).await,
    }
}

/// A `+` line: a staged partial, and nothing else.
///
/// ❗ Only a `.cmdr-tmp-` name is removed here. The old shared name test also
/// accepted the ASIDE marker, which would have made this the one path that
/// deletes a user's original on sight — an aside says the replacement that
/// displaced it never landed. A kinded record is the only way to ask for
/// anything but a plain removal.
async fn settle_legacy(host: &EngineHost, temp: &RecordedTemp) -> Outcome {
    match temp {
        RecordedTemp::Local(path) => {
            if !is_a_staged_partial(path) {
                return Outcome::LeftAlone;
            }
            Surface::Local.remove_temp(path).await.into_outcome()
        }
        RecordedTemp::OnVolume(temp) => {
            if !is_a_staged_partial(&temp.path) {
                return Outcome::LeftAlone;
            }
            // `resolve`, not `get`: the site is passing a path, and a read-only
            // routed volume (an archive, a git snapshot) is one this sweep must
            // never delete through.
            let resolved = host.volumes().resolve(&temp.volume_id, &temp.path).await;
            let is_routed = resolved.is_routed();
            let Some(volume) = resolved.volume.filter(|_| !is_routed) else {
                return Outcome::Deferred;
            };
            Surface::Volume(volume).remove_temp(&resolved.path).await.into_outcome()
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
            LogPrivate::file_path(path)
        );
    }
    named
}

/// An `A` line: the kind decides.
async fn settle_tracked(host: &EngineHost, item: &TrackedItem) -> Outcome {
    let Located::At {
        surface,
        path,
        destination,
        drive_name,
    } = locate(host, item).await
    else {
        return Outcome::Deferred;
    };
    if !name_matches_kind(&item.kind, &path) {
        log::warn!(
            target: "copy",
            "the in-flight ledger holds a {:?} record whose name isn't that shape, leaving it: {}",
            item.kind,
            LogPrivate::path(&path)
        );
        return Outcome::LeftAlone;
    }
    settle_kind(
        host,
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
async fn locate(host: &EngineHost, item: &TrackedItem) -> Located {
    let destination = item.kind.destination().map(Path::to_path_buf);
    match &item.home {
        ItemHome::Local => Located::At {
            surface: Surface::Local,
            path: item.path.clone(),
            destination,
            drive_name: None,
        },
        ItemHome::Mount { volume_id } => {
            // Plain `get`: only the mount's root is read; the item is reached at `root.join(path)`.
            let Some(volume) = host.volumes().get(volume_id) else {
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
            let resolved = host.volumes().resolve(volume_id, &item.path).await;
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
        ItemKind::Temp | ItemKind::Landing { .. } => name.contains(cmdr_fs::staging::STAGING_TEMP_MARKER),
        ItemKind::FileAside { .. }
        | ItemKind::DisplacedFile { .. }
        | ItemKind::DirOverwriteAside { .. }
        | ItemKind::VolumeAside { .. } => name.contains(cmdr_fs::staging::STAGING_ASIDE_MARKER),
        ItemKind::StagingDir => cmdr_fs::staging::is_staging_dir_name(&name),
    }
}

/// The rules themselves, one arm per kind.
async fn settle_kind(
    host: &EngineHost,
    surface: &Surface,
    kind: &ItemKind,
    path: &Path,
    destination: Option<&Path>,
    drive_name: Option<&str>,
) -> Outcome {
    match kind {
        ItemKind::Temp => surface.remove_temp(path).await.into_outcome(),
        // ❌ Never a removal: from the moment it was recorded the way may have
        // been cleared, and then the temp is the destination's only copy. The
        // temp is read FIRST, so one the landing rename already moved retires
        // rather than deferring at every launch.
        ItemKind::Landing { expected_size, .. } => {
            let Some(destination) = destination else {
                return Outcome::LeftAlone;
            };
            match surface.standing_at(path).await {
                Standing::Missing => Outcome::AlreadyGone,
                Standing::Unknown => Outcome::Deferred,
                Standing::File { size } if size == *expected_size => match surface.standing_at(destination).await {
                    Standing::Missing => finish_landing(surface, path, destination).await,
                    // What a crash before the clearing delete leaves: the name
                    // still holds what it held, and the temp is a spare copy.
                    Standing::File { .. } | Standing::Other => {
                        recover(surface, path, destination, Rescued::Landing).await
                    }
                    Standing::Unknown => Outcome::Deferred,
                },
                // Not the bytes the landing recorded, so nothing proves they're
                // the file: they never take its name, and they don't stay on a
                // scratch one either.
                Standing::File { .. } | Standing::Other => {
                    let Some(kept) = keep_beside(surface, path, destination, Rescued::UnprovenLanding).await else {
                        return Outcome::Deferred;
                    };
                    announce_landing_kept(host, drive_name, destination, &kept);
                    Outcome::Recovered
                }
            }
        }
        ItemKind::StagingDir => match surface.remove_empty_dir(path).await {
            Removal::Removed => Outcome::Swept,
            Removal::AlreadyGone => Outcome::AlreadyGone,
            // ❗ The `remove_dir` REFUSING is the safety net working, not a
            // failure: what's inside can be the only copy of a move's files.
            Removal::Refused => {
                log::warn!(
                    target: "copy",
                    "an unfinished move's staging folder still holds files, so it stays where it is: {}",
                    LogPrivate::dir_path(path)
                );
                announce_leftovers_kept(host, drive_name, path);
                Outcome::LeftAlone
            }
        },
        ItemKind::FileAside { expected_size, .. } => {
            let Some(destination) = destination else {
                return Outcome::LeftAlone;
            };
            let aside = match surface.standing_at(path).await {
                Standing::Missing => return Outcome::AlreadyGone,
                Standing::Unknown => return Outcome::Deferred,
                aside => aside,
            };
            match surface.standing_at(destination).await {
                Standing::Missing => restore(surface, path, destination).await,
                // The one case where the aside is provably redundant: the
                // replacement is a whole file of exactly the size it was
                // supposed to reach.
                Standing::File { size } if size == *expected_size => match aside {
                    Standing::File { .. } => surface.remove_file(path).await.into_outcome(),
                    // A file→folder overwrite sets a whole DIRECTORY aside. The
                    // replacement is provably complete, so the user did get what
                    // they asked for — but removing a directory means recursing,
                    // and ❌ no sweep here recurses over something that was the
                    // user's. It gets a real name instead, and they decide.
                    _ => recover(surface, path, destination, Rescued::Aside).await,
                },
                Standing::File { .. } | Standing::Other => recover(surface, path, destination, Rescued::Aside).await,
                Standing::Unknown => Outcome::Deferred,
            }
        }
        // The three kinds whose replacement can't be checked against a number: a
        // directory that filled leaf by leaf, or one a closure materialized. The
        // original is kept whatever is standing there. The aside is read FIRST,
        // as a landing's temp is: one whose rename never happened (a rename
        // aside the transport cut off, recorded because it may have) retires
        // rather than deferring at every launch.
        ItemKind::DisplacedFile { .. } | ItemKind::DirOverwriteAside { .. } | ItemKind::VolumeAside { .. } => {
            let Some(destination) = destination else {
                return Outcome::LeftAlone;
            };
            match surface.standing_at(path).await {
                Standing::Missing => return Outcome::AlreadyGone,
                Standing::Unknown => return Outcome::Deferred,
                Standing::File { .. } | Standing::Other => {}
            }
            match surface.standing_at(destination).await {
                Standing::Missing => restore(surface, path, destination).await,
                Standing::Unknown => Outcome::Deferred,
                Standing::File { .. } | Standing::Other => recover(surface, path, destination, Rescued::Aside).await,
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
                LogPrivate::path(aside),
                LogPrivate::path(destination)
            );
            Outcome::Restored
        }
        // Something appeared between the read and the rename. The bytes are
        // still the user's, so they go beside it rather than over it.
        Renaming::Occupied => recover(surface, aside, destination, Rescued::Aside).await,
        Renaming::Refused => Outcome::Deferred,
    }
}

/// Gives a landing's temp the name it was on its way to, the destination having
/// been free: the landing rename the crash (or the give-up) cut off.
async fn finish_landing(surface: &Surface, temp: &Path, destination: &Path) -> Outcome {
    match surface.rename_no_replace(temp, destination).await {
        Renaming::Renamed => {
            log::info!(
                target: "copy",
                "landed {} at {}: its landing was cut off after the way was cleared",
                LogPrivate::file_path(temp),
                LogPrivate::file_path(destination)
            );
            Outcome::Restored
        }
        // Something took the name between the read and the rename: the new
        // bytes go beside it, ❌ never over it.
        Renaming::Occupied => recover(surface, temp, destination, Rescued::Landing).await,
        Renaming::Refused => Outcome::Deferred,
    }
}

/// Whose bytes [`recover`] is keeping, for the line it logs.
#[derive(Clone, Copy)]
enum Rescued {
    /// An aside: bytes that were already the user's.
    Aside,
    /// A landing's temp: the complete new bytes a transfer was putting there.
    Landing,
    /// A landing's temp that isn't the size it was recorded at.
    UnprovenLanding,
}

/// Keeps an aside's (or a landing's) bytes under a ` (recovered)` name beside
/// where they belong, because something else is standing at their own name.
///
/// The same convention `DisplacedEntry::keep_as_recovered_sibling` and the
/// volume engine's rescue use, so a person meets one shape of rescued file
/// rather than three.
async fn recover(surface: &Surface, aside: &Path, destination: &Path, rescued: Rescued) -> Outcome {
    match keep_beside(surface, aside, destination, rescued).await {
        Some(_) => Outcome::Recovered,
        None => Outcome::Deferred,
    }
}

/// [`recover`], answering the name the bytes are kept under, or `None` when
/// they couldn't move and stay where they are (the record kept for a later try).
async fn keep_beside(surface: &Surface, aside: &Path, destination: &Path, rescued: Rescued) -> Option<PathBuf> {
    let recovered = recovered_sibling(destination);
    let mut candidates = NameCandidates::for_file(&recovered);
    // The bare ` (recovered)` name first; `NameCandidates` starts at ` (1)`.
    let mut candidate = recovered.clone();
    loop {
        match surface.rename_no_replace(aside, &candidate).await {
            Renaming::Renamed => {
                match rescued {
                    Rescued::Aside => log::warn!(
                        target: "copy",
                        "{} was displaced by something that never finished landing, and its own name is taken, \
                         so it's kept at {}",
                        LogPrivate::path(destination),
                        LogPrivate::path(&candidate)
                    ),
                    Rescued::Landing => log::warn!(
                        target: "copy",
                        "the new bytes for {} never finished landing, and the name is taken, so they're kept at {}",
                        LogPrivate::file_path(destination),
                        LogPrivate::file_path(&candidate)
                    ),
                    Rescued::UnprovenLanding => log::warn!(
                        target: "copy",
                        "an unfinished landing for {} isn't the size it was recorded at, so it's kept at {}",
                        LogPrivate::file_path(destination),
                        LogPrivate::file_path(&candidate)
                    ),
                }
                return Some(candidate);
            }
            Renaming::Occupied => {}
            // A read-only destination or a dead mount refuses every candidate
            // identically, so trying more of them buys nothing.
            Renaming::Refused => return None,
        }
        if candidates.attempts() >= RESCUE_NAME_ATTEMPTS {
            log::warn!(
                target: "copy",
                "every ` (N)` variant of {} is taken, so {} keeps its scratch name for now",
                LogPrivate::path(&recovered),
                LogPrivate::path(aside)
            );
            return None;
        }
        candidate = candidates.current();
        candidates.advance();
    }
}

/// Tells the frontend a staging folder is staying where it is, so the files
/// inside it have a findable home rather than being a silent absence at the
/// source.
///
/// The sweep runs off the launch thread and, later, inside a volume-arrival
/// task, so it has no operation and no event sink to speak through: the notice
/// is an engine-wide fact (`EngineEvents::leftovers_kept`).
///
/// Silent when the record doesn't name a drive: the copy names one, and a
/// Mac-internal staging folder (a move whose destination is the boot volume)
/// gets the `warn` alone. Its disk never goes away, so it meets this rule again
/// at the next launch.
fn announce_leftovers_kept(host: &EngineHost, drive_name: Option<&str>, path: &Path) {
    let Some(volume_name) = drive_name else {
        return;
    };
    let Some(folder_name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return;
    };
    host.events().leftovers_kept(LeftoversKeptEvent {
        volume_name: volume_name.to_string(),
        kept: KeptLeftover::StagingFolder { folder_name },
    });
}

/// Tells the frontend an unfinished landing it couldn't prove whole is kept
/// under `kept`, beside `destination`, the way [`announce_leftovers_kept`]
/// tells it about a staging folder (and silent for a Mac-internal path, which
/// names no drive).
fn announce_landing_kept(host: &EngineHost, drive_name: Option<&str>, destination: &Path, kept: &Path) {
    let Some(volume_name) = drive_name else {
        return;
    };
    let name = |path: &Path| path.file_name().map(|n| n.to_string_lossy().into_owned());
    let (Some(file_name), Some(kept_name)) = (name(destination), name(kept)) else {
        return;
    };
    host.events().leftovers_kept(LeftoversKeptEvent {
        volume_name: volume_name.to_string(),
        kept: KeptLeftover::UnfinishedLanding { file_name, kept_name },
    });
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

#[path = "in_flight_sweep_surface.rs"]
mod surface;
use surface::{Removal, Renaming, Standing, Surface};

#[cfg(test)]
#[path = "in_flight_sweep_tests.rs"]
mod tests;
