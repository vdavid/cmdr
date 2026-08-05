//! What a walk knows about itself that its batches can't carry.
//!
//! A cover walk reports through batches of discovered entries, and a batch fills
//! at 2 000 entries or not at all: a walk grinding through a slow tree, or parked
//! on a directory that hangs, emits nothing for as long as that lasts. A consumer
//! deriving "folders scanned" and "where it is" from those batches therefore shows
//! zero and no path while the walk is very much alive, which reads as frozen.
//!
//! These counters move per DIRECTORY READ instead, so progress follows the walk
//! rather than the batch flow. [`WalkHeartbeat::abandoned`] rides along for the
//! same reason: how much ground a walk gave up on is a fact only the walk has, and
//! no batch carries it either.
//!
//! Every field is its own `Arc`, matching [`ScanProgress`](super::ScanProgress):
//! a consumer clones the one counter it reads and keeps reading it after the walk
//! handle has moved to another thread.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_fs::ignore_poison::IgnorePoison;

/// A live walk's pulse: how far it has got, where it is, and what it gave up on.
///
/// Cheap to clone (three `Arc`s) and safe to read from any thread. Writes cost one
/// relaxed atomic increment plus, for the path, one short mutex section per
/// directory read — negligible beside the `readdir` it brackets.
#[derive(Clone, Debug)]
pub(crate) struct WalkHeartbeat {
    /// Directories whose read has STARTED. Counted at the start rather than the
    /// end so a walk sitting on one hung directory still reports the ones its
    /// other workers are in.
    dirs_scanned: Arc<AtomicU64>,
    /// How many times the walk gave up on ground it started: a read abandoned
    /// after a stall, or a subtree pruned by the consecutive-failure budget. A
    /// count of EVENTS, not of directories — one give-up prunes a whole subtree —
    /// so treat it as "did this happen", never as a number to show.
    abandoned: Arc<AtomicU64>,
    /// The directory being read right now. Indicative, not a cursor: the local
    /// walker reads up to eight at once and this is whichever started last.
    current: Arc<Mutex<Option<String>>>,
}

impl WalkHeartbeat {
    /// A fresh pulse for one walk.
    pub(crate) fn new() -> Self {
        Self {
            dirs_scanned: Arc::new(AtomicU64::new(0)),
            abandoned: Arc::new(AtomicU64::new(0)),
            current: Arc::new(Mutex::new(None)),
        }
    }

    /// A directory's read is about to start.
    pub(crate) fn entering(&self, path: &Path) {
        self.dirs_scanned.fetch_add(1, Ordering::Relaxed);
        *self.current.lock_ignore_poison() = Some(path.to_string_lossy().into_owned());
    }

    /// Record `count` more give-ups (see [`Self::abandoned_count`]).
    pub(crate) fn abandoned(&self, count: u64) {
        if count > 0 {
            self.abandoned.fetch_add(count, Ordering::Relaxed);
        }
    }

    /// The counter itself, so a consumer can keep reading it after the walk handle
    /// has gone somewhere else.
    pub(crate) fn dirs_scanned_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.dirs_scanned)
    }

    /// The current-directory slot, same reason.
    pub(crate) fn current_dir_slot(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.current)
    }

    /// How many times this walk gave up on ground it started. Non-zero means its
    /// rows are a lower bound even if it ran to the end.
    pub(crate) fn abandoned_count(&self) -> u64 {
        self.abandoned.load(Ordering::Relaxed)
    }
}
