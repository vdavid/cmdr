//! The `.cmdr-tmp-*` partials Cmdr is writing RIGHT NOW, kept in two ledgers.
//!
//! - **The operation's own**, `WriteOperationState::in_flight_temps`, in memory.
//!   It answers "what did this operation's abandoned tasks leave behind?" while
//!   the operation is still alive, and its sweep is
//!   `transfer::volume::cleanup::clean_abandoned_staged_writes`.
//! - **A process-wide file in the app data dir**, so the answer survives the
//!   process. An in-memory list dies with a force-quit or a crash — exactly the
//!   two endings that leave partials — and the directory scan that would
//!   otherwise find them (`reap_stale_transfer_temps`)
//!   only runs when something copies into that same directory, and only for
//!   leftovers over an hour old. So a quit-orphaned temp could sit there for
//!   days. [`sweep_persisted_orphans`] clears the recorded ones at the next
//!   launch instead.
//!
//! **No age gate on the persisted sweep, and that's safe.** The hour the
//! directory scan waits exists to protect a temp a CONCURRENT Cmdr is streaming
//! into. These paths aren't guesses from a name pattern: each one is a path this
//! app recorded when it minted the UUID in it, so no other instance can own it,
//! and the instance lock already keeps two processes off one data dir. What is
//! recorded and still on disk at startup is ours and is garbage.
//!
//! ## A record names its path space, not just its path
//!
//! A staged partial is a sibling of the file being written, so it lives wherever
//! the DESTINATION lives: on the local filesystem for a local copy, and in an
//! SMB / SFTP / WebDAV / MTP volume's own path space for a transfer to one. A
//! path alone can't tell those apart, so every record carries a [`TempHome`]:
//! either the local filesystem, or a volume ID. The sweep then deletes through
//! the volume that wrote it (`Volume::delete`), never through `std::fs`.
//!
//! **A volume the sweep can't reach keeps its record.** The startup sweep runs
//! before `init_volume_manager`, and a NAS registers later still (or not at all
//! this session), so a volume-borne record is normally DEFERRED: re-recorded on
//! disk, held in [`Store::pending`], and acted on the moment that volume ID
//! arrives in the registry (`VolumeManager::on_volume_arrival`). ❌ The sweep
//! never dials, mounts, or authenticates anything to reach a volume: a launch
//! that blocks on a dead mount, or that pops a NAS password box, is worse than
//! the leftover it was chasing. A record whose volume never comes back rides
//! along to the next launch, which costs one short line in the log file.
//!
//! ## Granularity: one append per change, on an open handle, never fsynced
//!
//! This sits on the per-file hot path — a 2 000-file local copy hits it 4 000
//! times — so the write has to be about as cheap as a write can be:
//!
//! - **One `write(2)` per change**, appending a single line to a handle held
//!   open for the session. ❌ No rewrite-the-whole-file (a create + write +
//!   rename per change measured at **+0.4 ms per file**, tripling a
//!   many-small-files copy), ❌ no per-change path resolution.
//! - **❌ No `fsync`.** An fsync here would cost milliseconds per file and turn
//!   a copy into a flush-per-file crawl. An unsynced write is already in the
//!   page cache, so the record survives the process dying — a quit, a panic, a
//!   `SIGKILL`, which is every ending this exists for. A power loss can lose
//!   it, and then the hour-gated directory scan is the backstop; that's the
//!   right trade, since a power loss can equally lose the temp's own directory
//!   entry and leave nothing to sweep.
//! - **Compaction is cheap and unconditional**: past [`COMPACT_ABOVE_BYTES`] the
//!   log is rewritten down to just what's recorded (a handful of paths), so
//!   nothing accumulates across a long copy or a long session.
//!
//! The format is one line per record: `+` or `-`, then the record as a JSON
//! value (so a newline in a filename can't forge a record). A bare JSON STRING
//! is a local path; a JSON OBJECT (`{"volume_id":…,"path":…}`) is a path in that
//! volume's space. A trailing torn line — the process died mid-`write` — is
//! ignored on read. A path that isn't valid UTF-8 can't be written as JSON and
//! goes unrecorded; the hour-gated directory scan remains its backstop.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Once};

use serde::{Deserialize, Serialize};

use super::state::WriteOperationState;
use crate::file_system::volume::VolumeError;
use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;

/// The persisted log's file name inside the app data dir.
const STORE_FILENAME: &str = "in-flight-temps.log";

/// Rewrite the log down to just what's recorded once it has grown past this.
/// Small enough that a session never carries a big file, large enough that a
/// serial copy doesn't rewrite after every single file (~50 files' worth).
const COMPACT_ABOVE_BYTES: u64 = 8 * 1024;

/// Which path space a partial lives in, so a later launch reaches it through the
/// same one that wrote it.
///
/// ❌ Never guess this from the path. An absolute-looking path means one thing
/// on the local filesystem and quite another inside a share's own namespace, and
/// resolving the wrong one is how a sweep silently does nothing on a NAS — or,
/// worse, removes a local file the ledger never meant.
#[derive(Clone, Copy, Debug)]
pub(super) enum TempHome<'a> {
    /// The local filesystem, addressed by an absolute OS path. What
    /// `overwrite.rs`'s synchronous staging writes.
    LocalFs,
    /// One volume's own path space, keyed by the volume ID — the identity that
    /// survives a remount, so a record written last week still names the same
    /// share today.
    Volume(&'a str),
}

/// One partial as the log holds it.
///
/// Serialized untagged, which is what makes the two shapes tell themselves
/// apart on disk and keeps a local record byte-identical to what every earlier
/// build wrote.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
enum RecordedTemp {
    /// A path on the local filesystem.
    ///
    /// This is also what a BARE-PATH line means: that's all the old one-field
    /// format could express correctly, since a volume path recorded without its
    /// volume resolves against the local filesystem — usually as nothing, and
    /// occasionally as somebody else's file.
    Local(PathBuf),
    /// A path in one volume's own space.
    OnVolume(VolumeTemp),
}

/// A partial living in a volume's path space.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct VolumeTemp {
    /// The volume ID the path belongs to (`cmdr_fs::volume::ids`).
    volume_id: String,
    /// The path, in that volume's own space.
    path: PathBuf,
}

/// What one sweep did, so the outcome is observable rather than inferred from
/// files that quietly aren't there any more.
///
/// Returned to whoever can wait ([`SweepHandle::wait`]) and logged in one line
/// either way. **Every recorded path lands in exactly one counter**, which is
/// what keeps a silent no-op impossible: the numbers have to add up to what the
/// ledger held.
///
// DEFAULT-OK: all-zero is the truthful state of a sweep that hasn't visited
// anything yet, and it's what a launch with an empty ledger honestly reports.
// The counts are this run's own tallies, not a claim about the disk.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepTally {
    /// Partials this sweep removed.
    pub swept: usize,
    /// Records whose file was already gone — the common, healthy case (the temp
    /// landed under its real name before the crash).
    pub already_gone: usize,
    /// Records left for later because their volume isn't reachable yet.
    pub deferred: usize,
    /// Records the sweep refused to act on (a path that isn't one of our
    /// scratch files) or couldn't remove.
    pub left_alone: usize,
}

impl SweepTally {
    /// Whether this sweep found anything at all worth saying out loud.
    fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// Folds another sweep's counts in, so one launch reports one line.
    fn add(&mut self, other: Self) {
        self.swept += other.swept;
        self.already_gone += other.already_gone;
        self.deferred += other.deferred;
        self.left_alone += other.left_alone;
    }
}

/// The process-wide half: the open log, and what it claims exists.
///
// DEFAULT-OK: the zero value is the truthful pre-startup state — no log open
// yet, nothing written to it, and nothing recorded, which is exactly where a
// process begins. It claims nothing about the disk.
#[derive(Default)]
struct Store {
    /// `None` until [`init_and_sweep`] runs, which is also what keeps unit
    /// tests from touching disk unless they ask to.
    log: Option<File>,
    /// Bytes appended since the last truncation, tracked here so the compaction
    /// check costs no syscall.
    logged_bytes: u64,
    /// Every record the log currently claims exists: this session's in-flight
    /// partials plus the deferred orphans below. Compaction rewrites the log
    /// from exactly this set, so anything missing here is forgotten on disk.
    recorded: BTreeSet<RecordedTemp>,
    /// The deferred orphans: recorded by an EARLIER run, on a volume that hasn't
    /// shown up in the registry yet. A subset of [`recorded`](Self::recorded) —
    /// never this session's own live partials, which nothing may sweep.
    pending: BTreeSet<VolumeTemp>,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(|| Mutex::new(Store::default()));

/// Guards the one-time install of the volume-arrival listener.
static ARRIVAL_LISTENER: Once = Once::new();

/// Records `temp` as a partial this operation is writing, in both ledgers.
///
/// Call before the first byte can land there, and pair with [`deregister`] the
/// moment the file stops being a partial.
///
/// `home` is where the path lives. `None` means the caller couldn't say — a
/// volume transfer whose operation never named its destination volume, which
/// production never does. The operation's own ledger still gets the path (its
/// sweep deletes through the operation's own volume handle, so it needs no ID),
/// but nothing is persisted: a path recorded without its path space is one the
/// next launch could resolve against the wrong filesystem.
pub(super) fn register(state: &WriteOperationState, temp: &Path, home: Option<TempHome<'_>>) {
    state.in_flight_temps.lock_ignore_poison().push(temp.to_path_buf());
    let Some(record) = record_for(temp, home) else {
        log::debug!(
            target: "copy",
            "not persisting the in-flight temp {}: the operation didn't name the volume it writes to",
            temp.display()
        );
        return;
    };
    let mut store = STORE.lock_ignore_poison();
    store.recorded.insert(record.clone());
    append(&mut store, b'+', &record);
}

/// Stops tracking `temp`: it landed under its real name, or it's gone. `home`
/// must be the one [`register`] was given.
pub(super) fn deregister(state: &WriteOperationState, temp: &Path, home: Option<TempHome<'_>>) {
    state.in_flight_temps.lock_ignore_poison().retain(|p| p != temp);
    let Some(record) = record_for(temp, home) else {
        return;
    };
    let mut store = STORE.lock_ignore_poison();
    store.recorded.remove(&record);
    append(&mut store, b'-', &record);
    compact_if_large(&mut store);
}

/// The log-shaped record for a path in `home`.
fn record_for(temp: &Path, home: Option<TempHome<'_>>) -> Option<RecordedTemp> {
    match home? {
        TempHome::LocalFs => Some(RecordedTemp::Local(temp.to_path_buf())),
        TempHome::Volume(volume_id) => Some(RecordedTemp::OnVolume(VolumeTemp {
            volume_id: volume_id.to_string(),
            path: temp.to_path_buf(),
        })),
    }
}

/// Pushes whatever the ledger is holding out to the kernel, so the next launch's
/// sweep can see it. The quit teardown's last act before the process ends.
///
/// Today every record already reaches the kernel inside [`register`] — the log is
/// a bare `File`, so there is no user-space buffer to lose — and this is the
/// explicit fence that keeps it that way: if the handle ever gains a `BufWriter`,
/// the quit path won't silently start dropping the last partials it recorded.
/// Deliberately NOT an `fsync`; see the module docs on why a power loss is the
/// directory scan's problem, not this ledger's.
pub fn flush() {
    let mut store = STORE.lock_ignore_poison();
    let Some(log) = &mut store.log else {
        return;
    };
    if let Err(e) = log.flush() {
        log::warn!(target: "copy", "couldn't flush the in-flight temp ledger before exit: {e}");
    }
}

/// The background sweep [`init_and_sweep`] started, and the only way to learn
/// that it has finished.
///
/// **The launch path drops it.** Waiting there is the one thing this must never
/// do: a recorded partial can sit on a Finder-mounted NAS that is no longer
/// answering, and `unlink` on a dead mount blocks for a minute or two, which
/// reads to the user as an app that won't launch. Whoever can afford to wait —
/// a test asserting on what the sweep removed — calls [`SweepHandle::wait`]
/// instead of racing a wall-clock deadline it can only lose under load.
///
/// The handle stays in the signature on every build rather than behind a
/// `cfg(test)`: a function whose shape changes between the app and its tests
/// is a function the tests no longer describe.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the launch path drops the handle; only a caller that can wait joins it"
    )
)]
pub struct SweepHandle(Option<std::thread::JoinHandle<SweepTally>>);

#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the launch path drops the handle; only a caller that can wait joins it"
    )
)]
impl SweepHandle {
    /// Blocks until the sweep has visited every path the ledger recorded, and
    /// answers what it did.
    ///
    /// ❌ Never call this from a launch path; see the type docs for why.
    pub fn wait(self) -> SweepTally {
        let Some(handle) = self.0 else {
            return SweepTally::default();
        };
        handle.join().unwrap_or_else(|_| {
            log::warn!(target: "copy", "the orphaned-partial sweep panicked; some partials may remain");
            SweepTally::default()
        })
    }
}

/// Points the persisted ledger at the app data dir and clears whatever an
/// earlier run left behind. Call once at startup, before any copy can start.
///
/// Only the app-data-dir work happens inline. **The deletes go to their own
/// thread** (see [`SweepHandle`] for why nothing on the launch path waits on
/// it): the records it acts on were already retired from the log by the
/// truncate below, so a new copy can start underneath it safely.
///
/// A record naming a volume that isn't in the registry yet is re-recorded and
/// held pending instead, which is why the truncate can't simply throw the log
/// away.
pub fn init_and_sweep(data_dir: &Path) -> SweepHandle {
    let path = data_dir.join(STORE_FILENAME);
    let recorded = read_recorded(&path);

    // Truncating as we open is what retires the records we're about to act on:
    // sweeping twice would be harmless, but a log that only ever grew wouldn't.
    match File::options().create(true).append(true).open(&path) {
        Ok(log) => {
            // `truncate` isn't legal alongside `append`; retire the replayed
            // records with an explicit `ftruncate` instead.
            let _ = log.set_len(0);
            let mut store = STORE.lock_ignore_poison();
            store.log = Some(log);
            store.logged_bytes = 0;
        }
        Err(e) => log::warn!(
            target: "copy",
            "couldn't open the in-flight temp ledger at {}: {e}. A copy interrupted this session will \
             leave its partial for the next transfer into that directory to reap.",
            path.display()
        ),
    }

    if recorded.is_empty() {
        return SweepHandle(None);
    }

    // Split by path space. The local ones this thread can act on directly; a
    // volume's can only be reached through its volume, which at this point in
    // the launch usually isn't registered yet.
    let mut locals: Vec<PathBuf> = Vec::new();
    let mut on_volumes: Vec<VolumeTemp> = Vec::new();
    for record in recorded {
        match record {
            RecordedTemp::Local(path) => locals.push(path),
            RecordedTemp::OnVolume(temp) => on_volumes.push(temp),
        }
    }
    if !on_volumes.is_empty() {
        defer(on_volumes);
        // Installed only once a launch has something waiting on a volume, so an
        // app whose ledger is clean (the overwhelming case) carries no listener.
        ARRIVAL_LISTENER.call_once(|| {
            get_volume_manager().on_volume_arrival(sweep_arrived_volume);
        });
    }

    match std::thread::Builder::new()
        .name("cmdr-temp-sweep".to_string())
        .spawn(move || sweep_persisted_orphans(&locals))
    {
        Ok(sweep) => SweepHandle(Some(sweep)),
        Err(e) => {
            log::warn!(target: "copy", "couldn't start the orphaned-partial sweep: {e}");
            SweepHandle(None)
        }
    }
}

/// Re-records `temps` and holds them until their volumes show up.
///
/// The re-record is what carries them past the truncate in [`init_and_sweep`]:
/// a record the sweep couldn't act on has to outlive the launch that replayed
/// it, or a NAS orphan is forgotten by the one ledger that knew about it.
fn defer(temps: Vec<VolumeTemp>) {
    let mut store = STORE.lock_ignore_poison();
    for temp in temps {
        let record = RecordedTemp::OnVolume(temp.clone());
        store.recorded.insert(record.clone());
        append(&mut store, b'+', &record);
        store.pending.insert(temp);
    }
}

/// Removes the local partials an earlier run recorded and never finished, then
/// takes whatever pending volume is already reachable.
///
/// Skips anything that isn't one of ours by name: the ledger should only ever
/// hold `.cmdr-tmp-*` paths, and a delete driven by a file is worth one cheap
/// check that the file wasn't tampered with. A path that's already gone (the
/// normal case — it landed under its real name) is counted, not silent.
fn sweep_persisted_orphans(locals: &[PathBuf]) -> SweepTally {
    let mut tally = SweepTally::default();
    for temp in locals {
        if !is_one_of_ours(temp) {
            tally.left_alone += 1;
            continue;
        }
        match std::fs::remove_file(temp) {
            Ok(()) => tally.swept += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => tally.already_gone += 1,
            Err(e) => {
                tally.left_alone += 1;
                log::debug!(
                    target: "copy",
                    "couldn't sweep the orphaned transfer partial {}: {e}",
                    temp.display()
                );
            }
        }
    }

    // A volume registered before the sweep thread got here (the boot volume, an
    // external disk) can be served right away; everything else waits for its
    // arrival. Asking the registry whether an ID is present is a lock and a hash
    // lookup, so a dead NAS costs nothing and blocks nobody.
    for volume_id in pending_volume_ids() {
        if get_volume_manager().get(&volume_id).is_none() {
            continue;
        }
        let claimed = claim_pending(&volume_id);
        tally.add(tauri::async_runtime::block_on(sweep_on_volume(&volume_id, claimed)));
    }
    // ASSIGNED, not added: a record a reachable volume then refused is already
    // back in `pending`, so counting both would report it twice. What's still
    // waiting when the launch finishes is the one honest number.
    tally.deferred = pending_count();

    report(&tally);
    tally
}

/// Says what the sweep did, so a partial that survives leaves a trail.
fn report(tally: &SweepTally) {
    if tally.is_empty() {
        return;
    }
    log::info!(
        target: "copy",
        "recorded transfer partials: {} swept, {} already gone, {} waiting for their volume, {} left alone",
        tally.swept, tally.already_gone, tally.deferred, tally.left_alone
    );
}

/// The volume registry took on `volume_id`: clear whatever the ledger has been
/// holding for it.
///
/// Cheap when there's nothing waiting, which is every registration after the
/// first launch that had an orphan. The deletes go to a task, so a registration
/// never waits on a share.
fn sweep_arrived_volume(volume_id: &str) {
    let claimed = claim_pending(volume_id);
    if claimed.is_empty() {
        return;
    }
    let volume_id = volume_id.to_string();
    tauri::async_runtime::spawn(async move {
        let tally = sweep_on_volume(&volume_id, claimed).await;
        report(&tally);
    });
}

/// Removes `temps` through the volume that owns them.
///
/// Anything it can't remove goes back to pending, so the next time that volume
/// arrives (this session or a later launch) the sweep tries again. ❌ Never
/// reconnects or authenticates: the volume is used exactly as the registry hands
/// it over.
async fn sweep_on_volume(volume_id: &str, temps: Vec<VolumeTemp>) -> SweepTally {
    let mut tally = SweepTally::default();
    for temp in temps {
        if !is_one_of_ours(&temp.path) {
            tally.left_alone += 1;
            retire(&RecordedTemp::OnVolume(temp));
            continue;
        }
        // `resolve`, not `get`: the site is passing a path, and a read-only
        // routed volume (an archive, a git snapshot) is one this sweep must
        // never delete through.
        let resolved = get_volume_manager().resolve(volume_id, &temp.path).await;
        let is_routed = resolved.is_routed();
        let Some(volume) = resolved.volume.filter(|_| !is_routed) else {
            tally.deferred += 1;
            defer(vec![temp]);
            continue;
        };
        match volume.delete(&resolved.path).await {
            Ok(()) => {
                tally.swept += 1;
                retire(&RecordedTemp::OnVolume(temp));
            }
            Err(VolumeError::NotFound(_)) => {
                tally.already_gone += 1;
                retire(&RecordedTemp::OnVolume(temp));
            }
            Err(e) => {
                log::debug!(
                    target: "copy",
                    "couldn't sweep the orphaned transfer partial {} on `{volume_id}`: {e}",
                    temp.path.display()
                );
                tally.deferred += 1;
                defer(vec![temp]);
            }
        }
    }
    tally
}

/// Whether the ledger's entry really names one of our scratch files.
///
/// The sweep deletes files and follows a file to decide which, so a corrupted
/// or hand-edited store must not become a delete-anything primitive.
fn is_one_of_ours(temp: &Path) -> bool {
    let is_ours = temp
        .file_name()
        .is_some_and(|n| cmdr_fs::staging::is_staging_temp_name(&n.to_string_lossy()));
    if !is_ours {
        log::warn!(
            target: "copy",
            "in-flight temp ledger holds a path that isn't one of our scratch files, leaving it: {}",
            temp.display()
        );
    }
    is_ours
}

/// Drops a record the sweep is done with, on disk too.
fn retire(record: &RecordedTemp) {
    let mut store = STORE.lock_ignore_poison();
    store.recorded.remove(record);
    append(&mut store, b'-', record);
}

/// The volume IDs the ledger is currently waiting on.
fn pending_volume_ids() -> BTreeSet<String> {
    STORE
        .lock_ignore_poison()
        .pending
        .iter()
        .map(|temp| temp.volume_id.clone())
        .collect()
}

/// How many records are still waiting for a volume.
fn pending_count() -> usize {
    STORE.lock_ignore_poison().pending.len()
}

/// Takes the pending records for `volume_id`, so exactly one sweep acts on each.
///
/// They stay in [`Store::recorded`] until a sweep retires them: a claim that
/// fails has to leave the log still claiming the file exists.
fn claim_pending(volume_id: &str) -> Vec<VolumeTemp> {
    let mut store = STORE.lock_ignore_poison();
    let claimed: Vec<VolumeTemp> = store
        .pending
        .iter()
        .filter(|temp| temp.volume_id == volume_id)
        .cloned()
        .collect();
    for temp in &claimed {
        store.pending.remove(temp);
    }
    claimed
}

/// Replays the log and returns what an earlier session left in flight.
///
/// Every failure means "nothing recorded": a missing file is the normal
/// clean-exit case, and an unreadable one is not worth failing a launch over. A
/// line that doesn't parse is skipped rather than aborting the replay — the
/// last one can be a torn `write` from the process dying, which is exactly the
/// case this whole ledger exists for.
fn read_recorded(path: &Path) -> Vec<RecordedTemp> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut live: BTreeSet<RecordedTemp> = BTreeSet::new();
    for line in contents.lines() {
        let Some((op, encoded)) = line.split_at_checked(1) else {
            continue;
        };
        let Ok(temp) = serde_json::from_str::<RecordedTemp>(encoded) else {
            continue;
        };
        match op {
            "+" => {
                live.insert(temp);
            }
            "-" => {
                live.remove(&temp);
            }
            _ => {}
        }
    }
    live.into_iter().collect()
}

/// Appends one record. See the module docs for the format and why there's no
/// fsync.
///
/// Best-effort: a record we couldn't write costs a leftover the hour-gated
/// directory scan still catches, so it must never fail a copy.
fn append(store: &mut Store, op: u8, record: &RecordedTemp) {
    let Some(log) = &mut store.log else {
        return;
    };
    let Ok(encoded) = serde_json::to_string(record) else {
        // Not valid UTF-8, so it can't be a JSON string. Rare enough to accept:
        // the directory scan is this path's backstop.
        log::debug!(target: "copy", "not recording an in-flight temp: its name isn't UTF-8");
        return;
    };
    let mut line = Vec::with_capacity(encoded.len() + 2);
    line.push(op);
    line.extend_from_slice(encoded.as_bytes());
    line.push(b'\n');
    match log.write_all(&line) {
        Ok(()) => store.logged_bytes += line.len() as u64,
        Err(e) => log::debug!(target: "copy", "couldn't record an in-flight temp: {e}"),
    }
}

/// Rewrites the log down to just what's recorded once it has grown past
/// [`COMPACT_ABOVE_BYTES`], so a long copy can't grow an unbounded file.
///
/// ❌ Don't gate this on "nothing is in flight": the concurrent cross-volume
/// driver keeps a window open for the whole transfer, so an idle-only rule would
/// let a 100k-file copy append megabytes before it ever got a chance to run.
/// Rewriting the recorded set (a handful of paths) costs one truncate and one
/// write every ~50 files.
fn compact_if_large(store: &mut Store) {
    if store.logged_bytes < COMPACT_ABOVE_BYTES {
        return;
    }
    let recorded: Vec<RecordedTemp> = store.recorded.iter().cloned().collect();
    let Some(log) = &mut store.log else {
        return;
    };
    // The handle is in append mode, so writes go to the new end without a seek.
    if let Err(e) = log.set_len(0) {
        log::debug!(target: "copy", "couldn't compact the in-flight temp ledger: {e}");
        return;
    }
    store.logged_bytes = 0;
    for record in &recorded {
        append(store, b'+', record);
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use super::{File, PathBuf, RecordedTemp, STORE};
    use crate::ignore_poison::IgnorePoison;
    use std::sync::{Mutex, MutexGuard};

    /// Serializes every test that installs its own ledger into [`STORE`].
    ///
    /// [`STORE`] is ONE singleton for the whole test binary, and installing a
    /// log into it redirects every `register` in the process, from any thread,
    /// into that file. Two tests doing it at once is how one test's records
    /// land in another's log, and how a startup-sweep fixture ends up replaying
    /// an empty log and never sweeping at all. Both shapes reproduced at
    /// `--test-threads=2` (47 failures in 60 runs) before this lock existed.
    static SINGLE_FILE: Mutex<()> = Mutex::new(());

    /// Exclusive use of the process-wide ledger for the length of the guard.
    ///
    /// Hold it across the WHOLE test body, ❌ never just the part that writes:
    /// the moment it drops, another test may install its log and take over
    /// every `register` this one still had coming.
    pub(in crate::file_system::write_operations) struct StoreGuard {
        previous: Option<File>,
        previous_bytes: u64,
        // Declared last so it's released after `Drop` has put the singleton
        // back: the next test in line must never see this one's log.
        _single_file: MutexGuard<'static, ()>,
    }

    /// Takes the ledger for the length of the guard and leaves it EMPTY: no log
    /// open, nothing recorded, which is exactly where a process begins.
    pub(in crate::file_system::write_operations) fn take_store() -> StoreGuard {
        // Poison here just means an earlier test panicked while holding it. The
        // singleton was restored by that guard's `Drop` on the way out, so the
        // state is sound and the next test deserves a real verdict, not an
        // unwrap on someone else's failure.
        let single_file = SINGLE_FILE.lock_ignore_poison();
        let mut store = STORE.lock_ignore_poison();
        let previous = store.log.take();
        let previous_bytes = std::mem::take(&mut store.logged_bytes);
        store.recorded.clear();
        store.pending.clear();
        StoreGuard {
            previous,
            previous_bytes,
            _single_file: single_file,
        }
    }

    /// [`take_store`], then points the ledger at `data_dir` so [`super::register`]
    /// records into a file this test can read back.
    pub(in crate::file_system::write_operations) fn use_store_in(data_dir: &std::path::Path) -> StoreGuard {
        let guard = take_store();
        let log = File::options()
            .create(true)
            .append(true)
            .open(data_dir.join(super::STORE_FILENAME))
            .expect("open a test in-flight temp ledger");
        log.set_len(0).expect("start the test ledger empty");
        STORE.lock_ignore_poison().log = Some(log);
        guard
    }

    impl StoreGuard {
        /// Drops the process's handle on the ledger without giving the
        /// singleton back to the next test: what a crash looks like to the next
        /// launch. The log on disk is left exactly as it was, which is the
        /// whole point of the fixture.
        pub(in crate::file_system::write_operations) fn simulate_process_exit(&self) {
            let mut store = STORE.lock_ignore_poison();
            store.log = None;
            store.logged_bytes = 0;
            store.recorded.clear();
            store.pending.clear();
        }
    }

    impl Drop for StoreGuard {
        fn drop(&mut self) {
            let mut store = STORE.lock_ignore_poison();
            store.log = self.previous.take();
            store.logged_bytes = self.previous_bytes;
            store.recorded.clear();
            store.pending.clear();
        }
    }

    /// The set the process currently believes is in flight.
    ///
    /// Process-wide, so a concurrent transfer test's staged write shows up here
    /// too: ask whether it holds the path under test, ❌ never whether it's
    /// empty.
    pub(in crate::file_system::write_operations) fn live_paths() -> Vec<PathBuf> {
        STORE
            .lock_ignore_poison()
            .recorded
            .iter()
            .map(|record| match record {
                RecordedTemp::Local(path) => path.clone(),
                RecordedTemp::OnVolume(temp) => temp.path.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "in_flight_temps_tests.rs"]
mod tests;
