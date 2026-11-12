//! Making written files durable before a copy reports them done, without paying
//! a device barrier per file.
//!
//! Two steps, because macOS splits them: [`sync_to_drive`] hands one file's
//! bytes to its drive (`fsync(2)`, ~0.03 ms on an internal SSD), and a
//! [`DeviceBarriers`] pass asks the drive to empty its write cache
//! (`F_FULLFSYNC`, ~4 ms on that SSD and tens of ms on a spinning disk). One
//! barrier covers every file handed over before it, so a many-file copy pays it
//! on an interval and once at the end, never per file. On Linux `fdatasync`
//! already flushes the drive's cache, so the barrier there is nothing.
//! The rule, the measurements, and what it trades:
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
/// [`DeviceBarriers`] pass covers it.
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

/// The device barriers one writer owes: every device it handed a file to since
/// the last barrier, and the directories whose new entries still need their
/// `fsync`. A copy notes each file after [`sync_to_drive`]; a pass runs on its
/// own every [`interval`](Self::with_interval), and [`flush`](Self::flush) runs
/// the closing one, before the copy reports complete or deletes a moved source.
///
/// Thread-safe: concurrent copies note into one tracker, and a pass runs its
/// syncs outside the lock. A barrier that fails leaves its device owed, so the
/// next pass tries it again; a later barrier that holds covers everything
/// before it, so that answer is the one that counts.
pub struct DeviceBarriers {
    interval: Duration,
    owed: Mutex<Owed>,
    issued: AtomicU64,
    /// The time the passes and lone barriers took: [`Self::time_spent`].
    spent: Mutex<Spent>,
}

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
    /// One open handle per device a file was handed to, to issue its barrier
    /// through. The latest note wins, so the handle names a recent item.
    devices: HashMap<Option<DeviceId>, Holder>,
    /// Directories that gained entries, in first-seen order, each once.
    dirs: Vec<PathBuf>,
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
                last_pass: crate::rt::now(),
            }),
            issued: AtomicU64::new(0),
            spent: Mutex::new(Spent {
                done: Duration::ZERO,
                running: 0,
                since: None,
            }),
        }
    }

    /// Notes that `file` (at `path`) was handed to its drive by a
    /// [`sync_to_drive`] that answered `Ok`, so its device owes a barrier. Runs
    /// the interval pass when it's due. ❗ Only after the sync returned: a
    /// barrier covers what was handed over BEFORE it started.
    pub fn note_synced(&self, file: &File, path: &Path) {
        if NEEDS_DEVICE_BARRIER {
            match file.try_clone() {
                Ok(handle) => {
                    let device = file.metadata().ok().as_ref().and_then(device_of);
                    self.owed.lock_ignore_poison().devices.insert(
                        device,
                        Holder {
                            file: handle,
                            path: path.to_path_buf(),
                        },
                    );
                }
                // No handle to keep: barrier through this one now, so the
                // file isn't left uncovered.
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
        self.pass_if_due();
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

    /// The closing pass: `fsync`s every noted directory, then barriers every
    /// device still owed one. Tries every device before answering the first
    /// failure. Nothing owed is nothing to do.
    pub fn flush(&self) -> Result<(), BarrierFailure> {
        self.timed(|| self.pass())
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

    /// How long this tracker has spent in passes (directory syncs and barriers)
    /// and lone barriers, in total, a running one included up to now.
    /// Monotonic. The ETA reads it so a barrier a big file happened to pay
    /// isn't learned as a cost of every file (`cmdr-operations`' `eta.rs`);
    /// counting the running pass live is what keeps a barrier that spans two
    /// progress emits from being taken out of the second one whole.
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
        if due && let Err(failure) = self.timed(|| self.pass()) {
            log_interval_failure(&failure);
        }
    }

    fn pass(&self) -> Result<(), BarrierFailure> {
        let (dirs, mut devices) = {
            let mut owed = self.owed.lock_ignore_poison();
            owed.last_pass = crate::rt::now();
            (std::mem::take(&mut owed.dirs), std::mem::take(&mut owed.devices))
        };
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

    fn issue(&self, file: &File, path: &Path) -> Result<(), BarrierFailure> {
        self.issued.fetch_add(1, Ordering::Relaxed);
        device_barrier(file).map_err(|error| BarrierFailure {
            path: path.to_path_buf(),
            error,
        })
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
