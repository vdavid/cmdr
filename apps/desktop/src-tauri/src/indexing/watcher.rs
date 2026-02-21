//! Drive-level FSEvents watcher for the indexing system.
//!
//! Monitors an entire volume root using macOS FSEvents with file-level granularity.
//! Provides event IDs for scan/watch reconciliation and `sinceWhen` replay on cold start.
//! Uses [`cmdr-fsevent-stream`] under the hood.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc;

use cmdr_fsevent_stream::ffi::{
    kFSEventStreamCreateFlagFileEvents, kFSEventStreamCreateFlagNoDefer, kFSEventStreamCreateFlagUseCFTypes,
    kFSEventStreamEventIdSinceNow,
};
use cmdr_fsevent_stream::stream::{Event, EventStreamHandler, StreamFlags, create_event_stream};

// ── Public types ─────────────────────────────────────────────────────

/// A single filesystem change event from FSEvents, enriched with parsed flags.
#[derive(Debug, Clone)]
pub struct FsChangeEvent {
    /// The absolute path affected by the event.
    pub path: String,
    /// Monotonically increasing event ID from FSEvents.
    pub event_id: u64,
    /// Parsed event flags.
    pub flags: FsEventFlags,
}

/// Parsed FSEvents flags relevant to the indexing system.
#[derive(Debug, Clone, Default)]
pub struct FsEventFlags {
    pub must_scan_sub_dirs: bool,
    pub item_created: bool,
    pub item_removed: bool,
    pub item_renamed: bool,
    pub item_modified: bool,
    pub item_is_file: bool,
    pub item_is_dir: bool,
    pub item_is_symlink: bool,
    pub history_done: bool,
}

/// Errors that can occur when starting the watcher.
#[derive(Debug)]
pub enum WatcherError {
    Io(std::io::Error),
    StreamCreate(String),
}

impl std::fmt::Display for WatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatcherError::Io(e) => write!(f, "I/O error: {e}"),
            WatcherError::StreamCreate(msg) => write!(f, "Failed to create FSEvents stream: {msg}"),
        }
    }
}

impl std::error::Error for WatcherError {}

// ── DriveWatcher ─────────────────────────────────────────────────────

/// Watches an entire volume root for filesystem changes via macOS FSEvents.
///
/// Runs the FSEvents stream on a dedicated tokio task, forwarding parsed events
/// through an `mpsc` channel. Supports `sinceWhen` for cold-start replay.
pub struct DriveWatcher {
    /// Whether the watcher is running.
    running: Arc<AtomicBool>,
    /// Last processed event ID (atomically updated as events arrive).
    last_event_id: Arc<AtomicU64>,
    /// Handle to abort the FSEvents run loop thread.
    handler: Option<EventStreamHandler>,
    /// Task that reads the event stream and forwards events.
    forward_task: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl DriveWatcher {
    /// Start watching `root` for filesystem changes.
    ///
    /// - `since_when`: FSEvents event ID to replay from. Use `0` for "since now"
    ///   (maps to `kFSEventStreamEventIdSinceNow`).
    /// - `event_sender`: channel to receive parsed events on.
    ///
    /// The watcher runs until [`stop`](Self::stop) is called or the sender is dropped.
    pub fn start(
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

        log::info!("DriveWatcher started on {} (sinceWhen={since_when})", root.display());

        // Spawn a task to read the async event stream and forward events.
        // Use tauri::async_runtime::spawn because the watcher can start from
        // the synchronous Tauri setup() hook where no Tokio runtime context exists.
        let running_clone = Arc::clone(&running);
        let last_id_clone = Arc::clone(&last_event_id);

        let forward_task = tauri::async_runtime::spawn(async move {
            let mut stream = event_stream.into_flatten();
            while let Some(event) = stream.next().await {
                if !running_clone.load(Ordering::Relaxed) {
                    break;
                }

                last_id_clone.store(event.id, Ordering::Relaxed);

                let parsed = parse_event(&event);
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
            handler: Some(handler),
            forward_task: Some(forward_task),
        })
    }

    /// Stop the watcher. Aborts the FSEvents run loop and waits for cleanup.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);

        if let Some(mut handler) = self.handler.take() {
            handler.abort();
        }

        if let Some(task) = self.forward_task.take() {
            task.abort();
        }

        log::info!(
            "DriveWatcher stopped (last_event_id={})",
            self.last_event_id.load(Ordering::Relaxed)
        );
    }

    /// Return the last event ID seen by the watcher.
    pub fn last_event_id(&self) -> u64 {
        self.last_event_id.load(Ordering::Relaxed)
    }

    /// Check if the watcher is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for DriveWatcher {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Convert a `cmdr_fsevent_stream::Event` into our `FsChangeEvent`.
fn parse_event(event: &Event) -> FsChangeEvent {
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
            item_is_symlink: event.flags.contains(StreamFlags::IS_SYMLINK),
            history_done: event.flags.contains(StreamFlags::HISTORY_DONE),
        },
    }
}

/// Get the current system-wide FSEvents event ID.
///
/// Useful for determining `sinceWhen` at the start of a scan.
pub fn current_event_id() -> u64 {
    // Safety: FSEventsGetCurrentEventId is a simple read of the global counter
    unsafe { cmdr_fsevent_stream::ffi::FSEventsGetCurrentEventId() }
}

// ── Tests ────────────────────────────────────────────────────────────

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

        let parsed = parse_event(&event);
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

        let parsed = parse_event(&event);
        assert!(parsed.flags.must_scan_sub_dirs);
        assert!(parsed.flags.item_is_dir);
    }

    #[test]
    fn current_event_id_returns_nonzero() {
        let id = current_event_id();
        assert!(id > 0, "system FSEvents event ID should be nonzero");
    }

    #[test]
    fn fs_event_flags_default() {
        let flags = FsEventFlags::default();
        assert!(!flags.must_scan_sub_dirs);
        assert!(!flags.item_created);
        assert!(!flags.item_removed);
    }
}
