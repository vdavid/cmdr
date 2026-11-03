//! What Cmdr has on a disk RIGHT NOW that isn't the user's file yet, kept in two
//! ledgers.
//!
//! - **The operation's own**, `WriteOperationState::in_flight_temps`, in memory.
//!   It answers "what did this operation's abandoned tasks leave behind?" while
//!   the operation is still alive, and its sweep is
//!   `transfer::volume::cleanup::clean_abandoned_staged_writes`. ❗ Staged
//!   partials only: that sweep DELETES what it finds, and a landing, an aside,
//!   or a staging directory can hold the user's only copy.
//! - **The engine's file in the app data dir**, so the answer survives the
//!   process. Only the engine the host installed claims it
//!   (`Operations::claim_ledger`), once. An in-memory list dies with a force-quit or a crash — exactly the
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
//! ## Four things get recorded, and only one of them is disposable
//!
//! A **temp** (`.cmdr-tmp-*`) holds bytes on their way in; nothing else has
//! them, so a leftover is garbage and goes. A **landing** is the same file once
//! it's complete and the way to its name may be cleared: from then on it can be
//! the destination's only copy, so its record changes kind
//! ([`hold_for_landing`]: the landing on disk first, then the temp retired)
//! and a sweep only ever finishes the landing or keeps the bytes under a real
//! name. An **aside** (`.cmdr-temp-*`) holds
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
//! arrives in the registry or reconnects in place
//! (`OperationVolumes::on_volume_arrival`). ❌ The sweep
//! never dials, mounts, or authenticates anything to reach a volume: a launch
//! that blocks on a dead mount, or that pops a NAS password box, is worse than
//! the leftover it was chasing. A record whose volume never comes back rides
//! along to the next launch, which costs one short line in the log file.
//!
//! ## Granularity: one append per change, on its own thread, never fsynced
//!
//! This sits on the per-file hot path — a 2 000-file local copy hits it 4 000
//! times — so the write has to be about as cheap as a write can be:
//!
//! - **One `write(2)` per change**, appending a single line to a handle held
//!   open for the session. ❌ No rewrite-the-whole-file (a create + write +
//!   rename per change measured at **+0.4 ms per file**, tripling a
//!   many-small-files copy), ❌ no per-change path resolution.
//! - **❌ Never on the caller's thread.** The callers are async runtime workers,
//!   so the store only updates its in-memory set and queues the line; a writer
//!   thread does the `write(2)` (`in_flight_log_writer.rs`, which has the why
//!   and the crash window it costs). ❗ Only a TEMP's record is fire-and-forget:
//!   [`track`] waits until any other kind is written, since an aside
//!   is recorded before the rename that puts the user's original there.
//! - **❌ No `fsync`.** An fsync here would cost milliseconds per file and turn
//!   a copy into a flush-per-file crawl. An unsynced write is already in the
//!   page cache, so the record survives the process dying — a quit, a panic, a
//!   `SIGKILL`, which is every ending this exists for. A power loss can lose
//!   it, and then the hour-gated directory scan is the backstop; that's the
//!   right trade, since a power loss can equally lose the temp's own directory
//!   entry and leave nothing to sweep.
//! - **Compaction is cheap and unconditional**: past [`COMPACT_ABOVE_BYTES`] the
//!   log is rewritten down to just what's recorded (a handful of paths), so
//!   nothing accumulates across a long copy or a long session. The new log is
//!   written beside the old and renamed over it, so a kill mid-compaction never
//!   leaves an empty one (`in_flight_log_writer.rs::swap_in`).
//!
//! The format is one line per record: an op byte, then the record as a JSON
//! value (so a newline in a filename can't forge a record). `A` and `a` add and
//! retire a KINDED record ([`TrackedItem`]), which is all either engine writes.
//! `+` and `-` add and retire a bare TEMP in the shape earlier builds wrote (a
//! JSON STRING for a local path, a JSON OBJECT (`{"volume_id":…,"path":…}`) for
//! a path in that volume's space); a log one of them left still replays and
//! sweeps. A trailing torn line — the process died mid-`write` — is
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
use std::path::Path;
use std::sync::Arc;

use super::host::EngineHost;
use super::state::WriteOperationState;
use super::sync_primitives::{self as primitives, Mutex, MutexGuard, Once, mpsc, thread};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use log_writer::LogWriter;

/// The persisted log's file name inside the app data dir.
const STORE_FILENAME: &str = "in-flight-temps.log";

/// How long the quit teardown waits for queued records to reach the kernel. A
/// local append takes microseconds; this only bounds a wedged disk, which must
/// not hold the quit.
const FLUSH_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

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
/// Each engine has one (its `EngineStores`), which every operation state it
/// runs records into unless it carries another ([`Ledger::of`]). Only the
/// installed engine's ever opens a file, through its one claim
/// ([`Operations::claim_ledger`](super::Operations::claim_ledger)); every other
/// engine's records into memory and never reads, truncates, or sweeps a log. A
/// test builds its own and hands it to the states it drives
/// (`WriteOperationState::with_in_flight_ledger`), so a test that replays and
/// sweeps its log only ever meets its own records. ❌ Don't go back to one
/// ledger the tests install a log into: a sweep test then replays the records
/// of every transfer test running beside it and deletes their live temps
/// mid-copy, under plain `cargo test`.
#[derive(Clone)]
pub(super) struct Ledger(Arc<LedgerInner>);

struct LedgerInner {
    store: Mutex<Store>,
    /// Guards the one-time install of this ledger's volume-arrival listener.
    arrival_listener: Once,
}

impl Ledger {
    /// A ledger with no log open and nothing recorded.
    pub(super) fn new() -> Self {
        Self(Arc::new(LedgerInner {
            store: Mutex::new(Store::default()),
            arrival_listener: Once::new(),
        }))
    }

    /// The ledger `state` records into: a test's own, else its engine's.
    fn of(state: &WriteOperationState) -> &Self {
        state
            .in_flight_ledger
            .as_ref()
            .unwrap_or_else(|| state.scope().stores().ledger())
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        primitives::lock_ignore_poison(&self.0.store)
    }

    /// Asks the registry to tell this ledger when a volume arrives, once.
    ///
    /// Installed lazily, so an app whose ledger stays clean (the overwhelming
    /// case) carries no listener at all. The listener is `host`'s registry's,
    /// and the sweep it starts runs on `host`. It holds both weakly: a ledger
    /// or an engine that's gone stops answering rather than living on in the
    /// registry.
    fn ensure_arrival_listener(&self, host: &Arc<EngineHost>) {
        self.0.arrival_listener.call_once(|| {
            let ledger = Arc::downgrade(&self.0);
            let weak_host = Arc::downgrade(host);
            host.volumes().on_volume_arrival(Box::new(move |volume_id: &str| {
                if let (Some(inner), Some(host)) = (ledger.upgrade(), weak_host.upgrade()) {
                    sweep::on_volume_arrival(&Ledger(inner), &host, volume_id);
                }
            }));
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
    /// `None` until a claim opens it ([`Ledger::init_and_sweep`]), which keeps unit
    /// tests from touching disk unless they ask to.
    log: Option<LogWriter>,
    /// Whether a claim set out to keep this ledger on disk, whether or not its
    /// log opened. A record that must be on disk before its caller acts can
    /// only be confirmed by a ledger that keeps one; one that never set out to
    /// (a test's engine) has nothing to confirm against ([`OnDisk::NotKept`]).
    keeps_a_log: bool,
    /// Bytes queued since the log last started over, tracked here so the compaction
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

/// Records a staged write's `temp` as a tracked [`ItemKind::Temp`] in `home`,
/// plus the operation's own in-memory ledger: the cross-volume engine's
/// [`track_in`], sync because a temp's record only queues.
///
/// Call before the first byte can land there, and pair with [`retire_temp`]
/// the moment the file stops being a partial, or with [`hold_for_landing`]
/// before anything clears the way for it. No file I/O happens on the caller's
/// thread.
///
/// `home: None` means the caller couldn't say where the path lives (an
/// operation that never named its destination volume, which production never
/// does). The operation's own ledger still gets the path (its sweep deletes
/// through the operation's own volume handle, so it needs no ID), but nothing
/// is persisted: a path recorded without its path space is one the next launch
/// could resolve against the wrong filesystem.
pub(super) fn track_temp(state: &WriteOperationState, home: Option<RecordHome>, temp: &Path) {
    match home {
        Some(home) => {
            record(state, home, ItemKind::Temp, temp);
        }
        None => {
            state.in_flight_temps.lock_ignore_poison().push(temp.to_path_buf());
            log::debug!(
                target: "copy",
                "not persisting the in-flight temp {}: the operation didn't name the volume it writes to",
                LogPrivate::file_path(temp)
            );
        }
    }
}

/// Stops tracking a temp [`track_temp`] recorded: it landed under its real
/// name, or it's gone. `home` must be the one `track_temp` was given.
pub(super) fn retire_temp(state: &WriteOperationState, home: Option<RecordHome>, temp: &Path) {
    state.in_flight_temps.lock_ignore_poison().retain(|p| p != temp);
    if let Some(home) = home {
        Ledger::of(state).retire_record(&tracked_record(&home, ItemKind::Temp, temp));
    }
}

/// Whether a record that must be on disk before its caller acts made it there
/// ([`track`], [`track_in`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OnDisk {
    /// The log has the line: in the page cache, which a crash or a kill can't
    /// take.
    Written,
    /// This ledger keeps nothing on disk, by design: no claim set out to open
    /// a log for it (an engine the host didn't install, as a test's is). There's
    /// nothing to confirm against, and no later launch would read it.
    NotKept,
    /// The ledger keeps a log and this line isn't in it: the log never opened,
    /// its writer is gone, the write failed, or the record can't be written (a
    /// path that isn't UTF-8).
    Missing,
}

/// Whether this operation's persisted ledger records `path` on `volume_id`,
/// under any kind, in flight or waiting for its volume to come back.
///
/// `path` is in the volume's own namespace and `root` is that volume's root, so
/// each record is compared in its own path space (`RecordHome::record_path`):
/// as it is for a volume-space record, relative to `root` for a mount-homed one.
pub(super) fn is_recorded_on(state: &WriteOperationState, volume_id: &str, root: &Path, path: &Path) -> bool {
    Ledger::of(state)
        .store()
        .recorded
        .iter()
        .any(|record| record.names(volume_id, root, path))
}

/// Records `temp` as a partial in both ledgers, in the `+` line shape every
/// build before the kinded records wrote. Nothing in production writes it any
/// more; the replay cells use it to leave the log an earlier build would have.
///
/// `home: None` keeps the path in memory only, as [`track_temp`] does.
#[cfg(test)]
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

/// Retires a [`register`]ed temp, in the `-` line shape earlier builds wrote.
/// `home` must be the one `register` was given.
#[cfg(test)]
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

/// The `+`-line record for a path in `home`.
#[cfg(test)]
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
/// A TEMP also joins the operation's own in-memory ledger, the way
/// [`track_temp`] does. ❌ Nothing else may: that ledger's sweep deletes what it finds, and
/// every other kind can be the user's only copy.
///
/// ❗ **Anything but a temp is written before this returns**, so a
/// caller that records an aside and THEN renames the user's original onto it
/// can't crash in between with nothing knowing what the aside is. A temp only
/// queues: it holds incoming bytes, so a lost record costs a leftover, never
/// data. Blocks on the ledger's writer for that, so call it from blocking code
/// (every caller is the sync local engine); the async engine has
/// [`track_in`]. Its callers act whether or not the line made it; the one that
/// must not, a landing about to clear the way, asks [`hold_for_landing`].
pub(super) fn track(state: &WriteOperationState, kind: ItemKind, absolute: &Path) -> TrackedRecord {
    let home = home_for(state, absolute);
    if matches!(kind, ItemKind::Temp) {
        return record(state, home, kind, absolute);
    }
    let (done, reached) = mpsc::sync_channel(1);
    let (tracked, answered) = record_confirmed(state, home, kind, absolute, move |written| {
        let _ = done.send(written);
    });
    if answered.is_none() {
        // An Err means the writer is gone, and there is nothing left to wait for.
        let _ = reached.recv();
    }
    tracked
}

/// [`track`], with the path space named outright rather than worked out from
/// the operation's sides, and awaited rather than blocked on.
///
/// For the cross-volume engine, whose paths are already in the destination
/// volume's own namespace ([`RecordHome::volume_space`]). Every kind handed
/// here waits until the writer has tried its line (the engine records a temp
/// with [`track_temp`]), and the answer says whether it's on disk.
pub(super) async fn track_in(
    state: &WriteOperationState,
    home: RecordHome,
    kind: ItemKind,
    path: &Path,
) -> (TrackedRecord, OnDisk) {
    let (done, reached) = tokio::sync::oneshot::channel();
    let (tracked, answered) = record_confirmed(state, home, kind, path, move |written| {
        let _ = done.send(written);
    });
    let on_disk = match answered {
        Some(on_disk) => on_disk,
        // An Err means the writer is gone with the line still queued.
        None => match reached.await {
            Ok(true) => OnDisk::Written,
            Ok(false) | Err(_) => OnDisk::Missing,
        },
    };
    (tracked, on_disk)
}

/// Records `kind` at `absolute` in both ledgers, its line queued and not
/// waited for: a temp's.
fn record(state: &WriteOperationState, home: RecordHome, kind: ItemKind, absolute: &Path) -> TrackedRecord {
    let (tracked, mut store) = insert(state, home, kind, absolute);
    append(&mut store, tracked.record.add_op(), &tracked.record);
    drop(store);
    tracked
}

/// [`record`], with the writer telling `written` whether the line reached the
/// log. Answers at once instead (`Some`) when there's no line to wait for, and
/// `written` never runs.
fn record_confirmed(
    state: &WriteOperationState,
    home: RecordHome,
    kind: ItemKind,
    absolute: &Path,
    written: impl FnOnce(bool) + Send + 'static,
) -> (TrackedRecord, Option<OnDisk>) {
    let (tracked, mut store) = insert(state, home, kind, absolute);
    let answered = match (&store.log, encode_line(tracked.record.add_op(), &tracked.record)) {
        (None, _) if !store.keeps_a_log => Some(OnDisk::NotKept),
        (None, _) | (Some(_), None) => Some(OnDisk::Missing),
        (Some(log), Some(line)) => {
            let length = line.len() as u64;
            if log.append_then(line, written) {
                store.logged_bytes += length;
                None
            } else {
                Some(OnDisk::Missing)
            }
        }
    };
    drop(store);
    (tracked, answered)
}

/// [`record`] and [`record_confirmed`]'s shared body: the in-memory halves,
/// with the store still held for the line.
fn insert<'a>(
    state: &'a WriteOperationState,
    home: RecordHome,
    kind: ItemKind,
    absolute: &Path,
) -> (TrackedRecord, MutexGuard<'a, Store>) {
    let is_temp = matches!(kind, ItemKind::Temp);
    let record = tracked_record(&home, kind, absolute);

    if is_temp {
        state.in_flight_temps.lock_ignore_poison().push(absolute.to_path_buf());
    }
    let mut store = Ledger::of(state).store();
    store.recorded.insert(record.clone());
    let tracked = TrackedRecord {
        record,
        absolute: absolute.to_path_buf(),
    };
    (tracked, store)
}

/// The record `kind` at `absolute` gets in `home`, every path in it stated the
/// way that home stores them. The same inputs always make the same record,
/// which is what lets [`retire_temp`] retire one by its path.
fn tracked_record(home: &RecordHome, kind: ItemKind, absolute: &Path) -> Record {
    let kind = match kind.destination() {
        Some(destination) => {
            let stated = home.record_path(destination);
            kind.with_destination(stated)
        }
        None => kind,
    };
    Record::Tracked(TrackedItem {
        kind,
        path: home.record_path(absolute),
        home: home.home.clone(),
    })
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
    ledger.ensure_arrival_listener(state.scope().host_arc());
}

/// [`keep_for_arrival`] for a temp [`track_temp`] recorded in `home`, whose
/// record its writer no longer holds: a stopped operation's cleanup couldn't
/// remove it (`transfer/leftover_cleanup.rs`). `home: None` was never
/// persisted, so there's nothing to hold; the hour-gated reap is its backstop.
pub(super) fn keep_temp_for_arrival(state: &WriteOperationState, home: Option<RecordHome>, temp: &Path) {
    let Some(home) = home else {
        return;
    };
    let record = TrackedRecord {
        record: tracked_record(&home, ItemKind::Temp, temp),
        absolute: temp.to_path_buf(),
    };
    keep_for_arrival(state, record);
}

/// The background sweep a ledger claim started, and the only way to learn
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
pub struct SweepHandle(Option<thread::JoinHandle<SweepTally>>);

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

impl Ledger {
    /// Waits, at most [`FLUSH_WAIT`], for everything queued so far to reach the
    /// log file. The wait happens with the store released.
    fn flush_log(&self) {
        let (done, reached) = mpsc::sync_channel(1);
        let queued = self.store().log.as_ref().is_some_and(|log| {
            log.barrier(move || {
                // A waiter that gave up has nothing left to tell.
                let _ = done.send(());
            })
        });
        if queued && reached.recv_timeout(FLUSH_WAIT).is_err() {
            log::warn!(
                target: "copy",
                "the in-flight temp ledger didn't finish writing within {FLUSH_WAIT:?}; its last records may be missing"
            );
        }
    }

    /// Drops a record the sweep is done with, on disk too.
    fn retire_record(&self, record: &Record) {
        let mut store = self.store();
        store.recorded.remove(record);
        store.pending.remove(record);
        append(&mut store, record.retire_op(), record);
        compact_if_large(&mut store);
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
    ///
    /// ❗ One lock for the pick and the take: two arrivals of one volume (the
    /// launch sweep finding it registered, its listener firing) race here, and
    /// a pick that let go before taking hands both the same records. Pinned by
    /// `in_flight_shuttle_tests.rs`.
    fn claim_pending(&self, volume_id: &str) -> Vec<Record> {
        let waits_on_it = |record: &Record| record.volume_id() == Some(volume_id);
        self.store().pending.extract_if(.., waits_on_it).collect()
    }
}

/// Queues one record under `op`. See the module docs for the format and why
/// there's no fsync.
///
/// Best-effort: a record we couldn't write costs a leftover the hour-gated
/// directory scan still catches, so it must never fail a copy.
fn append(store: &mut Store, op: u8, record: &Record) {
    let Some(log) = &store.log else {
        return;
    };
    let Some(line) = encode_line(op, record) else {
        return;
    };
    store.logged_bytes += line.len() as u64;
    log.append(line);
}

/// One record's log line under `op`.
///
/// `None` for a path that isn't valid UTF-8, so it can't be a JSON string. Rare
/// enough to accept: the directory scan is that path's backstop.
fn encode_line(op: u8, record: &Record) -> Option<Vec<u8>> {
    let Some(encoded) = record.encode() else {
        log::debug!(target: "copy", "not recording an in-flight leftover: its name isn't UTF-8");
        return None;
    };
    let mut line = Vec::with_capacity(encoded.len() + 2);
    line.push(op);
    line.extend_from_slice(encoded.as_bytes());
    line.push(b'\n');
    Some(line)
}

/// Rewrites the log down to just what's recorded once it has grown past
/// [`COMPACT_ABOVE_BYTES`], so a long copy can't grow an unbounded file.
///
/// ❌ Don't gate this on "nothing is in flight": the concurrent cross-volume
/// driver keeps a window open for the whole transfer, so an idle-only rule would
/// let a 100k-file copy append megabytes before it ever got a chance to run.
/// Rewriting the recorded set (a handful of paths) costs one write and one
/// rename every ~50 files, queued behind the appends before it like any other
/// line.
fn compact_if_large(store: &mut Store) {
    if store.logged_bytes < COMPACT_ABOVE_BYTES {
        return;
    }
    let Some(log) = &store.log else {
        return;
    };
    let contents: Vec<u8> = store
        .recorded
        .iter()
        .filter_map(|record| encode_line(record.add_op(), record))
        .flatten()
        .collect();
    store.logged_bytes = contents.len() as u64;
    log.rewrite(contents);
}

#[path = "in_flight_log_writer.rs"]
mod log_writer;

#[path = "in_flight_landing.rs"]
mod landing;
pub(super) use landing::{NotOnDisk, back_to_temp, hold_aside, hold_for_landing};

#[path = "in_flight_records.rs"]
mod records;
use records::{ItemHome, home_for};
pub(super) use records::{ItemKind, Record, RecordHome, RecordedTemp, TrackedItem, TrackedRecord, dest_home};
#[cfg(test)]
pub(super) use records::{TempHome, VolumeTemp};

#[path = "in_flight_sweep.rs"]
mod sweep;

#[path = "in_flight_claim.rs"]
mod claim;
pub use claim::AlreadyClaimed;
#[cfg(test)]
use claim::read_recorded;

#[cfg(test)]
#[path = "in_flight_temps_test_support.rs"]
mod test_support;

#[cfg(test)]
#[path = "in_flight_temps_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "in_flight_temps_claim_tests.rs"]
mod claim_tests;

#[cfg(all(test, feature = "shuttle"))]
#[path = "in_flight_shuttle_tests.rs"]
mod shuttle_tests;
