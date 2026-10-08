//! What Cmdr has on a disk RIGHT NOW that isn't the user's file yet, kept in two
//! ledgers.
//!
//! - **The operation's own**, `WriteOperationState::in_flight_temps`, in memory.
//!   It answers "what did this operation's abandoned tasks leave behind?" while
//!   the operation is still alive, and its sweep is
//!   `transfer::volume::cleanup::clean_abandoned_staged_writes`. ❗ Staged
//!   partials only: that sweep DELETES what it finds, and an aside or a staging
//!   directory can hold the user's only copy.
//! - **A process-wide file in the app data dir**, so the answer survives the
//!   process. An in-memory list dies with a force-quit or a crash — exactly the
//!   two endings that leave leftovers — and the directory scan that would
//!   otherwise find them (`reap_stale_transfer_temps`)
//!   only runs when something copies into that same directory, and only for
//!   leftovers over an hour old. So a quit-orphaned temp could sit there for
//!   days. The sweep in `in_flight_sweep.rs` clears the recorded ones at the
//!   next launch instead, and again whenever a volume they wait on returns.
//!
//! **No age gate on the persisted sweep, and that's safe.** The hour the
//! directory scan waits exists to protect a temp a CONCURRENT Cmdr is streaming
//! into. These paths aren't guesses from a name pattern: each one is a path this
//! app recorded when it minted the UUID in it, so no other instance can own it,
//! and the instance lock already keeps two processes off one data dir. What is
//! recorded and still on disk at startup is ours.
//!
//! ## Three things get recorded, and only one of them is disposable
//!
//! A **temp** (`.cmdr-tmp-*`) holds bytes on their way in; nothing else has
//! them, so a leftover is garbage and goes. An **aside** (`.cmdr-temp-*`) holds
//! bytes that were ALREADY the user's, renamed out of the way so a replacement
//! could take their name — a leftover one is a file whose replacement never
//! landed, and removing it would be the one unrecoverable mistake this whole
//! module exists to prevent. A **staging directory** (`.cmdr-staging-<op>`) is a
//! cross-filesystem move's half-built tree, which can be the only copy of
//! everything in it when the drive left before Phase 3 ran.
//!
//! So a record carries its [`ItemKind`], and `in_flight_sweep.rs` acts per kind.
//!
//! ## A record names its path space, not just its path
//!
//! A staged partial is a sibling of the file being written, so it lives wherever
//! the DESTINATION lives: on the local filesystem for a Mac-internal copy, and
//! in an SMB / SFTP / WebDAV / MTP volume's own path space for a transfer to
//! one. A path alone can't tell those apart, so every record carries a home:
//! either the local filesystem, or a volume ID.
//!
//! **A path on a removable drive is volume-homed too**, even though it's an
//! ordinary local path. A drive can be unplugged at the next launch, and a
//! record resolved against its mount point then reads as already gone — so the
//! ledger forgets the only trace of a leftover that's still sitting on the
//! drive. Volume-homed, the record is deferred to the drive's return instead,
//! and its path is stored RELATIVE to the volume root, so a remount at
//! `/Volumes/Backups 1` still finds it ([`home_for`]).
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
//! The format is one line per record: an op byte, then the record as a JSON
//! value (so a newline in a filename can't forge a record). `+` and `-` add and
//! retire a bare TEMP, in the shape every build since the ledger existed writes:
//! a JSON STRING for a local path, a JSON OBJECT (`{"volume_id":…,"path":…}`)
//! for a path in that volume's space. `A` and `a` add and retire a KINDED record
//! ([`TrackedItem`]). A trailing torn line — the process died mid-`write` — is
//! ignored on read. A path that isn't valid UTF-8 can't be written as JSON and
//! goes unrecorded; the hour-gated directory scan remains its backstop.
//!
//! **Why two op bytes rather than one widened shape.** A build without the
//! kinded records skips an op byte it doesn't know and truncates the log at
//! launch, so rolling back to it forgets these records and deletes nothing. A
//! widened `+` line would have been read by that build as a temp, and a temp is
//! the one kind it removes on sight — which is exactly the aside it must never
//! touch.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Once};

use super::state::WriteOperationState;
use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;

/// The persisted log's file name inside the app data dir.
const STORE_FILENAME: &str = "in-flight-temps.log";

/// Rewrite the log down to just what's recorded once it has grown past this.
/// Small enough that a session never carries a big file, large enough that a
/// serial copy doesn't rewrite after every single file (~50 files' worth).
const COMPACT_ABOVE_BYTES: u64 = 8 * 1024;

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
    /// Leftovers this sweep removed.
    pub swept: usize,
    /// Records whose file was already gone — the common, healthy case (the temp
    /// landed under its real name before the crash).
    pub already_gone: usize,
    /// Records left for later because their volume isn't reachable yet.
    pub deferred: usize,
    /// Records the sweep refused to act on (a path that isn't one of our
    /// scratch files) or couldn't remove.
    pub left_alone: usize,
    /// Originals an aside put back at their own name, because the replacement
    /// that displaced them never landed.
    pub restored: usize,
    /// Originals kept under a ` (recovered)` name, because something else is
    /// standing where they belong.
    pub recovered: usize,
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
        self.restored += other.restored;
        self.recovered += other.recovered;
    }
}

/// One persisted ledger: the open log, what it claims exists, and the listener
/// that settles its deferred records when their volume arrives.
///
/// Production has exactly one, [`Ledger::process`], which every operation state
/// records into unless it carries another ([`Ledger::of`]). A test builds its
/// own and hands it to the states it drives
/// (`WriteOperationState::with_in_flight_ledger`), so a test that
/// replays and sweeps its log only ever meets its own records. ❌ Don't go back
/// to one singleton the tests install a log into: a sweep test then replays the
/// records of every transfer test running beside it and deletes their live
/// temps mid-copy, under plain `cargo test`.
#[derive(Clone)]
pub(super) struct Ledger(Arc<LedgerInner>);

struct LedgerInner {
    store: Mutex<Store>,
    /// Guards the one-time install of this ledger's volume-arrival listener.
    arrival_listener: Once,
}

static PROCESS_LEDGER: LazyLock<Ledger> = LazyLock::new(Ledger::new);

impl Ledger {
    /// A ledger with no log open and nothing recorded.
    fn new() -> Self {
        Self(Arc::new(LedgerInner {
            store: Mutex::new(Store::default()),
            arrival_listener: Once::new(),
        }))
    }

    /// The app's one ledger, in the app data dir once [`init_and_sweep`] has run.
    pub(super) fn process() -> Self {
        PROCESS_LEDGER.clone()
    }

    /// The ledger `state` records into: a test's own, else the process's.
    fn of(state: &WriteOperationState) -> &Self {
        state.in_flight_ledger.as_ref().unwrap_or_else(|| &*PROCESS_LEDGER)
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        self.0.store.lock_ignore_poison()
    }

    /// Asks the registry to tell this ledger when a volume arrives, once.
    ///
    /// Installed lazily, so an app whose ledger stays clean (the overwhelming
    /// case) carries no listener at all. The listener holds the ledger weakly:
    /// a test's ledger that's gone stops answering rather than living on in the
    /// registry.
    fn ensure_arrival_listener(&self) {
        self.0.arrival_listener.call_once(|| {
            let ledger = Arc::downgrade(&self.0);
            get_volume_manager().on_volume_arrival(move |volume_id| {
                if let Some(inner) = ledger.upgrade() {
                    sweep::on_volume_arrival(&Ledger(inner), volume_id);
                }
            });
        });
    }
}

/// What one ledger's log holds, and what it claims exists.
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
    /// leftovers plus the deferred orphans below. Compaction rewrites the log
    /// from exactly this set, so anything missing here is forgotten on disk.
    recorded: BTreeSet<Record>,
    /// The deferred orphans: recorded by an EARLIER run or by this one, on a
    /// volume that isn't reachable right now. A subset of
    /// [`recorded`](Self::recorded), and volume-homed by construction — a local
    /// record has nothing to wait for.
    pending: BTreeSet<Record>,
}

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
    let mut store = Ledger::of(state).store();
    store.recorded.insert(record.clone());
    append(&mut store, record.add_op(), &record);
}

/// Stops tracking `temp`: it landed under its real name, or it's gone. `home`
/// must be the one [`register`] was given.
pub(super) fn deregister(state: &WriteOperationState, temp: &Path, home: Option<TempHome<'_>>) {
    state.in_flight_temps.lock_ignore_poison().retain(|p| p != temp);
    let Some(record) = record_for(temp, home) else {
        return;
    };
    let mut store = Ledger::of(state).store();
    store.recorded.remove(&record);
    append(&mut store, record.retire_op(), &record);
    compact_if_large(&mut store);
}

/// The log-shaped record for a path in `home`.
fn record_for(temp: &Path, home: Option<TempHome<'_>>) -> Option<Record> {
    let temp = match home? {
        TempHome::LocalFs => RecordedTemp::Local(temp.to_path_buf()),
        TempHome::Volume(volume_id) => RecordedTemp::OnVolume(VolumeTemp {
            volume_id: volume_id.to_string(),
            path: temp.to_path_buf(),
        }),
    };
    Some(Record::Legacy(temp))
}

/// Records something at `absolute` under the rules its `kind` earns at the next
/// sweep, and hands back the record for whoever made it to settle.
///
/// Paths in `kind` are absolute too, as every producer holds them; the record
/// states them the way its home needs.
///
/// A TEMP also joins the operation's own in-memory ledger, the way [`register`]
/// does. ❌ Nothing else may: that ledger's sweep deletes what it finds, and
/// every other kind can be the user's only copy.
pub(super) fn track(state: &WriteOperationState, kind: ItemKind, absolute: &Path) -> TrackedRecord {
    record(state, home_for(state, absolute), kind, absolute)
}

/// [`track`], with the path space named outright rather than worked out from
/// the operation's sides.
///
/// For the cross-volume engine, whose paths are already in the destination
/// volume's own namespace ([`RecordHome::volume_space`]).
pub(super) fn track_in(state: &WriteOperationState, home: RecordHome, kind: ItemKind, path: &Path) -> TrackedRecord {
    record(state, home, kind, path)
}

/// [`track`] and [`track_in`]'s shared body.
fn record(state: &WriteOperationState, home: RecordHome, kind: ItemKind, absolute: &Path) -> TrackedRecord {
    let kind = match kind.destination() {
        Some(destination) => {
            let stated = home.record_path(destination);
            kind.with_destination(stated)
        }
        None => kind,
    };
    let is_temp = matches!(kind, ItemKind::Temp);
    let record = Record::Tracked(TrackedItem {
        kind,
        path: home.record_path(absolute),
        home: home.home,
    });

    if is_temp {
        state.in_flight_temps.lock_ignore_poison().push(absolute.to_path_buf());
    }
    let mut store = Ledger::of(state).store();
    store.recorded.insert(record.clone());
    append(&mut store, record.add_op(), &record);

    TrackedRecord {
        record,
        absolute: absolute.to_path_buf(),
    }
}

/// The thing this record names is settled — it landed under its real name, or
/// it's provably gone — so stop claiming it exists.
pub(super) fn retire(state: &WriteOperationState, record: &TrackedRecord) {
    state
        .in_flight_temps
        .lock_ignore_poison()
        .retain(|p| p != &record.absolute);
    Ledger::of(state).retire_record(&record.record);
}

/// The thing is still on disk and its volume isn't reachable, so hold the record
/// and act on it the moment that volume comes back — this session included.
///
/// A local-homed record just stays recorded: the next launch's sweep is what
/// answers for it, since the boot volume never "arrives".
pub(super) fn keep_for_arrival(state: &WriteOperationState, record: TrackedRecord) {
    state
        .in_flight_temps
        .lock_ignore_poison()
        .retain(|p| p != &record.absolute);
    if record.record.volume_id().is_none() {
        return;
    }
    let ledger = Ledger::of(state);
    ledger.store().pending.insert(record.record);
    ledger.ensure_arrival_listener();
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
    let ledger = Ledger::process();
    let mut store = ledger.store();
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
/// do: a recorded leftover can sit on a Finder-mounted NAS that is no longer
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
            log::warn!(target: "copy", "the orphaned-leftover sweep panicked; some leftovers may remain");
            SweepTally::default()
        })
    }
}

/// Points the persisted ledger at the app data dir and settles whatever an
/// earlier run left behind. Call once at startup, before any copy can start.
///
/// Only the app-data-dir work happens inline. **The disk work goes to its own
/// thread** (see [`SweepHandle`] for why nothing on the launch path waits on
/// it): the records it acts on were already retired from the log by the
/// truncate below, so a new copy can start underneath it safely.
///
/// A record naming a volume that isn't in the registry yet is re-recorded and
/// held pending instead, which is why the truncate can't simply throw the log
/// away.
pub fn init_and_sweep(data_dir: &Path) -> SweepHandle {
    Ledger::process().init_and_sweep(data_dir)
}

impl Ledger {
    /// [`init_and_sweep`], for this ledger.
    fn init_and_sweep(&self, data_dir: &Path) -> SweepHandle {
        let path = data_dir.join(STORE_FILENAME);
        let recorded = read_recorded(&path);

        // Truncating as we open is what retires the records we're about to act
        // on: sweeping twice would be harmless, but a log that only ever grew
        // wouldn't.
        match File::options().create(true).append(true).open(&path) {
            Ok(log) => {
                // `truncate` isn't legal alongside `append`; retire the replayed
                // records with an explicit `ftruncate` instead.
                let _ = log.set_len(0);
                let mut store = self.store();
                store.log = Some(log);
                store.logged_bytes = 0;
            }
            Err(e) => log::warn!(
                target: "copy",
                "couldn't open the in-flight temp ledger at {}: {e}. A copy interrupted this session will \
                 leave its leftovers for the next transfer into that directory to reap.",
                path.display()
            ),
        }

        if recorded.is_empty() {
            return SweepHandle(None);
        }

        // Split by path space. The local ones this thread can act on directly; a
        // volume's can only be reached once that volume is registered, which at
        // this point in the launch it usually isn't.
        let (on_volumes, locals): (Vec<Record>, Vec<Record>) =
            recorded.into_iter().partition(|record| record.volume_id().is_some());
        if !on_volumes.is_empty() {
            self.defer(on_volumes);
            self.ensure_arrival_listener();
        }

        let ledger = self.clone();
        match std::thread::Builder::new()
            .name("cmdr-temp-sweep".to_string())
            .spawn(move || sweep::persisted_orphans(&ledger, &locals))
        {
            Ok(sweep) => SweepHandle(Some(sweep)),
            Err(e) => {
                log::warn!(target: "copy", "couldn't start the orphaned-leftover sweep: {e}");
                SweepHandle(None)
            }
        }
    }

    /// Re-records `records` and holds the ones with a volume to wait for.
    ///
    /// The re-record is what carries them past the truncate in
    /// [`init_and_sweep`]: a record the sweep couldn't act on has to outlive the
    /// launch that replayed it, or a NAS orphan is forgotten by the one ledger
    /// that knew about it. A LOCAL record is re-recorded but not held pending:
    /// nothing is going to arrive for it, so the next launch is when it gets
    /// another look.
    fn defer(&self, records: Vec<Record>) {
        let mut store = self.store();
        for record in records {
            store.recorded.insert(record.clone());
            append(&mut store, record.add_op(), &record);
            if record.volume_id().is_some() {
                store.pending.insert(record);
            }
        }
    }

    /// Drops a record the sweep is done with, on disk too.
    fn retire_record(&self, record: &Record) {
        let mut store = self.store();
        store.recorded.remove(record);
        store.pending.remove(record);
        append(&mut store, record.retire_op(), record);
    }

    /// The volume IDs the ledger is currently waiting on.
    fn pending_volume_ids(&self) -> BTreeSet<String> {
        self.store()
            .pending
            .iter()
            .filter_map(|record| record.volume_id().map(str::to_string))
            .collect()
    }

    /// How many records are still waiting for a volume.
    fn pending_count(&self) -> usize {
        self.store().pending.len()
    }

    /// Takes the pending records for `volume_id`, so exactly one sweep acts on
    /// each.
    ///
    /// They stay in [`Store::recorded`] until a sweep retires them: a claim that
    /// fails has to leave the log still claiming the thing exists.
    fn claim_pending(&self, volume_id: &str) -> Vec<Record> {
        let mut store = self.store();
        let claimed: Vec<Record> = store
            .pending
            .iter()
            .filter(|record| record.volume_id() == Some(volume_id))
            .cloned()
            .collect();
        for record in &claimed {
            store.pending.remove(record);
        }
        claimed
    }
}

/// Replays the log and returns what an earlier session left behind.
///
/// Every failure means "nothing recorded": a missing file is the normal
/// clean-exit case, and an unreadable one is not worth failing a launch over. A
/// line that doesn't parse is skipped rather than aborting the replay — the
/// last one can be a torn `write` from the process dying, which is exactly the
/// case this whole ledger exists for. An op byte from a NEWER build is skipped
/// the same way, which is what lets a rollback forget those records instead of
/// acting on them with the wrong rules.
fn read_recorded(path: &Path) -> Vec<Record> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut live: BTreeSet<Record> = BTreeSet::new();
    for line in contents.lines() {
        let Some((op, encoded)) = line.split_at_checked(1) else {
            continue;
        };
        let record = match op {
            "+" | "-" => serde_json::from_str::<RecordedTemp>(encoded).ok().map(Record::Legacy),
            "A" | "a" => serde_json::from_str::<TrackedItem>(encoded).ok().map(Record::Tracked),
            _ => None,
        };
        let Some(record) = record else {
            continue;
        };
        if op == "+" || op == "A" {
            live.insert(record);
        } else {
            live.remove(&record);
        }
    }
    live.into_iter().collect()
}

/// Appends one record under `op`. See the module docs for the format and why
/// there's no fsync.
///
/// Best-effort: a record we couldn't write costs a leftover the hour-gated
/// directory scan still catches, so it must never fail a copy.
fn append(store: &mut Store, op: u8, record: &Record) {
    let Some(log) = &mut store.log else {
        return;
    };
    let Some(encoded) = record.encode() else {
        // Not valid UTF-8, so it can't be a JSON string. Rare enough to accept:
        // the directory scan is this path's backstop.
        log::debug!(target: "copy", "not recording an in-flight leftover: its name isn't UTF-8");
        return;
    };
    let mut line = Vec::with_capacity(encoded.len() + 2);
    line.push(op);
    line.extend_from_slice(encoded.as_bytes());
    line.push(b'\n');
    match log.write_all(&line) {
        Ok(()) => store.logged_bytes += line.len() as u64,
        Err(e) => log::debug!(target: "copy", "couldn't record an in-flight leftover: {e}"),
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
    let recorded: Vec<Record> = store.recorded.iter().cloned().collect();
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
        append(store, record.add_op(), record);
    }
}

#[path = "in_flight_records.rs"]
mod records;
use records::{ItemHome, home_for};
pub(super) use records::{
    ItemKind, Record, RecordHome, RecordedTemp, TempHome, TrackedItem, TrackedRecord, VolumeTemp,
};

#[path = "in_flight_sweep.rs"]
mod sweep;
pub use sweep::init_app_handle as init_sweep_app_handle;

/// A test's own ledger. Each test builds one, hands it to the states it drives
/// (`WriteOperationState::with_in_flight_ledger`), and asserts on it alone, so
/// no other test's records ever land in its log or its sweep.
#[cfg(test)]
impl Ledger {
    /// A ledger for a process that hasn't launched yet: no log, nothing
    /// recorded. [`launch_in`](Self::launch_in) is that launch.
    pub(super) fn for_test() -> Self {
        Self::new()
    }

    /// A ledger recording into a fresh log in `data_dir`, so a test can read
    /// back what [`register`] wrote. Drop it to stand in for the process
    /// dying: the log on disk stays exactly as it was, and a new
    /// [`for_test`](Self::for_test) ledger launching against `data_dir` is the
    /// next run.
    pub(super) fn recording_in(data_dir: &Path) -> Self {
        let ledger = Self::new();
        let log = File::options()
            .create(true)
            .append(true)
            .open(data_dir.join(STORE_FILENAME))
            .expect("open a test in-flight temp ledger");
        log.set_len(0).expect("start the test ledger empty");
        ledger.store().log = Some(log);
        ledger
    }

    /// [`init_and_sweep`] against this ledger: a launch that replays
    /// `data_dir`'s log.
    pub(super) fn launch_in(&self, data_dir: &Path) -> SweepHandle {
        self.init_and_sweep(data_dir)
    }

    /// The set this ledger currently believes is on disk.
    pub(super) fn live_paths(&self) -> Vec<std::path::PathBuf> {
        self.store()
            .recorded
            .iter()
            .map(Record::path)
            .map(Path::to_path_buf)
            .collect()
    }
}

#[cfg(test)]
#[path = "in_flight_temps_tests.rs"]
mod tests;
