#![allow(
    dead_code,
    reason = "Subscriber side wires up during session integration in this milestone"
)]

//! Viewer file watcher: shared `notify-debouncer-full` singleton with one
//! `Subscription` per `ViewerSession`.
//!
//! Mirrors the structural pattern of [`crate::file_system::watcher`] (one
//! debouncer per process, registrations stored in a `HashMap`, drop-driven
//! unregistration). Tailored for single-file watches: each subscription owns a
//! channel that receives `WatcherEvent`s coalesced and classified from the raw
//! notify-debouncer events.
//!
//! Classification per debounce window:
//! - `Replaced` when an inode / device id change is observed (rename + atomic
//!   replace, log rotation that swaps the file out)
//! - `Shrunk` when the file's size dropped vs. last-known size, or the file
//!   went missing momentarily (truncation, in-place reset)
//! - `Grew(new_size)` when the size grew vs. last-known
//! - `MetadataOnly` when only mtime / permissions / etc. changed
//!
//! Subscriptions consume events from a `crossbeam-channel`-style `mpsc`
//! receiver. Dropping a `ViewerSubscription` releases the registration via the
//! internal `Arc` strong-count check; once no subscriber holds it, the path is
//! unwatched from the shared debouncer.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use notify_debouncer_full::{
    DebounceEventResult, Debouncer, RecommendedCache, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
};

use crate::ignore_poison::IgnorePoison;

/// Debounce window for raw notify events. Matches the `file_system` watcher's
/// rationale: 300 ms collapses a multi-write append into one event without
/// adding visible latency.
const DEBOUNCE_MS: u64 = 300;

/// Bounded subscription channel: capacity high enough that bursts don't drop,
/// low enough that a stuck consumer doesn't grow without bound. The watcher
/// uses `try_send` so a full channel is treated as a coalesced event (the
/// subscriber will pick up state on next read either way).
const SUBSCRIPTION_CHANNEL_CAPACITY: usize = 64;

/// Classified file-system event for a tailed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherEvent {
    /// File size grew to `new_size` bytes.
    Grew(u64),
    /// File was truncated or shrunk.
    Shrunk,
    /// File was replaced (rename, atomic-write swap, inode change).
    Replaced,
    /// Metadata changed but the byte content did not (mtime, permissions).
    MetadataOnly,
}

/// Internal per-path bookkeeping shared by the manager and the debouncer
/// callback. The `Mutex` here protects mutable per-path state (subscriber
/// list, last-known metadata) without blocking the global manager lock.
struct PathState {
    path: PathBuf,
    /// Subscribers wanting events for this path.
    subscribers: Vec<SyncSender<WatcherEvent>>,
    /// Last seen file size; drives Grew / Shrunk classification.
    last_size: u64,
    /// Last seen inode (Unix). `None` until populated.
    #[cfg(unix)]
    last_ino: Option<u64>,
    /// Drop sentinel: subscriptions hold a `Weak<Arc<()>>` derived from this
    /// `Arc`. When the strong-count drops back to one (manager only), the
    /// manager unregisters the path.
    drop_token: Arc<()>,
}

/// Handle returned by `subscribe`. Receives `WatcherEvent`s; dropping it
/// unregisters the subscriber and (when it was the last) unwatches the path.
pub struct ViewerSubscription {
    rx: Receiver<WatcherEvent>,
    path: PathBuf,
    /// Held so the manager's `Weak::upgrade` succeeds while the subscription
    /// is alive. When this drops, the manager observes a stale registration
    /// and unwatches the path on the next event or `subscribe` call.
    _drop_token: Arc<()>,
    sender_id: usize,
}

impl ViewerSubscription {
    /// Returns the next event without blocking.
    #[allow(dead_code, reason = "exposed for session manager and integration tests")]
    pub fn try_recv(&self) -> Option<WatcherEvent> {
        self.rx.try_recv().ok()
    }

    /// Waits up to `timeout` for the next event.
    #[allow(dead_code, reason = "used by integration tests")]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<WatcherEvent> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Blocks until the next event arrives or the watcher disconnects.
    /// Returns `None` on disconnect.
    pub fn recv(&self) -> Option<WatcherEvent> {
        self.rx.recv().ok()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ViewerSubscription {
    fn drop(&mut self) {
        VIEWER_WATCHER_MANAGER.unsubscribe(&self.path, self.sender_id);
    }
}

/// Shared singleton.
pub struct ViewerWatcherManager {
    inner: Mutex<ManagerInner>,
}

struct ManagerInner {
    states: HashMap<PathBuf, Arc<Mutex<PathState>>>,
    debouncers: HashMap<PathBuf, Debouncer<RecommendedWatcher, RecommendedCache>>,
    /// Monotonic counter to give each subscriber a stable id (Vec<SyncSender>
    /// alone can't be compared by identity).
    next_sender_id: usize,
    /// Per-path: list of (sender_id, sender) pairs. Indirection avoids
    /// re-walking `states` for every send.
    subscriber_ids: HashMap<PathBuf, Vec<(usize, SyncSender<WatcherEvent>)>>,
}

/// Process-wide singleton.
pub static VIEWER_WATCHER_MANAGER: LazyLock<ViewerWatcherManager> = LazyLock::new(ViewerWatcherManager::new);

impl ViewerWatcherManager {
    fn new() -> Self {
        Self {
            inner: Mutex::new(ManagerInner {
                states: HashMap::new(),
                debouncers: HashMap::new(),
                next_sender_id: 0,
                subscriber_ids: HashMap::new(),
            }),
        }
    }

    /// Subscribe to events for `path`. The first subscriber creates the
    /// underlying debouncer; subsequent subscribers reuse it.
    pub fn subscribe(&self, path: &Path) -> std::io::Result<ViewerSubscription> {
        // Canonicalise so symlink-equivalent paths (`/var/folders/...` vs.
        // `/private/var/folders/...` on macOS) hit the same registration and
        // so FSEvents emits paths that match our stored target.
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

        let (tx, rx) = sync_channel::<WatcherEvent>(SUBSCRIPTION_CHANNEL_CAPACITY);

        let mut inner = self.inner.lock_ignore_poison();
        inner.next_sender_id += 1;
        let sender_id = inner.next_sender_id;

        // First subscriber for this path: install the debouncer and state.
        let needs_debouncer = !inner.states.contains_key(&canonical);

        if needs_debouncer {
            let (initial_size, _initial_ino) = initial_metadata(&canonical);
            let state = Arc::new(Mutex::new(PathState {
                path: canonical.clone(),
                subscribers: Vec::new(),
                last_size: initial_size,
                #[cfg(unix)]
                last_ino: _initial_ino,
                drop_token: Arc::new(()),
            }));
            inner.states.insert(canonical.clone(), state.clone());

            // Watch the parent directory: notify-rs on macOS can't reliably
            // observe a single file when atomic-replace swaps the inode. The
            // parent-watch + path-filter pattern matches what existing
            // log-tail libraries do.
            let watch_target = canonical
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| canonical.clone());
            let state_for_cb = Arc::downgrade(&state);
            let target_path = canonical.clone();

            let mut debouncer = new_debouncer(
                Duration::from_millis(DEBOUNCE_MS),
                None,
                move |result: DebounceEventResult| {
                    let Some(state) = state_for_cb.upgrade() else {
                        return;
                    };
                    if let Ok(events) = result {
                        // Filter to events touching the target path; the
                        // parent-watch sees siblings too.
                        let mut touched = false;
                        for event in &events {
                            if event.paths.iter().any(|p| p == &target_path) {
                                touched = true;
                                break;
                            }
                        }
                        if !touched {
                            return;
                        }
                        classify_and_emit(&state);
                    }
                    // Errors are ignored: a transient FS-event error usually
                    // resolves on the next debounce window.
                },
            )
            .map_err(|e| std::io::Error::other(format!("create viewer watcher: {}", e)))?;

            debouncer
                .watch(&watch_target, RecursiveMode::NonRecursive)
                .map_err(|e| std::io::Error::other(format!("watch viewer path: {}", e)))?;

            inner.debouncers.insert(canonical.clone(), debouncer);
        }

        // Register sender on the path state.
        let state = inner.states.get(&canonical).expect("state inserted above").clone();
        let drop_token = state.lock_ignore_poison().drop_token.clone();

        state.lock_ignore_poison().subscribers.push(tx.clone());
        inner
            .subscriber_ids
            .entry(canonical.clone())
            .or_default()
            .push((sender_id, tx));

        Ok(ViewerSubscription {
            rx,
            path: canonical,
            _drop_token: drop_token,
            sender_id,
        })
    }

    fn unsubscribe(&self, path: &Path, sender_id: usize) {
        let canonical = path.to_path_buf();
        let mut inner = self.inner.lock_ignore_poison();
        let Some(senders) = inner.subscriber_ids.get_mut(&canonical) else {
            return;
        };
        senders.retain(|(id, _)| *id != sender_id);
        let remaining: Vec<SyncSender<WatcherEvent>> = senders.iter().map(|(_, s)| s.clone()).collect();
        if remaining.is_empty() {
            inner.subscriber_ids.remove(&canonical);
            inner.debouncers.remove(&canonical);
            inner.states.remove(&canonical);
        } else if let Some(state) = inner.states.get(&canonical) {
            state.lock_ignore_poison().subscribers = remaining;
        }
    }

    /// Test-only: count active path watches.
    #[cfg(test)]
    pub fn watch_count(&self) -> usize {
        self.inner.lock_ignore_poison().states.len()
    }
}

fn initial_metadata(path: &Path) -> (u64, Option<u64>) {
    match fs::metadata(path) {
        Ok(m) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                (m.len(), Some(m.ino()))
            }
            #[cfg(not(unix))]
            {
                (m.len(), None)
            }
        }
        Err(_) => (0, None),
    }
}

fn classify_and_emit(state_arc: &Arc<Mutex<PathState>>) {
    let mut state = state_arc.lock_ignore_poison();

    let metadata = fs::metadata(&state.path);
    let event = match metadata {
        Ok(m) => {
            let new_size = m.len();
            #[cfg(unix)]
            let new_ino = {
                use std::os::unix::fs::MetadataExt;
                Some(m.ino())
            };
            #[cfg(not(unix))]
            let new_ino: Option<u64> = None;

            #[cfg(unix)]
            let inode_changed = match (state.last_ino, new_ino) {
                (Some(prev), Some(curr)) => prev != curr,
                _ => false,
            };
            #[cfg(not(unix))]
            let inode_changed = false;

            let event = if inode_changed {
                WatcherEvent::Replaced
            } else if new_size > state.last_size {
                WatcherEvent::Grew(new_size)
            } else if new_size < state.last_size {
                WatcherEvent::Shrunk
            } else {
                WatcherEvent::MetadataOnly
            };

            state.last_size = new_size;
            #[cfg(unix)]
            {
                state.last_ino = new_ino;
            }
            event
        }
        Err(_) => {
            // File disappeared. Treat as Replaced; the consumer will reopen and
            // either find a fresh inode or see the file's actual absence on the
            // reopen.
            WatcherEvent::Replaced
        }
    };

    // Send to every subscriber; ignore full / disconnected receivers (a slow
    // consumer would only see a coalesced view anyway).
    for tx in &state.subscribers {
        let _ = tx.try_send(event.clone());
    }
}

/// Helper: convenience hook for tests that want to push events into the
/// subscription channels without standing up the debouncer.
#[cfg(test)]
pub fn test_only_emit(path: &Path, event: WatcherEvent) -> usize {
    let inner = VIEWER_WATCHER_MANAGER.inner.lock_ignore_poison();
    if let Some(state) = inner.states.get(path) {
        let subs = state.lock_ignore_poison();
        let mut sent = 0;
        for tx in &subs.subscribers {
            if tx.try_send(event.clone()).is_ok() {
                sent += 1;
            }
        }
        return sent;
    }
    0
}
