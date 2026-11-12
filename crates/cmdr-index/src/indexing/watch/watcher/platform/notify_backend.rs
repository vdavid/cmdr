//! Linux and Windows: the `notify` crate (inotify on Linux, `ReadDirectoryChangesW` on Windows),
//! recursive per watched root. No event IDs and no journal, so on startup the indexer rescans,
//! comparing the disk against SQLite; live events carry a synthetic monotonic counter as
//! `event_id`.
//!
//! **A queue overflow is a `MustScanSubDirs` on every watched root** ([`rescan_anchors`]), so
//! events the OS dropped get the rescan FSEvents' own flag gets on macOS, through the same
//! routing, throttles, and status.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::pluralize::pluralize;
use tokio::sync::mpsc;

use super::super::{FsChangeEvent, FsEventFlags, WatcherBackend, WatcherError};

/// No journal to replay: a restart rescans.
pub(crate) const SUPPORTS_EVENT_REPLAY: bool = false;

/// Start watching `root`; `since_when` is ignored (no journal replay).
pub(crate) fn start(
    root: &Path,
    since_when: u64,
    event_sender: mpsc::UnboundedSender<FsChangeEvent>,
) -> Result<Box<dyn WatcherBackend>, WatcherError> {
    start_branches(root, &[root.to_path_buf()], since_when, event_sender)
}

/// Watch the covered `branches` themselves, not the volume root (see
/// [`NotifyBackend::start_branches`]).
pub(crate) fn start_branches(
    volume_root: &Path,
    branches: &[PathBuf],
    since_when: u64,
    event_sender: mpsc::UnboundedSender<FsChangeEvent>,
) -> Result<Box<dyn WatcherBackend>, WatcherError> {
    Ok(Box::new(NotifyBackend::start_branches(
        volume_root,
        branches,
        since_when,
        event_sender,
    )?))
}

/// Watches an entire volume root for filesystem changes via inotify (through the `notify` crate).
///
/// Uses `notify::RecommendedWatcher` in recursive mode. Events are translated into
/// `FsChangeEvent` with a synthetic monotonic counter for `event_id` (inotify has no
/// system-wide event IDs like FSEvents).
///
/// On startup the indexer always does a full rescan -- there is no journal replay.
pub(crate) struct NotifyBackend {
    /// Whether the watcher is running.
    running: Arc<AtomicBool>,
    /// Monotonic event counter (synthetic, not a system event ID).
    event_counter: Arc<AtomicU64>,
    /// The `notify` watcher handle. Dropped on stop.
    ///
    /// Shared and locked rather than owned outright, because a branch-watched
    /// volume adds paths to a RUNNING watcher: `notify` supports that, and it's
    /// what saves a volume from restarting its stream every time a search covers
    /// new ground.
    watcher: Arc<std::sync::Mutex<notify::RecommendedWatcher>>,
    /// The roots the OS accepted a watch on, which is the ground an overflow puts
    /// in doubt. Shared with the forward task, and grown by `watch_branch`.
    watched_roots: WatchedRoots,
    /// Task that reads notify events from the channel and forwards them.
    forward_task: Option<tokio::task::JoinHandle<()>>,
}

impl NotifyBackend {
    /// Watch a volume the way a branch-watched volume has to be watched here: the
    /// covered `branches` themselves, not the volume root.
    ///
    /// ❌ Don't copy macOS's shape and watch `_volume_root` instead. `notify`'s
    /// recursive mode registers ONE inotify watch per directory against
    /// `max_user_watches` (8,192 by default on many distributions), so watching a
    /// whole drive to then discard most of its events would spend the machine's
    /// watch budget on ground we don't answer for — and fail outright on a big
    /// tree. FSEvents has no such cost, which is why the two platforms differ here
    /// and nowhere else in this file.
    fn start_branches(
        _volume_root: &Path,
        branches: &[PathBuf],
        _since_when: u64,
        event_sender: mpsc::UnboundedSender<FsChangeEvent>,
    ) -> Result<Self, WatcherError> {
        use notify::Watcher;

        let roots = branches;
        let running = Arc::new(AtomicBool::new(true));
        let event_counter = Arc::new(AtomicU64::new(1));

        // Create a std channel for notify events, then forward to the tokio mpsc.
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();

        let watcher = notify::RecommendedWatcher::new(notify_tx, notify::Config::default())
            .map_err(|e| WatcherError::StreamCreate(format!("inotify: {e}")))?;
        let watcher = Arc::new(std::sync::Mutex::new(watcher));

        // One branch that's gone (a drive renamed under its index) costs that branch,
        // ❌ never the others: failing the whole start left every live branch
        // unwatched for the session (#695). Nothing watched at all is still an error.
        let watched_roots = WatchedRoots::default();
        let mut first_refusal = None;
        for root in roots {
            match watch_one(&watcher, root) {
                Ok(()) => note_watched(&watched_roots, root),
                Err(e) => {
                    log::warn!("Watcher: skipping a branch inotify refused: {e}");
                    first_refusal.get_or_insert(e);
                }
            }
        }
        if watched_roots.lock_ignore_poison().is_empty()
            && let Some(e) = first_refusal
        {
            return Err(e);
        }
        // Name the roots, ❌ never just count them. A branch watch covering
        // ground the user isn't working in reads identically to one covering the
        // right ground when the line is a bare number, and "the index went quiet"
        // is then unfalsifiable from a log alone (CI run 33909203247).
        let root_count = roots.len().max(1);
        log::info!(
            "Watcher: started on {root_count} path(s) (inotify, recursive): {}",
            std::fmt::from_fn(|f| {
                for (i, root) in roots.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}", LogPrivate::dir_path(root))?;
                }
                Ok(())
            }),
        );

        let running_clone = Arc::clone(&running);
        let counter_clone = Arc::clone(&event_counter);
        let roots_clone = Arc::clone(&watched_roots);

        let forward_task = crate::indexing::host::runtime::spawn(async move {
            // Bridge the std channel to the tokio world via spawn_blocking
            let handle = tokio::task::spawn_blocking(move || {
                while let Ok(result) = notify_rx.recv() {
                    if !running_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    let events = match result {
                        Ok(event) => translate_notify_event(event, &counter_clone, &roots_clone),
                        Err(e) => {
                            log::debug!("DriveWatcher: notify error: {e}");
                            continue;
                        }
                    };
                    for ev in events {
                        // Unbounded send never blocks (Fix 2: no backpressure into
                        // the notify backend); the event loop caps memory instead.
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
            watcher,
            watched_roots,
            forward_task: Some(forward_task),
        })
    }
}

impl WatcherBackend for NotifyBackend {
    /// Also watch `branch`, on a watcher that's already running.
    ///
    /// The registration walks the branch adding one inotify watch per directory,
    /// which is slow enough on a big tree to matter, so it runs on a blocking task
    /// rather than on the caller's thread (a search's, holding the index
    /// registry's lock). The window that opens is the one between asking and
    /// landing; the branch is being WALKED across it, and its events buffer until
    /// the walk ends either way.
    fn watch_branch(&self, branch: &Path) {
        let watcher = Arc::clone(&self.watcher);
        let watched_roots = Arc::clone(&self.watched_roots);
        let branch = branch.to_path_buf();
        crate::indexing::host::runtime::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || match watch_one(&watcher, &branch) {
                Ok(()) => {
                    note_watched(&watched_roots, &branch);
                    log::info!("Watcher: now watching branch {}", LogPrivate::dir_path(&branch));
                }
                Err(e) => log::warn!("Watcher: can't watch {}: {e}", LogPrivate::dir_path(&branch)),
            })
            .await;
        });
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);

        if let Some(task) = self.forward_task.take() {
            task.abort();
        }

        log::info!("Watcher: stopped (inotify)");
    }

    /// Return the last synthetic event counter value.
    fn last_event_id(&self) -> u64 {
        self.event_counter.load(Ordering::Relaxed)
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Never sets. The flag means OUR bridge dropped events, and this one is unbounded
    /// from `notify` to the loop. The OS dropping events is a different signal with its
    /// own route: [`rescan_anchors`].
    fn overflow_flag(&self) -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
}

impl Drop for NotifyBackend {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.stop();
        }
    }
}

/// Register one recursive inotify watch, mapping the two failures a caller can
/// do nothing about (a vanished path, an exhausted `max_user_watches`) onto one
/// typed error.
fn watch_one(watcher: &std::sync::Mutex<notify::RecommendedWatcher>, root: &Path) -> Result<(), WatcherError> {
    use notify::{RecursiveMode, Watcher};

    watcher
        .lock_ignore_poison()
        .watch(root, RecursiveMode::Recursive)
        // The cause without its paths: `notify` appends the root it was given, which the
        // variant already carries typed.
        .map_err(|e| WatcherError::WatchRefused {
            root: root.to_path_buf(),
            cause: notify::Error::new(e.kind).to_string(),
        })
}

/// The roots a running watcher holds a watch on.
type WatchedRoots = Arc<std::sync::Mutex<Vec<PathBuf>>>;

/// Record a root the OS accepted a watch on (once, however often it's asked for).
fn note_watched(watched_roots: &WatchedRoots, root: &Path) {
    let mut roots = watched_roots.lock_ignore_poison();
    if !roots.iter().any(|known| known == root) {
        roots.push(root.to_path_buf());
    }
}

/// What a queue overflow becomes: one `MustScanSubDirs` anchor per watched root.
///
/// The OS says only THAT it dropped events, not where, so every root this watcher
/// holds is in doubt: the same meaning as FSEvents coalescing to its stream's root.
/// Naming the roots as anchors (rather than raising `overflow_flag`) puts the signal
/// on the route macOS's takes, and that route is what bounds it: the loop merges a
/// burst of overflows into one anchor per root per flush, a volume-wide root meets
/// the sweep window (counted into the drive's status while it waits), a branch root
/// meets the cost-proportional throttle on the single-flight drain, and an anchor
/// that lands during a scan waits for the scan to end.
///
/// Which backends say so (verified against the source of `notify` 8.2.1, the locked
/// version, 2026-11-12):
///
/// - **inotify**: `IN_Q_OVERFLOW` arrives as `EventKind::Other` with `Flag::Rescan`.
///   The kernel queues one per overflow episode, however many events it dropped.
/// - **`ReadDirectoryChangesW`**: never. `notify` ignores the completion's byte
///   count, so a notification that overflowed its 16 KiB buffer (zero bytes, meaning
///   "enumerate the directory yourself") reads as an empty batch. Nothing here can
///   see it, so a Windows index can still drift after a burst (#1128). The flag is
///   `notify`'s contract for every backend, so this handles it the day Windows sets it.
fn rescan_anchors(watched_roots: &[PathBuf], counter: &AtomicU64) -> Vec<FsChangeEvent> {
    log::info!(
        "Watcher: the system dropped file change events (its queue overflowed), so {} will be rescanned",
        pluralize(watched_roots.len() as u64, "watched path")
    );
    watched_roots
        .iter()
        .map(|root| FsChangeEvent {
            path: root.to_string_lossy().to_string(),
            event_id: counter.fetch_add(1, Ordering::Relaxed),
            flags: FsEventFlags {
                must_scan_sub_dirs: true,
                item_is_dir: true,
                ..Default::default()
            },
        })
        .collect()
}

/// What a queue overflow over `roots` puts on the watcher's channel, for a test that
/// drives it on through the reconciler's routing.
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn overflow_events_for_test(roots: &[PathBuf]) -> Vec<FsChangeEvent> {
    let watched_roots = WatchedRoots::default();
    for root in roots {
        note_watched(&watched_roots, root);
    }
    let overflow = notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan);
    translate_notify_event(overflow, &AtomicU64::new(1), &watched_roots)
}

/// Translate a `notify::Event` into zero or more `FsChangeEvent`s.
///
/// The `notify` crate emits one event per filesystem operation, possibly affecting
/// multiple paths (for example, a rename emits one event with both old and new paths).
/// An event flagged for a rescan carries no paths at all: see [`rescan_anchors`].
fn translate_notify_event(
    event: notify::Event,
    counter: &AtomicU64,
    watched_roots: &std::sync::Mutex<Vec<PathBuf>>,
) -> Vec<FsChangeEvent> {
    use notify::EventKind;

    // Before the kind: the flag is what says events were lost, on any kind.
    if event.need_rescan() {
        return rescan_anchors(&watched_roots.lock_ignore_poison(), counter);
    }

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
fn classify_paths(paths: &[PathBuf]) -> FsEventFlags {
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

#[cfg(test)]
mod tests {
    use super::super::super::{current_event_id, supports_event_replay};
    use super::*;
    use std::sync::atomic::AtomicU64;

    fn no_roots() -> std::sync::Mutex<Vec<PathBuf>> {
        std::sync::Mutex::new(Vec::new())
    }

    /// What `notify` 8.2.1's inotify backend sends for `IN_Q_OVERFLOW`.
    fn overflow_event() -> notify::Event {
        notify::Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan)
    }

    /// The bug behind #1105: the overflow event has kind `Other`, which the translation
    /// skipped as a non-mutation, so nothing ever made up for the lost events.
    #[test]
    fn a_queue_overflow_asks_for_a_rescan_of_every_watched_root() {
        let counter = AtomicU64::new(7);
        let roots = std::sync::Mutex::new(vec![PathBuf::from("/home/me/code"), PathBuf::from("/mnt/photos")]);

        let results = translate_notify_event(overflow_event(), &counter, &roots);

        let anchors: Vec<(&str, u64)> = results.iter().map(|e| (e.path.as_str(), e.event_id)).collect();
        assert_eq!(anchors, vec![("/home/me/code", 7), ("/mnt/photos", 8)]);
        for event in &results {
            assert!(event.flags.must_scan_sub_dirs, "the anchor asks for its subtree");
            assert!(event.flags.item_is_dir);
            assert!(
                !event.flags.item_created && !event.flags.item_removed && !event.flags.item_modified,
                "an anchor is not a change to the root itself"
            );
        }
    }

    /// The flag is what counts, whichever kind a backend sets beside it.
    #[test]
    fn the_rescan_flag_wins_over_the_event_kind() {
        let counter = AtomicU64::new(1);
        let roots = std::sync::Mutex::new(vec![PathBuf::from("/home/me/code")]);
        let event = notify::Event::new(notify::EventKind::Any).set_flag(notify::event::Flag::Rescan);

        let results = translate_notify_event(event, &counter, &roots);

        assert_eq!(results.len(), 1);
        assert!(results[0].flags.must_scan_sub_dirs);
    }

    #[test]
    fn an_other_event_without_the_rescan_flag_is_still_skipped() {
        let counter = AtomicU64::new(1);
        let roots = std::sync::Mutex::new(vec![PathBuf::from("/home/me/code")]);

        let results = translate_notify_event(notify::Event::new(notify::EventKind::Other), &counter, &roots);

        assert!(results.is_empty());
    }

    #[test]
    fn a_root_asked_for_twice_is_one_anchor() {
        let roots = WatchedRoots::default();
        note_watched(&roots, Path::new("/home/me/code"));
        note_watched(&roots, Path::new("/home/me/code"));
        note_watched(&roots, Path::new("/mnt/photos"));

        let results = translate_notify_event(overflow_event(), &AtomicU64::new(1), &roots);

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn translate_create_event() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/test_file.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter, &no_roots());
        assert!(!results.is_empty());
        assert!(results[0].flags.item_created);
        assert_eq!(results[0].event_id, 1);
    }

    #[test]
    fn translate_remove_event() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Remove(notify::event::RemoveKind::File),
            paths: vec![PathBuf::from("/tmp/deleted_file.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter, &no_roots());
        assert!(!results.is_empty());
        assert!(results[0].flags.item_removed);
    }

    #[test]
    fn translate_rename_event() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::From)),
            paths: vec![PathBuf::from("/tmp/old_name.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter, &no_roots());
        assert!(!results.is_empty());
        assert!(results[0].flags.item_renamed);
    }

    #[test]
    fn translate_access_event_is_skipped() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Access(notify::event::AccessKind::Read),
            paths: vec![PathBuf::from("/tmp/read_file.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter, &no_roots());
        assert!(results.is_empty());
    }

    #[test]
    fn event_counter_increments() {
        let counter = AtomicU64::new(10);
        let event = notify::Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter, &no_roots());
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].event_id, 10);
        assert_eq!(results[1].event_id, 11);
        assert_eq!(counter.load(Ordering::Relaxed), 12);
    }

    #[test]
    fn current_event_id_is_zero_without_a_journal() {
        assert_eq!(current_event_id(Path::new("/")), 0);
    }

    #[test]
    fn no_journal_means_no_replay() {
        assert!(!supports_event_replay());
    }
}
