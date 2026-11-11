//! Drive-level filesystem watcher for the indexing system: one [`DriveWatcher`] over a
//! per-OS [`WatcherBackend`] (`watcher/platform/`).
//!
//! - **macOS**: FSEvents via `cmdr-fsevent-stream`, file-level, with event IDs for scan/watch
//!   reconciliation and `sinceWhen` cold-start replay.
//! - **Linux and Windows**: the `notify` crate, recursive per watched root. No event IDs, so on
//!   startup the indexer rescans, comparing the disk against SQLite; live events carry a
//!   synthetic monotonic counter as `event_id`.
//! - **Tests on macOS**: a scripted fake journal (`watcher/fake_journal.rs`) for roots a test
//!   registers, so the phase machine runs without `fseventsd`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tokio::sync::mpsc;

// ── Public types ─────────────────────────────────────────────────────

/// A single filesystem change event, enriched with parsed flags.
#[derive(Debug, Clone)]
pub struct FsChangeEvent {
    /// The absolute path affected by the event.
    pub path: String,
    /// Monotonically increasing event ID (FSEvents on macOS, synthetic counter on Linux).
    pub event_id: u64,
    /// Parsed event flags.
    pub flags: FsEventFlags,
}

/// Parsed event flags relevant to the indexing system.
#[derive(Debug, Clone, Default)]
pub struct FsEventFlags {
    pub must_scan_sub_dirs: bool,
    pub item_created: bool,
    pub item_removed: bool,
    pub item_renamed: bool,
    pub item_modified: bool,
    pub item_is_file: bool,
    pub item_is_dir: bool,
    pub history_done: bool,
}

mod error;
pub use error::WatcherError;
mod journal;
pub use journal::{current_event_id, journal_uuid};

/// Whether this OS's watcher keeps a journal of event IDs a restart replays from (FSEvents
/// does; `notify` doesn't, so a restart rescans). A property of the backend, asked before any
/// watcher exists.
pub fn supports_event_replay() -> bool {
    platform::SUPPORTS_EVENT_REPLAY
}

mod platform;

/// What every OS's watcher does once started. The start itself is per OS (`platform/`),
/// because what a start takes differs: a journal position, or a set of branches.
pub(crate) trait WatcherBackend: Send {
    /// Also watch `branch`, for a volume watched branch by branch. A no-op where one watch
    /// over the volume root already carries it.
    fn watch_branch(&self, branch: &Path);
    /// Stops the stream; the event channel closes once its task ends.
    fn stop(&mut self);
    /// The last event ID seen (a synthetic counter without a journal).
    fn last_event_id(&self) -> u64;
    /// Whether the watcher is running.
    fn is_running(&self) -> bool;
    /// The flag the backend sets when its stream overflowed and events were dropped.
    fn overflow_flag(&self) -> Arc<AtomicBool>;
}

/// Watches a volume for filesystem changes, forwarding them as [`FsChangeEvent`]s over an
/// unbounded channel until [`stop`](Self::stop) or until the receiver drops.
pub struct DriveWatcher {
    backend: Box<dyn WatcherBackend>,
}

impl DriveWatcher {
    /// Start watching `root`. `since_when` is the journal position to replay from (`0` for
    /// "since now"), ignored where the OS keeps no journal ([`supports_event_replay`]).
    pub fn start(
        root: &Path,
        since_when: u64,
        event_sender: mpsc::UnboundedSender<FsChangeEvent>,
    ) -> Result<Self, WatcherError> {
        platform::start(root, since_when, event_sender).map(|backend| Self { backend })
    }

    /// Watch a volume whose index a search walk built, covering `branches`. FSEvents watches
    /// `volume_root` (free per directory, and it replays the whole journal); `notify` watches
    /// each branch (a watch per directory against the OS's budget). Either way the loop's
    /// [`WatchScope`](super::branches::WatchScope) decides which events belong to covered ground.
    pub fn start_branches(
        volume_root: &Path,
        branches: &[PathBuf],
        since_when: u64,
        event_sender: mpsc::UnboundedSender<FsChangeEvent>,
    ) -> Result<Self, WatcherError> {
        platform::start_branches(volume_root, branches, since_when, event_sender).map(|backend| Self { backend })
    }

    /// Also watch `branch` on the running watcher.
    pub fn watch_branch(&self, branch: &Path) {
        self.backend.watch_branch(branch);
    }

    /// Stop the watcher.
    pub fn stop(&mut self) {
        self.backend.stop();
    }

    /// The last event ID seen.
    #[allow(dead_code, reason = "a diagnostics accessor no caller reads today")]
    pub fn last_event_id(&self) -> u64 {
        self.backend.last_event_id()
    }

    /// Whether the watcher is running.
    #[allow(dead_code, reason = "a diagnostics accessor no caller reads today")]
    pub fn is_running(&self) -> bool {
        self.backend.is_running()
    }

    /// A shared handle to the overflow flag, for passing to async tasks.
    pub fn overflow_flag(&self) -> Arc<AtomicBool> {
        self.backend.overflow_flag()
    }
}

/// The test-only fake FSEvents journal (`watcher/fake_journal.rs`).
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod fake_journal;
