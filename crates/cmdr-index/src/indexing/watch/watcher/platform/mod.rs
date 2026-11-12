//! Picks this OS's watcher backend, and its one property the lifecycle asks before a watcher
//! exists: whether the OS keeps a journal to replay from.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use super::{FsChangeEvent, WatcherBackend, WatcherError};

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod notify_backend;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

type Started = Result<Box<dyn WatcherBackend>, WatcherError>;

const _: fn(&Path, u64, mpsc::UnboundedSender<FsChangeEvent>) -> Started = start;
const _: fn(&Path, &[PathBuf], u64, mpsc::UnboundedSender<FsChangeEvent>) -> Started = start_branches;
const _: bool = SUPPORTS_EVENT_REPLAY;

/// Starts a watcher over `root`, replaying from `since_when` where the OS keeps a journal.
pub(super) use imp::start;

/// Starts a watcher for a branch-watched volume: the OS decides whether that's one watch over
/// `volume_root` (FSEvents, free per directory) or one per branch (`notify`, a watch per
/// directory against the OS's budget).
pub(super) use imp::start_branches;

/// Whether this OS's watcher keeps a journal of event IDs a restart replays from.
pub(super) use imp::SUPPORTS_EVENT_REPLAY;

/// What a queue overflow puts on the channel, where `notify` is the backend.
#[cfg(all(test, target_os = "linux"))]
pub(crate) use notify_backend::overflow_events_for_test;
