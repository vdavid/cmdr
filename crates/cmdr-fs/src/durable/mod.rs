//! Making written files durable before a copy reports them done, without paying
//! a sync or a device barrier per file.
//!
//! Two steps, because macOS splits them: [`sync_to_drive`] hands one file's
//! bytes to its drive (`fsync(2)`, ~0.03 ms on an internal SSD), and a
//! [`DeviceBarriers`] pass asks the drive to empty its write cache
//! (`F_FULLFSYNC`, ~4 ms on that SSD and tens of ms on a spinning disk). One
//! barrier covers every file handed over before it, so a many-file copy pays it
//! on an interval and once at the end, never per file. On a local volume the
//! hand-over waits for that pass too ([`DeviceBarriers::note_written`]), so a
//! file between passes costs no sync at all. On Linux `fdatasync` already
//! flushes the drive's cache, so each file keeps its own and the barrier there
//! is nothing. The rule, the measurements, and what it trades:
//! `crates/cmdr-operations/src/transfer/DETAILS.md` § Durability.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::file_identity::{DeviceId, device_of};
use crate::ignore_poison::IgnorePoison;
use crate::log_private::LogPrivate;
use crate::os_error::OsErrorClass;

/// How long an operation's handed-over files wait for a device barrier while it
/// runs. A power cut mid-copy loses at most about this much finished work, and
/// even a 50 ms barrier (a spinning disk's, estimated) this often costs under 3% of the
/// copy's time.
pub const BARRIER_INTERVAL: Duration = Duration::from_secs(2);

mod platform;

use platform::NEEDS_DEVICE_BARRIER;

/// The classes a filesystem answers for a sync it doesn't offer at all, by
/// class because the numbers differ by platform (Linux's `ENOTSUP` and
/// `EOPNOTSUPP` are one number, macOS keeps them apart, `ENOSYS` is 78 on macOS
/// and 38 on Linux). Network and FUSE mounts are the likely ones.
pub const SYNC_UNSUPPORTED: [OsErrorClass; 5] = [
    OsErrorClass::Unsupported,
    OsErrorClass::OperationNotSupported,
    OsErrorClass::InvalidArgument,
    OsErrorClass::NotImplemented,
    OsErrorClass::InappropriateIoctl,
];

/// Whether a sync failed because the filesystem doesn't do that sync at all.
/// Every other errno (`EIO`, `ENOSPC`, `ENXIO`, …), and an error carrying none,
/// is a real failure.
#[must_use]
pub fn is_sync_unsupported(e: &io::Error) -> bool {
    OsErrorClass::of_io(e).is_some_and(|class| SYNC_UNSUPPORTED.contains(&class))
}

/// `sync_data`, which on macOS is `F_FULLFSYNC`, falling back to `fsync(2)`
/// where the filesystem doesn't offer the full barrier: an SMB mount of a Samba
/// share answers `ENOTSUP` (verified against the Docker guest share, macOS 27.0,
/// 2026-10-11), which used to fail every chunked copy onto it. SQLite falls back
/// the same way. For a writer of ONE file; a copy of many hands each to the
/// drive with [`sync_to_drive`] and barriers through [`DeviceBarriers`].
///
/// ❗ On a network mount this is also what makes a copy's dates stick: writes
/// still buffered when the dates go on reach the server at the handle's real
/// close, and the server stamps that close as the mtime. So a copy flushes
/// first and sets the dates after (`metadata::apply_staged`).
pub fn sync_file_data(file: &File) -> io::Result<()> {
    platform::sync_file_data(file)
}

/// Hands one written file's bytes, and the metadata that finds them, to its
/// drive: `fsync(2)` on macOS (std's `sync_data` is `F_FULLFSYNC` there, the
/// device barrier this skips), `fdatasync` elsewhere. Durable once a later
/// [`DeviceBarriers`] pass covers it. A copy of many files calls
/// [`DeviceBarriers::note_written`] instead, which runs this only where it
/// can't wait for the pass.
///
/// ❗ Same ordering as [`sync_file_data`]: on a network mount this is what makes
/// the dates set after it stick.
pub fn sync_to_drive(file: &File) -> io::Result<()> {
    platform::sync_to_drive(file)
}

/// [`sync_to_drive`] for an open directory, so the entry changes inside it reach
/// the drive: `fsync(2)` on macOS, `fsync` (std's `sync_all`) elsewhere.
pub fn sync_dir_to_drive(dir: &File) -> io::Result<()> {
    platform::sync_dir_to_drive(dir)
}

/// Asks the drive holding `file` to empty its write cache, which makes every
/// file and directory handed to it before durable. `F_FULLFSYNC` on macOS, and
/// nothing elsewhere, where the per-item sync already flushed the cache. A
/// filesystem with no barrier to offer ([`is_sync_unsupported`]; an SMB mount
/// answers `ENOTSUP`) counts as done: each item already had its `fsync`, which
/// on a network mount is the server's flush.
pub fn device_barrier(file: &File) -> io::Result<()> {
    platform::device_barrier(file)
}

/// What a barrier pass couldn't confirm: the item its barrier went through and
/// the OS error.
#[derive(Debug)]
pub struct BarrierFailure {
    /// The file or directory the failed barrier was issued on.
    pub path: PathBuf,
    /// What the drive answered.
    pub error: io::Error,
}

/// The syncs and device barriers one writer owes: the files it wrote that
/// still wait for their sync, every device it handed a file to since the last
/// barrier, and the directories whose new entries still need their `fsync`. A
/// copy notes each file as its bytes are written
/// ([`note_written`](Self::note_written)); a pass runs on its own every
/// [`interval`](Self::with_interval), and [`flush`](Self::flush) runs the
/// closing one, before the copy reports complete or deletes a moved source.
/// A pass syncs each waiting file, then each noted directory, then barriers
/// each device.
///
/// Thread-safe: concurrent copies note into one tracker, and a pass runs its
/// syncs outside the lock. A barrier that fails leaves its device owed, so the
/// next pass tries it again; a later barrier that holds covers everything
/// before it, so that answer is the one that counts. A file sync that fails is
/// never retried (a second `fsync` after a failed one proves nothing): the
/// next [`flush`](Self::flush) answers it.
pub struct DeviceBarriers {
    interval: Duration,
    owed: Mutex<Owed>,
    issued: AtomicU64,
    /// The time the passes and lone barriers took: [`Self::time_spent`].
    spent: Mutex<Spent>,
    #[cfg(any(test, feature = "testing"))]
    script: Mutex<SyncScript>,
}

/// Test seam: every file sync this tracker ran, and the answer a test picked
/// for some of them.
// DEFAULT-OK: no syncs run yet and no script installed is exactly a new tracker's state.
#[cfg(any(test, feature = "testing"))]
#[derive(Default)]
struct SyncScript {
    synced: Vec<PathBuf>,
    volume_syncs: u64,
    fail: Option<SyncAnswer>,
}

/// A test's answer for a file sync: `Some` instead of syncing.
#[cfg(any(test, feature = "testing"))]
type SyncAnswer = Box<dyn Fn(&Path) -> Option<io::Error> + Send + Sync>;

/// Barrier time as the union of every running pass, so two threads' passes
/// that overlap count their shared stretch once.
struct Spent {
    /// Time in finished stretches.
    done: Duration,
    /// How many passes run now, and since when one has been running.
    running: u32,
    since: Option<crate::rt::Instant>,
}

struct Owed {
    /// One open handle per device a file was handed to (or written to, for a
    /// file still waiting), to issue its barrier through. The latest note
    /// wins, so the handle names a recent item.
    devices: HashMap<Option<DeviceId>, Holder>,
    /// Directories that gained entries, in first-seen order, each once.
    dirs: Vec<PathBuf>,
    /// Files written but not yet handed to their drive, by the name they have
    /// now, each with its device. ❌ Never an open handle per file: two seconds
    /// of tiny files is thousands of them.
    written: HashMap<PathBuf, Option<DeviceId>>,
    /// Whether a device's written files can wait for the pass, asked once per
    /// device (`platform::defers_data_sync`).
    defers: HashMap<Option<DeviceId>, bool>,
    /// The first file sync a pass couldn't confirm since the last flush, which
    /// the next flush answers.
    unconfirmed: Option<BarrierFailure>,
    last_pass: crate::rt::Instant,
}

struct Holder {
    file: File,
    path: PathBuf,
}

impl Default for DeviceBarriers {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DeviceBarriers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceBarriers")
            .field("interval", &self.interval)
            .field("issued", &self.barriers_issued())
            .field("time_spent", &self.time_spent())
            .finish_non_exhaustive()
    }
}

impl DeviceBarriers {
    /// A tracker that runs a pass every [`BARRIER_INTERVAL`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_interval(BARRIER_INTERVAL)
    }

    /// A tracker that runs a pass whenever `interval` has gone by since the
    /// last one, checked at each note. `Duration::MAX` leaves only the closing
    /// pass; `Duration::ZERO` barriers at every note.
    #[must_use]
    pub fn with_interval(interval: Duration) -> Self {
        Self {
            interval,
            owed: Mutex::new(Owed {
                devices: HashMap::new(),
                dirs: Vec::new(),
                written: HashMap::new(),
                defers: HashMap::new(),
                unconfirmed: None,
                last_pass: crate::rt::now(),
            }),
            issued: AtomicU64::new(0),
            spent: Mutex::new(Spent {
                done: Duration::ZERO,
                running: 0,
                since: None,
            }),
            #[cfg(any(test, feature = "testing"))]
            script: Mutex::new(SyncScript::default()),
        }
    }

    /// Notes that `file` (at `path`) was handed to its drive by a
    /// [`sync_to_drive`] that answered `Ok`, so its device owes a barrier. Runs
    /// the interval pass when it's due. ❗ Only after the sync returned: a
    /// barrier covers what was handed over BEFORE it started.
    pub fn note_synced(&self, file: &File, path: &Path) {
        if NEEDS_DEVICE_BARRIER {
            let device = file.metadata().ok().as_ref().and_then(device_of);
            self.keep_holder(file, path, device);
        }
        self.pass_if_due();
    }

    /// Notes that `file` (at `path`) has all its bytes, unsynced. On a local
    /// volume where a barrier follows anyway (macOS), its sync waits for the
    /// next pass, which hands over everything written since the last one in a
    /// row: a file between passes costs no sync at all. Anywhere else (a
    /// network mount, where the `fsync` IS the server's flush; Linux, where it
    /// IS the barrier) it syncs here, and a failure answers here. Runs the
    /// interval pass when it's due.
    ///
    /// The pass finds the file by name, so whoever renames it says so
    /// ([`note_renamed`](Self::note_renamed)), and whoever deletes it too
    /// ([`forget`](Self::forget)). A name the pass can't find falls back to a
    /// whole-volume sync, so a missed rename costs time, never durability.
    ///
    /// ❗ Call it before the file's dates go on: on a network mount, writes
    /// still buffered when the dates go on reach the server at the handle's
    /// real close, which stamps its own mtime.
    pub fn note_written(&self, file: &File, path: &Path) -> io::Result<()> {
        let device = file.metadata().ok().as_ref().and_then(device_of);
        let defers = {
            let mut owed = self.owed.lock_ignore_poison();
            *owed
                .defers
                .entry(device)
                .or_insert_with(|| platform::defers_data_sync(file))
        };
        if !defers {
            return self.hand_over_file(file, path);
        }
        self.keep_holder(file, path, device);
        self.owed
            .lock_ignore_poison()
            .written
            .insert(path.to_path_buf(), device);
        self.pass_if_due();
        Ok(())
    }

    /// A written file still waiting for its sync moved from `from` to `to`,
    /// and so does a failure noted under `from`, so the flush names the file
    /// the user sees. Nothing to do for any other path.
    pub fn note_renamed(&self, from: &Path, to: &Path) {
        let mut owed = self.owed.lock_ignore_poison();
        if let Some(device) = owed.written.remove(from) {
            owed.written.insert(to.to_path_buf(), device);
        }
        if let Some(failure) = owed.unconfirmed.as_mut().filter(|failure| failure.path == from) {
            failure.path = to.to_path_buf();
        }
    }

    /// A written file still waiting for its sync is gone (a failed landing
    /// threw its temp away), so it owes nothing.
    pub fn forget(&self, path: &Path) {
        self.owed.lock_ignore_poison().written.remove(path);
    }

    /// Hands the file at `path` to its drive now, ahead of the next pass, and
    /// notes its device owed a barrier. For a landing that REPLACES an
    /// existing file: renaming unsynced bytes over the user's file means an OS
    /// crash can leave neither version. A failure answers here.
    pub fn hand_over(&self, path: &Path) -> io::Result<()> {
        self.forget(path);
        self.hand_over_file(&File::open(path)?, path)
    }

    /// Hands the open `file` (at `path`) to its drive now
    /// ([`sync_to_drive`]) and notes its device owed a barrier. What
    /// [`note_written`](Self::note_written) does where it can't defer, and
    /// what a writer whose concurrent copies already overlap their syncs calls
    /// for each file (a volume's `write_from_stream`). A failure answers here.
    pub fn hand_over_file(&self, file: &File, path: &Path) -> io::Result<()> {
        self.sync_handle(file, path)?;
        self.note_synced(file, path);
        Ok(())
    }

    /// Hands every written file still waiting to its drive now, without a
    /// barrier: ahead of renames the tracker won't hear about (a cross-FS
    /// move's staged tree going to its final place). A failure is the next
    /// [`flush`](Self::flush)'s answer.
    pub fn hand_over_written(&self) {
        let written = std::mem::take(&mut self.owed.lock_ignore_poison().written);
        // The devices stay owed (no barrier here), so a fallback finds its
        // handle among them.
        self.sync_written(written, &HashMap::new());
    }

    /// Keeps a handle on `file` to issue its device's barrier through. Without
    /// one, barriers through `file` now, so it isn't left uncovered.
    fn keep_holder(&self, file: &File, path: &Path, device: Option<DeviceId>) {
        if !NEEDS_DEVICE_BARRIER {
            return;
        }
        match file.try_clone() {
            Ok(handle) => {
                self.owed.lock_ignore_poison().devices.insert(
                    device,
                    Holder {
                        file: handle,
                        path: path.to_path_buf(),
                    },
                );
            }
            Err(e) => {
                log::debug!(
                    target: "write_durability",
                    "barrier: couldn't keep a handle on {}, so it gets its own barrier: {e}",
                    LogPrivate::file_path(path)
                );
                self.timed(|| self.issue(file, path))
                    .unwrap_or_else(|failure| log_interval_failure(&failure));
            }
        }
    }

    /// Notes that `dir` gained an entry, so the next pass `fsync`s it (each
    /// directory once per pass) before its barrier. Best effort, like the
    /// per-file directory sync it replaces: a directory that refuses `fsync` is
    /// logged, never a failure.
    pub fn note_dir(&self, dir: &Path) {
        {
            let mut owed = self.owed.lock_ignore_poison();
            if !owed.dirs.iter().any(|seen| seen == dir) {
                owed.dirs.push(dir.to_path_buf());
            }
        }
        self.pass_if_due();
    }

    /// The closing pass: syncs every written file still waiting, `fsync`s every
    /// noted directory, then barriers every device still owed one. Tries every
    /// item before answering. The answer is the first file sync that failed
    /// since the last flush, an interval pass's included, else the first
    /// barrier that failed. Nothing owed is nothing to do.
    pub fn flush(&self) -> Result<(), BarrierFailure> {
        let pass = self.pass();
        match self.owed.lock_ignore_poison().unconfirmed.take() {
            Some(failure) => Err(failure),
            None => pass,
        }
    }

    /// How often this tracker runs a pass while its operation writes: the
    /// operation's barrier cadence, which a caller pacing its own barriers (a
    /// cross-volume move's held source sweeps) follows too.
    #[must_use]
    pub const fn interval(&self) -> Duration {
        self.interval
    }

    /// How many barriers this tracker issued, for the closing log line and the
    /// tests that pin "not one per file".
    #[must_use]
    pub fn barriers_issued(&self) -> u64 {
        self.issued.load(Ordering::Relaxed)
    }

    /// How long this tracker has spent in barriers (each pass's directory
    /// syncs and device barriers) and lone barriers, in total, a running one
    /// included up to now. Monotonic. The ETA reads it so a barrier a big file
    /// happened to pay isn't learned as a cost of every file (`cmdr-operations`'
    /// `eta.rs`); counting the running pass live is what keeps a barrier that
    /// spans two progress emits from being taken out of the second one whole.
    ///
    /// ❗ A pass's FILE syncs aren't in it: each is its file's own cost, which
    /// the ETA's cost model has to learn. Counted here, the ETA priced the
    /// files left short (`eta_workload_tests.rs`, the batched cells).
    #[must_use]
    pub fn time_spent(&self) -> Duration {
        let spent = self.spent.lock_ignore_poison();
        spent.done + spent.since.map_or(Duration::ZERO, crate::rt::since)
    }

    /// Runs `f`, counting the time it runs toward [`Self::time_spent`].
    fn timed<R>(&self, f: impl FnOnce() -> R) -> R {
        /// Ends the stretch on drop, so a pass that unwinds can't leave the
        /// clock running.
        struct Running<'a>(&'a Mutex<Spent>);
        impl Drop for Running<'_> {
            fn drop(&mut self) {
                let mut spent = self.0.lock_ignore_poison();
                spent.running = spent.running.saturating_sub(1);
                if spent.running == 0
                    && let Some(since) = spent.since.take()
                {
                    spent.done += crate::rt::since(since);
                }
            }
        }
        {
            let mut spent = self.spent.lock_ignore_poison();
            if spent.running == 0 {
                spent.since = Some(crate::rt::now());
            }
            spent.running += 1;
        }
        let _running = Running(&self.spent);
        f()
    }

    fn pass_if_due(&self) {
        let due = {
            let owed = self.owed.lock_ignore_poison();
            crate::rt::since(owed.last_pass) >= self.interval
        };
        if due && let Err(failure) = self.pass() {
            log_interval_failure(&failure);
        }
    }

    fn pass(&self) -> Result<(), BarrierFailure> {
        let (written, dirs, mut devices) = {
            let mut owed = self.owed.lock_ignore_poison();
            owed.last_pass = crate::rt::now();
            (
                std::mem::take(&mut owed.written),
                std::mem::take(&mut owed.dirs),
                std::mem::take(&mut owed.devices),
            )
        };
        // Untimed: each file's sync is that file's own cost, which the ETA's
        // cost model has to see (`time_spent`).
        self.sync_written(written, &devices);
        self.timed(|| {
            Self::sync_dirs(dirs, &mut devices);
            self.issue_barriers(devices)
        })
    }

    /// `fsync`s each noted directory, keeping a handle on its device for the
    /// barrier. Best effort: a directory that refuses is logged.
    fn sync_dirs(dirs: Vec<PathBuf>, devices: &mut HashMap<Option<DeviceId>, Holder>) {
        for dir in dirs {
            match File::open(&dir).and_then(|handle| sync_dir_to_drive(&handle).map(|()| handle)) {
                Ok(handle) if NEEDS_DEVICE_BARRIER => {
                    let device = handle.metadata().ok().as_ref().and_then(device_of);
                    devices.entry(device).or_insert(Holder {
                        file: handle,
                        path: dir,
                    });
                }
                Ok(_) => {}
                Err(e) => log::debug!(
                    target: "write_durability",
                    "barrier: directory fsync skipped for {}: {e}",
                    LogPrivate::dir_path(&dir)
                ),
            }
        }
    }

    /// Barriers each device. A device whose barrier fails stays owed.
    fn issue_barriers(&self, devices: HashMap<Option<DeviceId>, Holder>) -> Result<(), BarrierFailure> {
        let mut first_failure = None;
        for (device, holder) in devices {
            if let Err(failure) = self.issue(&holder.file, &holder.path) {
                first_failure.get_or_insert(failure);
                // Still owed: the next pass tries again, unless a newer note
                // already brought a fresher handle.
                self.owed.lock_ignore_poison().devices.entry(device).or_insert(holder);
            }
        }
        first_failure.map_or(Ok(()), Err)
    }

    /// Hands each written file to its drive, by the name it has now. A failure
    /// is kept for the next flush, never retried. A name that's gone (renamed
    /// or deleted by someone who didn't say) gets its whole volume synced, once
    /// per device, through a handle on that device.
    ///
    /// One at a time: the local engine, the one writer that defers, copies one
    /// file at a time too, and its syncs measured nearly free by the pass
    /// (`transfer/DETAILS.md` § Durability). A concurrent writer would stay
    /// correct but slower: a file it renames while a pass holds the old name
    /// costs a whole-volume sync.
    fn sync_written(&self, written: HashMap<PathBuf, Option<DeviceId>>, devices: &HashMap<Option<DeviceId>, Holder>) {
        let mut gone: Vec<(Option<DeviceId>, PathBuf)> = Vec::new();
        for (path, device) in written {
            match File::open(&path).and_then(|file| self.sync_handle(&file, &path)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    if !gone.iter().any(|(seen, _)| *seen == device) {
                        gone.push((device, path));
                    }
                }
                Err(error) => self.keep_unconfirmed(BarrierFailure { path, error }),
            }
        }
        for (device, path) in gone {
            log::debug!(
                target: "write_durability",
                "barrier: {} isn't where it was written, so its whole volume gets a sync",
                LogPrivate::file_path(&path)
            );
            let owed_handle = || {
                let owed = self.owed.lock_ignore_poison();
                owed.devices
                    .get(&device)
                    .and_then(|holder| holder.file.try_clone().ok())
            };
            let synced = match devices.get(&device).map(|holder| &holder.file) {
                Some(file) => self.sync_volume(file),
                None => match owed_handle() {
                    Some(file) => self.sync_volume(&file),
                    None => Err(io::Error::other("no handle left on the volume")),
                },
            };
            if let Err(error) = synced {
                self.keep_unconfirmed(BarrierFailure { path, error });
            }
        }
    }

    /// A writer that doesn't fail its write on a sync error (a volume's
    /// `write_from_stream`, whose bytes are written either way) leaves the
    /// error here, so the next [`flush`](Self::flush) answers it. Without
    /// that, a refused sync where no barrier follows (Linux, a network mount)
    /// would reach no one.
    pub fn note_unconfirmed(&self, path: &Path, error: io::Error) {
        self.keep_unconfirmed(BarrierFailure {
            path: path.to_path_buf(),
            error,
        });
    }

    fn keep_unconfirmed(&self, failure: BarrierFailure) {
        log::warn!(
            target: "write_durability",
            "barrier: the drive didn't confirm the data of {}, so the operation can't call it durable: {}",
            LogPrivate::file_path(&failure.path),
            // allowed-log-private: an errno from `fsync` on an open fd, which names no path
            failure.error
        );
        self.owed.lock_ignore_poison().unconfirmed.get_or_insert(failure);
    }

    /// One file's [`sync_to_drive`], through the test seam.
    fn sync_handle(&self, file: &File, path: &Path) -> io::Result<()> {
        #[cfg(any(test, feature = "testing"))]
        {
            let mut script = self.script.lock_ignore_poison();
            script.synced.push(path.to_path_buf());
            if let Some(error) = script.fail.as_ref().and_then(|fail| fail(path)) {
                return Err(error);
            }
        }
        #[cfg(not(any(test, feature = "testing")))]
        let _ = path;
        sync_to_drive(file)
    }

    fn sync_volume(&self, file: &File) -> io::Result<()> {
        #[cfg(any(test, feature = "testing"))]
        {
            self.script.lock_ignore_poison().volume_syncs += 1;
        }
        platform::sync_volume(file)
    }

    fn issue(&self, file: &File, path: &Path) -> Result<(), BarrierFailure> {
        self.issued.fetch_add(1, Ordering::Relaxed);
        device_barrier(file).map_err(|error| BarrierFailure {
            path: path.to_path_buf(),
            error,
        })
    }
}

/// Test seam: what the file syncs did, and a way to make some of them fail.
#[cfg(any(test, feature = "testing"))]
impl DeviceBarriers {
    /// Answers each file sync this tracker runs with `fail(path)` when that's
    /// `Some`, instead of syncing. Replaces any earlier script.
    pub fn script_syncs(&self, fail: impl Fn(&Path) -> Option<io::Error> + Send + Sync + 'static) {
        self.script.lock_ignore_poison().fail = Some(Box::new(fail));
    }

    /// Every path a file sync ran on, in order, scripted ones included.
    #[must_use]
    pub fn synced_paths(&self) -> Vec<PathBuf> {
        self.script.lock_ignore_poison().synced.clone()
    }

    /// How many whole-volume syncs the passes fell back to.
    #[must_use]
    pub fn volume_syncs(&self) -> u64 {
        self.script.lock_ignore_poison().volume_syncs
    }
}

fn log_interval_failure(failure: &BarrierFailure) {
    log::warn!(
        target: "write_durability",
        "barrier: the drive holding {} didn't confirm a flush, so the closing pass tries again: {}",
        LogPrivate::path(&failure.path),
        // allowed-log-private: an errno from `fcntl` on an open fd, which names no path
        failure.error
    );
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
