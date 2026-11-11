//! Linux and Windows: the `notify` crate (inotify on Linux, `ReadDirectoryChangesW` on Windows),
//! recursive per watched root. No event IDs and no journal, so on startup the indexer rescans,
//! comparing the disk against SQLite; live events carry a synthetic monotonic counter as
//! `event_id`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use cmdr_fs::log_private::LogPrivate;
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
        let mut watched = 0;
        let mut first_refusal = None;
        for root in roots {
            match watch_one(&watcher, root) {
                Ok(()) => watched += 1,
                Err(e) => {
                    log::warn!("Watcher: skipping a branch inotify refused: {e}");
                    first_refusal.get_or_insert(e);
                }
            }
        }
        if watched == 0
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

        let forward_task = crate::indexing::host::runtime::spawn(async move {
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
        let branch = branch.to_path_buf();
        crate::indexing::host::runtime::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || match watch_one(&watcher, &branch) {
                Ok(()) => log::info!("Watcher: now watching branch {}", LogPrivate::dir_path(&branch)),
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

    /// `notify` never reports an overflow here, so the flag never sets.
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
    use cmdr_fs::ignore_poison::IgnorePoison;
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

/// Translate a `notify::Event` into zero or more `FsChangeEvent`s.
///
/// The `notify` crate emits one event per filesystem operation, possibly affecting
/// multiple paths (for example, a rename emits one event with both old and new paths).
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

    #[test]
    fn translate_create_event() {
        let counter = AtomicU64::new(1);
        let event = notify::Event {
            kind: notify::EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/test_file.txt")],
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
            paths: vec![PathBuf::from("/tmp/deleted_file.txt")],
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
            paths: vec![PathBuf::from("/tmp/old_name.txt")],
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
            paths: vec![PathBuf::from("/tmp/read_file.txt")],
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
            paths: vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
            attrs: Default::default(),
        };

        let results = translate_notify_event(event, &counter);
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
