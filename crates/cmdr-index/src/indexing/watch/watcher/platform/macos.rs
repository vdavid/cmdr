//! macOS: FSEvents through `cmdr-fsevent-stream`, with event IDs for scan/watch reconciliation
//! and `sinceWhen` cold-start replay. One stream over the volume root carries every path under
//! it at no cost per directory, so a branch-watched volume watches its root too, and the loop's
//! `WatchScope` decides which events belong to covered ground.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fsevent_stream::ffi::{
    kFSEventStreamCreateFlagFileEvents, kFSEventStreamCreateFlagNoDefer, kFSEventStreamCreateFlagUseCFTypes,
    kFSEventStreamEventIdSinceNow,
};
use cmdr_fsevent_stream::stream::{Event, EventStreamHandler, StreamFlags, create_event_stream};
use futures_util::StreamExt;
use tokio::sync::mpsc;

#[cfg(test)]
use super::super::fake_journal;
use super::super::{FsChangeEvent, FsEventFlags, WatcherBackend, WatcherError};

/// FSEvents keeps a journal with event IDs, so a restart replays from the last one.
pub(crate) const SUPPORTS_EVENT_REPLAY: bool = true;

/// Start watching `root`; `since_when` is the FSEvents event ID to replay from (`0` for "since
/// now"). A root on the test-only fake journal gets that backend instead of a stream.
pub(crate) fn start(
    root: &Path,
    since_when: u64,
    event_sender: mpsc::UnboundedSender<FsChangeEvent>,
) -> Result<Box<dyn WatcherBackend>, WatcherError> {
    #[cfg(test)]
    if fake_journal::covers(root) {
        return Ok(Box::new(fake_journal::watcher(root, since_when, event_sender)));
    }
    Ok(Box::new(FsEventsBackend::start(root, since_when, event_sender)?))
}

/// Watch a volume whose index a search walk built, covering `_branches`.
///
/// One stream over the volume root, exactly as an indexed volume gets: an
/// FSEvents stream costs nothing per directory below its root, and pointing it
/// at the volume root is what makes `since_when` replay the whole journal
/// rather than only what the current branches happen to cover — a branch added
/// last session would otherwise have no history to replay. Which of those
/// events belong to covered ground is the loop's `WatchScope` to decide.
pub(crate) fn start_branches(
    volume_root: &Path,
    _branches: &[PathBuf],
    since_when: u64,
    event_sender: mpsc::UnboundedSender<FsChangeEvent>,
) -> Result<Box<dyn WatcherBackend>, WatcherError> {
    start(volume_root, since_when, event_sender)
}

/// Watches an entire volume root for filesystem changes via macOS FSEvents.
///
/// Runs the FSEvents stream on a dedicated tokio task, forwarding parsed events
/// through an `mpsc` channel. Supports `sinceWhen` for cold-start replay.
pub(crate) struct FsEventsBackend {
    /// Whether the watcher is running.
    running: Arc<AtomicBool>,
    /// Last processed event ID (atomically updated as events arrive).
    last_event_id: Arc<AtomicU64>,
    /// Set to `true` when the FSEvents channel overflows and events are dropped.
    overflow: Arc<AtomicBool>,
    /// Handle to abort the FSEvents run loop thread.
    handler: Option<EventStreamHandler>,
    /// Task that reads the event stream and forwards events.
    forward_task: Option<tokio::task::JoinHandle<()>>,
}

impl FsEventsBackend {
    /// Start watching `root` for filesystem changes.
    ///
    /// - `since_when`: FSEvents event ID to replay from. Use `0` for "since now" (maps to
    ///   `kFSEventStreamEventIdSinceNow`).
    /// - `event_sender`: channel to receive parsed events on.
    ///
    /// The watcher runs until [`stop`](Self::stop) is called or the sender is dropped.
    fn start(
        root: &Path,
        since_when: u64,
        event_sender: mpsc::UnboundedSender<FsChangeEvent>,
    ) -> Result<Self, WatcherError> {
        let running = Arc::new(AtomicBool::new(true));
        let last_event_id = Arc::new(AtomicU64::new(0));

        let since = if since_when == 0 {
            kFSEventStreamEventIdSinceNow
        } else {
            since_when
        };

        let flags =
            kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagNoDefer | kFSEventStreamCreateFlagFileEvents;

        let (event_stream, handler) = create_event_stream(
            [root],
            since,
            Duration::from_millis(100), // 100ms latency for batching
            flags,
        )
        .map_err(WatcherError::Io)?;

        let overflow = event_stream.overflow_flag();

        log::info!(
            "Watcher: started on {} (since_when={since_when})",
            LogPrivate::dir_path(root)
        );

        // Spawn a task to read the async event stream and forward events.
        // Spawn through the host runtime seam, which resolves a handle instead of
        // inheriting one: the watcher can start from the app's synchronous setup() hook,
        // where there's no ambient Tokio runtime for `tokio::spawn` to find.
        let running_clone = Arc::clone(&running);
        let last_id_clone = Arc::clone(&last_event_id);

        let forward_task = crate::indexing::host::runtime::spawn(async move {
            let mut stream = event_stream.into_flatten();
            while let Some(event) = stream.next().await {
                if !running_clone.load(Ordering::Relaxed) {
                    break;
                }

                last_id_clone.store(event.id, Ordering::Relaxed);

                let parsed = parse_fsevent(&event);
                // Unbounded send never blocks, so the FSEvents stream is never
                // backpressured into dropping events (Fix 2: a slow drain must not
                // cascade into a forced full scan). The event loop bounds memory via
                // the ingestion hard cap instead.
                if event_sender.send(parsed).is_err() {
                    // Receiver dropped, stop forwarding
                    break;
                }
            }
            log::debug!("DriveWatcher forward task exiting");
        });

        Ok(Self {
            running,
            last_event_id,
            overflow,
            handler: Some(handler),
            forward_task: Some(forward_task),
        })
    }
}

impl WatcherBackend for FsEventsBackend {
    /// Stop the watcher. Aborts the FSEvents run loop and waits for cleanup.
    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);

        if let Some(mut handler) = self.handler.take() {
            handler.abort();
        }

        if let Some(task) = self.forward_task.take() {
            task.abort();
        }

        log::info!(
            "Watcher: stopped (last_event_id={})",
            self.last_event_id.load(Ordering::Relaxed)
        );
    }

    /// Return the last event ID seen by the watcher.
    fn last_event_id(&self) -> u64 {
        self.last_event_id.load(Ordering::Relaxed)
    }

    /// Check if the watcher is currently running.
    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Returns a shared handle to the overflow flag for passing to async tasks.
    fn overflow_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.overflow)
    }

    /// Also watch `branch`, for a volume watched branch by branch.
    ///
    /// A no-op here: one FSEvents stream over the volume root already carries
    /// every path under it, at no cost per directory, and the loop's own
    /// `WatchScope` is what decides which of those
    /// paths belong to covered ground. Linux can't do that: see the `notify` backend.
    fn watch_branch(&self, _branch: &Path) {}
}

impl Drop for FsEventsBackend {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}

/// Convert a `cmdr_fsevent_stream::Event` into our `FsChangeEvent`.
#[cfg(target_os = "macos")]
fn parse_fsevent(event: &Event) -> FsChangeEvent {
    FsChangeEvent {
        path: event.path.to_string_lossy().to_string(),
        event_id: event.id,
        flags: FsEventFlags {
            must_scan_sub_dirs: event.flags.contains(StreamFlags::MUST_SCAN_SUBDIRS),
            item_created: event.flags.contains(StreamFlags::ITEM_CREATED),
            item_removed: event.flags.contains(StreamFlags::ITEM_REMOVED),
            item_renamed: event.flags.contains(StreamFlags::ITEM_RENAMED),
            item_modified: event.flags.contains(StreamFlags::ITEM_MODIFIED),
            item_is_file: event.flags.contains(StreamFlags::IS_FILE),
            item_is_dir: event.flags.contains(StreamFlags::IS_DIR),
            history_done: event.flags.contains(StreamFlags::HISTORY_DONE),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_event_flags() {
        let event = Event {
            path: "/test/file.txt".into(),
            inode: None,
            flags: StreamFlags::ITEM_CREATED | StreamFlags::IS_FILE,
            raw_flags: 0,
            id: 42,
        };

        let parsed = parse_fsevent(&event);
        assert_eq!(parsed.path, "/test/file.txt");
        assert_eq!(parsed.event_id, 42);
        assert!(parsed.flags.item_created);
        assert!(parsed.flags.item_is_file);
        assert!(!parsed.flags.item_removed);
        assert!(!parsed.flags.must_scan_sub_dirs);
        assert!(!parsed.flags.item_is_dir);
    }

    #[test]
    fn parse_event_must_scan_sub_dirs() {
        let event = Event {
            path: "/test/dir".into(),
            inode: None,
            flags: StreamFlags::MUST_SCAN_SUBDIRS | StreamFlags::IS_DIR,
            raw_flags: 0,
            id: 100,
        };

        let parsed = parse_fsevent(&event);
        assert!(parsed.flags.must_scan_sub_dirs);
        assert!(parsed.flags.item_is_dir);
    }

    #[test]
    fn fs_event_flags_default() {
        let flags = FsEventFlags::default();
        assert!(!flags.must_scan_sub_dirs);
        assert!(!flags.item_created);
        assert!(!flags.item_removed);
    }
}
