//! Per-repo watcher: subscribes to `.git/*` mutable-state paths and recomputes
//! `RepoInfo` whenever they change.
//!
//! Frontend never polls. The chip subscribes once and updates reactively from
//! whatever the host does with a [`GitStateSink`] report. Debounce is 200 ms per
//! repo, matching the existing listing watcher in `file_system/listing/`, and
//! [`recompute_and_report`] drops a repeat inside that window so one burst of
//! writes costs one report however the debouncer batches it.
//!
//! Two halves, split so a test can assert one without paying for the other:
//! [`GitWatcherRegistry`] does the bookkeeping (one watch per repository,
//! refcounted, torn down with the last subscriber), and a [`GitWatcherBackend`]
//! is what actually talks to the operating system.
//!
//! ❌ Nothing here names a window. The watcher recomputes a typed snapshot and
//! reports it; wording it and refreshing panes is the host's, through the sink.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::friendly_error::git::{FriendlyGitError, FriendlyGitErrorKind};
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};

use crate::repo::{RepoCache, RepoHandle, RepoInfo, repo_info};
use crate::state_sink::GitStateSink;

/// How long a burst of `.git/*` writes is allowed to settle before one report
/// goes out. A `git checkout` rewrites `HEAD`, `index`, and a pile of refs, and
/// the chip wants the state after all of it rather than a report per file.
///
/// Doubles as the window [`recompute_and_report`] coalesces a repeat inside.
///
/// `pub` only so the `testing`-gated [`crate::test_fixtures`] can re-export it:
/// this module is private, so nothing outside reaches it any other way. A suite
/// asserting "one report per burst" has to wait out this window to know the
/// count is final, and a hand-copied 200 would keep passing if this moved.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// What a backend calls once a repository's `.git/*` writes have settled.
type RepoChanged = Arc<dyn Fn() + Send + Sync>;

/// When one repository's watch last reported, and what that report carried.
///
/// One per live watch, created by [`GitWatcherRegistry::arm`] and shared with
/// the change callback, so it starts empty on every arm and dies with the last
/// unsubscribe. That's what makes the first report after arming unconditional.
type LastReport = Arc<Mutex<Option<(Instant, RepoInfo)>>>;

/// What a subscription arms so a repository's `.git/*` writes come back as one
/// debounced report.
///
/// A trait because arming a real FSEvents stream over ~10 paths is by far the
/// most expensive thing a subscribe does, and every cell that asserts only the
/// registry's bookkeeping has no use for it. Production always gets
/// [`NotifyWatcherBackend`]; a test asks [`GitPortal::with_scripted_watcher`]
/// for one it drives by hand.
///
/// [`GitPortal::with_scripted_watcher`]: crate::GitPortal::with_scripted_watcher
pub(crate) trait GitWatcherBackend: Send + Sync {
    /// Starts watching the gitdir of the repository at `repo_root`, calling
    /// `on_change` once per batch it emits, on a thread of the backend's own.
    /// ❗ A burst can arrive as more than one batch; making that ONE report is
    /// [`recompute_and_report`]'s job, ❌ not a promise a backend has to keep.
    ///
    /// The returned value keeps the watch alive and stops it when dropped, which
    /// is the whole contract: the registry stores it and never looks inside.
    fn watch(&self, repo_root: &Path, on_change: RepoChanged) -> Result<Box<dyn Send>, FriendlyGitError>;

    /// Pretends the gitdir at `repo_root` moved, answering whether a watch was
    /// armed for it. Only the scripted backend has such a door; the real one
    /// takes its events from the operating system and answers `false`.
    #[cfg(any(test, feature = "testing"))]
    fn fire(&self, _repo_root: &Path) -> bool {
        false
    }
}

/// The real backend: one `notify` debouncer per repository, watching the
/// `.git/*` paths a state change can touch.
pub(crate) struct NotifyWatcherBackend;

impl GitWatcherBackend for NotifyWatcherBackend {
    fn watch(&self, repo_root: &Path, on_change: RepoChanged) -> Result<Box<dyn Send>, FriendlyGitError> {
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            if result.is_err() {
                return;
            }
            on_change();
        })
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

        for path in watch_paths(repo_root) {
            // Some paths (`refs/`) are dirs, others (`HEAD`, `index`) are files.
            // notify happily handles both. Missing paths are common (no MERGE_HEAD
            // until a merge starts) – we register watches lazily by watching the
            // `.git` dir non-recursively as a fallback so create-then-modify still fires.
            if path.exists() {
                let mode = if path.is_dir() {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                let _ = debouncer.watch(&path, mode);
            }
        }
        // Always watch `.git` itself for create events on optional files.
        let dot_git = git_dir_path(repo_root);
        if dot_git.exists() {
            let _ = debouncer.watch(&dot_git, RecursiveMode::NonRecursive);
        }
        Ok(Box::new(debouncer))
    }
}

// ❌ Not `cfg(test)` alone: that's set only while this crate compiles its OWN
// test target, so a consumer's test build would see the scripted backend vanish
// and every bookkeeping cell would go back to paying for FSEvents.
#[cfg(any(test, feature = "testing"))]
pub(crate) use scripted::ScriptedWatcherBackend;

#[cfg(any(test, feature = "testing"))]
mod scripted {
    use super::{GitWatcherBackend, RepoChanged};
    use cmdr_fs::ignore_poison::IgnorePoison;
    use cmdr_fs::volume::friendly_error::git::FriendlyGitError;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    /// The armed repositories and what each one calls when it fires. Shared with
    /// every live watch guard, which is how a drop unarms exactly its own repo.
    type Armed = Arc<Mutex<HashMap<PathBuf, RepoChanged>>>;

    /// A backend that arms nothing with the operating system: it remembers which
    /// repositories have a watch and lets a test fire one by hand.
    #[derive(Default)]
    pub(crate) struct ScriptedWatcherBackend {
        armed: Armed,
    }

    impl ScriptedWatcherBackend {
        /// A backend with nothing armed yet.
        pub(crate) fn new() -> Self {
            Self::default()
        }
    }

    /// The live watch. Unarms its repository when the registry drops it, so the
    /// scripted backend tears down on exactly the same signal the real one does.
    struct ScriptedWatch {
        armed: Armed,
        repo_root: PathBuf,
    }

    impl Drop for ScriptedWatch {
        fn drop(&mut self) {
            self.armed.lock_ignore_poison().remove(&self.repo_root);
        }
    }

    impl GitWatcherBackend for ScriptedWatcherBackend {
        fn watch(&self, repo_root: &Path, on_change: RepoChanged) -> Result<Box<dyn Send>, FriendlyGitError> {
            self.armed
                .lock_ignore_poison()
                .insert(repo_root.to_path_buf(), on_change);
            Ok(Box::new(ScriptedWatch {
                armed: Arc::clone(&self.armed),
                repo_root: repo_root.to_path_buf(),
            }))
        }

        fn fire(&self, repo_root: &Path) -> bool {
            // Cloned out from under the lock: the callback reads the repository
            // and reports to the sink, which is not work to hold a mutex for.
            let armed = self.armed.lock_ignore_poison().get(repo_root).cloned();
            match armed {
                Some(on_change) => {
                    on_change();
                    true
                }
                None => false,
            }
        }
    }
}

/// One per repo. Owns the live watch and the subscriber count.
struct Subscription {
    refcount: u32,
    /// Keep the backend's watch alive so it doesn't stop. Opaque on purpose:
    /// what a backend hands back is its own business, and the registry only
    /// needs to hold it and drop it.
    _watch: Box<dyn Send>,
}

/// App-wide registry of per-repo subscriptions.
pub struct GitWatcherRegistry {
    backend: Arc<dyn GitWatcherBackend>,
    inner: Mutex<HashMap<PathBuf, Subscription>>,
}

impl GitWatcherRegistry {
    /// A registry watching the real filesystem.
    pub fn new() -> Self {
        Self::with_backend(Arc::new(NotifyWatcherBackend))
    }

    /// A registry over `backend`, which is what a test swaps.
    pub(crate) fn with_backend(backend: Arc<dyn GitWatcherBackend>) -> Self {
        Self {
            backend,
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Adds a subscriber for `repo_root`. Arms the watch on first call, bumps
    /// the refcount on subsequent ones. Answers the repository's handle and its
    /// canonical root, which is the spelling every later `unsubscribe` has to
    /// use.
    ///
    /// `repos` is shared rather than borrowed because the change callback
    /// outlives this call: every recompute opens the repository through the same
    /// cache the caller's own lookups use.
    pub fn arm(
        &self,
        repos: Arc<RepoCache>,
        sink: Arc<dyn GitStateSink>,
        repo_root: &Path,
    ) -> Result<(RepoHandle, PathBuf), FriendlyGitError> {
        let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        let (handle, root) = repos.discover(&canonical)?;

        let mut inner = self.inner.lock().expect("git watcher mutex poisoned");
        if let Some(sub) = inner.get_mut(&root) {
            sub.refcount = sub.refcount.saturating_add(1);
            return Ok((handle, root));
        }

        // First subscriber: arm the watch.
        let watcher_root = root.clone();
        let last_report: LastReport = Arc::new(Mutex::new(None));
        let on_change: RepoChanged =
            Arc::new(move || recompute_and_report(&repos, sink.as_ref(), &watcher_root, &last_report));
        let watch = self.backend.watch(&root, on_change)?;

        inner.insert(
            root.clone(),
            Subscription {
                refcount: 1,
                _watch: watch,
            },
        );
        Ok((handle, root))
    }

    /// [`arm`](Self::arm) plus the current `RepoInfo`, read synchronously so the
    /// chip never sees an empty interim state.
    ///
    /// ❗ The snapshot is the expensive half (`is_dirty` walks the worktree), so
    /// a caller that only wants the watcher running asks for `arm` instead.
    pub fn subscribe(
        &self,
        repos: Arc<RepoCache>,
        sink: Arc<dyn GitStateSink>,
        repo_root: &Path,
    ) -> Result<RepoInfo, FriendlyGitError> {
        let release = Arc::clone(&repos);
        let (handle, root) = self.arm(repos, sink, repo_root)?;
        repo_info(&handle, &root).inspect_err(|_| {
            // A caller handed an error will never unsubscribe, so give the hold
            // back rather than leaking a watch nobody knows about.
            self.unsubscribe(&release, &root);
        })
    }

    /// Drops a subscriber. Tears the watch down on the last unsubscribe, and
    /// releases what that repo was holding open: nobody is looking at it, so a
    /// full-repo-sized status snapshot and an open `gix` handle would be pure
    /// leak.
    pub fn unsubscribe(&self, repos: &RepoCache, repo_root: &Path) {
        let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
        let mut inner = self.inner.lock().expect("git watcher mutex poisoned");
        if let Some(sub) = inner.get_mut(&canonical) {
            sub.refcount = sub.refcount.saturating_sub(1);
            if sub.refcount == 0 {
                inner.remove(&canonical);
                repos.evict(&canonical);
                crate::status::invalidate_status_cache(&canonical);
            }
        }
    }

    /// For tests: count active repos.
    #[cfg(any(test, feature = "testing"))]
    pub fn active_repo_count(&self) -> usize {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.inner.lock_ignore_poison().len()
    }

    /// For tests: pretends `repo_root`'s gitdir moved, answering whether a watch
    /// was armed for it. Only a scripted backend can answer `true`.
    #[cfg(any(test, feature = "testing"))]
    pub fn fire(&self, repo_root: &Path) -> bool {
        self.backend.fire(repo_root)
    }
}

impl Default for GitWatcherRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Recomputes the repo's snapshot and hands it to the sink, unless the same
/// snapshot already went out for this burst.
///
/// The status cache is dropped BEFORE the report goes out: a subscriber reacts
/// to a report by asking for status, and a cache still holding the pre-change
/// walk would answer that with what the user just stopped seeing.
///
/// Any `.git/*` mutation we watch for is a superset of "the index might have
/// moved", so the drop is unconditional. Cheap (a `HashMap` remove), so it isn't
/// worth filtering by event type.
///
/// ## Why a repeat inside the window is nothing to tell
///
/// A backend calls this once per batch it emits, and `notify_debouncer_full`
/// emits on a tick cadence: one burst of writes whose per-path deadlines
/// straddle a tick boundary arrives as two batches, both recomputing the same
/// post-burst state. Each report costs a window event and a re-read of every
/// open portal pane, so the second one is pure waste.
///
/// The guard drops exactly that repeat and provably nothing else. An emission
/// is never sooner than [`DEBOUNCE`] after the write behind it, so a report
/// landing LESS than that after the previous one can only be about writes that
/// preceded the previous report's own recompute, which means they are already in
/// the snapshot it carried. An identical snapshot inside the window therefore
/// has no news in it.
///
/// ❌ Never widen this to "identical snapshot, whatever the gap". `RepoInfo`
/// answers the breadcrumb chip, and plenty of real changes leave it untouched:
/// a second `git branch`, a commit on a worktree that stays dirty, a `git add`
/// in a dirty repo. Each of those still has to reach the `.git/branches/` and
/// `.git/commits/` panes and the status column, and outside the window they do.
fn recompute_and_report(repos: &RepoCache, sink: &dyn GitStateSink, repo_root: &Path, last_report: &LastReport) {
    let Ok((handle, root)) = repos.discover(repo_root) else {
        return;
    };
    let Ok(info) = repo_info(&handle, &root) else {
        return;
    };
    crate::status::invalidate_status_cache(&root);

    {
        let mut last = last_report.lock_ignore_poison();
        if let Some((reported_at, reported)) = last.as_ref()
            && reported_at.elapsed() < DEBOUNCE
            && reported == &info
        {
            return;
        }
        // Stamped BEFORE the sink runs, so the gap this compares is the one the
        // debouncer scheduled. A slow sink then costs an extra report rather
        // than stretching the window over a change that came after it.
        *last = Some((Instant::now(), info.clone()));
    }
    sink.repo_changed(&root, info);
}

/// Returns the gitdir for a worktree (handles gitlink files).
fn git_dir_path(repo_root: &Path) -> PathBuf {
    let dot_git = repo_root.join(".git");
    if dot_git.is_file() {
        // gitlink: contents look like `gitdir: <path>` (relative or absolute).
        if let Ok(content) = std::fs::read_to_string(&dot_git)
            && let Some(stripped) = content.trim().strip_prefix("gitdir:")
        {
            let p = stripped.trim();
            let path = if Path::new(p).is_absolute() {
                PathBuf::from(p)
            } else {
                repo_root.join(p)
            };
            return path;
        }
    }
    dot_git
}

/// The set of paths inside `.git` whose changes should trigger a re-emit.
fn watch_paths(repo_root: &Path) -> Vec<PathBuf> {
    let git_dir = git_dir_path(repo_root);
    let mut paths: Vec<PathBuf> = [
        "HEAD",
        "ORIG_HEAD",
        "MERGE_HEAD",
        "FETCH_HEAD",
        "packed-refs",
        "index",
        "refs",
        "logs/HEAD",
    ]
    .iter()
    .map(|sub| git_dir.join(sub))
    .collect();

    // Linked worktrees: each has its own HEAD under
    // `<common-dir>/worktrees/<name>/HEAD`. We register one watch per
    // worktree at subscribe time. New worktrees added later are picked
    // up via the non-recursive `.git` watch (the `worktrees/` parent
    // directory's create event triggers a re-subscribe path on the
    // refresh – and even without that, a `git worktree add` always
    // touches `HEAD` in the main repo too, which fires a re-emit).
    //
    // Decision: per-worktree registration on enumeration rather than glob
    // support. notify-debouncer-full doesn't natively glob. Registering
    // each `worktrees/<name>/HEAD` keeps the notify config flat and
    // self-documenting; the cost is a few extra watcher entries per
    // worktree, which is negligible at typical worktree counts (1-5).
    let worktrees_dir = git_dir.join("worktrees");
    if let Ok(read) = std::fs::read_dir(&worktrees_dir) {
        for entry in read.flatten() {
            let head = entry.path().join("HEAD");
            if head.exists() {
                paths.push(head);
            }
        }
    }
    paths
}
