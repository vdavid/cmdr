//! Drive-level filesystem watcher for the indexing system.
//!
//! **macOS:** Uses FSEvents via [`cmdr-fsevent-stream`] for file-level granularity
//! with event IDs for scan/watch reconciliation and `sinceWhen` cold-start replay.
//!
//! **Linux:** Uses the [`notify`] crate (inotify backend) for recursive directory
//! watching. No event IDs -- on startup the indexer always does a full rescan
//! comparing filesystem state against SQLite. Live events flow through the same
//! `FsChangeEvent` type with `event_id` set to a monotonic counter.
//!
//! **Other platforms:** Stub implementations so the indexing system compiles.
//! `DriveWatcher::start` returns `WatcherError::StreamCreate` and
//! `current_event_id` returns `0`.

#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicU64;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use futures_util::StreamExt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use tokio::sync::mpsc;

#[cfg(target_os = "macos")]
use cmdr_fsevent_stream::ffi::{
    kFSEventStreamCreateFlagFileEvents, kFSEventStreamCreateFlagNoDefer, kFSEventStreamCreateFlagUseCFTypes,
    kFSEventStreamEventIdSinceNow,
};
#[cfg(target_os = "macos")]
use cmdr_fsevent_stream::stream::{Event, EventStreamHandler, StreamFlags, create_event_stream};

#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicU64;

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

/// Errors that can occur when starting the watcher.
#[derive(Debug)]
pub enum WatcherError {
    /// Used on macOS (FSEvents); not constructed on Linux.
    #[allow(dead_code, reason = "constructed on macOS, not Linux")]
    Io(std::io::Error),
    /// Used on Linux (inotify) and stub platforms; not constructed on macOS.
    #[allow(dead_code, reason = "constructed on Linux/stub, not macOS")]
    StreamCreate(String),
}

impl std::fmt::Display for WatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatcherError::Io(e) => write!(f, "I/O error: {e}"),
            WatcherError::StreamCreate(msg) => {
                write!(f, "Failed to create watcher: {msg}")
            }
        }
    }
}

impl std::error::Error for WatcherError {}

/// Whether the current platform supports event ID-based journal replay.
///
/// macOS FSEvents provides monotonic event IDs for cold-start replay.
/// Linux inotify has no journal -- always rescan on startup.
pub fn supports_event_replay() -> bool {
    cfg!(target_os = "macos")
}

// ── DriveWatcher (macOS) ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
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

        log::debug!("DriveWatcher started on {} (sinceWhen={since_when})", root.display());

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

                let parsed = parse_fsevent(&event);
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

        log::debug!(
            "DriveWatcher stopped (last_event_id={})",
            self.last_event_id.load(Ordering::Relaxed)
        );
    }

    /// Return the last event ID seen by the watcher.
    #[allow(dead_code, reason = "cross-platform API; used on other platforms")]
    pub fn last_event_id(&self) -> u64 {
        self.last_event_id.load(Ordering::Relaxed)
    }

    /// Check if the watcher is currently running.
    #[allow(dead_code, reason = "cross-platform API; used on other platforms")]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

#[cfg(target_os = "macos")]
impl Drop for DriveWatcher {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}

// ── DriveWatcher (Linux) ─────────────────────────────────────────────

#[cfg(target_os = "linux")]
/// Watches an entire volume root for filesystem changes via inotify (through the `notify` crate).
///
/// Uses `notify::RecommendedWatcher` in recursive mode. Events are translated into
/// `FsChangeEvent` with a synthetic monotonic counter for `event_id` (inotify has no
/// system-wide event IDs like FSEvents).
///
/// On startup the indexer always does a full rescan -- there is no journal replay.
pub struct DriveWatcher {
    /// Whether the watcher is running.
    running: Arc<AtomicBool>,
    /// Monotonic event counter (synthetic, not a system event ID).
    event_counter: Arc<AtomicU64>,
    /// The `notify` watcher handle. Dropped on stop.
    _watcher: notify::RecommendedWatcher,
    /// Task that reads notify events from the channel and forwards them.
    forward_task: Option<tauri::async_runtime::JoinHandle<()>>,
}

#[cfg(target_os = "linux")]
impl DriveWatcher {
    /// Start watching `root` for filesystem changes.
    ///
    /// `since_when` is ignored on Linux (no journal replay).
    pub fn start(
        root: &std::path::Path,
        _since_when: u64,
        event_sender: mpsc::UnboundedSender<FsChangeEvent>,
    ) -> Result<Self, WatcherError> {
        use notify::{RecursiveMode, Watcher};

        let running = Arc::new(AtomicBool::new(true));
        let event_counter = Arc::new(AtomicU64::new(1));

        // Create a std channel for notify events, then forward to the tokio mpsc.
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();

        let mut watcher = notify::RecommendedWatcher::new(notify_tx, notify::Config::default())
            .map_err(|e| WatcherError::StreamCreate(format!("inotify: {e}")))?;

        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| WatcherError::StreamCreate(format!("inotify watch: {e}")))?;

        log::debug!("DriveWatcher (inotify) started on {}", root.display());

        let running_clone = Arc::clone(&running);
        let counter_clone = Arc::clone(&event_counter);

        let forward_task = tauri::async_runtime::spawn(async move {
            // Bridge the std channel to the tokio world via spawn_blocking
            let handle = tokio::task::spawn_blocking(move || {
                while let Ok(result) = notify_rx.recv() {
                    if !running_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    let events = match result {
                        Ok(event) => translate_notify_event(event, &counter_clone),
                        Err(e) => {
                            log::debug!("DriveWatcher: notify error: {e}");
                            continue;
                        }
                    };
                    for ev in events {
                        if event_sender.send(ev).is_err() {
                            return; // receiver dropped
                        }
                    }
                }
            });
            let _ = handle.await;
            log::debug!("DriveWatcher (inotify) forward task exiting");
        });

        Ok(Self {
            running,
            event_counter,
            _watcher: watcher,
            forward_task: Some(forward_task),
        })
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);

        if let Some(task) = self.forward_task.take() {
            task.abort();
        }

        log::debug!("DriveWatcher (inotify) stopped");
    }

    /// Return the last synthetic event counter value.
    #[allow(dead_code, reason = "cross-platform API; used on other platforms")]
    pub fn last_event_id(&self) -> u64 {
        self.event_counter.load(Ordering::Relaxed)
    }

    #[allow(dead_code, reason = "cross-platform API; used on other platforms")]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

#[cfg(target_os = "linux")]
impl Drop for DriveWatcher {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}

/// Translate a `notify::Event` into zero or more `FsChangeEvent`s.
///
/// The `notify` crate emits one event per filesystem operation, possibly affecting
/// multiple paths (for example, a rename emits one event with both old and new paths).
#[cfg(target_os = "linux")]
fn translate_notify_event(event: notify::Event, counter: &AtomicU64) -> Vec<FsChangeEvent> {
    use notify::EventKind;

    let flags = match &event.kind {
        EventKind::Create(_) => FsEventFlags {
            item_created: true,
            ..classify_paths(&event.paths)
        },
        EventKind::Remove(_) => FsEventFlags {
            item_removed: true,
            ..classify_paths(&event.paths)
        },
        EventKind::Modify(modify_kind) => {
            use notify::event::ModifyKind;
            match modify_kind {
                ModifyKind::Name(_) => FsEventFlags {
                    item_renamed: true,
                    ..classify_paths(&event.paths)
                },
                _ => FsEventFlags {
                    item_modified: true,
                    ..classify_paths(&event.paths)
                },
            }
        }
        EventKind::Access(_) | EventKind::Other | EventKind::Any => {
            return Vec::new(); // skip non-mutation events
        }
    };

    event
        .paths
        .into_iter()
        .map(|path| {
            let id = counter.fetch_add(1, Ordering::Relaxed);
            FsChangeEvent {
                path: path.to_string_lossy().to_string(),
                event_id: id,
                flags: flags.clone(),
            }
        })
        .collect()
}

/// Classify the first path in the list as file, dir, or symlink.
#[cfg(target_os = "linux")]
fn classify_paths(paths: &[std::path::PathBuf]) -> FsEventFlags {
    if let Some(path) = paths.first()
        && let Ok(meta) = std::fs::symlink_metadata(path)
    {
        return FsEventFlags {
            item_is_file: meta.is_file(),
            item_is_dir: meta.is_dir(),
            ..Default::default()
        };
    }
    // Path may no longer exist (deletion); the caller sets the kind flags
    Default::default()
}

// ── DriveWatcher stub (other platforms) ──────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
/// Stub DriveWatcher for unsupported platforms.
pub struct DriveWatcher {
    _private: (),
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
impl DriveWatcher {
    pub fn start(
        _root: &std::path::Path,
        _since_when: u64,
        _event_sender: tokio::sync::mpsc::UnboundedSender<FsChangeEvent>,
    ) -> Result<Self, WatcherError> {
        Err(WatcherError::StreamCreate(
            "Filesystem watching is not supported on this platform".to_string(),
        ))
    }

    pub fn stop(&mut self) {}

    #[allow(dead_code, reason = "cross-platform API; used on other platforms")]
    pub fn last_event_id(&self) -> u64 {
        0
    }

    #[allow(dead_code, reason = "cross-platform API; used on other platforms")]
    pub fn is_running(&self) -> bool {
        false
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

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

/// Get the current system-wide FSEvents event ID.
///
/// Useful for determining `sinceWhen` at the start of a scan.
/// Returns `0` on non-macOS platforms (no event ID concept).
#[cfg(target_os = "macos")]
pub fn current_event_id() -> u64 {
    // Safety: FSEventsGetCurrentEventId is a simple read of the global counter
    unsafe { cmdr_fsevent_stream::ffi::FSEventsGetCurrentEventId() }
}

#[cfg(not(target_os = "macos"))]
pub fn current_event_id() -> u64 {
    0
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(all(test, target_os = "macos"))]
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

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn translate_create_event() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![std::path::PathBuf::from("/tmp/test_file.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter);
        assert!(!results.is_empty());
        assert!(results[0].flags.item_created);
        assert_eq!(results[0].event_id, 1);
    }

    #[test]
    fn translate_remove_event() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![std::path::PathBuf::from("/tmp/deleted_file.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter);
        assert!(!results.is_empty());
        assert!(results[0].flags.item_removed);
    }

    #[test]
    fn translate_rename_event() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::From)),
            paths: vec![std::path::PathBuf::from("/tmp/old_name.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter);
        assert!(!results.is_empty());
        assert!(results[0].flags.item_renamed);
    }

    #[test]
    fn translate_access_event_is_skipped() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![std::path::PathBuf::from("/tmp/read_file.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter);
        assert!(results.is_empty());
    }

    #[test]
    fn event_counter_increments() {
        let counter = AtomicU64::new(10);
        let event = notify::Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![
                std::path::PathBuf::from("/tmp/a.txt"),
                std::path::PathBuf::from("/tmp/b.txt"),
            ],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].event_id, 10);
        assert_eq!(results[1].event_id, 11);
        assert_eq!(counter.load(Ordering::Relaxed), 12);
    }

    #[test]
    fn current_event_id_returns_zero_on_linux() {
        assert_eq!(current_event_id(), 0);
    }

    #[test]
    fn supports_event_replay_false_on_linux() {
        assert!(!supports_event_replay());
    }
}
